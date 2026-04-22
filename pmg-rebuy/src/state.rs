//! Rebuy PM state + event appliers.

use examples_proto::{RebuyCompleted, RebuyFailed, RebuyInitiated, RebuyPhase, RebuyPhaseChanged};

#[derive(Default, Clone, Debug)]
pub struct RebuyState {
    pub reservation_id: Vec<u8>,
    pub player_root: Vec<u8>,
    pub tournament_root: Vec<u8>,
    pub table_root: Vec<u8>,
    pub seat: i32,
    pub fee: i64,
    pub chips_to_add: i64,
    pub phase: RebuyPhase,
}

impl RebuyState {
    pub fn is_initialized(&self) -> bool {
        !self.reservation_id.is_empty()
    }
}

pub fn apply_rebuy_initiated(state: &mut RebuyState, event: RebuyInitiated) {
    state.phase = event.phase();
    state.chips_to_add = event.chips_to_add;
    state.fee = event.fee.as_ref().map(|c| c.amount).unwrap_or(0);
    state.reservation_id = event.reservation_id;
    state.player_root = event.player_root;
    state.tournament_root = event.tournament_root;
    state.table_root = event.table_root;
    state.seat = event.seat;
}

pub fn apply_rebuy_phase_changed(state: &mut RebuyState, event: RebuyPhaseChanged) {
    state.phase = event.to_phase();
}

pub fn apply_rebuy_completed(state: &mut RebuyState, _event: RebuyCompleted) {
    state.phase = RebuyPhase::RebuyCompleted;
}

pub fn apply_rebuy_failed(state: &mut RebuyState, _event: RebuyFailed) {
    state.phase = RebuyPhase::RebuyFailed;
}

#[cfg(test)]
mod tests {
    use super::*;
    use examples_proto::{Currency, OrchestrationFailure};

    fn sample_initiated() -> RebuyInitiated {
        RebuyInitiated {
            player_root: vec![0x01, 0x02],
            tournament_root: vec![0x03, 0x04],
            table_root: vec![0x05, 0x06],
            reservation_id: vec![0xAA, 0xBB, 0xCC],
            seat: 4,
            fee: Some(Currency {
                amount: 250,
                currency_code: "USD".to_string(),
            }),
            chips_to_add: 1000,
            phase: RebuyPhase::RebuyApproving as i32,
            initiated_at: None,
        }
    }

    #[test]
    fn is_initialized_false_for_default() {
        let state = RebuyState::default();
        assert!(!state.is_initialized());
        assert!(state.reservation_id.is_empty());
    }

    #[test]
    fn is_initialized_true_when_reservation_id_set() {
        let state = RebuyState {
            reservation_id: vec![0x01],
            ..RebuyState::default()
        };
        assert!(state.is_initialized());
    }

    #[test]
    fn is_initialized_false_when_reservation_id_cleared() {
        let mut state = RebuyState {
            reservation_id: vec![0x01],
            ..RebuyState::default()
        };
        assert!(state.is_initialized());
        state.reservation_id.clear();
        assert!(!state.is_initialized());
    }

    #[test]
    fn apply_rebuy_initiated_populates_all_fields() {
        let mut state = RebuyState::default();
        apply_rebuy_initiated(&mut state, sample_initiated());

        assert_eq!(state.phase, RebuyPhase::RebuyApproving);
        assert_eq!(state.chips_to_add, 1000);
        assert_eq!(state.fee, 250);
        assert_eq!(state.reservation_id, vec![0xAA, 0xBB, 0xCC]);
        assert_eq!(state.player_root, vec![0x01, 0x02]);
        assert_eq!(state.tournament_root, vec![0x03, 0x04]);
        assert_eq!(state.table_root, vec![0x05, 0x06]);
        assert_eq!(state.seat, 4);
        assert!(state.is_initialized());
    }

    #[test]
    fn apply_rebuy_initiated_fee_defaults_to_zero_when_absent() {
        let mut state = RebuyState::default();
        let mut event = sample_initiated();
        event.fee = None;
        apply_rebuy_initiated(&mut state, event);

        assert_eq!(state.fee, 0);
        // Other fields still populated.
        assert_eq!(state.seat, 4);
        assert!(state.is_initialized());
    }

    #[test]
    fn apply_rebuy_phase_changed_updates_only_phase() {
        let mut state = RebuyState {
            reservation_id: vec![0xAA],
            seat: 7,
            fee: 500,
            chips_to_add: 2000,
            phase: RebuyPhase::RebuyApproving,
            ..RebuyState::default()
        };
        let event = RebuyPhaseChanged {
            reservation_id: vec![0xAA],
            from_phase: RebuyPhase::RebuyApproving as i32,
            to_phase: RebuyPhase::RebuyAddingChips as i32,
            changed_at: None,
        };
        apply_rebuy_phase_changed(&mut state, event);

        assert_eq!(state.phase, RebuyPhase::RebuyAddingChips);
        // All other fields preserved.
        assert_eq!(state.reservation_id, vec![0xAA]);
        assert_eq!(state.seat, 7);
        assert_eq!(state.fee, 500);
        assert_eq!(state.chips_to_add, 2000);
    }

    #[test]
    fn apply_rebuy_completed_sets_completed_phase() {
        let mut state = RebuyState {
            reservation_id: vec![0xAA],
            phase: RebuyPhase::RebuyAddingChips,
            ..RebuyState::default()
        };
        let event = RebuyCompleted {
            player_root: vec![],
            tournament_root: vec![],
            table_root: vec![],
            reservation_id: vec![0xAA],
            fee: None,
            chips_added: 1000,
            completed_at: None,
        };
        apply_rebuy_completed(&mut state, event);

        assert_eq!(state.phase, RebuyPhase::RebuyCompleted);
        // Other state preserved.
        assert_eq!(state.reservation_id, vec![0xAA]);
    }

    #[test]
    fn apply_rebuy_failed_sets_failed_phase() {
        let mut state = RebuyState {
            reservation_id: vec![0xAA],
            phase: RebuyPhase::RebuyApproving,
            ..RebuyState::default()
        };
        let event = RebuyFailed {
            player_root: vec![],
            tournament_root: vec![],
            reservation_id: vec![0xAA],
            failure: Some(OrchestrationFailure {
                code: "REBUY_DENIED".to_string(),
                message: "nope".to_string(),
                failed_at_phase: "APPROVING".to_string(),
                failed_at: None,
            }),
        };
        apply_rebuy_failed(&mut state, event);

        assert_eq!(state.phase, RebuyPhase::RebuyFailed);
        assert_eq!(state.reservation_id, vec![0xAA]);
    }
}
