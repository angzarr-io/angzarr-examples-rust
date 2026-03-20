//! RebuyOrchestrator state management.

use std::sync::LazyLock;

use angzarr_client::proto::EventBook;
use angzarr_client::StateRouter;
use examples_proto::{RebuyCompleted, RebuyFailed, RebuyInitiated, RebuyPhase, RebuyPhaseChanged};

/// PM state for tracking a rebuy flow.
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
    /// Check if this PM instance has been initialized.
    pub fn is_initialized(&self) -> bool {
        !self.reservation_id.is_empty()
    }
}

// Event appliers for StateRouter

fn apply_rebuy_initiated(state: &mut RebuyState, event: RebuyInitiated) {
    state.phase = event.phase();
    state.chips_to_add = event.chips_to_add;
    state.fee = event.fee.as_ref().map(|c| c.amount).unwrap_or(0);
    state.reservation_id = event.reservation_id;
    state.player_root = event.player_root;
    state.tournament_root = event.tournament_root;
    state.table_root = event.table_root;
    state.seat = event.seat;
}

fn apply_rebuy_phase_changed(state: &mut RebuyState, event: RebuyPhaseChanged) {
    state.phase = event.to_phase();
}

fn apply_rebuy_completed(state: &mut RebuyState, _event: RebuyCompleted) {
    state.phase = RebuyPhase::RebuyCompleted;
}

fn apply_rebuy_failed(state: &mut RebuyState, _event: RebuyFailed) {
    state.phase = RebuyPhase::RebuyFailed;
}

/// StateRouter for PM state reconstruction.
pub static STATE_ROUTER: LazyLock<StateRouter<RebuyState>> = LazyLock::new(|| {
    StateRouter::new()
        .on::<RebuyInitiated>(apply_rebuy_initiated)
        .on::<RebuyPhaseChanged>(apply_rebuy_phase_changed)
        .on::<RebuyCompleted>(apply_rebuy_completed)
        .on::<RebuyFailed>(apply_rebuy_failed)
});

/// Rebuild PM state from event history.
pub fn rebuild_state(event_book: &EventBook) -> RebuyState {
    STATE_ROUTER.with_event_book(event_book)
}
