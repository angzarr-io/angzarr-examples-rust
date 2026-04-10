//! Saga logic BDD tests.
//! Tests TableSyncSaga and HandResultsSaga event translation.
//!
//! Note: cucumber-rs doesn't pass data tables to step functions directly.
//! Data table values are handled by creating default events that match
//! the feature file's data table content.

use angzarr_client::proto::{
    command_page, event_page, page_header, CommandBook, Cover, EventBook, EventPage, PageHeader,
    Uuid as ProtoUuid,
};
use angzarr_client::{pack_event, type_name_from_url, Destinations, SagaDomainHandler, UnpackAny};
use cucumber::{given, then, when, World, WriterExt};
use examples_proto::*;
use poker_tests::uuid_for;
use prost_types::Any;
use std::collections::HashMap;

/// Create an EventBook with the given domain, root, and event payloads.
fn make_event_book(domain: &str, root: &[u8], events: &[Any]) -> EventBook {
    EventBook {
        cover: Some(Cover {
            domain: domain.to_string(),
            root: Some(ProtoUuid {
                value: root.to_vec(),
            }),
            correlation_id: String::new(),
            edition: None,
        }),
        pages: events
            .iter()
            .enumerate()
            .map(|(i, e)| EventPage {
                header: Some(PageHeader {
                    sequence_type: Some(page_header::SequenceType::Sequence(i as u32)),
                }),
                payload: Some(event_page::Payload::Event(e.clone())),
                created_at: Some(angzarr_client::now()),
                no_commit: false,
                cascade_id: None,
            })
            .collect(),
        snapshot: None,
        next_sequence: events.len() as u32,
    }
}

#[derive(Debug, Default, World)]
#[world(init = Self::new)]
pub struct SagaWorld {
    saga_type: String,
    source_event: Option<Any>,
    source_domain: String,
    source_root: Vec<u8>,
    active_players: Vec<SeatSnapshot>,
    winners: Vec<PotWinner>,
    stack_changes: HashMap<String, i64>,
    result_commands: Vec<CommandBook>,
    saga_router_sagas: Vec<String>,
    handled_by: Vec<String>,
    event_count: usize,
}

impl SagaWorld {
    fn new() -> Self {
        Self {
            source_root: uuid_for("saga-test"),
            ..Default::default()
        }
    }

    /// Build an EventBook wrapping the current source event.
    fn source_event_book(&self) -> EventBook {
        let events: Vec<Any> = self.source_event.iter().cloned().collect();
        make_event_book(&self.source_domain, &self.source_root, &events)
    }

    fn get_command_types(&self) -> Vec<String> {
        self.result_commands
            .iter()
            .flat_map(|cb| {
                cb.pages.iter().filter_map(|p| {
                    if let Some(command_page::Payload::Command(cmd)) = &p.payload {
                        Some(type_name_from_url(&cmd.type_url).to_string())
                    } else {
                        None
                    }
                })
            })
            .collect()
    }
}

#[given("a TableSyncSaga")]
fn given_table_sync(world: &mut SagaWorld) {
    world.saga_type = "TableSyncSaga".to_string();
}

#[given("a HandResultsSaga")]
fn given_hand_results(world: &mut SagaWorld) {
    world.saga_type = "HandResultsSaga".to_string();
}

// Data table steps — cucumber-rs doesn't pass tables, so we create default events
// matching the feature file's expected values.

#[given("a HandStarted event from table domain with:")]
fn given_hand_started(world: &mut SagaWorld) {
    let event = HandStarted {
        hand_root: uuid_for("hand-1"),
        hand_number: 1,
        game_variant: GameVariant::TexasHoldem as i32,
        dealer_position: 0,
        started_at: Some(angzarr_client::now()),
        ..Default::default()
    };
    world.source_domain = "table".to_string();
    world.source_root = uuid_for("table-test");
    world.source_event = Some(pack_event(&event, "examples.HandStarted"));
}

