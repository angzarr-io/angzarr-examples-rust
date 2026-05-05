//! DepositFunds command handler.

use angzarr_client::proto::EventBook;
use angzarr_client::CommandResult;
use examples_proto::{Currency, DepositFunds, FundsDeposited};
use examples_utils::{event_page, pack_event, reject};

use crate::errors::{AmountMustBePositive, PlayerNotFound};
use crate::state::PlayerState;

// docs:start:deposit_funds_guard
fn deposit_funds_guard(state: &PlayerState) -> CommandResult<()> {
    if !state.exists() {
        return Err(reject(PlayerNotFound));
    }
    Ok(())
}
// docs:end:deposit_funds_guard

// docs:start:deposit_funds_validate
fn deposit_funds_validate(cmd: &DepositFunds) -> CommandResult<i64> {
    let amount = cmd.amount.as_ref().map(|c| c.amount).unwrap_or(0);
    if amount <= 0 {
        return Err(reject(AmountMustBePositive { value: amount }));
    }
    Ok(amount)
}
// docs:end:deposit_funds_validate

// docs:start:deposit_funds_compute
fn deposit_funds_compute(cmd: &DepositFunds, state: &PlayerState, amount: i64) -> FundsDeposited {
    let new_balance = state.bankroll + amount;
    FundsDeposited {
        amount: cmd.amount.clone(),
        new_balance: Some(Currency {
            amount: new_balance,
            currency_code: "CHIPS".to_string(),
        }),
        deposited_at: Some(angzarr_client::now()),
    }
}
// docs:end:deposit_funds_compute

// docs:start:polyglot_handler
pub fn handle_deposit_funds(
    cmd: DepositFunds,
    state: &PlayerState,
    seq: u32,
) -> CommandResult<EventBook> {
    deposit_funds_guard(state)?;
    let amount = deposit_funds_validate(&cmd)?;

    let event = deposit_funds_compute(&cmd, state, amount);
    let event_any = pack_event(&event, "examples.FundsDeposited");

    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}
// docs:end:polyglot_handler

// docs:start:unit_test_deposit
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deposit_increases_bankroll() {
        let state = PlayerState {
            player_id: "player_1".to_string(),
            bankroll: 1000,
            ..Default::default()
        };
        let cmd = DepositFunds {
            amount: Some(Currency {
                amount: 500,
                currency_code: "CHIPS".to_string(),
            }),
        };

        let event = deposit_funds_compute(&cmd, &state, 500);

        assert_eq!(event.new_balance.unwrap().amount, 1500);
    }

    #[test]
    fn test_deposit_rejects_non_existent_player() {
        let state = PlayerState::default();

        let result = deposit_funds_guard(&state);

        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("does not exist"));
    }

    #[test]
    fn test_deposit_rejects_zero_amount() {
        let cmd = DepositFunds {
            amount: Some(Currency {
                amount: 0,
                currency_code: "CHIPS".to_string(),
            }),
        };

        let result = deposit_funds_validate(&cmd);

        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("positive"));
    }

    use crate::state::apply_registered;
    use angzarr_client::proto::event_page::Payload;
    use examples_proto::{PlayerRegistered, PlayerType};
    use prost::Message;

    fn registered() -> PlayerState {
        let mut s = PlayerState::default();
        apply_registered(
            &mut s,
            PlayerRegistered {
                display_name: "A".into(),
                email: "a@x".into(),
                player_type: PlayerType::Human as i32,
                ai_model_id: String::new(),
                registered_at: None,
            },
        );
        s
    }

    #[test]
    fn handle_deposit_funds_success_emits_event() {
        let mut state = registered();
        state.bankroll = 100;
        let cmd = DepositFunds {
            amount: Some(Currency {
                amount: 400,
                currency_code: "CHIPS".into(),
            }),
        };
        let book = handle_deposit_funds(cmd, &state, 1).expect("ok");
        assert_eq!(book.pages.len(), 1);
        let any = match book.pages[0].payload.as_ref() {
            Some(Payload::Event(a)) => a,
            _ => panic!(),
        };
        assert!(any.type_url.ends_with("examples.FundsDeposited"));
        let decoded = FundsDeposited::decode(any.value.as_slice()).unwrap();
        assert_eq!(decoded.new_balance.unwrap().amount, 500);
    }

    #[test]
    fn handle_deposit_funds_rejects_when_no_player() {
        let state = PlayerState::default();
        let cmd = DepositFunds {
            amount: Some(Currency {
                amount: 100,
                currency_code: "CHIPS".into(),
            }),
        };
        let err = handle_deposit_funds(cmd, &state, 1).unwrap_err();
        assert!(err.message.contains("does not exist"));
    }

    #[test]
    fn handle_deposit_funds_rejects_non_positive_amount() {
        let state = registered();
        let cmd = DepositFunds {
            amount: Some(Currency {
                amount: 0,
                currency_code: "CHIPS".into(),
            }),
        };
        let err = handle_deposit_funds(cmd, &state, 1).unwrap_err();
        assert!(err.message.contains("positive"));
    }
}
// docs:end:unit_test_deposit
