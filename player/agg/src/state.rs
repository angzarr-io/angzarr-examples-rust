//! Player aggregate state and event appliers.
//!
//! Pure data + free-function appliers. The live aggregate wiring
//! (`#[aggregate]` + `#[handles]`/`#[applies]`/`#[rejected]` methods) lives
//! in `main.rs`; appliers here are invoked from the generated `#[applies]`
//! methods so they remain reusable from tests or docs snippets.

use std::collections::HashMap;

use examples_proto::{
    BuyInConfirmed, BuyInRequested, BuyInReservationReleased, FundsDeposited, FundsReleased,
    FundsReserved, FundsTransferred, FundsWithdrawn, PlayerRegistered, PlayerType,
    RebuyFeeConfirmed, RebuyFeeReleased, RebuyRequested, RegistrationFeeConfirmed,
    RegistrationFeeReleased, RegistrationRequested,
};

/// Pending buy-in request.
#[derive(Debug, Clone, Default)]
pub struct PendingBuyIn {
    pub table_root: Vec<u8>,
    pub seat: i32,
    pub amount: i64,
}

/// Pending registration request.
#[derive(Debug, Clone, Default)]
pub struct PendingRegistration {
    pub tournament_root: Vec<u8>,
    pub fee: i64,
}

/// Pending rebuy request.
#[derive(Debug, Clone, Default)]
pub struct PendingRebuy {
    pub tournament_root: Vec<u8>,
    pub table_root: Vec<u8>,
    pub seat: i32,
    pub fee: i64,
    pub chips_to_add: i64,
}

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
    pub table_reservations: HashMap<String, i64>,
    pub status: String,
    pub pending_buy_ins: HashMap<String, PendingBuyIn>,
    pub pending_registrations: HashMap<String, PendingRegistration>,
    pub pending_rebuys: HashMap<String, PendingRebuy>,
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

// --- Core fund events ---

// docs:start:state_router
pub fn apply_registered(state: &mut PlayerState, event: PlayerRegistered) {
    state.player_id = format!("player_{}", event.email);
    state.display_name = event.display_name;
    state.email = event.email;
    state.player_type = PlayerType::try_from(event.player_type).unwrap_or_default();
    state.ai_model_id = event.ai_model_id;
    state.status = "active".to_string();
    state.bankroll = 0;
    state.reserved_funds = 0;
}

pub fn apply_deposited(state: &mut PlayerState, event: FundsDeposited) {
    if let Some(balance) = event.new_balance {
        state.bankroll = balance.amount;
    }
}

pub fn apply_withdrawn(state: &mut PlayerState, event: FundsWithdrawn) {
    if let Some(balance) = event.new_balance {
        state.bankroll = balance.amount;
    }
}

pub fn apply_reserved(state: &mut PlayerState, event: FundsReserved) {
    if let Some(balance) = event.new_reserved_balance {
        state.reserved_funds = balance.amount;
    }
    if let (Some(amount), table_root) = (event.amount, event.table_root) {
        let table_key = hex::encode(&table_root);
        state.table_reservations.insert(table_key, amount.amount);
    }
}

pub fn apply_released(state: &mut PlayerState, event: FundsReleased) {
    if let Some(balance) = event.new_reserved_balance {
        state.reserved_funds = balance.amount;
    }
    let table_key = hex::encode(&event.table_root);
    state.table_reservations.remove(&table_key);
}

pub fn apply_transferred(state: &mut PlayerState, event: FundsTransferred) {
    if let Some(balance) = event.new_balance {
        state.bankroll = balance.amount;
    }
}
// docs:end:state_router

// --- Buy-in orchestration ---

pub fn apply_buy_in_requested(state: &mut PlayerState, event: BuyInRequested) {
    let reservation_hex = hex::encode(&event.reservation_id);
    let amount = event.amount.as_ref().map(|c| c.amount).unwrap_or(0);

    state.reserved_funds += amount;

    state.pending_buy_ins.insert(
        reservation_hex,
        PendingBuyIn {
            table_root: event.table_root,
            seat: event.seat,
            amount,
        },
    );
}

pub fn apply_buy_in_confirmed(state: &mut PlayerState, event: BuyInConfirmed) {
    let reservation_hex = hex::encode(&event.reservation_id);

    if let Some(pending) = state.pending_buy_ins.remove(&reservation_hex) {
        state.reserved_funds -= pending.amount;
        let table_key = hex::encode(&pending.table_root);
        state.table_reservations.insert(table_key, pending.amount);
        state.bankroll -= pending.amount;
    }
}

pub fn apply_buy_in_released(state: &mut PlayerState, event: BuyInReservationReleased) {
    let reservation_hex = hex::encode(&event.reservation_id);

    if let Some(pending) = state.pending_buy_ins.remove(&reservation_hex) {
        state.reserved_funds -= pending.amount;
    }
}

// --- Registration orchestration ---

pub fn apply_registration_requested(state: &mut PlayerState, event: RegistrationRequested) {
    let reservation_hex = hex::encode(&event.reservation_id);
    let fee = event.fee.as_ref().map(|c| c.amount).unwrap_or(0);

    state.reserved_funds += fee;

    state.pending_registrations.insert(
        reservation_hex,
        PendingRegistration {
            tournament_root: event.tournament_root,
            fee,
        },
    );
}

pub fn apply_registration_confirmed(state: &mut PlayerState, event: RegistrationFeeConfirmed) {
    let reservation_hex = hex::encode(&event.reservation_id);

    if let Some(pending) = state.pending_registrations.remove(&reservation_hex) {
        state.reserved_funds -= pending.fee;
        state.bankroll -= pending.fee;
    }
}

pub fn apply_registration_released(state: &mut PlayerState, event: RegistrationFeeReleased) {
    let reservation_hex = hex::encode(&event.reservation_id);

    if let Some(pending) = state.pending_registrations.remove(&reservation_hex) {
        state.reserved_funds -= pending.fee;
    }
}

// --- Rebuy orchestration ---

pub fn apply_rebuy_requested(state: &mut PlayerState, event: RebuyRequested) {
    let reservation_hex = hex::encode(&event.reservation_id);
    let fee = event.fee.as_ref().map(|c| c.amount).unwrap_or(0);

    state.reserved_funds += fee;

    state.pending_rebuys.insert(
        reservation_hex,
        PendingRebuy {
            tournament_root: event.tournament_root,
            table_root: event.table_root,
            seat: event.seat,
            fee,
            chips_to_add: 0,
        },
    );
}

pub fn apply_rebuy_confirmed(state: &mut PlayerState, event: RebuyFeeConfirmed) {
    let reservation_hex = hex::encode(&event.reservation_id);

    if let Some(pending) = state.pending_rebuys.remove(&reservation_hex) {
        state.reserved_funds -= pending.fee;
        state.bankroll -= pending.fee;
    }
}

pub fn apply_rebuy_released(state: &mut PlayerState, event: RebuyFeeReleased) {
    let reservation_hex = hex::encode(&event.reservation_id);

    if let Some(pending) = state.pending_rebuys.remove(&reservation_hex) {
        state.reserved_funds -= pending.fee;
    }
}
