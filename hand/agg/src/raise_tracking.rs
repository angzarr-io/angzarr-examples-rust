//! Raise-tracking arithmetic helpers.
//!
//! Pure-math helpers used by the production handler in
//! [`crate::handlers::player_action`] when validating raises and by client
//! UIs that need the same min-raise computation. The server's `min_raise`
//! tracking must agree with these functions or clients will submit raises
//! the server rejects.

/// Returns the minimum raise *target* (i.e. the min `RaiseTo` amount).
///
/// Equals `current_bet + last_raise_increment`. After SB/BB are posted in
/// no-limit Hold'em, `current_bet == last_raise_increment == BB`, so the
/// first legal raise is to `2 * BB`.
pub fn min_raise_to(current_bet: i64, last_raise_increment: i64) -> i64 {
    current_bet + last_raise_increment
}

/// Returns the updated `last_raise_increment` after a `RAISE` to `new_bet`.
///
/// Implements `max(old, new_increment)` semantics: a raise that exceeds the
/// previous increment grows the tracker; a (legal) raise that doesn't (e.g.
/// across a new betting round where `current_bet == 0`) leaves it intact
/// unless the new increment is larger.
///
/// `new_bet` must be the absolute target the player raised TO, not the delta.
pub fn next_last_raise_increment(current_bet: i64, last_raise_increment: i64, new_bet: i64) -> i64 {
    let increment = new_bet - current_bet;
    last_raise_increment.max(increment)
}

/// Returns the absolute amount a player goes all-in TO.
///
/// Adds the player's remaining stack to whatever they've already committed
/// this round. Useful both as a UI helper and as a semantic anchor for the
/// "all-in for less than min raise" check (compare against [`min_raise_to`]).
pub fn all_in_to(stack: i64, current_bet_for_player: i64) -> i64 {
    stack + current_bet_for_player
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn min_raise_to_after_blinds() {
        assert_eq!(min_raise_to(10, 10), 20);
    }

    #[test]
    fn next_increment_grows_when_larger() {
        assert_eq!(next_last_raise_increment(10, 10, 30), 20);
    }

    #[test]
    fn next_increment_holds_when_smaller() {
        assert_eq!(next_last_raise_increment(100, 50, 130), 50);
    }

    #[test]
    fn all_in_to_sums_stack_and_committed() {
        assert_eq!(all_in_to(40, 0), 40);
        assert_eq!(all_in_to(20, 30), 50);
    }
}
