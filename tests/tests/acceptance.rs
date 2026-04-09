//! Acceptance tests for poker example applications.
//!
//! These tests run against a deployed Kubernetes cluster with the poker apps.
//! Commands are sent directly to aggregate coordinator sidecars via gRPC.
//! Events are persisted to PostgreSQL and published to RabbitMQ.
//!
//! The key pattern for cross-domain saga coordination is subscribe-then-send:
//! 1. Generate a correlation_id for the command
//! 2. Subscribe to EventStreamService filtered by that correlation_id
//! 3. Send the command with the same correlation_id
//! 4. Wait for expected downstream events on the stream (with timeout)
//! 5. Extract data from received events (e.g., hand root from CardsDealt)
//!
//! Environment variables:
//! - PLAYER_URL: Player aggregate coordinator (default: http://localhost:1310)
//! - TABLE_URL: Table aggregate coordinator (default: http://localhost:1311)
//! - HAND_URL: Hand aggregate coordinator (default: http://localhost:1312)
//! - STREAM_URL: Event stream service (default: http://localhost:1340)

use angzarr_client::proto::{
    command_handler_coordinator_service_client::CommandHandlerCoordinatorServiceClient,
    command_page, event_page, event_stream_service_client::EventStreamServiceClient, page_header,
    CommandBook, CommandPage, CommandRequest, CommandResponse, Cover, EventBook, EventStreamFilter,
    PageHeader, SyncMode, Uuid as ProtoUuid,
};
use examples_proto::{
    CreateTable, Currency, DepositFunds, GameVariant, JoinTable, PlayerType, RegisterPlayer,
    StartHand,
};
use prost::Message;
use prost_types::Any;
use std::env;
use std::time::Duration;
use tokio_stream::StreamExt;
use tonic::transport::Channel;
use uuid::Uuid;

// =============================================================================
// URL helpers
// =============================================================================

fn player_url() -> String {
    env::var("PLAYER_URL").unwrap_or_else(|_| "http://localhost:1310".to_string())
}

fn table_url() -> String {
    env::var("TABLE_URL").unwrap_or_else(|_| "http://localhost:1311".to_string())
}

fn hand_url() -> String {
    env::var("HAND_URL").unwrap_or_else(|_| "http://localhost:1312".to_string())
}

fn stream_url() -> String {
    env::var("STREAM_URL").unwrap_or_else(|_| "http://localhost:1340".to_string())
}

// =============================================================================
// UUID and command helpers
// =============================================================================

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

fn make_command_request_at_seq(
    domain: &str,
    root: ProtoUuid,
    command: Any,
    sequence: u32,
) -> CommandRequest {
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
        cascade_id: None,
    }
}

/// Build a command request with a caller-controlled correlation_id.
/// This is essential for the subscribe-then-send pattern — the test
/// subscribes to the stream with this ID before sending the command.
fn make_command_request_correlated(
    domain: &str,
    root: ProtoUuid,
    command: Any,
    correlation_id: String,
) -> CommandRequest {
    make_command_request_correlated_at_seq(domain, root, command, 0, correlation_id)
}

