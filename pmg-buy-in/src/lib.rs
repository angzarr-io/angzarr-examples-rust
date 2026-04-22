//! BuyIn Process Manager library.

pub mod handler;
pub mod state;

pub use state::BuyInState;

use angzarr_client::proto::ProcessManagerHandleResponse;
use angzarr_client::{process_manager, CommandResult};
use examples_proto::{
    BuyInCompleted, BuyInFailed, BuyInInitiated, BuyInPhaseChanged, BuyInRequested, PlayerSeated,
    SeatingRejected,
};

use crate::state::{
    apply_buy_in_completed, apply_buy_in_failed, apply_buy_in_initiated,
    apply_buy_in_phase_changed,
};

pub struct BuyInPm;

#[process_manager(
    name = "pmg-buy-in",
    pm_domain = "pmg-buy-in",
    sources = ["player", "table"],
    targets = ["table", "player"],
    state = BuyInState
)]
impl BuyInPm {
    // --- Event handlers ---

    #[handles(BuyInRequested)]
    fn on_buy_in_requested(
        &self,
        event: BuyInRequested,
        _state: &BuyInState,
    ) -> CommandResult<ProcessManagerHandleResponse> {
        handler::handle_buy_in_requested(event)
    }

    #[handles(PlayerSeated)]
    fn on_player_seated(
        &self,
        event: PlayerSeated,
        _state: &BuyInState,
    ) -> CommandResult<ProcessManagerHandleResponse> {
        handler::handle_player_seated(event)
    }

    #[handles(SeatingRejected)]
    fn on_seating_rejected(
        &self,
        event: SeatingRejected,
        _state: &BuyInState,
    ) -> CommandResult<ProcessManagerHandleResponse> {
        handler::handle_seating_rejected(event)
    }

    // --- Event appliers ---

    #[applies(BuyInInitiated)]
    fn apply_initiated(state: &mut BuyInState, event: BuyInInitiated) {
        apply_buy_in_initiated(state, event);
    }

    #[applies(BuyInPhaseChanged)]
    fn apply_phase_changed(state: &mut BuyInState, event: BuyInPhaseChanged) {
        apply_buy_in_phase_changed(state, event);
    }

    #[applies(BuyInCompleted)]
    fn apply_completed(state: &mut BuyInState, event: BuyInCompleted) {
        apply_buy_in_completed(state, event);
    }

    #[applies(BuyInFailed)]
    fn apply_failed(state: &mut BuyInState, event: BuyInFailed) {
        apply_buy_in_failed(state, event);
    }
}

#[cfg(test)]
mod applier_tests {
    //! Direct-call tests against the `#[applies]` methods on `BuyInPm`. These
    //! protect the delegation bodies from being silently replaced by `()` by
    //! `cargo-mutants`: if the body is stubbed out, these asserts will fail.
    use super::*;
    use examples_proto::{
        BuyInCompleted, BuyInFailed, BuyInInitiated, BuyInPhase, BuyInPhaseChanged, Currency,
        OrchestrationFailure,
    };

    #[test]
    fn apply_initiated_delegates_to_state_fn() {
        let mut state = BuyInState::default();
        BuyInPm::apply_initiated(
            &mut state,
            BuyInInitiated {
                player_root: vec![1],
                table_root: vec![2],
                reservation_id: vec![0xaa, 0xbb],
                seat: 5,
                amount: Some(Currency {
                    amount: 100,
                    currency_code: "USD".into(),
                }),
                phase: BuyInPhase::BuyInSeating as i32,
                initiated_at: None,
            },
        );
        assert_eq!(state.reservation_id, vec![0xaa, 0xbb]);
        assert_eq!(state.seat, 5);
        assert_eq!(state.amount, 100);
        assert_eq!(state.phase, BuyInPhase::BuyInSeating);
    }

    #[test]
    fn apply_phase_changed_delegates_to_state_fn() {
        let mut state = BuyInState {
            reservation_id: vec![0xaa],
            phase: BuyInPhase::BuyInSeating,
            ..BuyInState::default()
        };
        BuyInPm::apply_phase_changed(
            &mut state,
            BuyInPhaseChanged {
                reservation_id: vec![0xaa],
                from_phase: BuyInPhase::BuyInSeating as i32,
                to_phase: BuyInPhase::BuyInConfirming as i32,
                changed_at: None,
            },
        );
        assert_eq!(state.phase, BuyInPhase::BuyInConfirming);
    }

    #[test]
    fn apply_completed_delegates_to_state_fn() {
        let mut state = BuyInState {
            reservation_id: vec![0xaa],
            phase: BuyInPhase::BuyInConfirming,
            ..BuyInState::default()
        };
        BuyInPm::apply_completed(
            &mut state,
            BuyInCompleted {
                player_root: vec![],
                table_root: vec![],
                reservation_id: vec![0xaa],
                seat: 0,
                amount: None,
                completed_at: None,
            },
        );
        assert_eq!(state.phase, BuyInPhase::BuyInCompleted);
    }

    #[test]
    fn apply_failed_delegates_to_state_fn() {
        let mut state = BuyInState {
            reservation_id: vec![0xaa],
            phase: BuyInPhase::BuyInConfirming,
            ..BuyInState::default()
        };
        BuyInPm::apply_failed(
            &mut state,
            BuyInFailed {
                player_root: vec![],
                table_root: vec![],
                reservation_id: vec![0xaa],
                failure: Some(OrchestrationFailure {
                    code: "SEATING_REJECTED".into(),
                    message: "nope".into(),
                    failed_at_phase: "SEATING".into(),
                    failed_at: None,
                }),
            },
        );
        assert_eq!(state.phase, BuyInPhase::BuyInFailed);
    }
}

#[cfg(test)]
mod handler_tests {
    //! Direct-call tests against the `#[handles]` methods on `BuyInPm`. These
    //! guard against `cargo-mutants` replacing the delegation with a default
    //! `ProcessManagerHandleResponse` — we assert the returned response carries
    //! concrete content (a command page, a process event).
    use super::*;
    use examples_proto::{BuyInRequested, Currency, PlayerSeated, SeatingRejected};

    #[test]
    fn on_buy_in_requested_delegates_to_handler() {
        let pm = BuyInPm;
        let state = BuyInState::default();
        let response = pm
            .on_buy_in_requested(
                BuyInRequested {
                    reservation_id: b"res".to_vec(),
                    table_root: b"tbl".to_vec(),
                    seat: 3,
                    amount: Some(Currency {
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
            "table"
        );
        assert!(response.process_events.is_some());
    }

    #[test]
    fn on_player_seated_delegates_to_handler() {
        let pm = BuyInPm;
        let state = BuyInState::default();
        let response = pm
            .on_player_seated(
                PlayerSeated {
                    player_root: b"p".to_vec(),
                    reservation_id: b"r".to_vec(),
                    seat_position: 1,
                    stack: 1000,
                    seated_at: None,
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
    fn on_seating_rejected_delegates_to_handler() {
        let pm = BuyInPm;
        let state = BuyInState::default();
        let response = pm
            .on_seating_rejected(
                SeatingRejected {
                    player_root: b"p".to_vec(),
                    reservation_id: b"r".to_vec(),
                    requested_seat: 2,
                    reason: "seat_occupied".into(),
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
