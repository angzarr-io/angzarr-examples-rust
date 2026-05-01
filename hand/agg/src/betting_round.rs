//! Betting-round iteration driver.
//!
//! Codifies the "raise reopens action; round ends when all active matched
//! the new bet OR the last aggressor went all-in" semantics. Runs against
//! a data-only `[SeatState]` slice and a scripted action stream so it can
//! be exercised by gherkin and (eventually) be invoked by the hand-flow PM
//! with a live action source.

use crate::raise_tracking::min_raise_to;

/// Action variants the driver recognises.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Action {
    Fold,
    Check,
    Call,
    Bet,
    Raise,
}

/// Mutable per-seat state the driver tracks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeatState {
    pub seat: i32,
    pub stack: i64,
    pub bet: i64,
    pub folded: bool,
    pub all_in: bool,
}

impl SeatState {
    pub fn new(seat: i32, stack: i64) -> Self {
        Self {
            seat,
            stack,
            bet: 0,
            folded: false,
            all_in: false,
        }
    }
}

/// One scripted intent. The driver may rewrite `(action, amount)` to match
/// the legal-action invariants below before applying it.
pub type ScriptedAction = (Action, i64);

/// Result returned by [`betting_round`].
#[derive(Debug, Clone, Default)]
pub struct BettingRoundResult {
    pub seats_asked: Vec<i32>,
    pub seats: Vec<SeatState>,
    pub current_bet: i64,
    pub pot_delta: i64,
}

