//! Rebuy Process Manager library.

pub mod handler;
pub mod state;

pub use state::RebuyState;

use angzarr_client::proto::ProcessManagerHandleResponse;
use angzarr_client::{process_manager, CommandResult};
use examples_proto::{
    RebuyChipsAdded, RebuyCompleted, RebuyDenied, RebuyFailed, RebuyInitiated, RebuyPhaseChanged,
    RebuyProcessed, RebuyRequested,
};

use crate::state::{
    apply_rebuy_completed, apply_rebuy_failed, apply_rebuy_initiated, apply_rebuy_phase_changed,
};

pub struct RebuyPm;

#[process_manager(
    name = "pmg-rebuy",
    pm_domain = "pmg-rebuy",
    sources = ["player", "tournament", "table"],
    targets = ["tournament", "table", "player"],
    state = RebuyState
)]
impl RebuyPm {
    #[handles(RebuyRequested)]
    fn on_rebuy_requested(
        &self,
        event: RebuyRequested,
        _state: &RebuyState,
    ) -> CommandResult<ProcessManagerHandleResponse> {
        handler::handle_rebuy_requested(event)
    }

    #[handles(RebuyProcessed)]
    fn on_rebuy_processed(
        &self,
        event: RebuyProcessed,
        state: &RebuyState,
    ) -> CommandResult<ProcessManagerHandleResponse> {
        handler::handle_rebuy_processed(event, state)
    }

    #[handles(RebuyDenied)]
    fn on_rebuy_denied(
        &self,
        event: RebuyDenied,
        _state: &RebuyState,
    ) -> CommandResult<ProcessManagerHandleResponse> {
        handler::handle_rebuy_denied(event)
    }

    #[handles(RebuyChipsAdded)]
    fn on_chips_added(
        &self,
        event: RebuyChipsAdded,
        _state: &RebuyState,
    ) -> CommandResult<ProcessManagerHandleResponse> {
        handler::handle_chips_added(event)
    }

    // --- Event appliers ---

    #[applies(RebuyInitiated)]
    fn apply_initiated(state: &mut RebuyState, event: RebuyInitiated) {
        apply_rebuy_initiated(state, event);
    }

    #[applies(RebuyPhaseChanged)]
    fn apply_phase_changed(state: &mut RebuyState, event: RebuyPhaseChanged) {
        apply_rebuy_phase_changed(state, event);
    }

    #[applies(RebuyCompleted)]
    fn apply_completed(state: &mut RebuyState, event: RebuyCompleted) {
        apply_rebuy_completed(state, event);
    }

    #[applies(RebuyFailed)]
    fn apply_failed(state: &mut RebuyState, event: RebuyFailed) {
        apply_rebuy_failed(state, event);
    }
}

#[cfg(test)]
mod applier_tests {
    //! Direct-call tests against the `#[applies]` methods on `RebuyPm`.
    use super::*;
    use examples_proto::{Currency, OrchestrationFailure, RebuyPhase};

    #[test]
    fn apply_initiated_delegates_to_state_fn() {
        let mut state = RebuyState::default();
        RebuyPm::apply_initiated(
            &mut state,
            RebuyInitiated {
                player_root: vec![1],
                tournament_root: vec![2],
                table_root: vec![3],
                reservation_id: vec![0xaa],
                seat: 5,
                fee: Some(Currency {
                    amount: 200,
                    currency_code: "USD".into(),
                }),
                chips_to_add: 1000,
                phase: RebuyPhase::RebuyApproving as i32,
                initiated_at: None,
            },
        );
        assert_eq!(state.reservation_id, vec![0xaa]);
        assert_eq!(state.seat, 5);
        assert_eq!(state.fee, 200);
        assert_eq!(state.chips_to_add, 1000);
        assert_eq!(state.phase, RebuyPhase::RebuyApproving);
    }

