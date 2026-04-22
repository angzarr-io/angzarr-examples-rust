//! Player aggregate library.

pub mod handlers;
pub mod state;

pub use state::PlayerState;

use angzarr_client::proto::{BusinessResponse, EventBook, Notification};
use angzarr_client::{aggregate, CommandResult};
use examples_proto::{
    BuyInConfirmed, BuyInRequested, BuyInReservationReleased, ConfirmBuyIn, ConfirmRebuyFee,
    ConfirmRegistrationFee, DepositFunds, FundsDeposited, FundsReleased, FundsReserved,
    FundsTransferred, FundsWithdrawn, InitiateBuyIn, InitiateRebuy, InitiateTournamentRegistration,
    PlayerRegistered, RebuyFeeConfirmed, RebuyFeeReleased, RebuyRequested, RegisterPlayer,
    RegistrationFeeConfirmed, RegistrationFeeReleased, RegistrationRequested, ReleaseBuyIn,
    ReleaseFunds, ReleaseRebuyFee, ReleaseRegistrationFee, ReserveFunds, TransferFunds,
    WithdrawFunds,
};

use crate::state::{
    apply_buy_in_confirmed, apply_buy_in_released, apply_buy_in_requested, apply_deposited,
    apply_rebuy_confirmed, apply_rebuy_released, apply_rebuy_requested, apply_registered,
    apply_registration_confirmed, apply_registration_released, apply_registration_requested,
    apply_released, apply_reserved, apply_transferred, apply_withdrawn,
};

// docs:start:command_router
/// Player aggregate - dispatches commands and applies events.
pub struct PlayerAggregate;

#[aggregate(domain = "player", state = PlayerState)]
impl PlayerAggregate {
    // --- Core fund command handlers ---

    #[handles(RegisterPlayer)]
    fn on_register_player(
        &self,
        cmd: RegisterPlayer,
        state: &PlayerState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        handlers::handle_register_player(cmd, state, seq)
    }

    #[handles(DepositFunds)]
    fn on_deposit_funds(
        &self,
        cmd: DepositFunds,
        state: &PlayerState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        handlers::handle_deposit_funds(cmd, state, seq)
    }

    #[handles(WithdrawFunds)]
    fn on_withdraw_funds(
        &self,
        cmd: WithdrawFunds,
        state: &PlayerState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        handlers::handle_withdraw_funds(cmd, state, seq)
    }

    #[handles(ReserveFunds)]
    fn on_reserve_funds(
        &self,
        cmd: ReserveFunds,
        state: &PlayerState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        handlers::handle_reserve_funds(cmd, state, seq)
    }

    #[handles(ReleaseFunds)]
    fn on_release_funds(
        &self,
        cmd: ReleaseFunds,
        state: &PlayerState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        handlers::handle_release_funds(cmd, state, seq)
    }

    #[handles(TransferFunds)]
    fn on_transfer_funds(
        &self,
        cmd: TransferFunds,
        state: &PlayerState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        handlers::handle_transfer_funds(cmd, state, seq)
    }

    // --- Buy-in orchestration ---

    #[handles(InitiateBuyIn)]
    fn on_initiate_buy_in(
        &self,
        cmd: InitiateBuyIn,
        state: &PlayerState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        handlers::handle_initiate_buy_in(cmd, state, seq)
    }

    #[handles(ConfirmBuyIn)]
    fn on_confirm_buy_in(
        &self,
        cmd: ConfirmBuyIn,
        state: &PlayerState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        handlers::handle_confirm_buy_in(cmd, state, seq)
    }

    #[handles(ReleaseBuyIn)]
    fn on_release_buy_in(
        &self,
        cmd: ReleaseBuyIn,
        state: &PlayerState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        handlers::handle_release_buy_in(cmd, state, seq)
    }

    // --- Registration orchestration ---

    #[handles(InitiateTournamentRegistration)]
    fn on_initiate_registration(
        &self,
        cmd: InitiateTournamentRegistration,
        state: &PlayerState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        handlers::handle_initiate_tournament_registration(cmd, state, seq)
    }

