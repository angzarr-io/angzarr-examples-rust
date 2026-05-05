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

/// Outcome of feeding one or more short all-ins through the tracker.
///
/// Per TDA Rule 47A second sentence: "If multiple short all-ins re-open
/// the betting, the minimum raise is always the last full valid bet or
/// raise of the round." The cumulative increment of consecutive short
/// all-ins is compared against the existing `last_raise_increment`; if it
/// reaches or exceeds the threshold, betting reopens for prior actors and
/// `last_raise_increment` is updated to the cumulative amount.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShortAllInOutcome {
    pub current_bet: i64,
    pub last_raise_increment: i64,
    pub cumulative_short: i64,
    pub action_reopened: bool,
}

/// Apply a single short all-in (whose absolute target is `new_bet`) to a
/// running tracker. The tracker carries `cumulative_short` across calls
/// to detect when consecutive shorts cumulatively reach a full raise.
///
/// Per TDA Rule 47A: if `new_bet - current_bet < last_raise_increment` it
/// is a "short" all-in. Otherwise the all-in is itself a full raise and
/// the tracker is updated normally (cumulative_short is reset).
pub fn apply_short_all_in(prior: ShortAllInOutcome, new_bet: i64) -> ShortAllInOutcome {
    let increment = new_bet - prior.current_bet;
    if increment >= prior.last_raise_increment {
        // Full raise — reopens action and resets the cumulative tracker.
        return ShortAllInOutcome {
            current_bet: new_bet,
            last_raise_increment: increment,
            cumulative_short: 0,
            action_reopened: true,
        };
    }
    // Short — accumulate. The threshold is the last_raise_increment that
    // existed BEFORE these shorts began, captured in `prior`.
    let cumulative = prior.cumulative_short + increment;
    if cumulative >= prior.last_raise_increment {
        ShortAllInOutcome {
            current_bet: new_bet,
            last_raise_increment: cumulative,
            cumulative_short: 0,
            action_reopened: true,
        }
    } else {
        ShortAllInOutcome {
            current_bet: new_bet,
            last_raise_increment: prior.last_raise_increment,
            cumulative_short: cumulative,
            action_reopened: false,
        }
    }
}

/// Per-round betting state at the start of a new street.
///
/// Per TDA Rule 47A "current betting round" wording: at the start of
/// every new street (post-flop, post-turn, post-river-redeal), the
/// minimum bet is the big blind and `current_bet` resets to 0. Carrying
/// preflop's `last_raise_increment` forward is a real-rule violation —
/// see `raise_tracking.feature` EU-1006/1011/1013.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreetResetState {
    pub current_bet: i64,
    pub last_raise_increment: i64,
}

/// Compute the per-round betting state at the start of a new street.
///
/// Per TDA Rule 47A: the minimum bet on a new street is the big blind,
/// and `current_bet` resets to 0. `last_raise_increment` becomes the BB
/// so the first BET must be at least the BB.
///
/// `big_blind` is permitted to be 0 (degenerate case: no BB established
/// yet, e.g. variant transitions before any blinds have been posted).
///
/// # Errors
/// Returns `Err` if `big_blind` is negative.
pub fn reset_per_round(big_blind: i64) -> Result<StreetResetState, String> {
    if big_blind < 0 {
        return Err(format!("big_blind must be non-negative, got {}", big_blind));
    }
    Ok(StreetResetState {
        current_bet: 0,
        last_raise_increment: big_blind,
    })
}