    #[test]
    fn apply_phase_changed_delegates_to_state_fn() {
        let mut state = RebuyState {
            reservation_id: vec![0xaa],
            phase: RebuyPhase::RebuyApproving,
            ..RebuyState::default()
        };
        RebuyPm::apply_phase_changed(
            &mut state,
            RebuyPhaseChanged {
                reservation_id: vec![0xaa],
                from_phase: RebuyPhase::RebuyApproving as i32,
                to_phase: RebuyPhase::RebuyAddingChips as i32,
                changed_at: None,
            },
        );
        assert_eq!(state.phase, RebuyPhase::RebuyAddingChips);
    }

    #[test]
    fn apply_completed_delegates_to_state_fn() {
        let mut state = RebuyState {
            reservation_id: vec![0xaa],
            phase: RebuyPhase::RebuyAddingChips,
            ..RebuyState::default()
        };
        RebuyPm::apply_completed(
            &mut state,
            RebuyCompleted {
                player_root: vec![],
                tournament_root: vec![],
                table_root: vec![],
                reservation_id: vec![0xaa],
                fee: None,
                chips_added: 0,
                completed_at: None,
            },
        );
        assert_eq!(state.phase, RebuyPhase::RebuyCompleted);
    }

    #[test]
    fn apply_failed_delegates_to_state_fn() {
        let mut state = RebuyState {
            reservation_id: vec![0xaa],
            phase: RebuyPhase::RebuyApproving,
            ..RebuyState::default()
        };
        RebuyPm::apply_failed(
            &mut state,
            RebuyFailed {
                player_root: vec![],
                tournament_root: vec![],
                reservation_id: vec![0xaa],
                failure: Some(OrchestrationFailure {
                    code: "REBUY_DENIED".into(),
                    message: "nope".into(),
                    failed_at_phase: "APPROVING".into(),
                    failed_at: None,
                }),
            },
        );
        assert_eq!(state.phase, RebuyPhase::RebuyFailed);
    }
}

#[cfg(test)]
mod handler_tests {
    //! Direct-call tests against the `#[handles]` methods on `RebuyPm`.
    use super::*;
    use examples_proto::Currency;

    #[test]
    fn on_rebuy_requested_delegates_to_handler() {
        let pm = RebuyPm;
        let state = RebuyState::default();
        let response = pm
            .on_rebuy_requested(
                RebuyRequested {
                    reservation_id: b"r".to_vec(),
                    tournament_root: b"t".to_vec(),
                    table_root: b"tbl".to_vec(),
                    seat: 2,
                    fee: Some(Currency {
                        amount: 150,
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
    fn on_rebuy_processed_delegates_to_handler() {
        let pm = RebuyPm;
        let state = RebuyState {
            reservation_id: b"r".to_vec(),
            player_root: b"p".to_vec(),
            table_root: b"tbl".to_vec(),
            seat: 3,
            ..RebuyState::default()
        };
        let response = pm
            .on_rebuy_processed(
                RebuyProcessed {
                    player_root: b"p".to_vec(),
                    reservation_id: b"r".to_vec(),
                    rebuy_cost: 200,
                    chips_added: 500,
                    rebuy_count: 1,
                    processed_at: None,
                },
                &state,
            )
            .expect("handler should succeed");
        assert_eq!(response.commands.len(), 1);
        assert_eq!(response.commands[0].cover.as_ref().unwrap().domain, "table");
        // handle_rebuy_processed does NOT emit PM events.
        assert!(response.process_events.is_none());
    }

    #[test]
    fn on_rebuy_denied_delegates_to_handler() {
        let pm = RebuyPm;
        let state = RebuyState::default();
        let response = pm
            .on_rebuy_denied(
                RebuyDenied {
                    player_root: b"p".to_vec(),
                    reservation_id: b"r".to_vec(),
                    reason: "insufficient".into(),
                    denied_at: None,
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
    fn on_chips_added_delegates_to_handler() {
        let pm = RebuyPm;
        let state = RebuyState::default();
        let response = pm
            .on_chips_added(
                RebuyChipsAdded {
                    player_root: b"p".to_vec(),
                    reservation_id: b"r".to_vec(),
                    seat: 4,
                    amount: 500,
                    new_stack: 1500,
                    added_at: None,
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
