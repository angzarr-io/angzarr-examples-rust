//! Substantial Action detector — TDA Rule 36.
//!
//! Per TDA Rule 36 (2024):
//! > Substantial Action is either A) any 2 actions in turn, at least
//! > one of which puts chips in the pot (i.e. any 2 actions except 2
//! > checks or 2 folds) or B) any combination of 3 actions in turn
//! > (check, bet, raise, call, fold). Posted blinds do not count
//! > towards SA.
//!
//! Why this matters: Rule 35-D says a misdeal cannot be declared once
//! SA has occurred. Rule 38 likewise locks in a single burn-per-street
//! after SA. Several other rules use SA as a "point of no return" —
//! declaring a hand killed, refunding bets, etc. all gate on this state.
//!
//! Mirrors `examples-python/main/hand/agg/substantial_action.py`.

use examples_proto::ActionType;

/// Returns `true` if `actions` constitutes substantial action.
///
/// `actions` is the ordered sequence of action types for post-blind
/// actions on the current street. Posted blinds must NOT be included —
/// Rule 36 explicitly excludes them.
///
/// # Logic
/// * 0 or 1 actions: not yet SA.
/// * Exactly 2 actions: SA iff at least one is a chip action (Call,
///   Bet, Raise, AllIn). Two checks or two folds (or check+fold /
///   fold+check) are NOT SA.
/// * 3 or more actions: SA regardless of mix.
pub fn is_substantial_action(actions: &[ActionType]) -> bool {
    let n = actions.len();
    if n < 2 {
        return false;
    }
    if n >= 3 {
        return true;
    }
    // Exactly 2 actions: SA iff at least one is a chip action.
    actions.iter().any(|a| is_chip_action(*a))
}

/// Returns `true` if the action puts chips in the pot.
///
/// Posted blinds (`BlindPosted`) are NOT included here — they are not
/// `ActionType` values; they have their own event type. The Rule 36
/// "blinds don't count" exclusion is enforced by the caller passing
/// only post-blind actions to `is_substantial_action`.
fn is_chip_action(a: ActionType) -> bool {
    matches!(
        a,
        ActionType::Call | ActionType::Bet | ActionType::Raise | ActionType::AllIn
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    use ActionType::{AllIn, Bet, Call, Check, Fold, Raise};

    #[test]
    fn no_actions_is_not_sa() {
        assert!(!is_substantial_action(&[]));
    }

    #[test]
    fn one_action_is_not_sa() {
        assert!(!is_substantial_action(&[Fold]));
        assert!(!is_substantial_action(&[Check]));
        assert!(!is_substantial_action(&[Raise]));
        assert!(!is_substantial_action(&[Call]));
    }

    #[test]
    fn two_folds_is_not_sa() {
        assert!(!is_substantial_action(&[Fold, Fold]));
    }

    #[test]
    fn two_checks_is_not_sa() {
        assert!(!is_substantial_action(&[Check, Check]));
    }

    #[test]
    fn check_then_fold_is_not_sa() {
        assert!(!is_substantial_action(&[Check, Fold]));
    }

    #[test]
    fn fold_then_check_is_not_sa() {
        assert!(!is_substantial_action(&[Fold, Check]));
    }

    #[test]
    fn call_then_fold_is_sa() {
        assert!(is_substantial_action(&[Call, Fold]));
    }

    #[test]
    fn raise_then_fold_is_sa() {
        assert!(is_substantial_action(&[Raise, Fold]));
    }

    #[test]
    fn bet_then_call_is_sa() {
        assert!(is_substantial_action(&[Bet, Call]));
    }

    #[test]
    fn call_then_check_is_sa() {
        assert!(is_substantial_action(&[Call, Check]));
    }

    #[test]
    fn all_in_then_fold_is_sa() {
        assert!(is_substantial_action(&[AllIn, Fold]));
    }

    #[test]
    fn three_folds_is_sa() {
        assert!(is_substantial_action(&[Fold, Fold, Fold]));
    }

    #[test]
    fn three_checks_is_sa() {
        assert!(is_substantial_action(&[Check, Check, Check]));
    }

    #[test]
    fn check_check_fold_is_sa() {
        assert!(is_substantial_action(&[Check, Check, Fold]));
    }

    #[test]
    fn four_actions_is_sa_regardless_of_mix() {
        assert!(is_substantial_action(&[Check, Check, Check, Fold]));
        assert!(is_substantial_action(&[Fold, Fold, Fold, Fold]));
        assert!(is_substantial_action(&[Call, Check, Fold, Raise]));
    }

    #[test]
    fn single_chip_action_alone_is_not_sa() {
        // Solo BET / CALL / ALL_IN / RAISE — not yet SA per Rule 36
        // (waits for a SECOND action).
        assert!(!is_substantial_action(&[Bet]));
        assert!(!is_substantial_action(&[Call]));
        assert!(!is_substantial_action(&[AllIn]));
    }

    #[test]
    fn eu_1232_outline_table() {
        // EU-1232 — TDA Rule 36 — exhaustive outline cases.
        assert!(!is_substantial_action(&[Fold, Fold]));
        assert!(!is_substantial_action(&[Check, Check]));
        assert!(is_substantial_action(&[Fold, Fold, Fold]));
        assert!(is_substantial_action(&[Call, Fold]));
        assert!(!is_substantial_action(&[Raise]));
        assert!(is_substantial_action(&[Raise, Fold]));
    }

    // === is_chip_action — every variant of ActionType ===
    //
    // These pin the chip-action classification for each ActionType.
    // Catches mutations that misclassify a single action variant.

    #[test]
    fn fold_is_not_a_chip_action() {
        assert!(!is_chip_action(Fold));
    }

    #[test]
    fn check_is_not_a_chip_action() {
        assert!(!is_chip_action(Check));
    }

    #[test]
    fn call_is_a_chip_action() {
        assert!(is_chip_action(Call));
    }

    #[test]
    fn bet_is_a_chip_action() {
        assert!(is_chip_action(Bet));
    }

    #[test]
    fn raise_is_a_chip_action() {
        assert!(is_chip_action(Raise));
    }

    #[test]
    fn all_in_is_a_chip_action() {
        assert!(is_chip_action(AllIn));
    }
}