/// Convenience constructor for the initial tracker state (no shorts yet).
pub fn short_all_in_initial(current_bet: i64, last_raise_increment: i64) -> ShortAllInOutcome {
    ShortAllInOutcome {
        current_bet,
        last_raise_increment,
        cumulative_short: 0,
        action_reopened: false,
    }
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

    /// EU-1140: two shorts of +30 each cumulatively reach 60, exceeding the
    /// 50 threshold — action reopens, tracker = 60.
    #[test]
    fn cumulative_shorts_reaching_full_raise_reopen_action() {
        let s0 = short_all_in_initial(100, 50);
        let s1 = apply_short_all_in(s0, 130);
        assert!(!s1.action_reopened);
        assert_eq!(s1.cumulative_short, 30);
        let s2 = apply_short_all_in(s1, 160);
        assert!(s2.action_reopened);
        assert_eq!(s2.last_raise_increment, 60);
        assert_eq!(s2.current_bet, 160);
    }

    /// EU-1141: two shorts of +20 and +10 cumulatively reach 30, still
    /// below the 50 threshold — action does NOT reopen.
    #[test]
    fn cumulative_shorts_below_threshold_do_not_reopen() {
        let s0 = short_all_in_initial(100, 50);
        let s1 = apply_short_all_in(s0, 120);
        assert!(!s1.action_reopened);
        let s2 = apply_short_all_in(s1, 130);
        assert!(!s2.action_reopened);
        assert_eq!(s2.last_raise_increment, 50);
        assert_eq!(s2.current_bet, 130);
        assert_eq!(s2.cumulative_short, 30);
    }

    /// A single short followed by a full raise resets the cumulative tracker.
    #[test]
    fn full_raise_after_short_resets_cumulative() {
        let s0 = short_all_in_initial(100, 50);
        let s1 = apply_short_all_in(s0, 120); // +20 short
        let s2 = apply_short_all_in(s1, 200); // +80 full raise
        assert!(s2.action_reopened);
        assert_eq!(s2.cumulative_short, 0);
        assert_eq!(s2.last_raise_increment, 80);
    }

    // === reset_per_round (TDA Rule 47A — per-street reset) ===

    #[test]
    fn reset_per_round_returns_zero_current_bet_and_bb_increment() {
        let s = reset_per_round(10).unwrap();
        assert_eq!(
            s,
            StreetResetState {
                current_bet: 0,
                last_raise_increment: 10
            }
        );
    }

    #[test]
    fn reset_per_round_works_with_various_big_blinds() {
        assert_eq!(reset_per_round(1).unwrap().last_raise_increment, 1);
        assert_eq!(reset_per_round(25).unwrap().last_raise_increment, 25);
        assert_eq!(reset_per_round(10_000).unwrap().last_raise_increment, 10_000);
    }

    #[test]
    fn reset_per_round_current_bet_is_always_zero() {
        for bb in [1, 10, 100, 1_000_000] {
            assert_eq!(reset_per_round(bb).unwrap().current_bet, 0);
        }
    }

    #[test]
    fn reset_per_round_accepts_zero_big_blind_as_degenerate_case() {
        let s = reset_per_round(0).unwrap();
        assert_eq!(
            s,
            StreetResetState {
                current_bet: 0,
                last_raise_increment: 0
            }
        );
    }

    #[test]
    fn reset_per_round_rejects_negative_big_blind() {
        assert_eq!(
            reset_per_round(-5).unwrap_err(),
            "big_blind must be non-negative, got -5"
        );
        assert_eq!(
            reset_per_round(-1).unwrap_err(),
            "big_blind must be non-negative, got -1"
        );
    }

    #[test]
    fn reset_per_round_min_raise_to_after_reset_equals_big_blind() {
        let s = reset_per_round(10).unwrap();
        assert_eq!(min_raise_to(s.current_bet, s.last_raise_increment), 10);
        let s2 = reset_per_round(100).unwrap();
        assert_eq!(min_raise_to(s2.current_bet, s2.last_raise_increment), 100);
    }

    /// Boundary test: after a prior short (cumulative_short=20), an all-in
    /// whose increment EQUALS the threshold MUST be classified as a full
    /// raise. The full-raise branch sets last_raise_increment=increment
    /// (50), NOT cumulative (20+50=70). Distinguishes the full-raise
    /// branch from the cumulative-reopen branch — catches a `>= → >`
    /// mutation on the full-raise condition.
    #[test]
    fn full_raise_after_short_uses_increment_not_cumulative() {
        let s0 = short_all_in_initial(100, 50);
        let s1 = apply_short_all_in(s0, 120);
        assert_eq!(s1.cumulative_short, 20);
        let s2 = apply_short_all_in(s1, 170); // increment=50 == prior.lri (boundary)
        assert!(s2.action_reopened);
        assert_eq!(s2.cumulative_short, 0);
        // Full-raise branch returns lri=increment=50.
        // Mutated (`>`) would fall through to cumulative branch and
        // return lri=cumulative=70. So this test catches the mutation.
        assert_eq!(
            s2.last_raise_increment, 50,
            "boundary increment must use full-raise branch (lri=increment)"
        );
        assert_eq!(s2.current_bet, 170);
    }
}
