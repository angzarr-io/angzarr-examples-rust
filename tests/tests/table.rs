//! Table aggregate BDD tests using cucumber-rs against Tier 5 Router API.

use std::collections::HashMap;

use agg_table::handlers::{
    handle_add_rebuy_chips, handle_create_table, handle_end_hand, handle_join_table,
    handle_leave_table, handle_seat_player, handle_start_hand,
};
use agg_table::state::{
    apply_chips_added, apply_hand_ended, apply_hand_started, apply_player_joined,
    apply_player_left, apply_player_sat_in, apply_player_sat_out, apply_player_seated,
    apply_rebuy_chips_added, apply_seating_rejected, apply_table_created, TableState,
};
use angzarr_client::proto::{event_page, EventBook};
use angzarr_client::{try_unpack, CommandRejectedError};
use cucumber::{given, then, when, World};
use examples_proto::{
    AddRebuyChips, ChipsAdded, CreateTable, EndHand, GameVariant, HandEnded, HandStarted,
    JoinTable, LeaveTable, PlayerJoined, PlayerLeft, PlayerSatIn, PlayerSatOut, PlayerSeated,
    PotResult, RebuyChipsAdded, SeatPlayer, SeatingRejected, StartHand, TableCreated,
};
use poker_tests::{generate_hand_root, uuid_for};
use prost_types::Any;

fn uuid_or_empty(s: &str) -> Vec<u8> {
    if s.is_empty() {
        Vec::new()
    } else {
        uuid_for(s)
    }
}

/// Test world for table aggregate.
#[derive(Debug, Default, World)]
pub struct TableWorld {
    events: Vec<Any>,
    result: Option<Result<EventBook, CommandRejectedError>>,

    // Table parameters from Given steps
    min_buy_in: i64,
    max_buy_in: i64,
    max_players: i32,
    player_stacks: HashMap<String, i64>,
    dealer_position: i32,
    hand_number: i64,
    /// Cucumber-declared cover for the current scenario. See player.rs
    /// for the rationale (unit-tier tests bypass the router).
    command_cover: Option<angzarr_client::proto::Cover>,
}

impl TableWorld {
    fn table_root(&self) -> Vec<u8> {
        uuid_for("test-table")
    }

    fn player_root(&self, player_id: &str) -> Vec<u8> {
        uuid_or_empty(player_id)
    }

    fn next_seq(&self) -> u32 {
        self.events.len() as u32
    }

    fn rebuild_state(&self) -> TableState {
        let mut state = TableState::default();
        for event_any in &self.events {
            if let Some(ev) = try_unpack::<TableCreated>(event_any) {
                apply_table_created(&mut state, ev);
            } else if let Some(ev) = try_unpack::<PlayerJoined>(event_any) {
                apply_player_joined(&mut state, ev);
            } else if let Some(ev) = try_unpack::<PlayerLeft>(event_any) {
                apply_player_left(&mut state, ev);
            } else if let Some(ev) = try_unpack::<PlayerSatOut>(event_any) {
                apply_player_sat_out(&mut state, ev);
            } else if let Some(ev) = try_unpack::<PlayerSatIn>(event_any) {
                apply_player_sat_in(&mut state, ev);
            } else if let Some(ev) = try_unpack::<HandStarted>(event_any) {
                apply_hand_started(&mut state, ev);
            } else if let Some(ev) = try_unpack::<HandEnded>(event_any) {
                apply_hand_ended(&mut state, ev);
            } else if let Some(ev) = try_unpack::<ChipsAdded>(event_any) {
                apply_chips_added(&mut state, ev);
            } else if let Some(ev) = try_unpack::<PlayerSeated>(event_any) {
                apply_player_seated(&mut state, ev);
            } else if let Some(ev) = try_unpack::<SeatingRejected>(event_any) {
                apply_seating_rejected(&mut state, ev);
            } else if let Some(ev) = try_unpack::<RebuyChipsAdded>(event_any) {
                apply_rebuy_chips_added(&mut state, ev);
            }
        }
        state
    }