    #[handles(ConfirmRegistrationFee)]
    fn on_confirm_registration(
        &self,
        cmd: ConfirmRegistrationFee,
        state: &PlayerState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        handlers::handle_confirm_registration_fee(cmd, state, seq)
    }

    #[handles(ReleaseRegistrationFee)]
    fn on_release_registration(
        &self,
        cmd: ReleaseRegistrationFee,
        state: &PlayerState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        handlers::handle_release_registration_fee(cmd, state, seq)
    }

    // --- Rebuy orchestration ---

    #[handles(InitiateRebuy)]
    fn on_initiate_rebuy(
        &self,
        cmd: InitiateRebuy,
        state: &PlayerState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        handlers::handle_initiate_rebuy(cmd, state, seq)
    }

    #[handles(ConfirmRebuyFee)]
    fn on_confirm_rebuy(
        &self,
        cmd: ConfirmRebuyFee,
        state: &PlayerState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        handlers::handle_confirm_rebuy_fee(cmd, state, seq)
    }

    #[handles(ReleaseRebuyFee)]
    fn on_release_rebuy(
        &self,
        cmd: ReleaseRebuyFee,
        state: &PlayerState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        handlers::handle_release_rebuy_fee(cmd, state, seq)
    }

    // --- Event appliers ---

    #[applies(PlayerRegistered)]
    fn apply_registered(state: &mut PlayerState, event: PlayerRegistered) {
        apply_registered(state, event);
    }

    #[applies(FundsDeposited)]
    fn apply_deposited(state: &mut PlayerState, event: FundsDeposited) {
        apply_deposited(state, event);
    }

    #[applies(FundsWithdrawn)]
    fn apply_withdrawn(state: &mut PlayerState, event: FundsWithdrawn) {
        apply_withdrawn(state, event);
    }

    #[applies(FundsReserved)]
    fn apply_reserved(state: &mut PlayerState, event: FundsReserved) {
        apply_reserved(state, event);
    }

    #[applies(FundsReleased)]
    fn apply_released(state: &mut PlayerState, event: FundsReleased) {
        apply_released(state, event);
    }

    #[applies(FundsTransferred)]
    fn apply_transferred(state: &mut PlayerState, event: FundsTransferred) {
        apply_transferred(state, event);
    }

    #[applies(BuyInRequested)]
    fn apply_buy_in_requested(state: &mut PlayerState, event: BuyInRequested) {
        apply_buy_in_requested(state, event);
    }

    #[applies(BuyInConfirmed)]
    fn apply_buy_in_confirmed(state: &mut PlayerState, event: BuyInConfirmed) {
        apply_buy_in_confirmed(state, event);
    }

    #[applies(BuyInReservationReleased)]
    fn apply_buy_in_released(state: &mut PlayerState, event: BuyInReservationReleased) {
        apply_buy_in_released(state, event);
    }

    #[applies(RegistrationRequested)]
    fn apply_registration_requested(state: &mut PlayerState, event: RegistrationRequested) {
        apply_registration_requested(state, event);
    }

    #[applies(RegistrationFeeConfirmed)]
    fn apply_registration_confirmed(state: &mut PlayerState, event: RegistrationFeeConfirmed) {
        apply_registration_confirmed(state, event);
    }

    #[applies(RegistrationFeeReleased)]
    fn apply_registration_released(state: &mut PlayerState, event: RegistrationFeeReleased) {
        apply_registration_released(state, event);
    }

    #[applies(RebuyRequested)]
    fn apply_rebuy_requested(state: &mut PlayerState, event: RebuyRequested) {
        apply_rebuy_requested(state, event);
    }

    #[applies(RebuyFeeConfirmed)]
    fn apply_rebuy_confirmed(state: &mut PlayerState, event: RebuyFeeConfirmed) {
        apply_rebuy_confirmed(state, event);
    }

    #[applies(RebuyFeeReleased)]
    fn apply_rebuy_released(state: &mut PlayerState, event: RebuyFeeReleased) {
        apply_rebuy_released(state, event);
    }

    // --- Rejection handler ---

    // docs:start:rejected_handler
    #[rejected(domain = "table", command = "JoinTable")]
    fn on_join_table_rejected(
        &self,
        notification: &Notification,
        state: &PlayerState,
    ) -> CommandResult<BusinessResponse> {
        handlers::handle_join_rejected(notification, state)
    }
    // docs:end:rejected_handler
}
// docs:end:command_router

