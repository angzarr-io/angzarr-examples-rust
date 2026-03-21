//! Player aggregate state.
//!
//! DOC: This file is referenced in docs/docs/examples/aggregates.mdx
//!      Update documentation when making changes to StateRouter patterns.

use std::collections::HashMap;
use std::sync::LazyLock;

use angzarr_client::proto::event_page::Payload;
use angzarr_client::proto::EventBook;
use angzarr_client::StateRouter;
use angzarr_client::UnpackAny;
use examples_proto::{
    BuyInConfirmed, BuyInRequested, BuyInReservationReleased, FundsDeposited, FundsReleased,
    FundsReserved, FundsTransferred, FundsWithdrawn, PlayerRegistered,
    PlayerState as ProtoPlayerState, PlayerType, RebuyFeeConfirmed, RebuyFeeReleased,
    RebuyRequested, RegistrationFeeConfirmed, RegistrationFeeReleased, RegistrationRequested,
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
    pub table_reservations: HashMap<String, i64>, // table_root_hex -> amount
    pub status: String,
    // Orchestration pending states
    pub pending_buy_ins: HashMap<String, PendingBuyIn>, // reservation_id_hex -> pending
    pub pending_registrations: HashMap<String, PendingRegistration>, // reservation_id_hex -> pending
    pub pending_rebuys: HashMap<String, PendingRebuy>, // reservation_id_hex -> pending
}

impl PlayerState {
    /// Check if the player exists.
    pub fn exists(&self) -> bool {
        !self.player_id.is_empty()
    }

    /// Get available balance (bankroll - reserved).
    pub fn available_balance(&self) -> i64 {
        self.bankroll - self.reserved_funds
    }

    /// Check if this is an AI player.
    pub fn is_ai(&self) -> bool {
        self.player_type == PlayerType::Ai
    }
}

// Event applier functions for StateRouter

// docs:start:state_router
fn apply_registered(state: &mut PlayerState, event: PlayerRegistered) {
    state.player_id = format!("player_{}", event.email);
    state.display_name = event.display_name;
    state.email = event.email;
    state.player_type = PlayerType::try_from(event.player_type).unwrap_or_default();
    state.ai_model_id = event.ai_model_id;
    state.status = "active".to_string();
    state.bankroll = 0;
    state.reserved_funds = 0;
}

fn apply_deposited(state: &mut PlayerState, event: FundsDeposited) {
    if let Some(balance) = event.new_balance {
        state.bankroll = balance.amount;
    }
}

fn apply_withdrawn(state: &mut PlayerState, event: FundsWithdrawn) {
    if let Some(balance) = event.new_balance {
        state.bankroll = balance.amount;
    }
}

fn apply_reserved(state: &mut PlayerState, event: FundsReserved) {
    if let Some(balance) = event.new_reserved_balance {
        state.reserved_funds = balance.amount;
    }
    if let (Some(amount), table_root) = (event.amount, event.table_root) {
        let table_key = hex::encode(&table_root);
        state.table_reservations.insert(table_key, amount.amount);
    }
}

fn apply_released(state: &mut PlayerState, event: FundsReleased) {
    if let Some(balance) = event.new_reserved_balance {
        state.reserved_funds = balance.amount;
    }
    let table_key = hex::encode(&event.table_root);
    state.table_reservations.remove(&table_key);
}

fn apply_transferred(state: &mut PlayerState, event: FundsTransferred) {
    if let Some(balance) = event.new_balance {
        state.bankroll = balance.amount;
    }
}

// --- Buy-in orchestration events ---