    fn result_event(&self) -> Option<Any> {
        self.result
            .as_ref()
            .and_then(|r: &Result<EventBook, CommandRejectedError>| {
                r.as_ref()
                    .ok()
                    .and_then(|eb| eb.pages.first())
                    .and_then(|p| match &p.payload {
                        Some(event_page::Payload::Event(e)) => Some(e.clone()),
                        _ => None,
                    })
            })
    }
}

fn pack_event_any<T: prost::Message + prost::Name>(event: &T) -> Any {
    examples_utils::pack_event(event, &T::full_name())
}

// =============================================================================
// Given steps
// =============================================================================

#[given("no prior events for the table aggregate")]
fn given_no_events(world: &mut TableWorld) {
    world.events.clear();
    world.min_buy_in = 200;
    world.max_buy_in = 1000;
    world.max_players = 9;
}

#[given(expr = "a TableCreated event for {string}")]
fn given_table_created(world: &mut TableWorld, table_name: String) {
    if world.min_buy_in == 0 {
        world.min_buy_in = 200;
    }
    if world.max_buy_in == 0 {
        world.max_buy_in = 1000;
    }
    if world.max_players == 0 {
        world.max_players = 9;
    }
    let event = TableCreated {
        table_name,
        game_variant: GameVariant::TexasHoldem as i32,
        small_blind: 5,
        big_blind: 10,
        min_buy_in: world.min_buy_in,
        max_buy_in: world.max_buy_in,
        max_players: world.max_players,
        action_timeout_seconds: 30,
        created_at: None,
    };
    world.events.push(pack_event_any(&event));
}

#[given(expr = "a TableCreated event for {string} with min_buy_in {int}")]
fn given_table_created_min_buyin(world: &mut TableWorld, table_name: String, min_buy_in: i64) {
    world.events.clear();
    world.min_buy_in = min_buy_in;
    world.max_buy_in = 1000;
    world.max_players = 9;
    given_table_created(world, table_name);
}

#[given(expr = "a TableCreated event for {string} with max_players {int}")]
fn given_table_created_max_players(world: &mut TableWorld, table_name: String, max_players: i32) {
    world.events.clear();
    world.min_buy_in = 200;
    world.max_buy_in = 1000;
    world.max_players = max_players;
    given_table_created(world, table_name);
}

#[given(expr = "a PlayerJoined event for player {string} at seat {int}")]
fn given_player_joined(world: &mut TableWorld, player_id: String, seat: i32) {
    let stack = world.player_stacks.get(&player_id).copied().unwrap_or(500);
    let event = PlayerJoined {
        player_root: world.player_root(&player_id),
        seat_position: seat,
        buy_in_amount: stack,
        stack,
        joined_at: None,
    };
    world.events.push(pack_event_any(&event));
}

#[given(expr = "a PlayerJoined event for player {string} at seat {int} with stack {int}")]
fn given_player_joined_stack(world: &mut TableWorld, player_id: String, seat: i32, stack: i64) {
    world.player_stacks.insert(player_id.clone(), stack);
    given_player_joined(world, player_id, seat);
}

#[given(expr = "a HandStarted event for hand {int}")]
fn given_hand_started(world: &mut TableWorld, hand_number: i64) {
    world.hand_number = hand_number;
    let table_root = world.table_root();
    let hand_root = generate_hand_root(&table_root, hand_number);
    let event = HandStarted {
        hand_root,
        hand_number,
        dealer_position: world.dealer_position,
        small_blind_position: 0,
        big_blind_position: 1,
        active_players: vec![],
        game_variant: GameVariant::TexasHoldem as i32,
        small_blind: 5,
        big_blind: 10,
        started_at: None,
        ..Default::default()
    };
    world.events.push(pack_event_any(&event));
}