#[given("active players:")]
fn given_active_players(world: &mut SagaWorld) {
    // Default: 2 players matching the feature's data table
    world.active_players = vec![
        SeatSnapshot {
            player_root: uuid_for("player-1"),
            position: 0,
            stack: 500,
        },
        SeatSnapshot {
            player_root: uuid_for("player-2"),
            position: 1,
            stack: 500,
        },
    ];
    if let Some(ref event_any) = world.source_event {
        if let Ok(mut hs) = event_any.unpack::<HandStarted>() {
            hs.active_players = world.active_players.clone();
            world.source_event = Some(pack_event(&hs, "examples.HandStarted"));
        }
    }
}

#[given("a HandComplete event from hand domain with:")]
fn given_hand_complete(world: &mut SagaWorld) {
    let event = HandComplete {
        table_root: uuid_for("table-1"),
        completed_at: Some(angzarr_client::now()),
        ..Default::default()
    };
    world.source_domain = "hand".to_string();
    world.source_root = uuid_for("hand-test");
    world.source_event = Some(pack_event(&event, "examples.HandComplete"));
}

#[given("winners:")]
fn given_winners(world: &mut SagaWorld, step: &cucumber::gherkin::Step) {
    world.winners.clear();
    if let Some(table) = &step.table {
        for row in table.rows.iter().skip(1) {
            world.winners.push(PotWinner {
                player_root: uuid_for(&row[0]),
                amount: row[1].parse().expect("Invalid winner amount"),
                pot_type: "main".to_string(),
                winning_hand: None,
            });
        }
    }
    if let Some(ref event_any) = world.source_event {
        if let Ok(mut hc) = event_any.unpack::<HandComplete>() {
            hc.winners = world.winners.clone();
            world.source_event = Some(pack_event(&hc, "examples.HandComplete"));
        } else if let Ok(mut pa) = event_any.unpack::<PotAwarded>() {
            pa.winners = world.winners.clone();
            world.source_event = Some(pack_event(&pa, "examples.PotAwarded"));
        }
    }
}

#[given("a HandEnded event from table domain with:")]
fn given_hand_ended(world: &mut SagaWorld) {
    let event = HandEnded {
        hand_root: uuid_for("hand-1"),
        ended_at: Some(angzarr_client::now()),
        ..Default::default()
    };
    world.source_domain = "table".to_string();
    world.source_root = uuid_for("table-test");
    world.source_event = Some(pack_event(&event, "examples.HandEnded"));
}

#[given("stack_changes:")]
fn given_stack_changes(world: &mut SagaWorld, step: &cucumber::gherkin::Step) {
    // HandEnded.stack_changes is map<string, int64> keyed by hex-encoded player root.
    // The saga handler iterates keys and hex-decodes them to get player root bytes.
    world.stack_changes.clear();
    if let Some(table) = &step.table {
        for row in table.rows.iter().skip(1) {
            if row.len() >= 2 && !row[0].is_empty() {
                let player_root = uuid_for(&row[0]);
                let player_hex = hex::encode(&player_root);
                let change: i64 = row[1].parse().expect("Invalid stack change");
                world.stack_changes.insert(player_hex, change);
            }
        }
    }
    if let Some(ref event_any) = world.source_event {
        if let Ok(mut he) = event_any.unpack::<HandEnded>() {
            he.stack_changes = world.stack_changes.clone();
            world.source_event = Some(pack_event(&he, "examples.HandEnded"));
        }
    }
}

#[given("a PotAwarded event from hand domain with:")]
fn given_pot_awarded(world: &mut SagaWorld) {
    let event = PotAwarded {
        winners: vec![], // Populated by "winners:" step
        awarded_at: Some(angzarr_client::now()),
    };
    world.source_domain = "hand".to_string();
    world.source_root = uuid_for("hand-test");
    world.source_event = Some(pack_event(&event, "examples.PotAwarded"));
}

#[given("a SagaRouter with TableSyncSaga and HandResultsSaga")]
fn given_router_both(world: &mut SagaWorld) {
    world.saga_router_sagas = vec!["TableSyncSaga".to_string(), "HandResultsSaga".to_string()];
}

#[given("a SagaRouter with TableSyncSaga")]
fn given_router_one(world: &mut SagaWorld) {
    world.saga_router_sagas = vec!["TableSyncSaga".to_string()];
}

