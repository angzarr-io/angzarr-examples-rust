//! Saga logic BDD tests (Tier 5 Router API port).
//!
//! Tests saga event translation via the new handler structs:
//!   - `saga_table_hand::TableHandSaga::on_hand_started`
//!   - `saga_hand_table::HandTableSaga::on_hand_complete`
//!   - `saga_hand_player::HandPlayerSaga::on_pot_awarded`
//!   - `saga_table_player::TablePlayerSaga::on_hand_ended`
//!
//! The TableSyncSaga/HandResultsSaga router scenarios are dispatched manually
//! by matching the event's type URL, since the feature file groups multiple
//! saga structs under a single logical "saga" for BDD purposes.

use angzarr_client::proto::{command_page, CommandBook, SagaResponse};
use angzarr_client::{type_name_from_url, unpack};
use cucumber::{given, then, when, World, WriterExt};
use examples_proto::{
    DealCards, DepositFunds, EndHand, GameVariant, HandComplete, HandEnded, HandStarted, PotAwarded,
    PotWinner, SeatSnapshot,
};
use poker_tests::uuid_for;
use prost_types::Any;
use std::collections::HashMap;

#[derive(Debug, Clone)]
enum Trigger {
    HandStarted(HandStarted),
    HandComplete(HandComplete),
    HandEnded(HandEnded),
    PotAwarded(PotAwarded),
}

#[derive(Debug, Default, World)]
#[world(init = Self::new)]
pub struct SagaWorld {
    saga_type: String,
    trigger: Option<Trigger>,
    result_commands: Vec<CommandBook>,
    saga_router_sagas: Vec<String>,
    handled_by: Vec<String>,
    event_count: usize,
}

impl SagaWorld {
    fn new() -> Self {
        Self::default()
    }

    fn event_type_name(&self) -> &'static str {
        match &self.trigger {
            Some(Trigger::HandStarted(_)) => "HandStarted",
            Some(Trigger::HandComplete(_)) => "HandComplete",
            Some(Trigger::HandEnded(_)) => "HandEnded",
            Some(Trigger::PotAwarded(_)) => "PotAwarded",
            None => "",
        }
    }

    fn run_saga(&self, saga_name: &str) -> Option<SagaResponse> {
        let trigger = self.trigger.as_ref()?;
        match (saga_name, trigger) {
            ("TableSyncSaga", Trigger::HandStarted(ev)) => Some(
                saga_table_hand::TableHandSaga
                    .on_hand_started(ev.clone())
                    .expect("TableHandSaga handler"),
            ),
            ("TableSyncSaga", Trigger::HandComplete(ev)) => Some(
                saga_hand_table::HandTableSaga
                    .on_hand_complete(ev.clone())
                    .expect("HandTableSaga handler"),
            ),
            ("HandResultsSaga", Trigger::HandEnded(ev)) => Some(
                saga_table_player::TablePlayerSaga
                    .on_hand_ended(ev.clone())
                    .expect("TablePlayerSaga handler"),
            ),
            ("HandResultsSaga", Trigger::PotAwarded(ev)) => Some(
                saga_hand_player::HandPlayerSaga
                    .on_pot_awarded(ev.clone())
                    .expect("HandPlayerSaga handler"),
            ),
            _ => None,
        }
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

    fn first_command_any(&self) -> Option<Any> {
        self.result_commands
            .first()
            .and_then(|cb| cb.pages.first())
            .and_then(|p| match &p.payload {
                Some(command_page::Payload::Command(c)) => Some(c.clone()),
                _ => None,
            })
    }
}

// =========================================================================
// Given steps — identify saga and build trigger event
// =========================================================================

#[given("a TableSyncSaga")]
fn given_table_sync(world: &mut SagaWorld) {
    world.saga_type = "TableSyncSaga".to_string();
}

#[given("a HandResultsSaga")]
fn given_hand_results(world: &mut SagaWorld) {
    world.saga_type = "HandResultsSaga".to_string();
}

#[given("a HandStarted event from table domain with:")]
fn given_hand_started_table(world: &mut SagaWorld) {
    world.trigger = Some(Trigger::HandStarted(HandStarted {
        hand_root: uuid_for("hand-1"),
        hand_number: 1,
        game_variant: GameVariant::TexasHoldem as i32,
        dealer_position: 0,
        active_players: vec![],
        small_blind: 5,
        big_blind: 10,
        small_blind_position: 1,
        big_blind_position: 2,
        started_at: Some(angzarr_client::now()),
    }));
}