#[given(expr = "a HandStarted event for hand {int} with dealer at seat {int}")]
fn given_hand_started_dealer(world: &mut TableWorld, hand_number: i64, dealer_seat: i32) {
    world.dealer_position = dealer_seat;
    given_hand_started(world, hand_number);
}

#[given(expr = "a HandEnded event for hand {int}")]
fn given_hand_ended(world: &mut TableWorld, hand_number: i64) {
    let table_root = world.table_root();
    let hand_root = generate_hand_root(&table_root, hand_number);
    let event = HandEnded {
        hand_root,
        results: vec![],
        stack_changes: HashMap::new(),
        ended_at: None,
    };
    world.events.push(pack_event_any(&event));
}

// =============================================================================
// When steps
// =============================================================================

#[when(regex = r"I handle a CreateTable command with name (.+) and variant (.+):")]
fn when_create_table(world: &mut TableWorld, step: &cucumber::gherkin::Step) {
    let (name, variant) = {
        let captures = regex::Regex::new(r#"name "([^"]*)" and variant "([^"]*)""#)
            .unwrap()
            .captures(&step.value)
            .unwrap();
        (
            captures.get(1).unwrap().as_str().to_string(),
            captures.get(2).unwrap().as_str().to_string(),
        )
    };

    let table = step.table.as_ref().expect("Expected data table");
    let row = &table.rows[1];

    let small_blind: i64 = row[0].parse().unwrap();
    let big_blind: i64 = row[1].parse().unwrap();
    let min_buy_in: i64 = row[2].parse().unwrap();
    let max_buy_in: i64 = row[3].parse().unwrap();
    let max_players: i32 = row[4].parse().unwrap();

    let game_variant = match variant.as_str() {
        "TEXAS_HOLDEM" => GameVariant::TexasHoldem,
        "FIVE_CARD_DRAW" => GameVariant::FiveCardDraw,
        "OMAHA" => GameVariant::Omaha,
        _ => GameVariant::TexasHoldem,
    };

    let cmd = CreateTable {
        table_name: name,
        game_variant: game_variant as i32,
        small_blind,
        big_blind,
        min_buy_in,
        max_buy_in,
        max_players,
        action_timeout_seconds: 30,
    };

    let state = world.rebuild_state();
    world.result = Some(handle_create_table(cmd, &state, world.next_seq()));
}

#[when(expr = "I handle a JoinTable command for player {string} at seat {int} with buy-in {int}")]
fn when_join_table(world: &mut TableWorld, player_id: String, seat: i32, buy_in: i64) {
    let cmd = JoinTable {
        player_root: world.player_root(&player_id),
        preferred_seat: seat,
        buy_in_amount: buy_in,
    };
    let state = world.rebuild_state();
    world.result = Some(handle_join_table(cmd, &state, world.next_seq()));
}

#[when(expr = "I handle a LeaveTable command for player {string}")]
fn when_leave_table(world: &mut TableWorld, player_id: String) {
    let cmd = LeaveTable {
        player_root: world.player_root(&player_id),
    };
    let state = world.rebuild_state();
    world.result = Some(handle_leave_table(cmd, &state, world.next_seq()));
}

#[when("I handle a StartHand command")]
fn when_start_hand(world: &mut TableWorld) {
    let cmd = StartHand {
        ..Default::default()
    };
    let state = world.rebuild_state();
    world.result = Some(handle_start_hand(cmd, &state, world.next_seq()));
}

#[when(expr = "I handle an EndHand command with winner {string} winning {int}")]
fn when_end_hand(world: &mut TableWorld, winner_id: String, amount: i64) {
    let table_root = world.table_root();
    let hand_number = world.hand_number;
    let hand_root = generate_hand_root(&table_root, hand_number);

    let cmd = EndHand {
        hand_root,
        results: vec![PotResult {
            winner_root: world.player_root(&winner_id),
            amount,
            pot_type: "main".to_string(),
            winning_hand: None,
        }],
    };

    let state = world.rebuild_state();
    world.result = Some(handle_end_hand(cmd, &state, world.next_seq()));
}

#[when(regex = r"I handle an EndHand command with results:")]
fn when_end_hand_with_results(world: &mut TableWorld, step: &cucumber::gherkin::Step) {
    let table_root = world.table_root();
    let hand_number = if world.hand_number > 0 {
        world.hand_number
    } else {
        1
    };
    let hand_root = generate_hand_root(&table_root, hand_number);

    let table = step.table.as_ref().expect("Expected data table");
    let results: Vec<PotResult> = table
        .rows
        .iter()
        .skip(1)
        .map(|row| {
            let player_id = &row[0];
            let change: i64 = row[1].parse().unwrap();
            PotResult {
                winner_root: world.player_root(player_id),
                amount: change,
                pot_type: "main".to_string(),
                winning_hand: None,
            }
        })
        .collect();

    let cmd = EndHand { hand_root, results };

    let state = world.rebuild_state();
    world.result = Some(handle_end_hand(cmd, &state, world.next_seq()));
}

#[when("I rebuild the table state")]
fn when_rebuild_state(_world: &mut TableWorld) {
    // State is rebuilt in Then steps
}

// =============================================================================
// Then steps
// =============================================================================

#[then(expr = "the result is a {word} event")]
fn then_result_is_event(world: &mut TableWorld, event_type: String) {
    let result = world.result.as_ref().expect("No result");
    let event_book = result.as_ref().expect("Expected success but got error");
    let event = event_book
        .pages
        .first()
        .and_then(|p| match &p.payload {
            Some(event_page::Payload::Event(e)) => Some(e),
            _ => None,
        })
        .expect("No event in result");

    let actual_type = angzarr_client::type_name_from_url(&event.type_url);
    assert_eq!(
        actual_type, event_type,
        "Expected {} but got {}",
        event_type, actual_type
    );
}

#[then(expr = "the command fails with status {string}")]
fn then_command_fails(world: &mut TableWorld, status: String) {
    let result = world.result.as_ref().expect("No result");
    let err = result.as_ref().unwrap_err();
    assert_eq!(
        err.status_code, status,
        "Expected status {}, got {}",
        status, err.status_code
    );
}

#[then(expr = "the error message contains {string}")]
fn then_error_contains(world: &mut TableWorld, expected: String) {
    let result = world.result.as_ref().expect("No result");
    let err = result.as_ref().unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.to_lowercase().contains(&expected.to_lowercase()),
        "Expected error to contain '{}' but got '{}'",
        expected,
        msg
    );
}

