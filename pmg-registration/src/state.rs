//! Registration PM state + event appliers.

use examples_proto::{
    RegistrationCompleted, RegistrationFailed, RegistrationInitiated, RegistrationPhase,
    RegistrationPhaseChanged,
};

#[derive(Default, Clone, Debug)]
pub struct RegistrationState {
    pub reservation_id: Vec<u8>,
    pub player_root: Vec<u8>,
    pub tournament_root: Vec<u8>,
    pub fee: i64,
    pub phase: RegistrationPhase,
}

impl RegistrationState {
    pub fn is_initialized(&self) -> bool {
        !self.reservation_id.is_empty()
    }
}

pub fn apply_registration_initiated(state: &mut RegistrationState, event: RegistrationInitiated) {
    state.phase = event.phase();
    state.fee = event.fee.as_ref().map(|c| c.amount).unwrap_or(0);
    state.reservation_id = event.reservation_id;
    state.player_root = event.player_root;
    state.tournament_root = event.tournament_root;
}

pub fn apply_registration_phase_changed(
    state: &mut RegistrationState,
    event: RegistrationPhaseChanged,
) {
    state.phase = event.to_phase();
}

pub fn apply_registration_completed(
    state: &mut RegistrationState,
    _event: RegistrationCompleted,
) {
    state.phase = RegistrationPhase::RegistrationCompleted;
}

pub fn apply_registration_failed(state: &mut RegistrationState, _event: RegistrationFailed) {
    state.phase = RegistrationPhase::RegistrationFailed;
}

#[cfg(test)]
mod tests {
    use super::*;
    use examples_proto::{Currency, OrchestrationFailure};

    fn reservation_bytes() -> Vec<u8> {
        vec![1, 2, 3, 4]
    }

    fn player_bytes() -> Vec<u8> {
        vec![9, 8, 7, 6]
    }

    fn tournament_bytes() -> Vec<u8> {
        vec![5, 5, 5, 5]
    }

    #[test]
    fn is_initialized_is_false_on_default_state() {
        let state = RegistrationState::default();
        assert!(!state.is_initialized());
        assert!(state.reservation_id.is_empty());
        assert_eq!(state.fee, 0);
        assert_eq!(state.phase, RegistrationPhase::Unspecified);
    }

    #[test]
    fn is_initialized_is_true_when_reservation_id_set() {
        let mut state = RegistrationState::default();
        state.reservation_id = reservation_bytes();
        assert!(state.is_initialized());
    }

    #[test]
    fn is_initialized_remains_false_when_other_fields_populated_but_reservation_empty() {
        let mut state = RegistrationState::default();
        state.player_root = player_bytes();
        state.tournament_root = tournament_bytes();
        state.fee = 500;
        state.phase = RegistrationPhase::RegistrationEnrolling;
        assert!(!state.is_initialized());
    }

    #[test]
    fn apply_registration_initiated_populates_every_field() {
        let mut state = RegistrationState::default();
        let event = RegistrationInitiated {
            player_root: player_bytes(),
            tournament_root: tournament_bytes(),
            reservation_id: reservation_bytes(),
            fee: Some(Currency {
                amount: 2500,
                currency_code: "USD".to_string(),
            }),
            phase: RegistrationPhase::RegistrationEnrolling as i32,
            initiated_at: None,
        };

        apply_registration_initiated(&mut state, event);

        assert!(state.is_initialized());
        assert_eq!(state.reservation_id, reservation_bytes());
        assert_eq!(state.player_root, player_bytes());
        assert_eq!(state.tournament_root, tournament_bytes());
        assert_eq!(state.fee, 2500);
        assert_eq!(state.phase, RegistrationPhase::RegistrationEnrolling);
    }

    #[test]
    fn apply_registration_initiated_treats_missing_fee_as_zero() {
        let mut state = RegistrationState::default();
        let event = RegistrationInitiated {
            player_root: player_bytes(),
            tournament_root: tournament_bytes(),
            reservation_id: reservation_bytes(),
            fee: None,
            phase: RegistrationPhase::RegistrationRequested as i32,
            initiated_at: None,
        };

        apply_registration_initiated(&mut state, event);

        assert_eq!(state.fee, 0);
        assert_eq!(state.phase, RegistrationPhase::RegistrationRequested);
        assert!(state.is_initialized());
    }

