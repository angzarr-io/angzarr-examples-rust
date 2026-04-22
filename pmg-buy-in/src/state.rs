//! BuyIn PM state + event appliers.

use examples_proto::{BuyInCompleted, BuyInFailed, BuyInInitiated, BuyInPhase, BuyInPhaseChanged};

#[derive(Default, Clone, Debug)]
pub struct BuyInState {
    pub reservation_id: Vec<u8>,
    pub player_root: Vec<u8>,
    pub table_root: Vec<u8>,
    pub seat: i32,
    pub amount: i64,
    pub phase: BuyInPhase,
}

impl BuyInState {
    pub fn is_initialized(&self) -> bool {
        !self.reservation_id.is_empty()
    }
}

pub fn apply_buy_in_initiated(state: &mut BuyInState, event: BuyInInitiated) {
    state.phase = event.phase();
    state.amount = event.amount.as_ref().map(|c| c.amount).unwrap_or(0);
    state.reservation_id = event.reservation_id;
    state.player_root = event.player_root;
    state.table_root = event.table_root;
    state.seat = event.seat;
}

pub fn apply_buy_in_phase_changed(state: &mut BuyInState, event: BuyInPhaseChanged) {
    state.phase = event.to_phase();
}

pub fn apply_buy_in_completed(state: &mut BuyInState, _event: BuyInCompleted) {
    state.phase = BuyInPhase::BuyInCompleted;
}

pub fn apply_buy_in_failed(state: &mut BuyInState, _event: BuyInFailed) {
    state.phase = BuyInPhase::BuyInFailed;
}

#[cfg(test)]
mod tests {
    use super::*;
    use examples_proto::{Currency, OrchestrationFailure};

    fn sample_initiated() -> BuyInInitiated {
        BuyInInitiated {
            player_root: b"player-root-123".to_vec(),
            table_root: b"table-root-456".to_vec(),
            reservation_id: b"reservation-789".to_vec(),
            seat: 4,
            amount: Some(Currency {
                amount: 5_000,
                currency_code: "USD".to_string(),
            }),
            phase: BuyInPhase::BuyInSeating as i32,
            initiated_at: None,
        }
    }

    #[test]
    fn is_initialized_is_false_for_default_state() {
        let state = BuyInState::default();
        assert!(!state.is_initialized());
        assert!(state.reservation_id.is_empty());
    }

    #[test]
    fn is_initialized_is_true_once_reservation_id_is_set() {
        let state = BuyInState {
            reservation_id: b"not-empty".to_vec(),
            ..BuyInState::default()
        };
        assert!(state.is_initialized());
    }

    #[test]
    fn is_initialized_after_apply_buy_in_initiated() {
        let mut state = BuyInState::default();
        assert!(!state.is_initialized());
        apply_buy_in_initiated(&mut state, sample_initiated());
        assert!(state.is_initialized());
    }

    #[test]
    fn apply_buy_in_initiated_copies_all_fields() {
        let mut state = BuyInState::default();
        apply_buy_in_initiated(&mut state, sample_initiated());

        assert_eq!(state.reservation_id, b"reservation-789".to_vec());
        assert_eq!(state.player_root, b"player-root-123".to_vec());
        assert_eq!(state.table_root, b"table-root-456".to_vec());
        assert_eq!(state.seat, 4);
        assert_eq!(state.amount, 5_000);
        assert_eq!(state.phase, BuyInPhase::BuyInSeating);
    }

    #[test]
    fn apply_buy_in_initiated_uses_zero_when_amount_missing() {
        let mut state = BuyInState::default();
        let mut event = sample_initiated();
        event.amount = None;
        apply_buy_in_initiated(&mut state, event);

        assert_eq!(state.amount, 0);
        // Other fields still populated.
        assert_eq!(state.reservation_id, b"reservation-789".to_vec());
        assert_eq!(state.seat, 4);
        assert_eq!(state.phase, BuyInPhase::BuyInSeating);
    }

    #[test]
    fn apply_buy_in_phase_changed_updates_phase_only() {
        let mut state = BuyInState::default();
        apply_buy_in_initiated(&mut state, sample_initiated());
        assert_eq!(state.phase, BuyInPhase::BuyInSeating);

        let event = BuyInPhaseChanged {
            reservation_id: b"reservation-789".to_vec(),
            from_phase: BuyInPhase::BuyInSeating as i32,
            to_phase: BuyInPhase::BuyInConfirming as i32,
            changed_at: None,
        };
        apply_buy_in_phase_changed(&mut state, event);

        assert_eq!(state.phase, BuyInPhase::BuyInConfirming);
        // Other fields unchanged.
        assert_eq!(state.reservation_id, b"reservation-789".to_vec());
        assert_eq!(state.player_root, b"player-root-123".to_vec());
        assert_eq!(state.table_root, b"table-root-456".to_vec());
        assert_eq!(state.seat, 4);
        assert_eq!(state.amount, 5_000);
    }

    #[test]
    fn apply_buy_in_completed_sets_completed_phase() {
        let mut state = BuyInState::default();
        apply_buy_in_initiated(&mut state, sample_initiated());

        let event = BuyInCompleted {
            player_root: b"player-root-123".to_vec(),
            table_root: b"table-root-456".to_vec(),
            reservation_id: b"reservation-789".to_vec(),
            seat: 4,
            amount: Some(Currency {
                amount: 5_000,
                currency_code: "USD".to_string(),
            }),
            completed_at: None,
        };
        apply_buy_in_completed(&mut state, event);

        assert_eq!(state.phase, BuyInPhase::BuyInCompleted);
        // Unchanged fields survive the transition.
        assert_eq!(state.reservation_id, b"reservation-789".to_vec());
        assert_eq!(state.seat, 4);
        assert_eq!(state.amount, 5_000);
    }

    #[test]
    fn apply_buy_in_failed_sets_failed_phase() {
        let mut state = BuyInState::default();
        apply_buy_in_initiated(&mut state, sample_initiated());

        let event = BuyInFailed {
            player_root: b"player-root-123".to_vec(),
            table_root: b"table-root-456".to_vec(),
            reservation_id: b"reservation-789".to_vec(),
            failure: Some(OrchestrationFailure {
                code: "SEATING_REJECTED".to_string(),
                message: "seat occupied".to_string(),
                failed_at_phase: "SEATING".to_string(),
                failed_at: None,
            }),
        };
        apply_buy_in_failed(&mut state, event);

        assert_eq!(state.phase, BuyInPhase::BuyInFailed);
        // Unchanged fields survive the transition.
        assert_eq!(state.reservation_id, b"reservation-789".to_vec());
        assert_eq!(state.player_root, b"player-root-123".to_vec());
        assert_eq!(state.seat, 4);
    }
}