#[then(expr = "the command is rejected with code {string}")]
fn then_command_rejected_with_code(world: &mut TableWorld, code: String) {
    let result = world
        .result
        .as_ref()
        .expect("Expected command to be rejected but it succeeded");
    let err = result
        .as_ref()
        .err()
        .expect("Expected command to be rejected but it succeeded");
    assert_eq!(
        err.code, code,
        "Expected rejection code '{}', got '{}'",
        code, err.code
    );
}

#[then(expr = "the rejection field {string} equals {string}")]
fn then_rejection_field_equals(world: &mut TableWorld, field: String, value: String) {
    let result = world
        .result
        .as_ref()
        .expect("Expected command to be rejected but it succeeded");
    let err = result
        .as_ref()
        .err()
        .expect("Expected command to be rejected but it succeeded");
    let actual = err.details.get(&field).cloned().unwrap_or_else(|| {
        panic!(
            "Rejection has no field '{}'; available: {:?}",
            field,
            err.details.keys().collect::<Vec<_>>()
        )
    });
    assert_eq!(
        actual, value,
        "Rejection field '{}': expected '{}', got '{}'",
        field, value, actual
    );
}

#[then(regex = r#"^the rejection cover has (.+)$"#)]
fn then_rejection_cover_has(world: &mut TableWorld, spec: String) {
    let cover = rejection_cover_or_fail_table(world).clone();
    for (field, value) in poker_tests::cover_field_pairs(&spec) {
        let actual = poker_tests::read_cover_field(&cover, &field);
        assert_eq!(
            actual, value,
            "Rejection cover {}: expected '{}', got '{}'",
            field, value, actual
        );
    }
}