    #[test]
    fn apply_registration_initiated_with_unknown_phase_i32_falls_back_to_unspecified() {
        // Prost's generated `phase()` accessor returns Unspecified when the i32 is
        // not a recognized variant.
        let mut state = RegistrationState::default();
        let event = RegistrationInitiated {
            player_root: player_bytes(),
            tournament_root: tournament_bytes(),
            reservation_id: reservation_bytes(),
            fee: Some(Currency {
                amount: 10,
                currency_code: "USD".to_string(),
            }),
            phase: 9999, // unknown
            initiated_at: None,
        };

        apply_registration_initiated(&mut state, event);

        assert_eq!(state.phase, RegistrationPhase::Unspecified);
        assert_eq!(state.fee, 10);
    }

    #[test]
    fn apply_registration_phase_changed_updates_phase_only() {
        let mut state = RegistrationState {
            reservation_id: reservation_bytes(),
            player_root: player_bytes(),
            tournament_root: tournament_bytes(),
            fee: 1234,
            phase: RegistrationPhase::RegistrationRequested,
        };

        let event = RegistrationPhaseChanged {
            reservation_id: reservation_bytes(),
            from_phase: RegistrationPhase::RegistrationRequested as i32,
            to_phase: RegistrationPhase::RegistrationConfirming as i32,
            changed_at: None,
        };

        apply_registration_phase_changed(&mut state, event);

        assert_eq!(state.phase, RegistrationPhase::RegistrationConfirming);
        // other fields untouched
        assert_eq!(state.reservation_id, reservation_bytes());
        assert_eq!(state.player_root, player_bytes());
        assert_eq!(state.tournament_root, tournament_bytes());
        assert_eq!(state.fee, 1234);
    }

    #[test]
    fn apply_registration_completed_sets_completed_phase_and_preserves_other_fields() {
        let mut state = RegistrationState {
            reservation_id: reservation_bytes(),
            player_root: player_bytes(),
            tournament_root: tournament_bytes(),
            fee: 7777,
            phase: RegistrationPhase::RegistrationConfirming,
        };

        let event = RegistrationCompleted {
            player_root: player_bytes(),
            tournament_root: tournament_bytes(),
            reservation_id: reservation_bytes(),
            fee: Some(Currency {
                amount: 7777,
                currency_code: "USD".to_string(),
            }),
            starting_stack: 15000,
            completed_at: None,
        };

        apply_registration_completed(&mut state, event);

        assert_eq!(state.phase, RegistrationPhase::RegistrationCompleted);
        assert_eq!(state.reservation_id, reservation_bytes());
        assert_eq!(state.player_root, player_bytes());
        assert_eq!(state.tournament_root, tournament_bytes());
        assert_eq!(state.fee, 7777);
    }

    #[test]
    fn apply_registration_failed_sets_failed_phase_and_preserves_other_fields() {
        let mut state = RegistrationState {
            reservation_id: reservation_bytes(),
            player_root: player_bytes(),
            tournament_root: tournament_bytes(),
            fee: 3000,
            phase: RegistrationPhase::RegistrationEnrolling,
        };

        let event = RegistrationFailed {
            player_root: player_bytes(),
            tournament_root: tournament_bytes(),
            reservation_id: reservation_bytes(),
            failure: Some(OrchestrationFailure {
                code: "ENROLLMENT_REJECTED".to_string(),
                message: "tournament full".to_string(),
                failed_at_phase: "ENROLLING".to_string(),
                failed_at: None,
            }),
        };

        apply_registration_failed(&mut state, event);

        assert_eq!(state.phase, RegistrationPhase::RegistrationFailed);
        assert_eq!(state.reservation_id, reservation_bytes());
        assert_eq!(state.player_root, player_bytes());
        assert_eq!(state.tournament_root, tournament_bytes());
        assert_eq!(state.fee, 3000);
    }
}