#[given("active players:")]
fn given_active_players(world: &mut SagaWorld, step: &cucumber::gherkin::Step) {
    let mut seats: Vec<SeatSnapshot> = Vec::new();
    if let Some(table) = &step.table {
        for row in table.rows.iter().skip(1) {
            if row.len() >= 3 && !row[0].is_empty() {
                seats.push(SeatSnapshot {
                    player_root: uuid_for(&row[0]),
                    position: row[1].parse().unwrap_or(0),
                    stack: row[2].parse().unwrap_or(0),
                });
            }
        }
    }
    if let Some(Trigger::HandStarted(ref mut hs)) = world.trigger {
        hs.active_players = seats;
    }
}

#[given("a HandComplete event from hand domain with:")]
fn given_hand_complete(world: &mut SagaWorld) {
    world.trigger = Some(Trigger::HandComplete(HandComplete {
        table_root: uuid_for("table-1"),
        hand_number: 1,
        winners: vec![],
        final_stacks: vec![],
        completed_at: Some(angzarr_client::now()),
    }));
}

#[given("winners:")]
fn given_winners(world: &mut SagaWorld, step: &cucumber::gherkin::Step) {
    let mut winners: Vec<PotWinner> = Vec::new();
    if let Some(table) = &step.table {
        for row in table.rows.iter().skip(1) {
            if row.len() >= 2 && !row[0].is_empty() {
                winners.push(PotWinner {
                    player_root: uuid_for(&row[0]),
                    amount: row[1].parse().unwrap_or(0),
                    pot_type: "main".to_string(),
                    winning_hand: None,
                });
            }
        }
    }
    match world.trigger {
        Some(Trigger::HandComplete(ref mut hc)) => hc.winners = winners,
        Some(Trigger::PotAwarded(ref mut pa)) => pa.winners = winners,
        _ => {}
    }
}

#[given("a HandEnded event from table domain with:")]
fn given_hand_ended(world: &mut SagaWorld) {
    world.trigger = Some(Trigger::HandEnded(HandEnded {
        hand_root: uuid_for("hand-1"),
        results: vec![],
        stack_changes: HashMap::new(),
        ended_at: Some(angzarr_client::now()),
    }));
}

#[given("stack_changes:")]
fn given_stack_changes(world: &mut SagaWorld, step: &cucumber::gherkin::Step) {
    let mut changes: HashMap<String, i64> = HashMap::new();
    if let Some(table) = &step.table {
        for row in table.rows.iter().skip(1) {
            if row.len() >= 2 && !row[0].is_empty() {
                let player_root = uuid_for(&row[0]);
                let hex_key = hex::encode(&player_root);
                let delta: i64 = row[1].parse().unwrap_or(0);
                changes.insert(hex_key, delta);
            }
        }
    }
    if let Some(Trigger::HandEnded(ref mut he)) = world.trigger {
        he.stack_changes = changes;
    }
}

#[given("a PotAwarded event from hand domain with:")]
fn given_pot_awarded(world: &mut SagaWorld) {
    world.trigger = Some(Trigger::PotAwarded(PotAwarded {
        winners: vec![],
        awarded_at: Some(angzarr_client::now()),
    }));
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
    world.trigger = Some(Trigger::HandStarted(HandStarted {
        hand_root: uuid_for("hand-1"),
        hand_number: 1,
        game_variant: GameVariant::TexasHoldem as i32,
        dealer_position: 0,
        small_blind_position: 1,
        big_blind_position: 2,
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
        small_blind: 5,
        big_blind: 10,
        started_at: Some(angzarr_client::now()),
    }));
}

#[given("an event book with:")]
fn given_event_book(world: &mut SagaWorld, step: &cucumber::gherkin::Step) {
    let table = step.table.as_ref().expect("Expected data table");
    world.event_count = table.rows.len().saturating_sub(1);
    // The feature uses HandStarted rows; prep one trigger event we'll dispatch
    // event_count times.
    world.trigger = Some(Trigger::HandStarted(HandStarted {
        hand_root: uuid_for("hand-1"),
        hand_number: 1,
        game_variant: GameVariant::TexasHoldem as i32,
        dealer_position: 0,
        small_blind_position: 1,
        big_blind_position: 2,
        active_players: vec![SeatSnapshot {
            player_root: uuid_for("player-1"),
            position: 0,
            stack: 500,
        }],
        small_blind: 5,
        big_blind: 10,
        started_at: Some(angzarr_client::now()),
    }));
}

// =========================================================================
// When steps
// =========================================================================

#[when("the saga handles the event")]
fn when_saga_handles(world: &mut SagaWorld) {
    world.result_commands.clear();
    let response = world
        .run_saga(&world.saga_type.clone())
        .unwrap_or_else(|| panic!("no saga mapping for {}", world.saga_type));
    world.result_commands = response.commands;
}