fn apply_buy_in_requested(state: &mut PlayerState, event: BuyInRequested) {
    let reservation_hex = hex::encode(&event.reservation_id);
    let amount = event.amount.as_ref().map(|c| c.amount).unwrap_or(0);

    // Reserve funds for this buy-in
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

fn apply_buy_in_confirmed(state: &mut PlayerState, event: BuyInConfirmed) {
    let reservation_hex = hex::encode(&event.reservation_id);

    if let Some(pending) = state.pending_buy_ins.remove(&reservation_hex) {
        // Move from reserved to table reservation
        state.reserved_funds -= pending.amount;
        let table_key = hex::encode(&pending.table_root);
        state.table_reservations.insert(table_key, pending.amount);
        // Deduct from bankroll (funds are now at the table)
        state.bankroll -= pending.amount;
    }
}

fn apply_buy_in_released(state: &mut PlayerState, event: BuyInReservationReleased) {
    let reservation_hex = hex::encode(&event.reservation_id);

    if let Some(pending) = state.pending_buy_ins.remove(&reservation_hex) {
        // Release reserved funds back to available
        state.reserved_funds -= pending.amount;
    }
}

// --- Registration orchestration events ---

fn apply_registration_requested(state: &mut PlayerState, event: RegistrationRequested) {
    let reservation_hex = hex::encode(&event.reservation_id);
    let fee = event.fee.as_ref().map(|c| c.amount).unwrap_or(0);

    // Reserve funds for registration fee
    state.reserved_funds += fee;

    state.pending_registrations.insert(
        reservation_hex,
        PendingRegistration {
            tournament_root: event.tournament_root,
            fee,
        },
    );
}

fn apply_registration_confirmed(state: &mut PlayerState, event: RegistrationFeeConfirmed) {
    let reservation_hex = hex::encode(&event.reservation_id);

    if let Some(pending) = state.pending_registrations.remove(&reservation_hex) {
        // Deduct fee from reserved and bankroll
        state.reserved_funds -= pending.fee;
        state.bankroll -= pending.fee;
    }
}

fn apply_registration_released(state: &mut PlayerState, event: RegistrationFeeReleased) {
    let reservation_hex = hex::encode(&event.reservation_id);

    if let Some(pending) = state.pending_registrations.remove(&reservation_hex) {
        // Release reserved funds
        state.reserved_funds -= pending.fee;
    }
}

// --- Rebuy orchestration events ---

fn apply_rebuy_requested(state: &mut PlayerState, event: RebuyRequested) {
    let reservation_hex = hex::encode(&event.reservation_id);
    let fee = event.fee.as_ref().map(|c| c.amount).unwrap_or(0);

    // Reserve funds for rebuy fee
    state.reserved_funds += fee;

    state.pending_rebuys.insert(
        reservation_hex,
        PendingRebuy {
            tournament_root: event.tournament_root,
            table_root: event.table_root,
            seat: event.seat,
            fee,
            chips_to_add: 0, // Will be set by PM
        },
    );
}

fn apply_rebuy_confirmed(state: &mut PlayerState, event: RebuyFeeConfirmed) {
    let reservation_hex = hex::encode(&event.reservation_id);

    if let Some(pending) = state.pending_rebuys.remove(&reservation_hex) {
        // Deduct fee from reserved and bankroll
        state.reserved_funds -= pending.fee;
        state.bankroll -= pending.fee;
    }
}

fn apply_rebuy_released(state: &mut PlayerState, event: RebuyFeeReleased) {
    let reservation_hex = hex::encode(&event.reservation_id);

    if let Some(pending) = state.pending_rebuys.remove(&reservation_hex) {
        // Release reserved funds
        state.reserved_funds -= pending.fee;
    }
}

/// StateRouter for fluent state reconstruction.
///
/// Type names are extracted via reflection using `prost::Name::full_name()`.
pub static STATE_ROUTER: LazyLock<StateRouter<PlayerState>> = LazyLock::new(|| {
    StateRouter::new()
        .on::<PlayerRegistered>(apply_registered)
        .on::<FundsDeposited>(apply_deposited)
        .on::<FundsWithdrawn>(apply_withdrawn)
        .on::<FundsReserved>(apply_reserved)
        .on::<FundsReleased>(apply_released)
        .on::<FundsTransferred>(apply_transferred)
        // Buy-in orchestration
        .on::<BuyInRequested>(apply_buy_in_requested)
        .on::<BuyInConfirmed>(apply_buy_in_confirmed)
        .on::<BuyInReservationReleased>(apply_buy_in_released)
        // Registration orchestration
        .on::<RegistrationRequested>(apply_registration_requested)
        .on::<RegistrationFeeConfirmed>(apply_registration_confirmed)
        .on::<RegistrationFeeReleased>(apply_registration_released)
        // Rebuy orchestration
        .on::<RebuyRequested>(apply_rebuy_requested)
        .on::<RebuyFeeConfirmed>(apply_rebuy_confirmed)
        .on::<RebuyFeeReleased>(apply_rebuy_released)
});
// docs:end:state_router

/// Rebuild player state from event history.
pub fn rebuild_state(event_book: &EventBook) -> PlayerState {
    // Start from snapshot if available
    if let Some(snapshot) = &event_book.snapshot {
        if let Some(snapshot_any) = &snapshot.state {
            if let Ok(proto_state) = snapshot_any.unpack::<ProtoPlayerState>() {
                let mut state = apply_snapshot(&proto_state);
                // Apply events since snapshot
                for page in &event_book.pages {
                    if let Some(Payload::Event(event)) = &page.payload {
                        STATE_ROUTER.apply_single(&mut state, event);
                    }
                }
                return state;
            }
        }
    }

    STATE_ROUTER.with_event_book(event_book)
}

fn apply_snapshot(snapshot: &ProtoPlayerState) -> PlayerState {
    let bankroll = snapshot.bankroll.as_ref().map(|c| c.amount).unwrap_or(0);
    let reserved_funds = snapshot
        .reserved_funds
        .as_ref()
        .map(|c| c.amount)
        .unwrap_or(0);

    PlayerState {
        player_id: snapshot.player_id.clone(),
        display_name: snapshot.display_name.clone(),
        email: snapshot.email.clone(),
        player_type: PlayerType::try_from(snapshot.player_type).unwrap_or_default(),
        ai_model_id: snapshot.ai_model_id.clone(),
        bankroll,
        reserved_funds,
        table_reservations: snapshot.table_reservations.clone(),
        status: snapshot.status.clone(),
        // Pending orchestration states are not persisted in snapshots
        // They are rebuilt from events
        pending_buy_ins: HashMap::new(),
        pending_registrations: HashMap::new(),
        pending_rebuys: HashMap::new(),
    }
}
