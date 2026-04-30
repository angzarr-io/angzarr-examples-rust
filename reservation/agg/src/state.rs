//! Reservation aggregate state.
//!
//! Pending records for the three lifecycle flavors (buy-in, rebuy, registration).
//! Funds live on the player aggregate; this aggregate's `Initiate*` handlers
//! optionally consult player state via sync DECISION before emitting the request
//! event.

use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct PendingBuyIn {
    pub player_root: Vec<u8>,
    pub table_root: Vec<u8>,
    pub seat: i32,
    pub amount: i64,
}

#[derive(Debug, Clone, Default)]
pub struct PendingRegistration {
    pub player_root: Vec<u8>,
    pub tournament_root: Vec<u8>,
    pub fee: i64,
}

#[derive(Debug, Clone, Default)]
pub struct PendingRebuy {
    pub player_root: Vec<u8>,
    pub tournament_root: Vec<u8>,
    pub table_root: Vec<u8>,
    pub seat: i32,
    pub fee: i64,
}

#[derive(Debug, Clone, Default)]
pub struct ReservationState {
    pub pending_buy_ins: HashMap<String, PendingBuyIn>,
    pub pending_registrations: HashMap<String, PendingRegistration>,
    pub pending_rebuys: HashMap<String, PendingRebuy>,
}