#[cfg(test)]
mod applier_tests {
    //! Direct-call tests against the `#[applies]` methods on `PlayerAggregate`.
    //! These catch mutations that replace the one-line delegation body with `()`.
    use super::*;
    use examples_proto::{Currency, PlayerType};

    fn currency(amount: i64) -> Currency {
        Currency {
            amount,
            currency_code: "CHIPS".into(),
        }
    }

    #[test]
    fn apply_registered_delegates_to_state_fn() {
        let mut state = PlayerState::default();
        PlayerAggregate::apply_registered(
            &mut state,
            PlayerRegistered {
                display_name: "Alice".into(),
                email: "alice@example.com".into(),
                player_type: PlayerType::Human as i32,
                ai_model_id: String::new(),
                registered_at: None,
            },
        );
        assert_eq!(state.player_id, "player_alice@example.com");
        assert_eq!(state.display_name, "Alice");
        assert_eq!(state.status, "active");
    }

    #[test]
    fn apply_deposited_delegates_to_state_fn() {
        let mut state = PlayerState::default();
        PlayerAggregate::apply_deposited(
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
    fn apply_withdrawn_delegates_to_state_fn() {
        let mut state = PlayerState {
            bankroll: 1000,
            ..PlayerState::default()
        };
        PlayerAggregate::apply_withdrawn(
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
    fn apply_reserved_delegates_to_state_fn() {
        let mut state = PlayerState {
            bankroll: 1000,
            ..PlayerState::default()
        };
        let table_root = vec![1u8, 2];
        PlayerAggregate::apply_reserved(
            &mut state,
            FundsReserved {
                amount: Some(currency(200)),
                table_root: table_root.clone(),
                new_available_balance: Some(currency(800)),
                new_reserved_balance: Some(currency(200)),
                reserved_at: None,
            },
        );
        assert_eq!(state.reserved_funds, 200);
        assert_eq!(
            state.table_reservations.get(&hex::encode(&table_root)),
            Some(&200)
        );
    }

    #[test]
    fn apply_released_delegates_to_state_fn() {
        let mut state = PlayerState {
            bankroll: 1000,
            reserved_funds: 200,
            ..PlayerState::default()
        };
        let table_root = vec![5u8];
        state
            .table_reservations
            .insert(hex::encode(&table_root), 200);
        PlayerAggregate::apply_released(
            &mut state,
            FundsReleased {
                amount: Some(currency(200)),
                table_root: table_root.clone(),
                new_available_balance: Some(currency(1000)),
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
    fn apply_transferred_delegates_to_state_fn() {
        let mut state = PlayerState {
            bankroll: 500,
            ..PlayerState::default()
        };
        PlayerAggregate::apply_transferred(
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
    fn apply_buy_in_requested_delegates_to_state_fn() {
        let mut state = PlayerState::default();
        let rid = vec![0xaa, 0xbb];
        PlayerAggregate::apply_buy_in_requested(
            &mut state,
            BuyInRequested {
                reservation_id: rid.clone(),
                table_root: vec![0xcc],
                seat: 2,
                amount: Some(currency(400)),
                requested_at: None,
            },
        );
        assert_eq!(state.reserved_funds, 400);
        let pending = state
            .pending_buy_ins
            .get(&hex::encode(&rid))
            .expect("pending buy-in");
        assert_eq!(pending.amount, 400);
        assert_eq!(pending.seat, 2);
    }

    #[test]
    fn apply_buy_in_confirmed_delegates_to_state_fn() {
        let mut state = PlayerState {
            bankroll: 500,
            ..PlayerState::default()
        };
        let rid = vec![0xaa];
        apply_buy_in_requested(
            &mut state,
            BuyInRequested {
                reservation_id: rid.clone(),
                table_root: vec![0xcc],
                seat: 1,
                amount: Some(currency(100)),
                requested_at: None,
            },
        );
        PlayerAggregate::apply_buy_in_confirmed(
            &mut state,
            BuyInConfirmed {
                reservation_id: rid.clone(),
                table_root: vec![0xcc],
                seat: 1,
                amount: Some(currency(100)),
                confirmed_at: None,
            },
        );
        assert_eq!(state.bankroll, 400);
        assert_eq!(state.reserved_funds, 0);
        assert!(state.pending_buy_ins.is_empty());
    }

    #[test]
    fn apply_buy_in_released_delegates_to_state_fn() {
        let mut state = PlayerState {
            bankroll: 500,
            ..PlayerState::default()
        };
        let rid = vec![0xaa];
        apply_buy_in_requested(
            &mut state,
            BuyInRequested {
                reservation_id: rid.clone(),
                table_root: vec![0xcc],
                seat: 1,
                amount: Some(currency(100)),
                requested_at: None,
            },
        );
        PlayerAggregate::apply_buy_in_released(
            &mut state,
            BuyInReservationReleased {
                reservation_id: rid.clone(),
                reason: "cancelled".into(),
                released_at: None,
            },
        );
        assert_eq!(state.reserved_funds, 0);
        assert!(state.pending_buy_ins.is_empty());
    }

    #[test]
    fn apply_registration_requested_delegates_to_state_fn() {
        let mut state = PlayerState::default();
        let rid = vec![0x10];
        PlayerAggregate::apply_registration_requested(
            &mut state,
            RegistrationRequested {
                reservation_id: rid.clone(),
                tournament_root: vec![0x99],
                fee: Some(currency(250)),
                requested_at: None,
            },
        );
        assert_eq!(state.reserved_funds, 250);
        assert!(state
            .pending_registrations
            .contains_key(&hex::encode(&rid)));
    }

    #[test]
    fn apply_registration_confirmed_delegates_to_state_fn() {
        let mut state = PlayerState {
            bankroll: 1000,
            ..PlayerState::default()
        };
        let rid = vec![0x10];
        apply_registration_requested(
            &mut state,
            RegistrationRequested {
                reservation_id: rid.clone(),
                tournament_root: vec![0x99],
                fee: Some(currency(250)),
                requested_at: None,
            },
        );
        PlayerAggregate::apply_registration_confirmed(
            &mut state,
            RegistrationFeeConfirmed {
                reservation_id: rid.clone(),
                tournament_root: vec![0x99],
                fee: Some(currency(250)),
                confirmed_at: None,
            },
        );
        assert_eq!(state.bankroll, 750);
        assert_eq!(state.reserved_funds, 0);
        assert!(state.pending_registrations.is_empty());
    }

    #[test]
    fn apply_registration_released_delegates_to_state_fn() {
        let mut state = PlayerState {
            bankroll: 1000,
            ..PlayerState::default()
        };
        let rid = vec![0x10];
        apply_registration_requested(
            &mut state,
            RegistrationRequested {
                reservation_id: rid.clone(),
                tournament_root: vec![0x99],
                fee: Some(currency(250)),
                requested_at: None,
            },
        );
        PlayerAggregate::apply_registration_released(
            &mut state,
            RegistrationFeeReleased {
                reservation_id: rid.clone(),
                reason: "cancelled".into(),
                released_at: None,
            },
        );
        assert_eq!(state.reserved_funds, 0);
        assert!(state.pending_registrations.is_empty());
    }

    #[test]
    fn apply_rebuy_requested_delegates_to_state_fn() {
        let mut state = PlayerState::default();
        let rid = vec![0x20];
        PlayerAggregate::apply_rebuy_requested(
            &mut state,
            RebuyRequested {
                reservation_id: rid.clone(),
                tournament_root: vec![1],
                table_root: vec![2],
                seat: 4,
                fee: Some(currency(300)),
                requested_at: None,
            },
        );
        assert_eq!(state.reserved_funds, 300);
        assert!(state.pending_rebuys.contains_key(&hex::encode(&rid)));
    }

    #[test]
    fn apply_rebuy_confirmed_delegates_to_state_fn() {
        let mut state = PlayerState {
            bankroll: 2000,
            ..PlayerState::default()
        };
        let rid = vec![0x20];
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
        PlayerAggregate::apply_rebuy_confirmed(
            &mut state,
            RebuyFeeConfirmed {
                reservation_id: rid.clone(),
                tournament_root: vec![1],
                fee: Some(currency(500)),
                chips_added: 500,
                confirmed_at: None,
            },
        );
        assert_eq!(state.bankroll, 1500);
        assert_eq!(state.reserved_funds, 0);
        assert!(state.pending_rebuys.is_empty());
    }

    #[test]
    fn apply_rebuy_released_delegates_to_state_fn() {
        let mut state = PlayerState {
            bankroll: 2000,
            ..PlayerState::default()
        };
        let rid = vec![0x20];
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
        PlayerAggregate::apply_rebuy_released(
            &mut state,
            RebuyFeeReleased {
                reservation_id: rid.clone(),
                reason: "denied".into(),
                released_at: None,
            },
        );
        assert_eq!(state.reserved_funds, 0);
        assert!(state.pending_rebuys.is_empty());
    }
}

#[cfg(test)]
mod handler_tests {
    //! Direct-call tests for each `#[handles]` method on `PlayerAggregate`.
    //! Each test asserts that the returned EventBook has `pages.len() == 1`,
    //! catching `Default::default()` stub mutations.
    use super::*;
    use crate::state::{
        apply_buy_in_requested, apply_deposited, apply_rebuy_requested, apply_registered,
        apply_registration_requested, apply_reserved,
    };
    use angzarr_client::proto::{CommandBook, Cover, Notification, PageHeader, Uuid as ProtoUuid};
    use angzarr_client::proto::{CommandPage, RejectionNotification};
    use examples_proto::{Currency, PlayerType};
    use prost::Message;
    use prost_types::Any;

    fn currency(amount: i64) -> Currency {
        Currency {
            amount,
            currency_code: "CHIPS".into(),
        }
    }

    fn registered_state() -> PlayerState {
        let mut s = PlayerState::default();
        apply_registered(
            &mut s,
            PlayerRegistered {
                display_name: "Alice".into(),
                email: "alice@example.com".into(),
                player_type: PlayerType::Human as i32,
                ai_model_id: String::new(),
                registered_at: None,
            },
        );
        s
    }

    fn funded_state(bankroll: i64) -> PlayerState {
        let mut s = registered_state();
        apply_deposited(
            &mut s,
            FundsDeposited {
                amount: Some(currency(bankroll)),
                new_balance: Some(currency(bankroll)),
                deposited_at: None,
            },
        );
        s
    }

    #[test]
    fn on_register_player_delegates_to_handler() {
        let agg = PlayerAggregate;
        let state = PlayerState::default();
        let book = agg
            .on_register_player(
                RegisterPlayer {
                    display_name: "Alice".into(),
                    email: "alice@example.com".into(),
                    player_type: PlayerType::Human as i32,
                    ai_model_id: String::new(),
                },
                &state,
                0,
            )
            .expect("handler should succeed");
        assert_eq!(book.pages.len(), 1);
    }

    #[test]
    fn on_deposit_funds_delegates_to_handler() {
        let agg = PlayerAggregate;
        let state = registered_state();
        let book = agg
            .on_deposit_funds(
                DepositFunds {
                    amount: Some(currency(500)),
                },
                &state,
                1,
            )
            .expect("handler should succeed");
        assert_eq!(book.pages.len(), 1);
    }

    #[test]
    fn on_withdraw_funds_delegates_to_handler() {
        let agg = PlayerAggregate;
        let state = funded_state(1000);
        let book = agg
            .on_withdraw_funds(
                WithdrawFunds {
                    amount: Some(currency(300)),
                },
                &state,
                2,
            )
            .expect("handler should succeed");
        assert_eq!(book.pages.len(), 1);
    }

    #[test]
    fn on_reserve_funds_delegates_to_handler() {
        let agg = PlayerAggregate;
        let state = funded_state(1000);
        let book = agg
            .on_reserve_funds(
                ReserveFunds {
                    amount: Some(currency(200)),
                    table_root: vec![0xab, 0xcd],
                },
                &state,
                3,
            )
            .expect("handler should succeed");
        assert_eq!(book.pages.len(), 1);
    }

    #[test]
    fn on_release_funds_delegates_to_handler() {
        let agg = PlayerAggregate;
        let mut state = funded_state(1000);
        let table_root = vec![0xab, 0xcd];
        apply_reserved(
            &mut state,
            FundsReserved {
                amount: Some(currency(200)),
                table_root: table_root.clone(),
                new_available_balance: Some(currency(800)),
                new_reserved_balance: Some(currency(200)),
                reserved_at: None,
            },
        );
        let book = agg
            .on_release_funds(ReleaseFunds { table_root }, &state, 4)
            .expect("handler should succeed");
        assert_eq!(book.pages.len(), 1);
    }

    #[test]
    fn on_transfer_funds_delegates_to_handler() {
        let agg = PlayerAggregate;
        let state = registered_state();
        let book = agg
            .on_transfer_funds(
                TransferFunds {
                    from_player_root: vec![9],
                    amount: Some(currency(100)),
                    hand_root: vec![1],
                    reason: "pot".into(),
                },
                &state,
                5,
            )
            .expect("handler should succeed");
        assert_eq!(book.pages.len(), 1);
    }

    #[test]
    fn on_initiate_buy_in_delegates_to_handler() {
        let agg = PlayerAggregate;
        let state = funded_state(1000);
        let book = agg
            .on_initiate_buy_in(
                InitiateBuyIn {
                    table_root: vec![1, 2, 3],
                    seat: 2,
                    amount: Some(currency(300)),
                },
                &state,
                6,
            )
            .expect("handler should succeed");
        assert_eq!(book.pages.len(), 1);
    }

    #[test]
    fn on_confirm_buy_in_delegates_to_handler() {
        let agg = PlayerAggregate;
        let mut state = funded_state(1000);
        let rid = vec![1, 2, 3];
        apply_buy_in_requested(
            &mut state,
            BuyInRequested {
                reservation_id: rid.clone(),
                table_root: vec![0xaa],
                seat: 1,
                amount: Some(currency(100)),
                requested_at: None,
            },
        );
        let book = agg
            .on_confirm_buy_in(ConfirmBuyIn { reservation_id: rid }, &state, 7)
            .expect("handler should succeed");
        assert_eq!(book.pages.len(), 1);
    }

    #[test]
    fn on_release_buy_in_delegates_to_handler() {
        let agg = PlayerAggregate;
        let mut state = funded_state(1000);
        let rid = vec![1, 2, 3];
        apply_buy_in_requested(
            &mut state,
            BuyInRequested {
                reservation_id: rid.clone(),
                table_root: vec![0xaa],
                seat: 1,
                amount: Some(currency(100)),
                requested_at: None,
            },
        );
        let book = agg
            .on_release_buy_in(
                ReleaseBuyIn {
                    reservation_id: rid,
                    reason: "cancel".into(),
                },
                &state,
                8,
            )
            .expect("handler should succeed");
        assert_eq!(book.pages.len(), 1);
    }

    #[test]
    fn on_initiate_registration_delegates_to_handler() {
        let agg = PlayerAggregate;
        let state = registered_state();
        let book = agg
            .on_initiate_registration(
                InitiateTournamentRegistration {
                    tournament_root: vec![0x99],
                },
                &state,
                9,
            )
            .expect("handler should succeed");
        assert_eq!(book.pages.len(), 1);
    }

    #[test]
    fn on_confirm_registration_delegates_to_handler() {
        let agg = PlayerAggregate;
        let mut state = registered_state();
        let rid = vec![0x30];
        apply_registration_requested(
            &mut state,
            RegistrationRequested {
                reservation_id: rid.clone(),
                tournament_root: vec![0x99],
                fee: Some(currency(250)),
                requested_at: None,
            },
        );
        let book = agg
            .on_confirm_registration(
                ConfirmRegistrationFee { reservation_id: rid },
                &state,
                10,
            )
            .expect("handler should succeed");
        assert_eq!(book.pages.len(), 1);
    }

    #[test]
    fn on_release_registration_delegates_to_handler() {
        let agg = PlayerAggregate;
        let mut state = registered_state();
        let rid = vec![0x30];
        apply_registration_requested(
            &mut state,
            RegistrationRequested {
                reservation_id: rid.clone(),
                tournament_root: vec![0x99],
                fee: Some(currency(250)),
                requested_at: None,
            },
        );
        let book = agg
            .on_release_registration(
                ReleaseRegistrationFee {
                    reservation_id: rid,
                    reason: "cancel".into(),
                },
                &state,
                11,
            )
            .expect("handler should succeed");
        assert_eq!(book.pages.len(), 1);
    }

    #[test]
    fn on_initiate_rebuy_delegates_to_handler() {
        let agg = PlayerAggregate;
        let state = registered_state();
        let book = agg
            .on_initiate_rebuy(
                InitiateRebuy {
                    tournament_root: vec![1],
                    table_root: vec![2],
                    seat: 3,
                },
                &state,
                12,
            )
            .expect("handler should succeed");
        assert_eq!(book.pages.len(), 1);
    }

    #[test]
    fn on_confirm_rebuy_delegates_to_handler() {
        let agg = PlayerAggregate;
        let mut state = registered_state();
        let rid = vec![0x40];
        apply_rebuy_requested(
            &mut state,
            RebuyRequested {
                reservation_id: rid.clone(),
                tournament_root: vec![1],
                table_root: vec![2],
                seat: 3,
                fee: Some(currency(200)),
                requested_at: None,
            },
        );
        let book = agg
            .on_confirm_rebuy(ConfirmRebuyFee { reservation_id: rid }, &state, 13)
            .expect("handler should succeed");
        assert_eq!(book.pages.len(), 1);
    }

    #[test]
    fn on_release_rebuy_delegates_to_handler() {
        let agg = PlayerAggregate;
        let mut state = registered_state();
        let rid = vec![0x40];
        apply_rebuy_requested(
            &mut state,
            RebuyRequested {
                reservation_id: rid.clone(),
                tournament_root: vec![1],
                table_root: vec![2],
                seat: 3,
                fee: Some(currency(200)),
                requested_at: None,
            },
        );
        let book = agg
            .on_release_rebuy(
                ReleaseRebuyFee {
                    reservation_id: rid,
                    reason: "cancel".into(),
                },
                &state,
                14,
            )
            .expect("handler should succeed");
        assert_eq!(book.pages.len(), 1);
    }

    #[test]
    fn on_join_table_rejected_delegates_to_handler() {
        let agg = PlayerAggregate;
        let mut state = funded_state(1000);
        let table_root = vec![1u8, 2, 3, 4];
        apply_reserved(
            &mut state,
            FundsReserved {
                amount: Some(currency(400)),
                table_root: table_root.clone(),
                new_available_balance: Some(currency(600)),
                new_reserved_balance: Some(currency(400)),
                reserved_at: None,
            },
        );

        // Build a Notification carrying a RejectionNotification that references
        // the above table_root; the handler pulls the release amount from state.
        let rejected_command = CommandBook {
            cover: Some(Cover {
                domain: "table".into(),
                root: Some(ProtoUuid {
                    value: table_root.clone(),
                }),
                correlation_id: String::new(),
                edition: None,
            }),
            pages: vec![CommandPage {
                header: Some(PageHeader {
                    sequence_type: None,
                }),
                merge_strategy: 0,
                payload: None,
            }],
        };
        let rejection = RejectionNotification {
            rejected_command: Some(rejected_command),
            rejection_reason: "seat_occupied".into(),
        };
        let payload_any = Any {
            type_url: format!(
                "{}{}",
                angzarr_client::TYPE_URL_PREFIX,
                "angzarr.RejectionNotification"
            ),
            value: rejection.encode_to_vec(),
        };
        let notification = Notification {
            cover: Some(Cover {
                domain: "player".into(),
                root: Some(ProtoUuid {
                    value: vec![0xff; 16],
                }),
                correlation_id: String::new(),
                edition: None,
            }),
            payload: Some(payload_any),
            sent_at: None,
        };

        let response = agg
            .on_join_table_rejected(&notification, &state)
            .expect("handler should succeed");
        // Compensation response should carry an Events book with one page.
        let book = match response.result {
            Some(angzarr_client::proto::business_response::Result::Events(b)) => b,
            other => panic!("expected Events variant, got {:?}", other.is_some()),
        };
        assert_eq!(book.pages.len(), 1);
    }
}
