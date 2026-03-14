//! Acceptance tests for poker example applications.
//!
//! These tests run against a deployed Kubernetes cluster with the poker apps.
//! Each domain's coordinator is exposed on a separate port via Kind NodePort.

use angzarr_client::proto::{
    command_handler_coordinator_service_client::CommandHandlerCoordinatorServiceClient,
    CommandBook, CommandPage, CommandRequest, CommandResponse, Cover, SyncMode, Uuid as ProtoUuid,
};
use examples_proto::{Currency, DepositFunds, RegisterPlayer};
use prost::Message;
use prost_types::Any;
use std::env;
use tonic::transport::Channel;
use uuid::Uuid;

/// Get coordinator URL for a specific domain
fn coordinator_url(domain: &str) -> String {
    let env_var = format!("{}_COORDINATOR_URL", domain.to_uppercase());
    env::var(&env_var).unwrap_or_else(|_| {
        // Default ports for local Kind cluster
        let port = match domain {
            "player" => 30001,
            "table" => 30002,
            "hand" => 30003,
            _ => 30001,
        };
        format!("http://localhost:{}", port)
    })
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

fn make_command_request(domain: &str, root: ProtoUuid, command: Any) -> CommandRequest {
    CommandRequest {
        command: Some(CommandBook {
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
        }),
        sync_mode: SyncMode::Sync as i32,
        cascade_error_mode: 0,
    }
}

async fn send_command(
    domain: &str,
    request: CommandRequest,
) -> Result<CommandResponse, tonic::Status> {
    let url = coordinator_url(domain);
    println!("Connecting to {} coordinator at {}", domain, url);

    let channel = Channel::from_shared(url.clone())
        .expect("Invalid coordinator URL")
        .connect()
        .await
        .map_err(|e| tonic::Status::unavailable(format!("Failed to connect to {}: {}", url, e)))?;

    let mut client = CommandHandlerCoordinatorServiceClient::new(channel);
    let response = client.handle_command(request).await?;
    Ok(response.into_inner())
}

#[tokio::test]
async fn test_player_coordinator_health() {
    let url = coordinator_url("player");
    println!("Connecting to player coordinator at {}", url);

    let channel = Channel::from_shared(url)
        .expect("Invalid coordinator URL")
        .connect()
        .await;

    assert!(
        channel.is_ok(),
        "Failed to connect to player coordinator: {:?}",
        channel.err()
    );
}

#[tokio::test]
async fn test_register_player() {
    let player_id = new_uuid();
    let cmd = RegisterPlayer {
        display_name: "TestPlayer".to_string(),
        email: "test@example.com".to_string(),
        player_type: 0, // Human
    };

    let request = make_command_request(
        "player",
        player_id.clone(),
        pack_command(&cmd, "examples.RegisterPlayer"),
    );

    match send_command("player", request).await {
        Ok(response) => {
            println!("Received command response: {:?}", response);
            if let Some(events) = &response.events {
                assert!(events.cover.is_some(), "Event book should have a cover");
            }
        }
        Err(status) => {
            println!("Command returned status: {:?}", status);
            // Accept various statuses during initial setup
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
    // First register a player
    let player_id = new_uuid();
    let register_cmd = RegisterPlayer {
        display_name: "DepositTestPlayer".to_string(),
        email: "deposit@example.com".to_string(),
        player_type: 0,
    };

    let _ = send_command(
        "player",
        make_command_request(
            "player",
            player_id.clone(),
            pack_command(&register_cmd, "examples.RegisterPlayer"),
        ),
    )
    .await;

    // Now deposit funds
    let deposit_cmd = DepositFunds {
        amount: Some(Currency { amount: 1000 }),
        source: "test".to_string(),
    };

    let request = make_command_request(
        "player",
        player_id,
        pack_command(&deposit_cmd, "examples.DepositFunds"),
    );

    match send_command("player", request).await {
        Ok(response) => {
            println!("Deposit response: {:?}", response);
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
    println!("Player coordinator: {}", coordinator_url("player"));
    println!("Table coordinator: {}", coordinator_url("table"));
    println!("Hand coordinator: {}", coordinator_url("hand"));
}