fn rejection_cover_or_fail_table(world: &mut TableWorld) -> &angzarr_client::proto::Cover {
    if let Some(cover) = world.command_cover.clone() {
        if let Some(Err(rej)) = world.result.as_mut() {
            if rej.cover.is_none() {
                rej.cover = Some(cover);
            }
        }
    }
    let result = world
        .result
        .as_ref()
        .expect("Expected command to be rejected but it succeeded");
    let err = result
        .as_ref()
        .err()
        .expect("Expected command to be rejected but it succeeded");
    err.cover.as_ref().expect(
        "Rejection has no cover stamped — declare one via `the command cover has ...` Given steps",
    )
}

#[given(regex = r#"^the command cover has (.+)$"#)]
fn given_cover_has_table(world: &mut TableWorld, spec: String) {
    let cover = world.command_cover.get_or_insert_with(Default::default);
    for (field, value) in poker_tests::cover_field_pairs(&spec) {
        poker_tests::write_cover_field(cover, &field, &value);
    }
}

#[then(expr = "the table event has table_name {string}")]
fn then_table_name(world: &mut TableWorld, expected: String) {
    let event = world.result_event().expect("No event");
    let table_created = try_unpack::<TableCreated>(&event).expect("Failed to decode");
    assert_eq!(table_created.table_name, expected);
}

#[then(expr = "the table event has game_variant {string}")]
fn then_game_variant(world: &mut TableWorld, expected: String) {
    let event = world.result_event().expect("No event");
    let expected_variant = match expected.as_str() {
        "TEXAS_HOLDEM" => GameVariant::TexasHoldem,
        "FIVE_CARD_DRAW" => GameVariant::FiveCardDraw,
        "OMAHA" => GameVariant::Omaha,
        _ => panic!("Unknown variant: {}", expected),
    };

    let actual_variant = if let Some(tc) = try_unpack::<TableCreated>(&event) {
        GameVariant::try_from(tc.game_variant).unwrap_or_default()
    } else if let Some(hs) = try_unpack::<HandStarted>(&event) {
        GameVariant::try_from(hs.game_variant).unwrap_or_default()
    } else {
        panic!("Unknown event type: {}", event.type_url);
    };

    assert_eq!(actual_variant, expected_variant);
}

#[then(expr = "the table event has small_blind {int}")]
fn then_small_blind(world: &mut TableWorld, expected: i64) {
    let event = world.result_event().expect("No event");
    let table_created = try_unpack::<TableCreated>(&event).expect("Failed to decode");
    assert_eq!(table_created.small_blind, expected);
}

#[then(expr = "the table event has big_blind {int}")]
fn then_big_blind(world: &mut TableWorld, expected: i64) {
    let event = world.result_event().expect("No event");
    let table_created = try_unpack::<TableCreated>(&event).expect("Failed to decode");
    assert_eq!(table_created.big_blind, expected);
}

#[then(expr = "the table event has seat_position {int}")]
fn then_seat_position(world: &mut TableWorld, expected: i32) {
    let event = world.result_event().expect("No event");
    let player_joined = try_unpack::<PlayerJoined>(&event).expect("Failed to decode");
    assert_eq!(player_joined.seat_position, expected);
}

#[then(expr = "the table event has buy_in_amount {int}")]
fn then_buy_in_amount(world: &mut TableWorld, expected: i64) {
    let event = world.result_event().expect("No event");
    let player_joined = try_unpack::<PlayerJoined>(&event).expect("Failed to decode");
    assert_eq!(player_joined.buy_in_amount, expected);
}