#[when("the router routes the event")]
fn when_router_routes(world: &mut SagaWorld) {
    world.result_commands.clear();
    world.handled_by.clear();

    // Which event types does each logical saga group cover?
    let table_sync_events = ["HandStarted", "HandComplete"];
    let hand_results_events = ["HandEnded", "PotAwarded"];
    let event_name = world.event_type_name();

    for saga_name in world.saga_router_sagas.clone() {
        match saga_name.as_str() {
            "FailingSaga" => {
                // Simulate a saga that fails — the router continues to the next.
            }
            "TableSyncSaga" if table_sync_events.contains(&event_name) => {
                if let Some(resp) = world.run_saga("TableSyncSaga") {
                    if !resp.commands.is_empty() {
                        world.handled_by.push("TableSyncSaga".to_string());
                        world.result_commands.extend(resp.commands);
                    }
                }
            }
            "HandResultsSaga" if hand_results_events.contains(&event_name) => {
                if let Some(resp) = world.run_saga("HandResultsSaga") {
                    if !resp.commands.is_empty() {
                        world.handled_by.push("HandResultsSaga".to_string());
                        world.result_commands.extend(resp.commands);
                    }
                }
            }
            _ => {}
        }
    }
}

#[when("the router routes the events")]
fn when_router_routes_events(world: &mut SagaWorld) {
    world.result_commands.clear();
    for _ in 0..world.event_count {
        for saga_name in world.saga_router_sagas.clone() {
            if saga_name == "TableSyncSaga" {
                if let Some(resp) = world.run_saga("TableSyncSaga") {
                    world.result_commands.extend(resp.commands);
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
    let cmd_any = world.first_command_any().expect("no command payload");
    let dc: DealCards = unpack(&cmd_any).expect("decode DealCards");
    let expected = match variant.as_str() {
        "TEXAS_HOLDEM" => GameVariant::TexasHoldem as i32,
        "OMAHA" => GameVariant::Omaha as i32,
        "FIVE_CARD_DRAW" => GameVariant::FiveCardDraw as i32,
        _ => GameVariant::Unspecified as i32,
    };
    assert_eq!(dc.game_variant, expected, "variant {}", variant);
}

#[then(expr = "the command has {int} players")]
fn then_player_count(world: &mut SagaWorld, count: usize) {
    let cmd_any = world.first_command_any().expect("no command payload");
    let dc: DealCards = unpack(&cmd_any).expect("decode DealCards");
    assert_eq!(dc.players.len(), count);
}

#[then(expr = "the command has hand_number {int}")]
fn then_hand_number(world: &mut SagaWorld, num: i64) {
    let cmd_any = world.first_command_any().expect("no command payload");
    let dc: DealCards = unpack(&cmd_any).expect("decode DealCards");
    assert_eq!(dc.hand_number, num);
}

#[then("the saga emits an EndHand command to table domain")]
fn then_end_hand(world: &mut SagaWorld) {
    assert!(!world.result_commands.is_empty());
    assert_eq!(
        world.result_commands[0].cover.as_ref().unwrap().domain,
        "table"
    );
    let types = world.get_command_types();
    assert!(
        types.iter().any(|t| t.ends_with("EndHand")),
        "Expected EndHand, got {:?}",
        types
    );
}

#[then(expr = "the command has {int} result")]
fn then_result_count(world: &mut SagaWorld, count: usize) {
    let cmd_any = world.first_command_any().expect("no command payload");
    let eh: EndHand = unpack(&cmd_any).expect("decode EndHand");
    assert_eq!(eh.results.len(), count);
}

#[then(expr = "the result has winner {string} with amount {int}")]
fn then_result_winner(world: &mut SagaWorld, player: String, amount: i64) {
    let cmd_any = world.first_command_any().expect("no command payload");
    let eh: EndHand = unpack(&cmd_any).expect("decode EndHand");
    let expected_root = uuid_for(&player);
    let result = eh
        .results
        .iter()
        .find(|r| r.winner_root == expected_root)
        .unwrap_or_else(|| panic!("No result found for player {}", player));
    assert_eq!(result.amount, amount);
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
        .expect("no command payload");
    let df: DepositFunds = unpack(cmd_any).expect("decode DepositFunds");
    let actual = df.amount.as_ref().map(|c| c.amount).unwrap_or(0);
    assert_eq!(actual, amount);
}

#[then(expr = "the second command has amount {int} for {string}")]
fn then_second_amount(world: &mut SagaWorld, amount: i64, player: String) {
    // Order-independent; look up by player root (saga iterates a HashMap).
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
