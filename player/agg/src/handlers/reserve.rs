//! ReserveFunds command handler.
//!
//! DOC: This file is referenced in docs/docs/examples/aggregates.mdx
//!      Update documentation when making changes to handler patterns.

use angzarr_client::proto::EventBook;
use angzarr_client::{event_page, pack_event, CommandRejectedError, CommandResult};
use examples_proto::{Currency, FundsReserved, ReserveFunds};

use crate::state::PlayerState;

// docs:start:reserve_funds_imp
fn reserve_funds_guard(state: &PlayerState) -> CommandResult<()> {
    if !state.exists() {
        return Err(CommandRejectedError::new("Player does not exist"));
    }
    Ok(())
}

fn reserve_funds_validate(cmd: &ReserveFunds, state: &PlayerState) -> CommandResult<i64> {
    let amount = cmd.amount.as_ref().map(|c| c.amount).unwrap_or(0);
    if amount <= 0 {
        return Err(CommandRejectedError::invalid_argument(
            "amount must be positive",
        ));
    }
    if amount > state.available_balance() {
        return Err(CommandRejectedError::new("Insufficient funds"));
    }

    let table_key = hex::encode(&cmd.table_root);
    if state.table_reservations.contains_key(&table_key) {
        return Err(CommandRejectedError::new(
            "Funds already reserved for this table",
        ));
    }

    Ok(amount)
}

fn reserve_funds_compute(cmd: &ReserveFunds, state: &PlayerState, amount: i64) -> FundsReserved {
    let new_reserved = state.reserved_funds + amount;
    let new_available = state.bankroll - new_reserved;

    FundsReserved {
        amount: cmd.amount.clone(),
        table_root: cmd.table_root.clone(),
        new_available_balance: Some(Currency {
            amount: new_available,
            currency_code: "CHIPS".to_string(),
        }),
        new_reserved_balance: Some(Currency {
            amount: new_reserved,
            currency_code: "CHIPS".to_string(),
        }),
        reserved_at: Some(angzarr_client::now()),
    }
}

pub fn handle_reserve_funds(
    cmd: ReserveFunds,
    state: &PlayerState,
    seq: u32,
) -> CommandResult<EventBook> {
    reserve_funds_guard(state)?;
    let amount = reserve_funds_validate(&cmd, state)?;

    let event = reserve_funds_compute(&cmd, state, amount);
    let event_any = pack_event(&event, "examples.FundsReserved");

    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}
// docs:end:reserve_funds_imp

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{apply_deposited, apply_registered, apply_reserved};
    use angzarr_client::proto::event_page::Payload;
    use examples_proto::{FundsDeposited, PlayerRegistered, PlayerType};
    use prost::Message;

    fn currency(amount: i64) -> Currency {
        Currency {
            amount,
            currency_code: "CHIPS".into(),
        }
    }

    fn funded_state(bankroll: i64) -> PlayerState {
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
        apply_deposited(
            &mut state,
            FundsDeposited {
                amount: Some(currency(bankroll)),
                new_balance: Some(currency(bankroll)),
                deposited_at: None,
            },
        );
        state
    }

    #[test]
    fn handle_reserve_funds_success_emits_funds_reserved() {
        let state = funded_state(1000);
        let cmd = ReserveFunds {
            amount: Some(currency(400)),
            table_root: vec![0xde, 0xad],
        };
        let book = handle_reserve_funds(cmd, &state, 3).expect("ok");
        assert_eq!(book.pages.len(), 1);
        let any = match book.pages[0].payload.as_ref() {
            Some(Payload::Event(any)) => any,
            _ => panic!("expected event"),
        };
        assert!(any.type_url.ends_with("examples.FundsReserved"));
        let decoded = FundsReserved::decode(any.value.as_slice()).expect("decode");
        assert_eq!(decoded.amount.unwrap().amount, 400);
        assert_eq!(decoded.new_available_balance.unwrap().amount, 600);
        assert_eq!(decoded.new_reserved_balance.unwrap().amount, 400);
    }

    #[test]
    fn handle_reserve_funds_rejects_when_no_player() {
        let state = PlayerState::default();
        let cmd = ReserveFunds {
            amount: Some(currency(100)),
            table_root: vec![1],
        };
        let err = handle_reserve_funds(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("does not exist"));
    }

    #[test]
    fn handle_reserve_funds_rejects_non_positive_amount() {
        let state = funded_state(1000);
        let cmd = ReserveFunds {
            amount: Some(currency(0)),
            table_root: vec![1],
        };
        let err = handle_reserve_funds(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("positive"));
    }

    #[test]
    fn handle_reserve_funds_rejects_insufficient_funds() {
        let state = funded_state(100);
        let cmd = ReserveFunds {
            amount: Some(currency(500)),
            table_root: vec![1],
        };
        let err = handle_reserve_funds(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("Insufficient"));
    }

    #[test]
    fn handle_reserve_funds_rejects_when_already_reserved_for_table() {
        let mut state = funded_state(1000);
        let table_root = vec![0xfe];
        apply_reserved(
            &mut state,
            FundsReserved {
                amount: Some(currency(100)),
                table_root: table_root.clone(),
                new_available_balance: Some(currency(900)),
                new_reserved_balance: Some(currency(100)),
                reserved_at: None,
            },
        );
        let cmd = ReserveFunds {
            amount: Some(currency(50)),
            table_root,
        };
        let err = handle_reserve_funds(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("already reserved"));
    }
}