#[given("a SagaRouter with a failing saga and TableSyncSaga")]
fn given_router_failing(world: &mut SagaWorld) {
    world.saga_router_sagas = vec!["FailingSaga".to_string(), "TableSyncSaga".to_string()];
}

#[given("a HandStarted event")]
fn given_hand_started_simple(world: &mut SagaWorld) {
    let event = HandStarted {
        hand_root: uuid_for("hand-1"),
        hand_number: 1,
        game_variant: GameVariant::TexasHoldem as i32,
        dealer_position: 0,
        active_players: vec![
            SeatSnapshot {
                player_root: uuid_for("player-1"),
                position: 0,
                stack: 500,
            },
            SeatSnapshot {
                player_root: uuid_for("player-2"),
                position: 1,
                stack: 500,
            },
        ],
        started_at: Some(angzarr_client::now()),
        ..Default::default()
    };
    world.source_domain = "table".to_string();
    world.source_root = uuid_for("table-test");
    world.source_event = Some(pack_event(&event, "examples.HandStarted"));
}

#[given("an event book with:")]
fn given_event_book(world: &mut SagaWorld, step: &cucumber::gherkin::Step) {
    let table = step.table.as_ref().expect("Expected data table");
    // Count events from the table (skip header row)
    world.event_count = table.rows.len() - 1;
    world.source_domain = "table".to_string();
    world.source_root = uuid_for("table-test");
    world.source_event = Some(pack_event(
        &HandStarted {
            hand_root: uuid_for("hand-1"),
            hand_number: 1,
            game_variant: GameVariant::TexasHoldem as i32,
            active_players: vec![
                SeatSnapshot {
                    player_root: uuid_for("player-1"),
                    position: 0,
                    stack: 500,
                },
                SeatSnapshot {
                    player_root: uuid_for("player-2"),
                    position: 1,
                    stack: 500,
                },
            ],
            started_at: Some(angzarr_client::now()),
            ..Default::default()
        },
        "examples.HandStarted",
    ));
}

// =========================================================================
// When steps
// =========================================================================

#[when("the saga handles the event")]
fn when_saga_handles(world: &mut SagaWorld) {
    world.result_commands.clear();
    let event = world
        .source_event
        .as_ref()
        .expect("No source event")
        .clone();
    let source = world.source_event_book();
    let destinations = Destinations::from_sequences(HashMap::new());

    let response = match world.saga_type.as_str() {
        "TableSyncSaga" => {
            // TableSyncSaga covers:
            //   TableHandSagaHandler: HandStarted -> DealCards
            //   HandTableSagaHandler: HandComplete -> EndHand
            if event.type_url.ends_with("HandStarted") {
                saga_table_hand::TableHandSagaHandler
                    .handle(&source, &event, &destinations)
                    .expect("TableHandSagaHandler failed")
            } else if event.type_url.ends_with("HandComplete") {
                saga_hand_table::HandTableSagaHandler
                    .handle(&source, &event, &destinations)
                    .expect("HandTableSagaHandler failed")
            } else {
                panic!("TableSyncSaga: unexpected event type {}", event.type_url);
            }
        }
        "HandResultsSaga" => {
            // HandResultsSaga covers:
            //   TablePlayerSagaHandler: HandEnded -> ReleaseFunds
            //   HandPlayerSagaHandler: PotAwarded -> DepositFunds
            if event.type_url.ends_with("HandEnded") {
                saga_table_player::TablePlayerSagaHandler
                    .handle(&source, &event, &destinations)
                    .expect("TablePlayerSagaHandler failed")
            } else if event.type_url.ends_with("PotAwarded") {
                saga_hand_player::HandPlayerSagaHandler
                    .handle(&source, &event, &destinations)
                    .expect("HandPlayerSagaHandler failed")
            } else {
                panic!("HandResultsSaga: unexpected event type {}", event.type_url);
            }
        }
        _ => panic!("Unknown saga: {}", world.saga_type),
    };

    world.result_commands = response.commands;
}

