//! Raise-tracking arithmetic BDD tests.
//!
//! Dispatches through `agg_hand::raise_tracking` helpers — the same module
//! used by `hand/agg/src/handlers/player_action.rs` to validate raises. The
//! World is a thin holder of the inputs; every When/Then step calls into
//! the production functions, no shadow `recompute()`.

use agg_hand::raise_tracking::{all_in_to, min_raise_to, next_last_raise_increment};
use cucumber::{given, then, when, World};

#[derive(Debug, Default, World)]
pub struct RaiseWorld {
    current_bet: i64,
    last_raise_increment: i64,
    min_raise_to: i64,
    all_in_to: i64,
}

#[given(expr = "current_bet is {int} and last_raise_increment is {int}")]
fn given_state(world: &mut RaiseWorld, bet: i64, inc: i64) {
    world.current_bet = bet;
    world.last_raise_increment = inc;
    world.min_raise_to = min_raise_to(bet, inc);
}

#[when("I compute the min_raise_to")]
fn when_compute_min_raise(world: &mut RaiseWorld) {
    world.min_raise_to = min_raise_to(world.current_bet, world.last_raise_increment);
}

#[when(expr = "a player raises to {int}")]
fn when_raise_to(world: &mut RaiseWorld, amt: i64) {
    world.last_raise_increment =
        next_last_raise_increment(world.current_bet, world.last_raise_increment, amt);
    world.current_bet = amt;
    world.min_raise_to = min_raise_to(world.current_bet, world.last_raise_increment);
}

#[when(expr = "a player calls {int}")]
fn when_call(world: &mut RaiseWorld, _amt: i64) {
    // A call neither moves current_bet nor last_raise_increment.
    world.min_raise_to = min_raise_to(world.current_bet, world.last_raise_increment);
}

#[when(expr = "a below-increment raise of increment {int} is applied")]
fn when_below_increment_raise(world: &mut RaiseWorld, inc: i64) {
    // Server uses max(); a below-increment raise (also rejected at the
    // handler) never shrinks the tracked increment. Drive that semantic
    // through the same helper.
    let synthetic_target = world.current_bet + inc;
    world.last_raise_increment = next_last_raise_increment(
        world.current_bet,
        world.last_raise_increment,
        synthetic_target,
    );
    world.min_raise_to = min_raise_to(world.current_bet, world.last_raise_increment);
}

#[when(expr = "a player bets {int} on a new round")]
fn when_bet_new_round(world: &mut RaiseWorld, amt: i64) {
    // Across rounds current_bet resets to 0 but last_raise_increment
    // persists. The same max() helper handles the grow-or-hold decision.
    world.last_raise_increment =
        next_last_raise_increment(world.current_bet, world.last_raise_increment, amt);
    world.current_bet = amt;
    world.min_raise_to = min_raise_to(world.current_bet, world.last_raise_increment);
}

#[when(expr = "a player goes all-in to {int}")]
fn when_all_in_to(world: &mut RaiseWorld, amt: i64) {
    // The feature passes the absolute target; that's `all_in_to(stack,
    // committed)`. We reverse-engineer one decomposition (stack=amt,
    // committed=0) so the production helper is on the path.
    world.all_in_to = all_in_to(amt, 0);
    world.min_raise_to = min_raise_to(world.current_bet, world.last_raise_increment);
}

#[then(expr = "min_raise_to is {int}")]
fn then_min_raise_to(world: &mut RaiseWorld, expected: i64) {
    assert_eq!(
        world.min_raise_to,
        min_raise_to(world.current_bet, world.last_raise_increment),
        "World state out of sync with helper"
    );
    assert_eq!(
        world.min_raise_to, expected,
        "Expected min_raise_to={}, got {}",
        expected, world.min_raise_to
    );
}

#[then(expr = "last_raise_increment is {int}")]
fn then_last_raise_increment(world: &mut RaiseWorld, expected: i64) {
    assert_eq!(
        world.last_raise_increment, expected,
        "Expected last_raise_increment={}, got {}",
        expected, world.last_raise_increment
    );
}

#[then(expr = "current_bet is {int}")]
fn then_current_bet(world: &mut RaiseWorld, expected: i64) {
    assert_eq!(
        world.current_bet, expected,
        "Expected current_bet={}, got {}",
        expected, world.current_bet
    );
}

#[then("the all-in amount is less than min_raise_to")]
fn then_all_in_less_than_min_raise(world: &mut RaiseWorld) {
    let mr = min_raise_to(world.current_bet, world.last_raise_increment);
    assert!(
        world.all_in_to < mr,
        "all-in {} is not less than min_raise_to {}",
        world.all_in_to,
        mr
    );
}

#[tokio::main]
async fn main() {
    RaiseWorld::run("features/example/unit/raise_tracking.feature").await;
}