/// Run a single betting round.
///
/// `seats` is the table in seat-order (clockwise) including folded/all-in
/// players (they're skipped, but their seat numbers anchor the rotation).
/// `actions` is the scripted player intent stream — consumed in order, one
/// per seat asked. When exhausted the driver folds.
///
/// `last_raise_increment` is the running tracker (BB at the start of a
/// preflop round). It is updated by raises via
/// [`crate::raise_tracking::next_last_raise_increment`].
///
/// Returns the seats asked to act in iteration order plus the post-round
/// seat state.
pub fn betting_round(
    starting_seat: i32,
    seats: &[SeatState],
    current_bet: i64,
    last_raise_increment: i64,
    actions: &[ScriptedAction],
    preflop: bool,
) -> BettingRoundResult {
    let mut seats: Vec<SeatState> = seats.to_vec();
    seats.sort_by_key(|s| s.seat);
    let all_seats: Vec<i32> = seats.iter().map(|s| s.seat).collect();

    let mut current_bet = current_bet;
    let mut last_raise_increment = last_raise_increment;
    let mut pot_delta: i64 = 0;
    let mut seats_asked: Vec<i32> = Vec::new();

    let active_count =
        |seats: &[SeatState]| seats.iter().filter(|s| !s.folded && !s.all_in).count();

    if active_count(&seats) < 2 {
        return BettingRoundResult {
            seats_asked,
            seats,
            current_bet,
            pot_delta,
        };
    }

    if !preflop {
        for s in seats.iter_mut() {
            s.bet = 0;
        }
        current_bet = 0;
    }

    // Pick first to act (skip if folded/all-in).
    let mut current_seat = if seats
        .iter()
        .any(|s| s.seat == starting_seat && !s.folded && !s.all_in)
    {
        starting_seat
    } else {
        seats
            .iter()
            .find(|s| !s.folded && !s.all_in)
            .map(|s| s.seat)
            .expect("active >=2")
    };

    let next_active = |seats: &[SeatState], current: i32| -> Option<i32> {
        let idx = all_seats.iter().position(|&s| s == current)?;
        for i in 1..=all_seats.len() {
            let cand = all_seats[(idx + i) % all_seats.len()];
            if let Some(p) = seats.iter().find(|p| p.seat == cand) {
                if !p.folded && !p.all_in {
                    return Some(cand);
                }
            }
        }
        None
    };

    let mut acted: std::collections::HashSet<i32> = std::collections::HashSet::new();
    let mut last_aggressor: Option<i32> = None;
    let mut script_idx: usize = 0;

    loop {
        let p = seats
            .iter()
            .find(|s| s.seat == current_seat)
            .cloned()
            .expect("seat in table");
        if p.folded || p.all_in {
            match next_active(&seats, current_seat) {
                Some(s) => {
                    current_seat = s;
                    continue;
                }
                None => break,
            }
        }

        if active_count(&seats) <= 1 {
            break;
        }

        let active_seats: Vec<i32> = seats
            .iter()
            .filter(|s| !s.folded && !s.all_in)
            .map(|s| s.seat)
            .collect();

        let all_bets_matched = active_seats
            .iter()
            .all(|s| seats.iter().find(|p| p.seat == *s).unwrap().bet == current_bet);

        let effective_aggressor = match last_aggressor {
            Some(a) if active_seats.contains(&a) => Some(a),
            _ => None,
        };

        if acted.contains(&current_seat)
            && all_bets_matched
            && (effective_aggressor.is_none() || Some(current_seat) == effective_aggressor)
        {
            break;
        }

        seats_asked.push(current_seat);

        let (mut action, mut amount) = if script_idx < actions.len() {
            let a = actions[script_idx];
            script_idx += 1;
            a
        } else {
            (Action::Fold, 0)
        };

        let to_call = (current_bet - p.bet).max(0);

        if action == Action::Call && to_call == 0 {
            action = Action::Check;
            amount = 0;
        }
        if action == Action::Check && to_call > 0 {
            action = Action::Call;
            amount = to_call;
        }
        if action == Action::Bet && current_bet > 0 {
            action = Action::Raise;
        }

        if action == Action::Raise {
            let mr = min_raise_to(current_bet, last_raise_increment);
            if amount < mr {
                if to_call > 0 && p.stack >= to_call {
                    action = Action::Call;
                    amount = to_call;
                } else if to_call == 0 {
                    action = Action::Check;
                    amount = 0;
                } else {
                    action = Action::Fold;
                    amount = 0;
                }
            }
        }

        if matches!(action, Action::Bet | Action::Raise) {
            let max_bet = p.stack + p.bet;
            if amount > max_bet {
                amount = max_bet;
            }
            let mr = min_raise_to(current_bet, last_raise_increment);
            if action == Action::Raise && amount < mr {
                if to_call > 0 && p.stack >= to_call {
                    action = Action::Call;
                    amount = to_call;
                } else if p.stack > 0 {
                    action = Action::Call;
                    amount = p.stack;
                } else {
                    action = Action::Fold;
                    amount = 0;
                }
            }
        }

        if action == Action::Call && to_call > p.stack {
            amount = p.stack;
        }

        // Apply.
        let pi = seats.iter().position(|s| s.seat == current_seat).unwrap();
        match action {
            Action::Fold => {
                seats[pi].folded = true;
            }
            Action::Check => {}
            Action::Call => {
                let call_amount = (current_bet - seats[pi].bet).min(seats[pi].stack);
                seats[pi].stack -= call_amount;
                seats[pi].bet += call_amount;
                pot_delta += call_amount;
                if seats[pi].stack == 0 {
                    seats[pi].all_in = true;
                }
            }
            Action::Bet | Action::Raise => {
                let bet_delta = amount - seats[pi].bet;
                seats[pi].stack -= bet_delta;
                seats[pi].bet = amount;
                pot_delta += bet_delta;
                let raise_inc = amount - current_bet;
                if raise_inc > last_raise_increment {
                    last_raise_increment = raise_inc;
                }
                current_bet = amount;
                last_aggressor = Some(current_seat);
                if seats[pi].stack == 0 {
                    seats[pi].all_in = true;
                }
            }
        }

        acted.insert(current_seat);

        match next_active(&seats, current_seat) {
            Some(s) => current_seat = s,
            None => break,
        }
        if active_count(&seats) < 2 {
            break;
        }
    }

    BettingRoundResult {
        seats_asked,
        seats,
        current_bet,
        pot_delta,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raise_reopens_action_for_blinds() {
        let mut seats: Vec<SeatState> = (0..6).map(|i| SeatState::new(i, 1000)).collect();
        // Blinds posted: seat 1 SB=5, seat 2 BB=10.
        seats[1].bet = 5;
        seats[1].stack -= 5;
        seats[2].bet = 10;
        seats[2].stack -= 10;
        let result = betting_round(
            3,
            &seats,
            10,
            10,
            &[
                (Action::Fold, 0),
                (Action::Fold, 0),
                (Action::Raise, 48),
                (Action::Fold, 0),
                (Action::Call, 43),
                (Action::Call, 38),
            ],
            true,
        );
        assert_eq!(result.seats_asked, vec![3, 4, 5, 0, 1, 2]);
    }

    #[test]
    fn round_terminates_after_all_in_aggressor() {
        let seats = vec![
            {
                let mut s = SeatState::new(0, 1000);
                s.bet = 5;
                s.stack -= 5;
                s
            },
            {
                let mut s = SeatState::new(1, 1000);
                s.bet = 10;
                s.stack -= 10;
                s
            },
            SeatState::new(2, 27),
            SeatState::new(3, 1000),
        ];
        let result = betting_round(
            2,
            &seats,
            10,
            10,
            &[
                (Action::Raise, 27),
                (Action::Call, 27),
                (Action::Call, 22),
                (Action::Call, 17),
            ],
            true,
        );
        assert_eq!(result.seats_asked, vec![2, 3, 0, 1]);
        let s2 = result.seats.iter().find(|s| s.seat == 2).unwrap();
        assert!(s2.all_in);
        assert_eq!(s2.stack, 0);
    }
}
