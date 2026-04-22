//! Registration Process Manager library.

pub mod handler;
pub mod state;

pub use state::RegistrationState;

use angzarr_client::proto::ProcessManagerHandleResponse;
use angzarr_client::{process_manager, CommandResult};
use examples_proto::{
    RegistrationCompleted, RegistrationFailed, RegistrationInitiated, RegistrationPhaseChanged,
    RegistrationRequested, TournamentEnrollmentRejected, TournamentPlayerEnrolled,
};

use crate::state::{
    apply_registration_completed, apply_registration_failed, apply_registration_initiated,
    apply_registration_phase_changed,
};

pub struct RegistrationPm;

#[process_manager(
    name = "pmg-registration",
    pm_domain = "pmg-registration",
    sources = ["player", "tournament"],
    targets = ["tournament", "player"],
    state = RegistrationState
)]
impl RegistrationPm {
    // --- Event handlers ---

    #[handles(RegistrationRequested)]
    fn on_registration_requested(
        &self,
        event: RegistrationRequested,
        _state: &RegistrationState,
    ) -> CommandResult<ProcessManagerHandleResponse> {
        handler::handle_registration_requested(event)
    }

    #[handles(TournamentPlayerEnrolled)]
    fn on_player_enrolled(
        &self,
        event: TournamentPlayerEnrolled,
        _state: &RegistrationState,
    ) -> CommandResult<ProcessManagerHandleResponse> {
        handler::handle_player_enrolled(event)
    }

    #[handles(TournamentEnrollmentRejected)]
    fn on_enrollment_rejected(
        &self,
        event: TournamentEnrollmentRejected,
        _state: &RegistrationState,
    ) -> CommandResult<ProcessManagerHandleResponse> {
        handler::handle_enrollment_rejected(event)
    }

    // --- Event appliers ---

    #[applies(RegistrationInitiated)]
    fn apply_initiated(state: &mut RegistrationState, event: RegistrationInitiated) {
        apply_registration_initiated(state, event);
    }

    #[applies(RegistrationPhaseChanged)]
    fn apply_phase_changed(state: &mut RegistrationState, event: RegistrationPhaseChanged) {
        apply_registration_phase_changed(state, event);
    }

    #[applies(RegistrationCompleted)]
    fn apply_completed(state: &mut RegistrationState, event: RegistrationCompleted) {
        apply_registration_completed(state, event);
    }

    #[applies(RegistrationFailed)]
    fn apply_failed(state: &mut RegistrationState, event: RegistrationFailed) {
        apply_registration_failed(state, event);
    }
}

#[cfg(test)]
mod applier_tests {
    //! Direct-call tests against the `#[applies]` methods on `RegistrationPm`.
    //! These guard against `cargo-mutants` replacing the delegation with `()`.
    use super::*;
    use examples_proto::{
        Currency, OrchestrationFailure, RegistrationCompleted, RegistrationFailed,
        RegistrationInitiated, RegistrationPhase, RegistrationPhaseChanged,
    };

    #[test]
    fn apply_initiated_delegates_to_state_fn() {
        let mut state = RegistrationState::default();
        RegistrationPm::apply_initiated(
            &mut state,
            RegistrationInitiated {
                player_root: vec![1, 2],
                tournament_root: vec![3, 4],
                reservation_id: vec![0xaa, 0xbb],
                fee: Some(Currency {
                    amount: 500,
                    currency_code: "USD".into(),
                }),
                phase: RegistrationPhase::RegistrationEnrolling as i32,
                initiated_at: None,
            },
        );
        assert_eq!(state.reservation_id, vec![0xaa, 0xbb]);
        assert_eq!(state.player_root, vec![1, 2]);
        assert_eq!(state.tournament_root, vec![3, 4]);
        assert_eq!(state.fee, 500);
        assert_eq!(state.phase, RegistrationPhase::RegistrationEnrolling);
    }

