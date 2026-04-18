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

#[cfg(test)]
mod tests {
    use super::*;
    use examples_proto::Currency;

    fn currency(amount: i64) -> Currency {
        Currency {
            amount,
            currency_code: "CHIPS".to_string(),
        }
    }

    fn registered_state() -> PlayerState {
        let mut state = PlayerState::default();
        apply_registered(
            &mut state,
            PlayerRegistered {
                display_name: "Alice".into(),
                email: "alice@example.com".into(),
                player_type: PlayerType::Human as i32,
                ai_model_id: String::new(),
                registered_at: None,
            },
        );
        state
    }

    #[test]
    fn apply_registered_sets_identity_and_status() {
        let state = registered_state();
        assert_eq!(state.player_id, "player_alice@example.com");
        assert_eq!(state.display_name, "Alice");
        assert_eq!(state.email, "alice@example.com");
        assert_eq!(state.status, "active");
        assert_eq!(state.bankroll, 0);
        assert_eq!(state.reserved_funds, 0);
        assert!(state.exists());
        assert!(!state.is_ai());
    }

    #[test]
    fn apply_registered_ai_player_flags_is_ai() {
        let mut state = PlayerState::default();
        apply_registered(
            &mut state,
            PlayerRegistered {
                display_name: "Bot".into(),
                email: "bot@example.com".into(),
                player_type: PlayerType::Ai as i32,
                ai_model_id: "gpt-5".into(),
                registered_at: None,
            },
        );
        assert!(state.is_ai());
        assert_eq!(state.ai_model_id, "gpt-5");
    }

    #[test]
    fn available_balance_computes_bankroll_minus_reserved() {
        let mut state = registered_state();
        state.bankroll = 1000;
        state.reserved_funds = 250;
        assert_eq!(state.available_balance(), 750);
    }

    #[test]
    fn exists_is_false_for_default() {
        assert!(!PlayerState::default().exists());
    }

    #[test]
    fn apply_deposited_updates_bankroll_when_new_balance_present() {
        let mut state = registered_state();
        apply_deposited(
            &mut state,
            FundsDeposited {
                amount: Some(currency(500)),
                new_balance: Some(currency(500)),
                deposited_at: None,
            },
        );
        assert_eq!(state.bankroll, 500);
    }

    #[test]
    fn apply_deposited_leaves_bankroll_untouched_without_new_balance() {
        let mut state = registered_state();
        state.bankroll = 42;
        apply_deposited(
            &mut state,
            FundsDeposited {
                amount: Some(currency(500)),
                new_balance: None,
                deposited_at: None,
            },
        );
        assert_eq!(state.bankroll, 42);
    }

    #[test]
    fn apply_withdrawn_updates_bankroll_when_new_balance_present() {
        let mut state = registered_state();
        state.bankroll = 1000;
        apply_withdrawn(
            &mut state,
            FundsWithdrawn {
                amount: Some(currency(300)),
                new_balance: Some(currency(700)),
                withdrawn_at: None,
            },
        );
        assert_eq!(state.bankroll, 700);
    }

    #[test]
    fn apply_withdrawn_without_new_balance_is_noop() {
        let mut state = registered_state();
        state.bankroll = 1000;
        apply_withdrawn(
            &mut state,
            FundsWithdrawn {
                amount: Some(currency(300)),
                new_balance: None,
                withdrawn_at: None,
            },
        );
        assert_eq!(state.bankroll, 1000);
    }

    #[test]
    fn apply_reserved_tracks_per_table_reservation() {
        let mut state = registered_state();
        state.bankroll = 1000;
        let table_root = vec![1u8, 2, 3, 4];
        apply_reserved(
            &mut state,
            FundsReserved {
                amount: Some(currency(250)),
                table_root: table_root.clone(),
                new_available_balance: Some(currency(750)),
                new_reserved_balance: Some(currency(250)),
                reserved_at: None,
            },
        );
        assert_eq!(state.reserved_funds, 250);
        assert_eq!(
            state.table_reservations.get(&hex::encode(&table_root)),
            Some(&250)
        );
    }

    #[test]
    fn apply_released_removes_table_reservation() {
        let mut state = registered_state();
        let table_root = vec![9u8, 9, 9];
        state
            .table_reservations
            .insert(hex::encode(&table_root), 100);
        state.reserved_funds = 100;
        apply_released(
            &mut state,
            FundsReleased {
                amount: Some(currency(100)),
                table_root: table_root.clone(),
                new_available_balance: Some(currency(0)),
                new_reserved_balance: Some(currency(0)),
                released_at: None,
            },
        );
        assert_eq!(state.reserved_funds, 0);
        assert!(!state
            .table_reservations
            .contains_key(&hex::encode(&table_root)));
    }

    #[test]
    fn apply_transferred_updates_bankroll_when_new_balance_present() {
        let mut state = registered_state();
        state.bankroll = 500;
        apply_transferred(
            &mut state,
            FundsTransferred {
                from_player_root: vec![1],
                to_player_root: vec![2],
                amount: Some(currency(200)),
                hand_root: vec![],
                reason: "pot_win".into(),
                new_balance: Some(currency(700)),
                transferred_at: None,
            },
        );
        assert_eq!(state.bankroll, 700);
    }

