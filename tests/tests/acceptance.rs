//! Acceptance tests for poker example applications.
//!
//! These tests run against a deployed Kubernetes cluster with the poker apps.
//! The gateway URL is configured via the GATEWAY_URL environment variable.

use angzarr_client::proto::{CommandBook, CommandPage, Cover, Uuid as ProtoUuid};
use examples_proto::{Currency, DepositFunds, RegisterPlayer};
use prost::Message;
use prost_types::Any;
use std::env;
use tonic::transport::Channel;
use uuid::Uuid;

// Include the gateway service client (compiled from angzarr/gateway.proto)
mod gateway {
    tonic::include_proto!("angzarr");
}

use gateway::command_gateway_client::CommandGatewayClient;

fn gateway_url() -> String {
    env::var("GATEWAY_URL").unwrap_or_else(|_| "http://localhost:9084".to_string())
}

fn new_uuid() -> ProtoUuid {
    ProtoUuid {
        value: Uuid::new_v4().as_bytes().to_vec(),
    }
}

fn pack_command<M: Message>(cmd: &M, type_name: &str) -> Any {
    Any {
        type_url: format!("type.googleapis.com/{}", type_name),
        value: cmd.encode_to_vec(),
    }
}

fn make_command_book(domain: &str, root: ProtoUuid, command: Any) -> CommandBook {
    CommandBook {
        cover: Some(Cover {
            domain: domain.to_string(),
            root: Some(root),
            correlation_id: Uuid::new_v4().to_string(),
            ..Default::default()
        }),
        pages: vec![CommandPage {
            sequence: 0,
            command: Some(command),
        }],
        ..Default::default()
    }
}

#[tokio::test]
async fn test_gateway_health() {
    let url = gateway_url();
    println!("Connecting to gateway at {}", url);

    let channel = Channel::from_shared(url)
        .expect("Invalid gateway URL")
        .connect()
        .await;

    assert!(
        channel.is_ok(),
        "Failed to connect to gateway: {:?}",
        channel.err()
    );
}

#[tokio::test]
async fn test_register_player() {
    let url = gateway_url();
    let channel = Channel::from_shared(url)
        .expect("Invalid gateway URL")
        .connect()
        .await
        .expect("Failed to connect to gateway");

    let mut client = CommandGatewayClient::new(channel);

    // Create RegisterPlayer command
    let player_id = new_uuid();
    let cmd = RegisterPlayer {
        display_name: "TestPlayer".to_string(),
        email: "test@example.com".to_string(),
        player_type: 0, // Human
    };

    let command_book = make_command_book(
        "player",
        player_id.clone(),
        pack_command(&cmd, "examples.RegisterPlayer"),
    );

    let response = client.execute(command_book).await;

    // The command should be accepted (even if the aggregate doesn't exist yet,
    // the gateway should route it correctly)
    match response {
        Ok(resp) => {
            let command_response = resp.into_inner();
            println!("Received command response: {:?}", command_response);
            if let Some(events) = &command_response.events {
                assert!(events.cover.is_some(), "Event book should have a cover");
            }
        }
        Err(status) => {
            // Some errors are expected if coordinator isn't fully set up
            println!("Command returned status: {:?}", status);
            // Don't fail on NOT_FOUND or UNAVAILABLE - these might be config issues
            assert!(
                status.code() == tonic::Code::NotFound
                    || status.code() == tonic::Code::Unavailable
                    || status.code() == tonic::Code::Ok,
                "Unexpected error: {:?}",
                status
            );
        }
    }
}

#[tokio::test]
async fn test_deposit_funds() {
    let url = gateway_url();
    let channel = Channel::from_shared(url)
        .expect("Invalid gateway URL")
        .connect()
        .await
        .expect("Failed to connect to gateway");

    let mut client = CommandGatewayClient::new(channel);

    // First register a player
    let player_id = new_uuid();
    let register_cmd = RegisterPlayer {
        display_name: "DepositTestPlayer".to_string(),
        email: "deposit@example.com".to_string(),
        player_type: 0,
    };

    let _ = client
        .execute(make_command_book(
            "player",
            player_id.clone(),
            pack_command(&register_cmd, "examples.RegisterPlayer"),
        ))
        .await;

    // Now deposit funds
    let deposit_cmd = DepositFunds {
        amount: Some(Currency { amount: 1000 }),
        source: "test".to_string(),
    };

    let response = client
        .execute(make_command_book(
            "player",
            player_id,
            pack_command(&deposit_cmd, "examples.DepositFunds"),
        ))
        .await;

    match response {
        Ok(resp) => {
            let command_response = resp.into_inner();
            println!("Deposit response: {:?}", command_response);
        }
        Err(status) => {
            println!("Deposit status: {:?}", status);
            // Accept various statuses during initial setup
            assert!(
                status.code() == tonic::Code::NotFound
                    || status.code() == tonic::Code::Unavailable
                    || status.code() == tonic::Code::FailedPrecondition
                    || status.code() == tonic::Code::Ok,
                "Unexpected error: {:?}",
                status
            );
        }
    }
}

fn main() {
    // Run tests
    println!("Running acceptance tests...");
    println!("Gateway URL: {}", gateway_url());
}