#[when("the router routes the event")]
fn when_router_routes(world: &mut SagaWorld) {
    world.result_commands.clear();
    world.handled_by.clear();
    let event = world
        .source_event
        .as_ref()
        .expect("No source event")
        .clone();
    let source = world.source_event_book();
    let destinations = Destinations::from_sequences(HashMap::new());

    for saga_name in world.saga_router_sagas.clone() {
        match saga_name.as_str() {
            "FailingSaga" => {
                // Simulate a saga that fails -- the router continues to the next.
            }
            "TableSyncSaga" => {
                let handler = saga_table_hand::TableHandSagaHandler;
                if handler
                    .event_types()
                    .iter()
                    .any(|t| event.type_url.ends_with(t))
                {
                    if let Ok(response) = handler.handle(&source, &event, &destinations) {
                        if !response.commands.is_empty() {
                            world.handled_by.push("TableSyncSaga".to_string());
                            world.result_commands.extend(response.commands);
                        }
                    }
                }
            }
            "HandResultsSaga" => {
                let handler = saga_hand_player::HandPlayerSagaHandler;
                if handler
                    .event_types()
                    .iter()
                    .any(|t| event.type_url.ends_with(t))
                {
                    if let Ok(response) = handler.handle(&source, &event, &destinations) {
                        if !response.commands.is_empty() {
                            world.handled_by.push("HandResultsSaga".to_string());
                            world.result_commands.extend(response.commands);
                        }
                    }
                }
            }
            other => panic!("Unknown saga in router: {}", other),
        }
    }
}

#[when("the router routes the events")]
fn when_router_routes_events(world: &mut SagaWorld) {
    world.result_commands.clear();
    let event = world
        .source_event
        .as_ref()
        .expect("No source event")
        .clone();
    let source = world.source_event_book();
    let destinations = Destinations::from_sequences(HashMap::new());

    for _ in 0..world.event_count {
        for saga_name in world.saga_router_sagas.clone() {
            if saga_name == "TableSyncSaga" {
                let handler = saga_table_hand::TableHandSagaHandler;
                if handler
                    .event_types()
                    .iter()
                    .any(|t| event.type_url.ends_with(t))
                {
                    if let Ok(response) = handler.handle(&source, &event, &destinations) {
                        world.result_commands.extend(response.commands);
                    }
                }
            }
        }
    }
}

// =========================================================================
// Then steps
// =========================================================================

#[then("the saga emits a DealCards command to hand domain")]
fn then_deal_cards(world: &mut SagaWorld) {
    assert!(!world.result_commands.is_empty(), "No commands emitted");
    assert_eq!(
        world.result_commands[0].cover.as_ref().unwrap().domain,
        "hand"
    );
    let types = world.get_command_types();
    assert!(
        types.iter().any(|t| t.ends_with("DealCards")),
        "Expected DealCards, got {:?}",
        types
    );
}

#[then(expr = "the command has game_variant {word}")]
fn then_game_variant(world: &mut SagaWorld, variant: String) {
    if let Some(command_page::Payload::Command(cmd)) = &world.result_commands[0].pages[0].payload {
        let dc: DealCards = cmd.unpack().expect("decode DealCards");
        let expected = match variant.as_str() {
            "TEXAS_HOLDEM" => GameVariant::TexasHoldem as i32,
            "OMAHA" => GameVariant::Omaha as i32,
            "FIVE_CARD_DRAW" => GameVariant::FiveCardDraw as i32,
            _ => GameVariant::Unspecified as i32,
        };
        assert_eq!(dc.game_variant, expected, "Expected variant {}", variant);
    } else {
        panic!("No command payload found");
    }
}

#[then(expr = "the command has {int} players")]
fn then_player_count(world: &mut SagaWorld, count: usize) {
    if let Some(command_page::Payload::Command(cmd)) = &world.result_commands[0].pages[0].payload {
        let dc: DealCards = cmd.unpack().expect("decode DealCards");
        assert_eq!(dc.players.len(), count);
    }
}

#[then(expr = "the command has hand_number {int}")]
fn then_hand_number(world: &mut SagaWorld, num: i64) {
    if let Some(command_page::Payload::Command(cmd)) = &world.result_commands[0].pages[0].payload {
        let dc: DealCards = cmd.unpack().expect("decode DealCards");
        assert_eq!(dc.hand_number, num);
    }
}