#[then(expr = "the table event has chips_cashed_out {int}")]
fn then_chips_cashed_out(world: &mut TableWorld, expected: i64) {
    let event = world.result_event().expect("No event");
    let player_left = try_unpack::<PlayerLeft>(&event).expect("Failed to decode");
    assert_eq!(player_left.chips_cashed_out, expected);
}

#[then(expr = "the table event has hand_number {int}")]
fn then_hand_number(world: &mut TableWorld, expected: i64) {
    let event = world.result_event().expect("No event");
    let hand_started = try_unpack::<HandStarted>(&event).expect("Failed to decode");
    assert_eq!(hand_started.hand_number, expected);
}

#[then(expr = "the table event has {int} active_players")]
fn then_active_players_count(world: &mut TableWorld, expected: usize) {
    let event = world.result_event().expect("No event");
    let hand_started = try_unpack::<HandStarted>(&event).expect("Failed to decode");
    assert_eq!(hand_started.active_players.len(), expected);
}

#[then(expr = "the table event has dealer_position {int}")]
fn then_dealer_position(world: &mut TableWorld, expected: i32) {
    let event = world.result_event().expect("No event");
    let hand_started = try_unpack::<HandStarted>(&event).expect("Failed to decode");
    assert_eq!(hand_started.dealer_position, expected);
}

#[then(expr = r"player {string} stack change is {int}")]
fn then_stack_change(world: &mut TableWorld, player_id: String, expected: i64) {
    let event = world.result_event().expect("No event");
    let hand_ended = try_unpack::<HandEnded>(&event).expect("Failed to decode");
    let player_hex = hex::encode(world.player_root(&player_id));
    let change = hand_ended
        .stack_changes
        .get(&player_hex)
        .copied()
        .unwrap_or(0);
    assert_eq!(change, expected);
}

#[then(expr = "the table state has {int} players")]
fn then_state_player_count(world: &mut TableWorld, expected: usize) {
    let state = world.rebuild_state();
    assert_eq!(state.player_count(), expected);
}

#[then(expr = "the table state has seat {int} occupied by {string}")]
fn then_seat_occupied(world: &mut TableWorld, seat: i32, player_id: String) {
    let state = world.rebuild_state();
    let seat_state = state.seats.get(&seat).expect("Seat not found");
    assert_eq!(
        hex::encode(&seat_state.player_root),
        hex::encode(world.player_root(&player_id))
    );
}

#[then(expr = "the table state has status {string}")]
fn then_state_status(world: &mut TableWorld, expected: String) {
    let state = world.rebuild_state();
    assert_eq!(state.status, expected);
}

#[then(expr = "the table state has hand_count {int}")]
fn then_state_hand_count(world: &mut TableWorld, expected: i64) {
    let state = world.rebuild_state();
    assert_eq!(state.hand_count, expected);
}

// =============================================================================
// New Given steps
// =============================================================================

#[given(expr = "a PlayerSatOut event for player {string}")]
fn given_player_sat_out(world: &mut TableWorld, player_id: String) {
    let event = PlayerSatOut {
        player_root: world.player_root(&player_id),
        sat_out_at: None,
    };
    world.events.push(pack_event_any(&event));
}

#[given(expr = "a PlayerSatIn event for player {string}")]
fn given_player_sat_in(world: &mut TableWorld, player_id: String) {
    let event = PlayerSatIn {
        player_root: world.player_root(&player_id),
        sat_in_at: None,
    };
    world.events.push(pack_event_any(&event));
}