    #[test]
    fn apply_buy_in_requested_reserves_funds_and_tracks_pending() {
        let mut state = registered_state();
        let reservation_id = vec![0xAA, 0xBB];
        apply_buy_in_requested(
            &mut state,
            BuyInRequested {
                reservation_id: reservation_id.clone(),
                table_root: vec![1, 2],
                seat: 3,
                amount: Some(currency(400)),
                requested_at: None,
            },
        );
        assert_eq!(state.reserved_funds, 400);
        let pending = state
            .pending_buy_ins
            .get(&hex::encode(&reservation_id))
            .expect("pending buy-in registered");
        assert_eq!(pending.amount, 400);
        assert_eq!(pending.seat, 3);
    }

    #[test]
    fn apply_buy_in_confirmed_moves_reserved_to_table_and_deducts_bankroll() {
        let mut state = registered_state();
        state.bankroll = 500;
        let reservation_id = vec![1, 2, 3];
        apply_buy_in_requested(
            &mut state,
            BuyInRequested {
                reservation_id: reservation_id.clone(),
                table_root: vec![9],
                seat: 1,
                amount: Some(currency(100)),
                requested_at: None,
            },
        );
        apply_buy_in_confirmed(
            &mut state,
            BuyInConfirmed {
                reservation_id: reservation_id.clone(),
                table_root: vec![9],
                seat: 1,
                amount: Some(currency(100)),
                confirmed_at: None,
            },
        );
        assert_eq!(state.reserved_funds, 0);
        assert_eq!(state.bankroll, 400);
        assert_eq!(
            state.table_reservations.get(&hex::encode(&[9u8])),
            Some(&100)
        );
        assert!(state.pending_buy_ins.is_empty());
    }

    #[test]
    fn apply_buy_in_released_unreserves_without_charging_bankroll() {
        let mut state = registered_state();
        state.bankroll = 500;
        let reservation_id = vec![1];
        apply_buy_in_requested(
            &mut state,
            BuyInRequested {
                reservation_id: reservation_id.clone(),
                table_root: vec![9],
                seat: 1,
                amount: Some(currency(100)),
                requested_at: None,
            },
        );
        apply_buy_in_released(
            &mut state,
            BuyInReservationReleased {
                reservation_id: reservation_id.clone(),
                reason: "cancelled".into(),
                released_at: None,
            },
        );
        assert_eq!(state.reserved_funds, 0);
        assert_eq!(state.bankroll, 500);
        assert!(state.pending_buy_ins.is_empty());
    }

    #[test]
    fn registration_flow_end_to_end() {
        let mut state = registered_state();
        state.bankroll = 1000;
        let rid = vec![7u8];
        apply_registration_requested(
            &mut state,
            RegistrationRequested {
                reservation_id: rid.clone(),
                tournament_root: vec![99],
                fee: Some(currency(250)),
                requested_at: None,
            },
        );
        assert_eq!(state.reserved_funds, 250);
        apply_registration_confirmed(
            &mut state,
            RegistrationFeeConfirmed {
                reservation_id: rid.clone(),
                tournament_root: vec![99],
                fee: Some(currency(250)),
                confirmed_at: None,
            },
        );
        assert_eq!(state.reserved_funds, 0);
        assert_eq!(state.bankroll, 750);
        assert!(state.pending_registrations.is_empty());
    }

    #[test]
    fn registration_released_reverts_reservation_without_bankroll_change() {
        let mut state = registered_state();
        state.bankroll = 1000;
        let rid = vec![8u8];
        apply_registration_requested(
            &mut state,
            RegistrationRequested {
                reservation_id: rid.clone(),
                tournament_root: vec![50],
                fee: Some(currency(150)),
                requested_at: None,
            },
        );
        apply_registration_released(
            &mut state,
            RegistrationFeeReleased {
                reservation_id: rid.clone(),
                reason: "cancelled".into(),
                released_at: None,
            },
        );
        assert_eq!(state.reserved_funds, 0);
        assert_eq!(state.bankroll, 1000);
        assert!(state.pending_registrations.is_empty());
    }

    #[test]
    fn rebuy_flow_end_to_end() {
        let mut state = registered_state();
        state.bankroll = 2000;
        let rid = vec![11u8];
        apply_rebuy_requested(
            &mut state,
            RebuyRequested {
                reservation_id: rid.clone(),
                tournament_root: vec![1],
                table_root: vec![2],
                seat: 4,
                fee: Some(currency(500)),
                requested_at: None,
            },
        );
        assert_eq!(state.reserved_funds, 500);
        assert!(state.pending_rebuys.contains_key(&hex::encode(&rid)));

        apply_rebuy_confirmed(
            &mut state,
            RebuyFeeConfirmed {
                reservation_id: rid.clone(),
                tournament_root: vec![1],
                fee: Some(currency(500)),
                chips_added: 500,
                confirmed_at: None,
            },
        );
        assert_eq!(state.reserved_funds, 0);
        assert_eq!(state.bankroll, 1500);
        assert!(state.pending_rebuys.is_empty());
    }

    #[test]
    fn rebuy_released_reverts_reservation() {
        let mut state = registered_state();
        state.bankroll = 2000;
        let rid = vec![12u8];
        apply_rebuy_requested(
            &mut state,
            RebuyRequested {
                reservation_id: rid.clone(),
                tournament_root: vec![1],
                table_root: vec![2],
                seat: 4,
                fee: Some(currency(500)),
                requested_at: None,
            },
        );
        apply_rebuy_released(
            &mut state,
            RebuyFeeReleased {
                reservation_id: rid.clone(),
                reason: "denied".into(),
                released_at: None,
            },
        );
        assert_eq!(state.reserved_funds, 0);
        assert_eq!(state.bankroll, 2000);
        assert!(state.pending_rebuys.is_empty());
    }
}