fn make_command_request_correlated_at_seq(
    domain: &str,
    root: ProtoUuid,
    command: Any,
    sequence: u32,
    correlation_id: String,
) -> CommandRequest {
    CommandRequest {
        command: Some(CommandBook {
            cover: Some(Cover {
                domain: domain.to_string(),
                root: Some(root),
                correlation_id,
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
        cascade_id: None,
    }
}

// =============================================================================
// Domain client helpers
// =============================================================================

async fn connect(
    url: &str,
) -> Result<CommandHandlerCoordinatorServiceClient<Channel>, tonic::Status> {
    let channel = Channel::from_shared(url.to_string())
        .expect("Invalid URL")
        .connect()
        .await
        .map_err(|e| tonic::Status::unavailable(format!("Failed to connect to {}: {}", url, e)))?;
    Ok(CommandHandlerCoordinatorServiceClient::new(channel))
}

async fn get_player_client(
) -> Result<CommandHandlerCoordinatorServiceClient<Channel>, tonic::Status> {
    connect(&player_url()).await
}

async fn get_table_client() -> Result<CommandHandlerCoordinatorServiceClient<Channel>, tonic::Status>
{
    connect(&table_url()).await
}

async fn get_hand_client() -> Result<CommandHandlerCoordinatorServiceClient<Channel>, tonic::Status>
{
    connect(&hand_url()).await
}

async fn send_player_command(request: CommandRequest) -> Result<CommandResponse, tonic::Status> {
    let mut client = get_player_client().await?;
    let response = client.handle_command(request).await?;
    Ok(response.into_inner())
}

async fn send_table_command(request: CommandRequest) -> Result<CommandResponse, tonic::Status> {
    let mut client = get_table_client().await?;
    let response = client.handle_command(request).await?;
    Ok(response.into_inner())
}

async fn send_hand_command(request: CommandRequest) -> Result<CommandResponse, tonic::Status> {
    let mut client = get_hand_client().await?;
    let response = client.handle_command(request).await?;
    Ok(response.into_inner())
}

// =============================================================================
// Event stream subscription helpers
// =============================================================================

/// Subscribe to the event stream for a correlation_id and collect EventBooks
/// until the predicate returns true or timeout expires.
///
/// Returns all collected EventBooks on success.
/// Panics with a descriptive message on timeout or stream error.
async fn wait_for_stream_events<F>(
    correlation_id: &str,
    timeout_secs: u64,
    predicate: F,
) -> Vec<EventBook>
where
    F: Fn(&[EventBook]) -> bool,
{
    let url = stream_url();
    let channel = Channel::from_shared(url.clone())
        .expect("Invalid stream URL")
        .connect()
        .await
        .unwrap_or_else(|e| panic!("Failed to connect to stream at {}: {}", url, e));

    let mut client = EventStreamServiceClient::new(channel);
    let response = client
        .subscribe(EventStreamFilter {
            correlation_id: correlation_id.to_string(),
        })
        .await
        .unwrap_or_else(|e| panic!("Subscribe failed for {}: {}", correlation_id, e));

    let mut stream = response.into_inner();
    let mut collected: Vec<EventBook> = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout_secs);

    loop {
        tokio::select! {
            item = stream.next() => {
                match item {
                    Some(Ok(event_book)) => {
                        let domain = event_book
                            .cover.as_ref()
                            .map(|c| c.domain.as_str())
                            .unwrap_or("?");
                        println!(
                            "Stream event: domain={}, events={}",
                            domain,
                            event_book.pages.len()
                        );
                        collected.push(event_book);
                        if predicate(&collected) {
                            return collected;
                        }
                    }
                    Some(Err(e)) => {
                        panic!(
                            "Stream error for {}: {}. Collected {} event(s) before error.",
                            correlation_id, e, collected.len()
                        );
                    }
                    None => {
                        panic!(
                            "Stream closed for {} with {} event(s) collected before predicate was satisfied.",
                            correlation_id, collected.len()
                        );
                    }
                }
            }
            _ = tokio::time::sleep_until(deadline) => {
                panic!(
                    "Timeout after {}s waiting for events on correlation {}. Collected {} event(s).",
                    timeout_secs, correlation_id, collected.len()
                );
            }
        }
    }
}

/// Find the first event matching the given type URL suffix in collected EventBooks.
fn find_event_in_books(books: &[EventBook], type_suffix: &str) -> Option<Any> {
    for book in books {
        for page in &book.pages {
            if let Some(event_page::Payload::Event(any)) = &page.payload {
                if any.type_url.ends_with(type_suffix) {
                    return Some(any.clone());
                }
            }
        }
    }
    None
}

/// Check if collected books contain an event with the given type URL suffix.
fn has_event(books: &[EventBook], type_suffix: &str) -> bool {
    find_event_in_books(books, type_suffix).is_some()
}

// =============================================================================
// Reusable setup helpers
// =============================================================================

async fn register_player(id: &ProtoUuid, name: &str) {
    let id_hex = hex::encode(&id.value);
    let cmd = RegisterPlayer {
        display_name: name.to_string(),
        email: format!("{}-{}@test.com", name.to_lowercase(), &id_hex[..8]),
        player_type: PlayerType::Human as i32,
        ai_model_id: String::new(),
    };
    send_player_command(make_command_request(
        "player",
        id.clone(),
        pack_command(&cmd, "examples.RegisterPlayer"),
    ))
    .await
    .unwrap_or_else(|e| panic!("RegisterPlayer for {} failed: {:?}", name, e));
}

async fn deposit_funds_with_retry(id: &ProtoUuid, amount: i64, seq: u32) {
    let cmd = DepositFunds {
        amount: Some(Currency {
            amount,
            currency_code: "USD".to_string(),
        }),
    };
    let max_attempts = 30;
    let mut last_err = None;
    for attempt in 1..=max_attempts {
        let request = make_command_request_at_seq(
            "player",
            id.clone(),
            pack_command(&cmd, "examples.DepositFunds"),
            seq,
        );
        match send_player_command(request).await {
            Ok(_) => {
                last_err = None;
                break;
            }
            Err(e) => {
                last_err = Some(e);
                tokio::time::sleep(Duration::from_millis(200 * attempt as u64)).await;
            }
        }
    }
    if let Some(e) = last_err {
        panic!(
            "DepositFunds failed after {} attempts: {:?}",
            max_attempts, e
        );
    }
}

// =============================================================================
// Tests
// =============================================================================

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
            println!(
                "Successfully registered player, got {} event(s)",
                events.pages.len()
            );
        }
        Err(status) => {
            panic!("RegisterPlayer failed: {:?}", status);
        }
    }
}