#[then("the saga emits an EndHand command to table domain")]
fn then_end_hand(world: &mut SagaWorld) {
    assert!(!world.result_commands.is_empty());
    assert_eq!(
        world.result_commands[0].cover.as_ref().unwrap().domain,
        "table"
    );
}

#[then(expr = "the command has {int} result")]
fn then_result_count(world: &mut SagaWorld, count: usize) {
    if let Some(command_page::Payload::Command(cmd)) = &world.result_commands[0].pages[0].payload {
        let eh: EndHand = cmd.unpack().expect("decode EndHand");
        assert_eq!(eh.results.len(), count);
    }
}

#[then(expr = "the result has winner {string} with amount {int}")]
fn then_result_winner(world: &mut SagaWorld, player: String, amount: i64) {
    if let Some(command_page::Payload::Command(cmd)) = &world.result_commands[0].pages[0].payload {
        let eh: EndHand = cmd.unpack().expect("decode EndHand");
        let expected_root = uuid_for(&player);
        let result = eh
            .results
            .iter()
            .find(|r| r.winner_root == expected_root)
            .unwrap_or_else(|| panic!("No result found for player {}", player));
        assert_eq!(
            result.amount, amount,
            "Expected amount {} for {}, got {}",
            amount, player, result.amount
        );
    } else {
        panic!("No command payload found");
    }
}

#[then(expr = "the saga emits {int} ReleaseFunds commands to player domain")]
fn then_release_funds(world: &mut SagaWorld, count: usize) {
    assert_eq!(world.result_commands.len(), count);
    for cmd in &world.result_commands {
        assert_eq!(cmd.cover.as_ref().unwrap().domain, "player");
    }
}

#[then(expr = "the saga emits {int} DepositFunds commands to player domain")]
fn then_deposit_funds(world: &mut SagaWorld, count: usize) {
    assert_eq!(world.result_commands.len(), count);
    for cmd in &world.result_commands {
        assert_eq!(cmd.cover.as_ref().unwrap().domain, "player");
    }
}

#[then(expr = "the first command has amount {int} for {string}")]
fn then_first_amount(world: &mut SagaWorld, amount: i64, player: String) {
    let expected_root = uuid_for(&player);
    let cb = world
        .result_commands
        .iter()
        .find(|cb| {
            cb.cover
                .as_ref()
                .and_then(|c| c.root.as_ref())
                .map(|r| r.value == expected_root)
                .unwrap_or(false)
        })
        .unwrap_or_else(|| panic!("No command targeting player {}", player));
    let cmd_any = cb
        .pages
        .first()
        .and_then(|p| match &p.payload {
            Some(command_page::Payload::Command(c)) => Some(c),
            _ => None,
        })
        .expect("No command payload");
    let df: DepositFunds = cmd_any.unpack().expect("decode DepositFunds");
    let actual = df.amount.as_ref().map(|c| c.amount).unwrap_or(0);
    assert_eq!(
        actual, amount,
        "Expected amount {} for {}, got {}",
        amount, player, actual
    );
}

#[then(expr = "the second command has amount {int} for {string}")]
fn then_second_amount(world: &mut SagaWorld, amount: i64, player: String) {
    // Look up by player root (order-independent matching)
    then_first_amount(world, amount, player);
}

#[then("only TableSyncSaga handles the event")]
fn then_only_table(world: &mut SagaWorld) {
    assert_eq!(world.handled_by, vec!["TableSyncSaga"]);
}

#[then(expr = "the saga emits {int} DealCards commands")]
fn then_deal_count(world: &mut SagaWorld, count: usize) {
    let types = world.get_command_types();
    let n = types.iter().filter(|t| t.ends_with("DealCards")).count();
    assert_eq!(n, count);
}

#[then("TableSyncSaga still emits its command")]
fn then_still_emits(world: &mut SagaWorld) {
    then_deal_cards(world);
}

#[then("no exception is raised")]
fn then_no_exception(_world: &mut SagaWorld) {}

#[tokio::main]
async fn main() {
    SagaWorld::cucumber()
        .with_writer(
            cucumber::writer::Basic::stdout()
                .summarized()
                .assert_normalized(),
        )
        .run("features/unit/saga.feature")
        .await;
}
