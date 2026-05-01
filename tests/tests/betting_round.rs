//! Betting-round iteration BDD tests.
//!
//! Dispatches through `agg_hand::betting_round::betting_round` — the real
//! production driver. The World holds only the inputs (seat list, scripted
//! actions); the When step calls the driver and stashes its result. No
//! shadow `MockPlayer`, no `BettingRoundTester`.

use std::collections::BTreeMap;

use agg_hand::betting_round::{betting_round, Action, BettingRoundResult, SeatState};
use cucumber::{given, then, when, World};

fn action_from_label(label: &str) -> Action {
    match label.trim().to_ascii_uppercase().as_str() {
        "FOLD" => Action::Fold,
        "CHECK" => Action::Check,
        "CALL" => Action::Call,
        "BET" => Action::Bet,
        "RAISE" => Action::Raise,
        other => panic!("Unknown action label: {}", other),
    }
}

#[derive(Debug, Default, World)]
pub struct BettingWorld {
    seats: BTreeMap<i32, SeatState>,
    big_blind: i64,
    current_bet: i64,
    last_raise_increment: i64,
    actions: Vec<(Action, i64)>,
    result: Option<BettingRoundResult>,
}

#[given(expr = "a {int}-player table with big_blind {int}")]
fn given_n_player_table(world: &mut BettingWorld, count: i32, bb: i64) {
    world.seats.clear();
    for i in 0..count {
        world.seats.insert(i, SeatState::new(i, 1000));
    }
    world.big_blind = bb;
    world.last_raise_increment = bb;
}

#[given("a table with players:")]
fn given_table_with_players(world: &mut BettingWorld, step: &cucumber::gherkin::Step) {
    let table = step.table.as_ref().expect("expected data table");
    let headings = &table.rows[0];
    let seat_col = headings.iter().position(|h| h == "seat").unwrap();
    let stack_col = headings.iter().position(|h| h == "stack").unwrap();

    world.seats.clear();
    for row in table.rows.iter().skip(1) {
        let seat: i32 = row[seat_col].parse().unwrap();
        let stack: i64 = row[stack_col].parse().unwrap();
        world.seats.insert(seat, SeatState::new(seat, stack));
    }
}

#[given(expr = "big_blind {int}")]
fn given_big_blind(world: &mut BettingWorld, bb: i64) {
    world.big_blind = bb;
    world.last_raise_increment = bb;
}

#[given(
    expr = "blinds posted: {string} seat {int} amount {int} and {string} seat {int} amount {int}"
)]
fn given_blinds_posted(
    world: &mut BettingWorld,
    _sb_name: String,
    sb_seat: i32,
    sb_amt: i64,
    _bb_name: String,
    bb_seat: i32,
    bb_amt: i64,
) {
    if let Some(p) = world.seats.get_mut(&sb_seat) {
        p.bet = sb_amt;
        p.stack -= sb_amt;
    }
    if let Some(p) = world.seats.get_mut(&bb_seat) {
        p.bet = bb_amt;
        p.stack -= bb_amt;
    }
    world.current_bet = bb_amt;
    if world.last_raise_increment == 0 {
        world.last_raise_increment = bb_amt;
    }
}

#[given("scripted actions:")]
fn given_scripted_actions(world: &mut BettingWorld, step: &cucumber::gherkin::Step) {
    let table = step.table.as_ref().expect("expected data table");
    let headings = &table.rows[0];
    let action_col = headings.iter().position(|h| h == "action").unwrap();
    let amount_col = headings.iter().position(|h| h == "amount").unwrap();
    world.actions = table
        .rows
        .iter()
        .skip(1)
        .map(|row| {
            let a = action_from_label(&row[action_col]);
            let amt: i64 = row[amount_col].parse().unwrap();
            (a, amt)
        })
        .collect();
}

#[when(expr = "I run a preflop betting round starting at seat {int}")]
fn when_run_preflop(world: &mut BettingWorld, seat: i32) {
    let seats: Vec<SeatState> = world.seats.values().cloned().collect();
    let result = betting_round(
        seat,
        &seats,
        world.current_bet,
        world.last_raise_increment,
        &world.actions,
        true,
    );
    world.result = Some(result);
}

fn result(world: &BettingWorld) -> &BettingRoundResult {
    world.result.as_ref().expect("betting round not run")
}

#[then(regex = r"^the seats asked to act are \[([^\]]*)\]$")]
fn then_seats_asked_csv(world: &mut BettingWorld, list: String) {
    let expected: Vec<i32> = list.split(',').map(|s| s.trim().parse().unwrap()).collect();
    let actual = &result(world).seats_asked;
    assert_eq!(
        actual, &expected,
        "Expected {:?}, got {:?}",
        expected, actual
    );
}

#[then(expr = "seat {int} is asked to act after seat {int}")]
fn then_seat_after(world: &mut BettingWorld, after: i32, before: i32) {
    let order = &result(world).seats_asked;
    let before_idx = order
        .iter()
        .position(|&s| s == before)
        .unwrap_or_else(|| panic!("seat {} never asked to act: {:?}", before, order));
    assert!(
        order[before_idx + 1..].contains(&after),
        "seat {} did not appear after seat {}; full order: {:?}",
        after,
        before,
        order
    );
}

#[then(expr = "seat {int} is asked to act exactly {int} time")]
fn then_seat_asked_exact_one(world: &mut BettingWorld, seat: i32, n: usize) {
    let count = result(world)
        .seats_asked
        .iter()
        .filter(|&&s| s == seat)
        .count();
    assert_eq!(
        count,
        n,
        "seat {} asked {} times, expected {}. full order: {:?}",
        seat,
        count,
        n,
        result(world).seats_asked
    );
}

#[then(expr = "seat {int} is asked to act exactly {int} times")]
fn then_seat_asked_exact(world: &mut BettingWorld, seat: i32, n: usize) {
    then_seat_asked_exact_one(world, seat, n);
}

#[then(expr = "seat {int} is all-in")]
fn then_seat_all_in(world: &mut BettingWorld, seat: i32) {
    let p = result(world)
        .seats
        .iter()
        .find(|s| s.seat == seat)
        .expect("seat in table");
    assert!(p.all_in, "seat {} is not all-in", seat);
}

#[then(expr = "seat {int} has stack {int}")]
fn then_seat_stack(world: &mut BettingWorld, seat: i32, stack: i64) {
    let p = result(world)
        .seats
        .iter()
        .find(|s| s.seat == seat)
        .expect("seat in table");
    assert_eq!(
        p.stack, stack,
        "seat {} has stack {}, expected {}",
        seat, p.stack, stack
    );
}

#[tokio::main]
async fn main() {
    BettingWorld::run("features/example/unit/betting_round.feature").await;
}