    #[test]
    fn apply_phase_changed_delegates_to_state_fn() {
        let mut state = RegistrationState {
            reservation_id: vec![0xaa],
            phase: RegistrationPhase::RegistrationEnrolling,
            ..RegistrationState::default()
        };
        RegistrationPm::apply_phase_changed(
            &mut state,
            RegistrationPhaseChanged {
                reservation_id: vec![0xaa],
                from_phase: RegistrationPhase::RegistrationEnrolling as i32,
                to_phase: RegistrationPhase::RegistrationConfirming as i32,
                changed_at: None,
            },
        );
        assert_eq!(state.phase, RegistrationPhase::RegistrationConfirming);
    }

    #[test]
    fn apply_completed_delegates_to_state_fn() {
        let mut state = RegistrationState {
            reservation_id: vec![0xaa],
            phase: RegistrationPhase::RegistrationConfirming,
            ..RegistrationState::default()
        };
        RegistrationPm::apply_completed(
            &mut state,
            RegistrationCompleted {
                player_root: vec![],
                tournament_root: vec![],
                reservation_id: vec![0xaa],
                fee: None,
                starting_stack: 0,
                completed_at: None,
            },
        );
        assert_eq!(state.phase, RegistrationPhase::RegistrationCompleted);
    }

    #[test]
    fn apply_failed_delegates_to_state_fn() {
        let mut state = RegistrationState {
            reservation_id: vec![0xaa],
            phase: RegistrationPhase::RegistrationEnrolling,
            ..RegistrationState::default()
        };
        RegistrationPm::apply_failed(
            &mut state,
            RegistrationFailed {
                player_root: vec![],
                tournament_root: vec![],
                reservation_id: vec![0xaa],
                failure: Some(OrchestrationFailure {
                    code: "ENROLLMENT_REJECTED".into(),
                    message: "nope".into(),
                    failed_at_phase: "ENROLLING".into(),
                    failed_at: None,
                }),
            },
        );
        assert_eq!(state.phase, RegistrationPhase::RegistrationFailed);
    }
}

#[cfg(test)]
mod handler_tests {
    //! Direct-call tests against the `#[handles]` methods on `RegistrationPm`.
    use super::*;
    use examples_proto::Currency;

    #[test]
    fn on_registration_requested_delegates_to_handler() {
        let pm = RegistrationPm;
        let state = RegistrationState::default();
        let response = pm
            .on_registration_requested(
                RegistrationRequested {
                    reservation_id: b"res".to_vec(),
                    tournament_root: b"tnmt".to_vec(),
                    fee: Some(Currency {
                        amount: 500,
                        currency_code: "USD".into(),
                    }),
                    requested_at: None,
                },
                &state,
            )
            .expect("handler should succeed");
        assert_eq!(response.commands.len(), 1);
        assert_eq!(
            response.commands[0].cover.as_ref().unwrap().domain,
            "tournament"
        );
        assert!(response.process_events.is_some());
    }

    #[test]
    fn on_player_enrolled_delegates_to_handler() {
        let pm = RegistrationPm;
        let state = RegistrationState::default();
        let response = pm
            .on_player_enrolled(
                TournamentPlayerEnrolled {
                    player_root: b"p".to_vec(),
                    reservation_id: b"r".to_vec(),
                    fee_paid: 300,
                    starting_stack: 10_000,
                    registration_number: 1,
                    enrolled_at: None,
                },
                &state,
            )
            .expect("handler should succeed");
        assert_eq!(response.commands.len(), 1);
        assert_eq!(
            response.commands[0].cover.as_ref().unwrap().domain,
            "player"
        );
        assert!(response.process_events.is_some());
    }

    #[test]
    fn on_enrollment_rejected_delegates_to_handler() {
        let pm = RegistrationPm;
        let state = RegistrationState::default();
        let response = pm
            .on_enrollment_rejected(
                TournamentEnrollmentRejected {
                    player_root: b"p".to_vec(),
                    reservation_id: b"r".to_vec(),
                    reason: "full".into(),
                    rejected_at: None,
                },
                &state,
            )
            .expect("handler should succeed");
        assert_eq!(response.commands.len(), 1);
        assert_eq!(
            response.commands[0].cover.as_ref().unwrap().domain,
            "player"
        );
        assert!(response.process_events.is_some());
    }
}
