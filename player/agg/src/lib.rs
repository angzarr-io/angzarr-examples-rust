//! Player aggregate library.
//!
//! After the reservation refactor the player aggregate dispatches ONLY the
//! bankroll primitives. Lifecycle (buy-in / rebuy / registration) handlers
//! moved to `reservation/agg`; the reservation PM translates lifecycle events
//! into ReserveFunds / DeductReservedFunds / ReleaseFunds calls.

pub mod errors;
pub mod handlers;
pub mod state;

pub use state::PlayerState;

use angzarr_client::proto::{BusinessResponse, EventBook, Notification};
use angzarr_client::{command_handler, CommandResult};
use examples_proto::{
    DeductReservedFunds, DepositFunds, FundsDeducted, FundsDeposited, FundsReleased, FundsReserved,
    FundsTransferred, FundsWithdrawn, PlayerRegistered, RegisterPlayer, ReleaseFunds, ReserveFunds,
    TransferFunds, WithdrawFunds,
};

use crate::state::{
    apply_deposited, apply_funds_deducted, apply_registered, apply_released, apply_reserved,
    apply_transferred, apply_withdrawn,
};

// docs:start:command_router
/// Player aggregate - dispatches bankroll commands and applies events.
pub struct PlayerAggregate;

#[command_handler(domain = "player", state = PlayerState)]
impl PlayerAggregate {
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

    #[handles(DeductReservedFunds)]
    fn on_deduct_reserved_funds(
        &self,
        cmd: DeductReservedFunds,
        state: &PlayerState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        handlers::handle_deduct_reserved_funds(cmd, state, seq)
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

    #[applies(FundsDeducted)]
    fn apply_deducted(state: &mut PlayerState, event: FundsDeducted) {
        apply_funds_deducted(state, event);
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
        let key = vec![1u8, 2];
        PlayerAggregate::apply_reserved(
            &mut state,
            FundsReserved {
                amount: Some(currency(200)),
                key: key.clone(),
                new_available_balance: Some(currency(800)),
                new_reserved_balance: Some(currency(200)),
                reserved_at: None,
            },
        );
        assert_eq!(state.reserved_funds, 200);
        assert_eq!(state.table_reservations.get(&hex::encode(&key)), Some(&200));
    }

    #[test]
    fn apply_released_delegates_to_state_fn() {
        let mut state = PlayerState {
            bankroll: 1000,
            reserved_funds: 200,
            ..PlayerState::default()
        };
        let key = vec![5u8];
        state.table_reservations.insert(hex::encode(&key), 200);
        PlayerAggregate::apply_released(
            &mut state,
            FundsReleased {
                amount: Some(currency(200)),
                key: key.clone(),
                new_available_balance: Some(currency(1000)),
                new_reserved_balance: Some(currency(0)),
                released_at: None,
            },
        );
        assert_eq!(state.reserved_funds, 0);
        assert!(!state.table_reservations.contains_key(&hex::encode(&key)));
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
    fn apply_deducted_delegates_to_state_fn() {
        let mut state = PlayerState {
            bankroll: 1000,
            reserved_funds: 200,
            ..PlayerState::default()
        };
        let key = vec![5u8];
        state.table_reservations.insert(hex::encode(&key), 200);
        PlayerAggregate::apply_deducted(
            &mut state,
            FundsDeducted {
                amount: Some(currency(200)),
                key: key.clone(),
                reservation_id: vec![0xaa],
                new_balance: Some(currency(800)),
                new_reserved_balance: Some(currency(0)),
                deducted_at: None,
            },
        );
        assert_eq!(state.bankroll, 800);
        assert_eq!(state.reserved_funds, 0);
        assert!(!state.table_reservations.contains_key(&hex::encode(&key)));
    }
}

#[cfg(test)]
mod handler_tests {
    //! Direct-call tests for each `#[handles]` method on `PlayerAggregate`.
    use super::*;
    use crate::state::{apply_deposited, apply_registered, apply_reserved};
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
                    key: vec![0xab, 0xcd],
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
        let key = vec![0xab, 0xcd];
        apply_reserved(
            &mut state,
            FundsReserved {
                amount: Some(currency(200)),
                key: key.clone(),
                new_available_balance: Some(currency(800)),
                new_reserved_balance: Some(currency(200)),
                reserved_at: None,
            },
        );
        let book = agg
            .on_release_funds(ReleaseFunds { key }, &state, 4)
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
    fn on_deduct_reserved_funds_delegates_to_handler() {
        let agg = PlayerAggregate;
        let mut state = funded_state(1000);
        let key = vec![0xab, 0xcd];
        apply_reserved(
            &mut state,
            FundsReserved {
                amount: Some(currency(200)),
                key: key.clone(),
                new_available_balance: Some(currency(800)),
                new_reserved_balance: Some(currency(200)),
                reserved_at: None,
            },
        );
        let book = agg
            .on_deduct_reserved_funds(
                DeductReservedFunds {
                    amount: Some(currency(200)),
                    key,
                    reservation_id: vec![0xaa],
                },
                &state,
                6,
            )
            .expect("handler should succeed");
        assert_eq!(book.pages.len(), 1);
    }

    #[test]
    fn on_join_table_rejected_delegates_to_handler() {
        let agg = PlayerAggregate;
        let mut state = funded_state(1000);
        let key = vec![1u8, 2, 3, 4];
        apply_reserved(
            &mut state,
            FundsReserved {
                amount: Some(currency(400)),
                key: key.clone(),
                new_available_balance: Some(currency(600)),
                new_reserved_balance: Some(currency(400)),
                reserved_at: None,
            },
        );

        let rejected_command = CommandBook {
            cover: Some(Cover {
                domain: "table".into(),
                root: Some(ProtoUuid { value: key.clone() }),
                correlation_id: String::new(),
                edition: None,
            }),
            pages: vec![CommandPage {
                header: Some(PageHeader {
                    sync_mode: None,
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
        let book = match response.result {
            Some(angzarr_client::proto::business_response::Result::Events(b)) => b,
            other => panic!("expected Events variant, got {:?}", other.is_some()),
        };
        assert_eq!(book.pages.len(), 1);
    }
}
