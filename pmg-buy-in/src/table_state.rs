//! Table state router for BuyIn PM destination state rebuilding.
//!
//! This module defines a StateRouter that automatically rebuilds TableStateHelper
//! from EventBooks, eliminating manual rebuild_table_state() boilerplate.
//!
//! Usage in PM:
//! ```rust,ignore
//! let router = ProcessManagerRouter::new("pmg-buy-in", "buy-in", rebuild_pm_state)
//!     .with_destination("table", table_state_router())
//!     .domain("player", BuyInPmHandler);
//! ```

use std::collections::HashMap;

use angzarr_client::StateRouter;
use examples_proto::{PlayerJoined, PlayerLeft, PlayerSeated, TableCreated};

/// Minimal table state for PM validation.
#[derive(Default)]
pub struct TableStateHelper {
    pub table_id: String,
    pub table_name: String,
    pub min_buy_in: i64,
    pub max_buy_in: i64,
    pub max_players: i32,
    pub seats: HashMap<i32, Vec<u8>>, // position -> player_root
}

impl TableStateHelper {
    /// Find seat position for a player, or None if not seated.
    pub fn find_seat_by_player(&self, player_root: &[u8]) -> Option<i32> {
        for (pos, root) in &self.seats {
            if root == player_root {
                return Some(*pos);
            }
        }
        None
    }

    /// Find next available seat, or None if table is full.
    pub fn next_available_seat(&self) -> Option<i32> {
        (0..self.max_players).find(|i| !self.seats.contains_key(i))
    }
}

// --- State appliers (pure functions) ---

fn apply_table_created(state: &mut TableStateHelper, event: TableCreated) {
    state.table_id = format!("table_{}", event.table_name);
    state.table_name = event.table_name;
    state.min_buy_in = event.min_buy_in;
    state.max_buy_in = event.max_buy_in;
    state.max_players = event.max_players;
}

fn apply_player_joined(state: &mut TableStateHelper, event: PlayerJoined) {
    state.seats.insert(event.seat_position, event.player_root);
}

fn apply_player_seated(state: &mut TableStateHelper, event: PlayerSeated) {
    state.seats.insert(event.seat_position, event.player_root);
}

fn apply_player_left(state: &mut TableStateHelper, event: PlayerLeft) {
    state.seats.remove(&event.seat_position);
}

// --- StateRouter configuration ---

/// Create the table state router for PM destination rebuilding.
pub fn table_state_router() -> StateRouter<TableStateHelper> {
    StateRouter::new()
        .on::<TableCreated>(apply_table_created)
        .on::<PlayerJoined>(apply_player_joined)
        .on::<PlayerSeated>(apply_player_seated)
        .on::<PlayerLeft>(apply_player_left)
}
