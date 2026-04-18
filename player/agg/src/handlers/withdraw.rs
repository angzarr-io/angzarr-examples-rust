//! WithdrawFunds command handler.

use angzarr_client::proto::EventBook;
use angzarr_client::{event_page, pack_event, CommandRejectedError, CommandResult};
use examples_proto::{Currency, FundsWithdrawn, WithdrawFunds};

use crate::state::PlayerState;

fn withdraw_funds_guard(state: &PlayerState) -> CommandResult<()> {
    if !state.exists() {
        return Err(CommandRejectedError::new("Player does not exist"));
    }
    Ok(())
}

fn withdraw_funds_validate(cmd: &WithdrawFunds, state: &PlayerState) -> CommandResult<i64> {
    let amount = cmd.amount.as_ref().map(|c| c.amount).unwrap_or(0);
    if amount <= 0 {
        return Err(CommandRejectedError::invalid_argument(
            "amount must be positive",
        ));
    }
    if amount > state.available_balance() {
        return Err(CommandRejectedError::new("insufficient available balance"));
    }
    Ok(amount)
}

fn withdraw_funds_compute(cmd: &WithdrawFunds, state: &PlayerState, amount: i64) -> FundsWithdrawn {
    let new_balance = state.bankroll - amount;
    FundsWithdrawn {
        amount: cmd.amount.clone(),
        new_balance: Some(Currency {
            amount: new_balance,
            currency_code: "CHIPS".to_string(),
        }),
        withdrawn_at: Some(angzarr_client::now()),
    }
}

pub fn handle_withdraw_funds(
    cmd: WithdrawFunds,
    state: &PlayerState,
    seq: u32,
) -> CommandResult<EventBook> {
    withdraw_funds_guard(state)?;
    let amount = withdraw_funds_validate(&cmd, state)?;

    let event = withdraw_funds_compute(&cmd, state, amount);
    let event_any = pack_event(&event, "examples.FundsWithdrawn");

    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{apply_deposited, apply_registered};
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
    fn handle_withdraw_funds_success_decreases_balance_in_event() {
        let state = funded_state(500);
        let cmd = WithdrawFunds {
            amount: Some(currency(200)),
        };
        let book = handle_withdraw_funds(cmd, &state, 2).expect("ok");
        assert_eq!(book.pages.len(), 1);
        let any = match book.pages[0].payload.as_ref() {
            Some(Payload::Event(any)) => any,
            _ => panic!("expected event"),
        };
        assert!(any.type_url.ends_with("examples.FundsWithdrawn"));
        let decoded = FundsWithdrawn::decode(any.value.as_slice()).expect("decode");
        assert_eq!(decoded.amount.unwrap().amount, 200);
        assert_eq!(decoded.new_balance.unwrap().amount, 300);
    }

    #[test]
    fn handle_withdraw_funds_rejects_when_no_player() {
        let state = PlayerState::default();
        let cmd = WithdrawFunds {
            amount: Some(currency(50)),
        };
        let err = handle_withdraw_funds(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("does not exist"));
    }

    #[test]
    fn handle_withdraw_funds_rejects_non_positive_amount() {
        let state = funded_state(500);
        let cmd = WithdrawFunds {
            amount: Some(currency(-10)),
        };
        let err = handle_withdraw_funds(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("positive"));
    }

    #[test]
    fn handle_withdraw_funds_rejects_insufficient_available_balance() {
        let state = funded_state(100);
        let cmd = WithdrawFunds {
            amount: Some(currency(500)),
        };
        let err = handle_withdraw_funds(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("insufficient"));
    }
}