#[tokio::test]
async fn test_register_and_deposit() {
    let player_id = new_uuid();
    let player_id_hex = hex::encode(&player_id.value);
    println!("Test: Register and deposit for player {}", player_id_hex);

    register_player(&player_id, "DepositTestPlayer").await;
    println!("Player registered successfully");

    deposit_funds_with_retry(&player_id, 1000, 1).await;
    println!("Successfully deposited funds");
}

#[tokio::test]
async fn test_duplicate_registration_fails() {
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
            println!(
                "Duplicate registration correctly rejected: {:?}",
                status.code()
            );
            assert!(
                status.code() == tonic::Code::AlreadyExists
                    || status.code() == tonic::Code::FailedPrecondition,
                "Expected AlreadyExists or FailedPrecondition, got {:?}",
                status.code()
            );
        }
    }
}

/// Full hand lifecycle test using the subscribe-then-send pattern.
///
/// This test exercises cross-domain saga coordination:
/// 1. Register 2 players and deposit funds (player domain)
/// 2. Create a table and seat both players (table domain)
/// 3. Start a hand — subscribing to the event stream first (table → hand via saga)
/// 4. Wait for CardsDealt event on the stream (proves saga completed)
/// 5. Extract the hand root from the stream events
#[tokio::test]
async fn test_hand_lifecycle() {
    println!("=== Hand lifecycle test ===");

    // --- Setup: Register 2 players and deposit funds ---
    let player1_id = new_uuid();
    let player2_id = new_uuid();
    println!(
        "Player1: {}, Player2: {}",
        hex::encode(&player1_id.value),
        hex::encode(&player2_id.value)
    );

    register_player(&player1_id, "LifecyclePlayer1").await;
    register_player(&player2_id, "LifecyclePlayer2").await;
    println!("Players registered");

    deposit_funds_with_retry(&player1_id, 10000, 1).await;
    deposit_funds_with_retry(&player2_id, 10000, 1).await;
    println!("Funds deposited");

    // --- Create table ---
    let table_id = new_uuid();
    let create_cmd = CreateTable {
        table_name: format!("lifecycle-{}", &hex::encode(&table_id.value)[..8]),
        game_variant: GameVariant::TexasHoldem as i32,
        small_blind: 10,
        big_blind: 20,
        min_buy_in: 200,
        max_buy_in: 2000,
        max_players: 6,
        action_timeout_seconds: 30,
    };
    let create_resp = send_table_command(make_command_request(
        "table",
        table_id.clone(),
        pack_command(&create_cmd, "examples.CreateTable"),
    ))
    .await
    .expect("CreateTable should succeed");
    assert!(
        create_resp.events.is_some(),
        "CreateTable should produce events"
    );
    println!("Table created");

    // --- Join both players ---
    let join1_cmd = JoinTable {
        player_root: player1_id.value.clone(),
        preferred_seat: 0,
        buy_in_amount: 1000,
    };
    send_table_command(make_command_request_at_seq(
        "table",
        table_id.clone(),
        pack_command(&join1_cmd, "examples.JoinTable"),
        1,
    ))
    .await
    .expect("JoinTable player1 should succeed");

    let join2_cmd = JoinTable {
        player_root: player2_id.value.clone(),
        preferred_seat: 1,
        buy_in_amount: 1000,
    };
    send_table_command(make_command_request_at_seq(
        "table",
        table_id.clone(),
        pack_command(&join2_cmd, "examples.JoinTable"),
        2,
    ))
    .await
    .expect("JoinTable player2 should succeed");
    println!("Players joined table");

    // --- Start hand with stream subscription ---
    let correlation_id = Uuid::new_v4().to_string();
    println!("Starting hand with correlation_id: {}", correlation_id);

    // Subscribe to the stream BEFORE sending the command.
    // Spawn the listener so it runs concurrently with the command send.
    let stream_handle = {
        let cid = correlation_id.clone();
        tokio::spawn(async move {
            wait_for_stream_events(&cid, 15, |books| {
                // Wait until we see CardsDealt (hand aggregate, created by saga)
                has_event(books, "CardsDealt")
            })
            .await
        })
    };

    // Brief pause to ensure subscription is registered before the command
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Send StartHand with the same correlation_id
    let start_cmd = StartHand {};
    let start_resp = send_table_command(make_command_request_correlated_at_seq(
        "table",
        table_id.clone(),
        pack_command(&start_cmd, "examples.StartHand"),
        3, // After CreateTable(0), JoinTable1(1), JoinTable2(2)
        correlation_id.clone(),
    ))
    .await
    .expect("StartHand should succeed");

    assert!(
        start_resp.events.is_some(),
        "StartHand should produce events"
    );
    println!("StartHand succeeded, waiting for saga events on stream...");

    // Wait for the stream to receive CardsDealt (proves saga completed)
    let stream_events = stream_handle.await.expect("Stream task should not panic");

    // Verify we got the expected events
    assert!(
        has_event(&stream_events, "HandStarted") || has_event(&stream_events, "CardsDealt"),
        "Stream should contain HandStarted and/or CardsDealt events"
    );
    assert!(
        has_event(&stream_events, "CardsDealt"),
        "Stream must contain CardsDealt (saga completed)"
    );

    // Extract hand domain info from the CardsDealt event
    let cards_dealt_book = stream_events
        .iter()
        .find(|b| {
            b.pages.iter().any(|p| {
                matches!(&p.payload, Some(event_page::Payload::Event(any)) if any.type_url.ends_with("CardsDealt"))
            })
        })
        .expect("Should find EventBook containing CardsDealt");

    let hand_domain = cards_dealt_book
        .cover
        .as_ref()
        .map(|c| c.domain.as_str())
        .unwrap_or("unknown");
    let hand_root = cards_dealt_book
        .cover
        .as_ref()
        .and_then(|c| c.root.as_ref())
        .map(|r| hex::encode(&r.value))
        .unwrap_or_else(|| "unknown".to_string());

    println!(
        "Saga completed: CardsDealt received from domain={}, hand_root={}",
        hand_domain, hand_root
    );
    println!(
        "Total stream events collected: {} EventBook(s)",
        stream_events.len()
    );
    println!("=== Hand lifecycle test passed ===");
}
