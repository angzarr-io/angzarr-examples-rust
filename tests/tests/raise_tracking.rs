//! Raise-tracking arithmetic BDD tests.
//!
//! Pure math — no handlers, no state machine. Each step reads/writes integer
//! fields on the World and computes `min_raise_to = current_bet + last_raise_increment`.

use cucumber::{given, then, when, World};

#[derive(Debug, Default, World)]
pub struct RaiseWorld {
    current_bet: i64,
    last_raise_increment: i64,
    min_raise_to: i64,
    all_in_to: i64,
}

impl RaiseWorld {
    fn recompute(&mut self) {
        self.min_raise_to = self.current_bet + self.last_raise_increment;
    }
}

#[given(expr = "current_bet is {int} and last_raise_increment is {int}")]
fn given_state(world: &mut RaiseWorld, bet: i64, inc: i64) {
    world.current_bet = bet;
    world.last_raise_increment = inc;
}

#[when("I compute the min_raise_to")]
fn when_compute_min_raise(world: &mut RaiseWorld) {
    world.recompute();
}

#[when(expr = "a player raises to {int}")]
fn when_raise_to(world: &mut RaiseWorld, amt: i64) {
    let increment = amt - world.current_bet;
    if increment > world.last_raise_increment {
        world.last_raise_increment = increment;
    }
    world.current_bet = amt;
    world.recompute();
}

#[when(expr = "a player calls {int}")]
fn when_call(world: &mut RaiseWorld, _amt: i64) {
    world.recompute();
}

#[when(expr = "a below-increment raise of increment {int} is applied")]
fn when_below_increment_raise(world: &mut RaiseWorld, inc: i64) {
    if inc > world.last_raise_increment {
        world.last_raise_increment = inc;
    }
    world.recompute();
}

#[when(expr = "a player bets {int} on a new round")]
fn when_bet_new_round(world: &mut RaiseWorld, amt: i64) {
    let increment = amt - world.current_bet;
    if increment > world.last_raise_increment {
        world.last_raise_increment = increment;
    }
    world.current_bet = amt;
    world.recompute();
}

#[when(expr = "a player goes all-in to {int}")]
fn when_all_in_to(world: &mut RaiseWorld, amt: i64) {
    world.all_in_to = amt;
    world.recompute();
}

#[then(expr = "min_raise_to is {int}")]
fn then_min_raise_to(world: &mut RaiseWorld, expected: i64) {
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
    let min_raise_to = world.current_bet + world.last_raise_increment;
    assert!(
        world.all_in_to < min_raise_to,
        "all-in {} is not less than min_raise_to {}",
        world.all_in_to,
        min_raise_to
    );
}

#[tokio::main]
async fn main() {
    RaiseWorld::run("features/example/unit/raise_tracking.feature").await;
}
