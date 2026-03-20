//! RegistrationOrchestrator state management.

use std::sync::LazyLock;

use angzarr_client::proto::EventBook;
use angzarr_client::StateRouter;
use examples_proto::{
    RegistrationCompleted, RegistrationFailed, RegistrationInitiated, RegistrationPhase,
    RegistrationPhaseChanged,
};

/// PM state for tracking a registration flow.
#[derive(Default, Clone, Debug)]
pub struct RegistrationState {
    pub reservation_id: Vec<u8>,
    pub player_root: Vec<u8>,
    pub tournament_root: Vec<u8>,
    pub fee: i64,
    pub phase: RegistrationPhase,
}

impl RegistrationState {
    /// Check if this PM instance has been initialized.
    pub fn is_initialized(&self) -> bool {
        !self.reservation_id.is_empty()
    }
}

// Event appliers for StateRouter

fn apply_registration_initiated(state: &mut RegistrationState, event: RegistrationInitiated) {
    state.phase = event.phase();
    state.fee = event.fee.as_ref().map(|c| c.amount).unwrap_or(0);
    state.reservation_id = event.reservation_id;
    state.player_root = event.player_root;
    state.tournament_root = event.tournament_root;
}

fn apply_registration_phase_changed(state: &mut RegistrationState, event: RegistrationPhaseChanged) {
    state.phase = event.to_phase();
}

fn apply_registration_completed(state: &mut RegistrationState, _event: RegistrationCompleted) {
    state.phase = RegistrationPhase::RegistrationCompleted;
}

fn apply_registration_failed(state: &mut RegistrationState, _event: RegistrationFailed) {
    state.phase = RegistrationPhase::RegistrationFailed;
}

/// StateRouter for PM state reconstruction.
pub static STATE_ROUTER: LazyLock<StateRouter<RegistrationState>> = LazyLock::new(|| {
    StateRouter::new()
        .on::<RegistrationInitiated>(apply_registration_initiated)
        .on::<RegistrationPhaseChanged>(apply_registration_phase_changed)
        .on::<RegistrationCompleted>(apply_registration_completed)
        .on::<RegistrationFailed>(apply_registration_failed)
});

/// Rebuild PM state from event history.
pub fn rebuild_state(event_book: &EventBook) -> RegistrationState {
    STATE_ROUTER.with_event_book(event_book)
}