#[given(expr = "a ChipsAdded event for player {string} with new_stack {int}")]
fn given_chips_added(world: &mut TableWorld, player_id: String, new_stack: i64) {
    let state = world.rebuild_state();
    let player_root = world.player_root(&player_id);
    let prev_stack = state
        .find_seat_by_player(&player_root)
        .map(|s| s.stack)
        .unwrap_or(0);
    let event = ChipsAdded {
        player_root,
        amount: new_stack - prev_stack,
        new_stack,
        added_at: None,
    };
    world.events.push(pack_event_any(&event));
}

// =============================================================================
// New When steps
// =============================================================================

#[when(
    expr = "I handle a SeatPlayer command for player {string} reservation {string} seat {int} amount {int}"
)]
fn when_seat_player(
    world: &mut TableWorld,
    player_id: String,
    reservation: String,
    seat: i32,
    amount: i64,
) {
    let cmd = SeatPlayer {
        player_root: world.player_root(&player_id),
        reservation_id: uuid_or_empty(&reservation),
        seat,
        amount,
        ..Default::default()
    };
    let state = world.rebuild_state();
    world.result = Some(handle_seat_player(cmd, &state, world.next_seq()));
}

#[when(
    expr = "I handle an AddRebuyChips command for player {string} reservation {string} seat {int} amount {int}"
)]
fn when_add_rebuy_chips(
    world: &mut TableWorld,
    player_id: String,
    reservation: String,
    seat: i32,
    amount: i64,
) {
    let cmd = AddRebuyChips {
        player_root: world.player_root(&player_id),
        reservation_id: uuid_or_empty(&reservation),
        seat,
        amount,
    };
    let state = world.rebuild_state();
    world.result = Some(handle_add_rebuy_chips(cmd, &state, world.next_seq()));
}

#[when("I handle an EndHand command with mismatched hand_root")]
fn when_end_hand_mismatched(world: &mut TableWorld) {
    let cmd = EndHand {
        hand_root: uuid_for("nonexistent-hand"),
        results: vec![],
    };
    let state = world.rebuild_state();
    world.result = Some(handle_end_hand(cmd, &state, world.next_seq()));
}

#[when(expr = "I start a hand and end it with winner {string} winning {int}")]
fn when_start_then_end_hand(world: &mut TableWorld, winner: String, amount: i64) {
    // StartHand
    let state = world.rebuild_state();
    let book = handle_start_hand(
        StartHand {
            ..Default::default()
        },
        &state,
        world.next_seq(),
    )
    .expect("start hand");
    let mut hand_root: Vec<u8> = Vec::new();
    if let Some(page) = book.pages.first() {
        if let Some(event_page::Payload::Event(e)) = &page.payload {
            if let Some(hs) = try_unpack::<HandStarted>(e) {
                world.hand_number = hs.hand_number;
                hand_root = hs.hand_root.clone();
            }
            world.events.push(e.clone());
        }
    }
    // EndHand using hand_root from HandStarted (handler-derived, not re-derived)
    let cmd = EndHand {
        hand_root,
        results: vec![PotResult {
            winner_root: world.player_root(&winner),
            amount,
            pot_type: "main".to_string(),
            winning_hand: None,
        }],
    };
    let state = world.rebuild_state();
    let result = handle_end_hand(cmd, &state, world.next_seq());
    if let Ok(book) = &result {
        for page in &book.pages {
            if let Some(event_page::Payload::Event(e)) = &page.payload {
                world.events.push(e.clone());
            }
        }
    }
    world.result = Some(result);
}

// =============================================================================
// New Then steps
// =============================================================================

#[then(expr = "the table state has {int} active_players")]
fn then_active_player_count(world: &mut TableWorld, expected: usize) {
    let state = world.rebuild_state();
    assert_eq!(state.active_player_count(), expected);
}

#[then(expr = "the table state has table_id {string}")]
fn then_state_table_id(world: &mut TableWorld, expected: String) {
    let state = world.rebuild_state();
    assert_eq!(state.table_id, expected);
}

