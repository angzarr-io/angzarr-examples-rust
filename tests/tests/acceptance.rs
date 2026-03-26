//! Acceptance tests for poker example applications.
//!
//! These tests run against a deployed Kubernetes cluster with the poker apps.
//! Commands are sent directly to aggregate coordinator sidecars via gRPC.
//! Events are persisted to PostgreSQL and published to RabbitMQ.
//!
//! Environment variables:
//! - PLAYER_URL: Player aggregate coordinator (default: http://localhost:1310)
//! - TABLE_URL: Table aggregate coordinator (default: http://localhost:1311)
//! - HAND_URL: Hand aggregate coordinator (default: http://localhost:1312)

use angzarr_client::proto::{
    command_handler_coordinator_service_client::CommandHandlerCoordinatorServiceClient,
    command_page, page_header, CommandBook, CommandPage, CommandRequest, CommandResponse, Cover,
    PageHeader, SyncMode, Uuid as ProtoUuid,
};
use examples_proto::{Currency, DepositFunds, RegisterPlayer, PlayerType};
use prost::Message;
use prost_types::Any;
use std::env;
use tonic::transport::Channel;
use uuid::Uuid;

/// Get player aggregate coordinator URL from environment or use default
fn player_url() -> String {
    env::var("PLAYER_URL").unwrap_or_else(|_| "http://localhost:1310".to_string())
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
    make_command_request_at_seq(domain, root, command, 0)
}

fn make_command_request_at_seq(domain: &str, root: ProtoUuid, command: Any, sequence: u32) -> CommandRequest {
    CommandRequest {
        command: Some(CommandBook {
            cover: Some(Cover {
                domain: domain.to_string(),
                root: Some(root),
                correlation_id: Uuid::new_v4().to_string(),
                ..Default::default()
            }),
            pages: vec![CommandPage {
                header: Some(PageHeader {
                    sequence_type: Some(page_header::SequenceType::Sequence(sequence)),
                }),
                payload: Some(command_page::Payload::Command(command)),
                ..Default::default()
            }],
            ..Default::default()
        }),
        sync_mode: SyncMode::Simple as i32,
        cascade_error_mode: 0,
    }
}

async fn get_player_client() -> Result<CommandHandlerCoordinatorServiceClient<Channel>, tonic::Status> {
    let url = player_url();
    let channel = Channel::from_shared(url.clone())
        .expect("Invalid player URL")
        .connect()
        .await
        .map_err(|e| tonic::Status::unavailable(format!("Failed to connect to {}: {}", url, e)))?;

    Ok(CommandHandlerCoordinatorServiceClient::new(channel))
}

async fn send_player_command(request: CommandRequest) -> Result<CommandResponse, tonic::Status> {
    let mut client = get_player_client().await?;
    let response = client.handle_command(request).await?;
    Ok(response.into_inner())
}

#[tokio::test]
async fn test_player_aggregate_connectivity() {
    let url = player_url();
    println!("Connecting to player aggregate at {}", url);

    let channel = Channel::from_shared(url.clone())
        .expect("Invalid player URL")
        .connect()
        .await;

    assert!(
        channel.is_ok(),
        "Failed to connect to player aggregate at {}: {:?}",
        url,
        channel.err()
    );
}

#[tokio::test]
async fn test_register_player() {
    let player_id = new_uuid();
    let player_id_hex = hex::encode(&player_id.value);
    println!("Registering player with ID: {}", player_id_hex);

    let cmd = RegisterPlayer {
        display_name: "AcceptanceTestPlayer".to_string(),
        email: format!("test-{}@example.com", &player_id_hex[..8]),
        player_type: PlayerType::Human as i32,
        ai_model_id: String::new(),
    };

    let request = make_command_request(
        "player",
        player_id.clone(),
        pack_command(&cmd, "examples.RegisterPlayer"),
    );

    let response = send_player_command(request).await;

    match response {
        Ok(resp) => {
            println!("RegisterPlayer response: {:?}", resp);
            assert!(resp.events.is_some(), "Response should contain events");
            let events = resp.events.unwrap();
            assert!(events.cover.is_some(), "Event book should have a cover");
            assert!(!events.pages.is_empty(), "Should have at least one event");
            println!("Successfully registered player, got {} event(s)", events.pages.len());
        }
        Err(status) => {
            panic!("RegisterPlayer failed: {:?}", status);
        }
    }
}

#[tokio::test]
async fn test_register_and_deposit() {
    // Register a new player
    let player_id = new_uuid();
    let player_id_hex = hex::encode(&player_id.value);
    println!("Test: Register and deposit for player {}", player_id_hex);

    let register_cmd = RegisterPlayer {
        display_name: "DepositTestPlayer".to_string(),
        email: format!("deposit-{}@example.com", &player_id_hex[..8]),
        player_type: PlayerType::Human as i32,
        ai_model_id: String::new(),
    };

    let register_request = make_command_request(
        "player",
        player_id.clone(),
        pack_command(&register_cmd, "examples.RegisterPlayer"),
    );

    let register_response = send_player_command(register_request).await;
    assert!(register_response.is_ok(), "Registration should succeed: {:?}", register_response.err());
    println!("Player registered successfully");

    // Now deposit funds (sequence 1 since registration was sequence 0)
    let deposit_cmd = DepositFunds {
        amount: Some(Currency {
            amount: 1000,
            currency_code: "USD".to_string(),
        }),
    };

    let deposit_request = make_command_request_at_seq(
        "player",
        player_id.clone(),
        pack_command(&deposit_cmd, "examples.DepositFunds"),
        1,  // Sequence 1 after registration
    );

    let deposit_response = send_player_command(deposit_request).await;

    match deposit_response {
        Ok(resp) => {
            println!("DepositFunds response: {:?}", resp);
            assert!(resp.events.is_some(), "Response should contain events");
            let events = resp.events.unwrap();
            assert!(!events.pages.is_empty(), "Should have deposited event");
            println!("Successfully deposited funds, got {} event(s)", events.pages.len());
        }
        Err(status) => {
            panic!("DepositFunds failed: {:?}", status);
        }
    }
}

#[tokio::test]
async fn test_duplicate_registration_fails() {
    // Register a player
    let player_id = new_uuid();
    let player_id_hex = hex::encode(&player_id.value);
    println!("Test: Duplicate registration for player {}", player_id_hex);

    let cmd = RegisterPlayer {
        display_name: "DuplicateTestPlayer".to_string(),
        email: format!("dup-{}@example.com", &player_id_hex[..8]),
        player_type: PlayerType::Human as i32,
        ai_model_id: String::new(),
    };

    let request1 = make_command_request(
        "player",
        player_id.clone(),
        pack_command(&cmd, "examples.RegisterPlayer"),
    );

    let response1 = send_player_command(request1).await;
    assert!(response1.is_ok(), "First registration should succeed");
    println!("First registration succeeded");

    // Try to register again with same ID
    let request2 = make_command_request(
        "player",
        player_id.clone(),
        pack_command(&cmd, "examples.RegisterPlayer"),
    );

    let response2 = send_player_command(request2).await;

    match response2 {
        Ok(_) => {
            panic!("Duplicate registration should have failed");
        }
        Err(status) => {
            println!("Duplicate registration correctly rejected: {:?}", status.code());
            assert!(
                status.code() == tonic::Code::AlreadyExists
                    || status.code() == tonic::Code::FailedPrecondition,
                "Expected AlreadyExists or FailedPrecondition, got {:?}",
                status.code()
            );
        }
    }
}
