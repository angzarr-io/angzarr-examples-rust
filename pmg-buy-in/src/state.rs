//! BuyInOrchestrator state management.

use std::sync::LazyLock;

use angzarr_client::proto::EventBook;
use angzarr_client::StateRouter;
use examples_proto::{
    BuyInCompleted, BuyInFailed, BuyInInitiated, BuyInPhase, BuyInPhaseChanged,
};

/// PM state for tracking a buy-in flow.
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
    /// Check if this PM instance has been initialized.
    pub fn is_initialized(&self) -> bool {
        !self.reservation_id.is_empty()
    }
}

// Event appliers for StateRouter

fn apply_buy_in_initiated(state: &mut BuyInState, event: BuyInInitiated) {
    state.phase = event.phase();
    state.amount = event.amount.as_ref().map(|c| c.amount).unwrap_or(0);
    state.reservation_id = event.reservation_id;
    state.player_root = event.player_root;
    state.table_root = event.table_root;
    state.seat = event.seat;
}

fn apply_buy_in_phase_changed(state: &mut BuyInState, event: BuyInPhaseChanged) {
    state.phase = event.to_phase();
}

fn apply_buy_in_completed(state: &mut BuyInState, _event: BuyInCompleted) {
    state.phase = BuyInPhase::BuyInCompleted;
}

fn apply_buy_in_failed(state: &mut BuyInState, _event: BuyInFailed) {
    state.phase = BuyInPhase::BuyInFailed;
}

/// StateRouter for PM state reconstruction.
pub static STATE_ROUTER: LazyLock<StateRouter<BuyInState>> = LazyLock::new(|| {
    StateRouter::new()
        .on::<BuyInInitiated>(apply_buy_in_initiated)
        .on::<BuyInPhaseChanged>(apply_buy_in_phase_changed)
        .on::<BuyInCompleted>(apply_buy_in_completed)
        .on::<BuyInFailed>(apply_buy_in_failed)
});

/// Rebuild PM state from event history.
pub fn rebuild_state(event_book: &EventBook) -> BuyInState {
    STATE_ROUTER.with_event_book(event_book)
}