#[then("the table state is full")]
fn then_state_is_full(world: &mut TableWorld) {
    let state = world.rebuild_state();
    assert_eq!(
        state.player_count() as i32,
        state.max_players,
        "Expected table to be full ({} of {})",
        state.player_count(),
        state.max_players
    );
}

#[then(expr = "the table state seat {int} has stack {int}")]
fn then_seat_stack(world: &mut TableWorld, seat: i32, expected: i64) {
    let state = world.rebuild_state();
    let seat_state = state.seats.get(&seat).expect("Seat not found");
    assert_eq!(seat_state.stack, expected);
}

#[then(expr = "the table state has current_hand_root empty")]
fn then_current_hand_root_empty(world: &mut TableWorld) {
    let state = world.rebuild_state();
    assert!(
        state.current_hand_root.is_empty(),
        "Expected current_hand_root empty"
    );
}

#[then("the small_blind_position equals the dealer_position")]
fn then_sb_equals_dealer(world: &mut TableWorld) {
    let event = world.result_event().expect("No event");
    let hs = try_unpack::<HandStarted>(&event).expect("HandStarted expected");
    assert_eq!(
        hs.small_blind_position, hs.dealer_position,
        "SB should equal dealer in heads-up"
    );
}

#[then("the small_blind_position differs from the dealer_position")]
fn then_sb_differs_dealer(world: &mut TableWorld) {
    let event = world.result_event().expect("No event");
    let hs = try_unpack::<HandStarted>(&event).expect("HandStarted expected");
    assert_ne!(
        hs.small_blind_position, hs.dealer_position,
        "SB should differ from dealer with 3+ players"
    );
}

#[then(expr = "the seating event has seat_position {int}")]
fn then_seating_event_seat_position(world: &mut TableWorld, expected: i32) {
    let event = world.result_event().expect("No event");
    let ps = try_unpack::<PlayerSeated>(&event).expect("PlayerSeated expected");
    assert_eq!(ps.seat_position, expected);
}

#[then(expr = "the seating event has stack {int}")]
fn then_seating_event_stack(world: &mut TableWorld, expected: i64) {
    let event = world.result_event().expect("No event");
    let ps = try_unpack::<PlayerSeated>(&event).expect("PlayerSeated expected");
    assert_eq!(ps.stack, expected);
}

#[then(expr = "the seating rejection reason contains {string}")]
fn then_seating_rejection_reason(world: &mut TableWorld, expected: String) {
    let event = world.result_event().expect("No event");
    let sr = try_unpack::<SeatingRejected>(&event).expect("SeatingRejected expected");
    assert!(
        sr.reason.to_lowercase().contains(&expected.to_lowercase()),
        "Expected SeatingRejected reason to contain '{}' but got '{}'",
        expected,
        sr.reason
    );
}

#[then(expr = "the rebuy event has amount {int}")]
fn then_rebuy_amount(world: &mut TableWorld, expected: i64) {
    let event = world.result_event().expect("No event");
    let rca = try_unpack::<RebuyChipsAdded>(&event).expect("RebuyChipsAdded expected");
    assert_eq!(rca.amount, expected);
}

#[then(expr = "the rebuy event has new_stack {int}")]
fn then_rebuy_new_stack(world: &mut TableWorld, expected: i64) {
    let event = world.result_event().expect("No event");
    let rca = try_unpack::<RebuyChipsAdded>(&event).expect("RebuyChipsAdded expected");
    assert_eq!(rca.new_stack, expected);
}

#[then(expr = "the rebuy event has seat {int}")]
fn then_rebuy_seat(world: &mut TableWorld, expected: i32) {
    let event = world.result_event().expect("No event");
    let rca = try_unpack::<RebuyChipsAdded>(&event).expect("RebuyChipsAdded expected");
    assert_eq!(rca.seat, expected);
}

#[tokio::main]
async fn main() {
    TableWorld::run("features/example/unit/table.feature").await;
}
