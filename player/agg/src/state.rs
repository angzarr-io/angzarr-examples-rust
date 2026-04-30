//! Player aggregate state and event appliers.
//!
//! After the reservation refactor the player aggregate owns ONLY bankroll +
//! per-key reservations (table_root or tournament_root). Lifecycle records
//! (buy-in / rebuy / registration) live in `reservation/agg`.

use std::collections::HashMap;

use examples_proto::{
    FundsDeducted, FundsDeposited, FundsReleased, FundsReserved, FundsTransferred, FundsWithdrawn,
    PlayerRegistered, PlayerType,
};

/// Player aggregate state rebuilt from events.
#[derive(Debug, Default, Clone)]
pub struct PlayerState {
    pub player_id: String,
    pub display_name: String,
    pub email: String,
    pub player_type: PlayerType,
    pub ai_model_id: String,
    pub bankroll: i64,
    pub reserved_funds: i64,
    /// Reservations keyed by hex-encoded `key` bytes (table_root for buy-in / rebuy,
    /// tournament_root for registration). Values are amounts. Field name retained
    /// for on-disk compatibility with prior snapshots.
    pub table_reservations: HashMap<String, i64>,
    pub status: String,
}

impl PlayerState {
    pub fn exists(&self) -> bool {
        !self.player_id.is_empty()
    }

    pub fn available_balance(&self) -> i64 {
        self.bankroll - self.reserved_funds
    }

    pub fn is_ai(&self) -> bool {
        self.player_type == PlayerType::Ai
    }
}

// --- Event appliers ---

// docs:start:state_router
pub fn apply_registered(state: &mut PlayerState, event: PlayerRegistered) {
    state.player_id = format!("player_{}", event.email);
    state.display_name = event.display_name;
    state.email = event.email;
    state.player_type = PlayerType::try_from(event.player_type).unwrap_or_default();
    state.ai_model_id = event.ai_model_id;
    state.status = "active".to_string();
}

pub fn apply_deposited(state: &mut PlayerState, event: FundsDeposited) {
    if let Some(amount) = event.amount {
        state.bankroll += amount.amount;
    }
}

pub fn apply_withdrawn(state: &mut PlayerState, event: FundsWithdrawn) {
    if let Some(amount) = event.amount {
        state.bankroll -= amount.amount;
    }
}

pub fn apply_reserved(state: &mut PlayerState, event: FundsReserved) {
    let amount = event.amount.as_ref().map(|c| c.amount).unwrap_or(0);
    state.reserved_funds += amount;
    let key_hex = hex::encode(&event.key);
    *state.table_reservations.entry(key_hex).or_insert(0) += amount;
}

pub fn apply_released(state: &mut PlayerState, event: FundsReleased) {
    let amount = event.amount.as_ref().map(|c| c.amount).unwrap_or(0);
    state.reserved_funds -= amount;
    let key_hex = hex::encode(&event.key);
    state.table_reservations.remove(&key_hex);
}

pub fn apply_transferred(state: &mut PlayerState, event: FundsTransferred) {
    if let Some(amount) = event.amount {
        state.bankroll += amount.amount;
    }
}

pub fn apply_funds_deducted(state: &mut PlayerState, event: FundsDeducted) {
    let amount = event.amount.as_ref().map(|c| c.amount).unwrap_or(0);
    state.reserved_funds -= amount;
    state.bankroll -= amount;
    let key_hex = hex::encode(&event.key);
    state.table_reservations.remove(&key_hex);
}
// docs:end:state_router
