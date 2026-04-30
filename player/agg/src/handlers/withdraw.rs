//! WithdrawFunds command handler.

use angzarr_client::proto::EventBook;
use angzarr_client::{CommandRejectedError, CommandResult};
use examples_utils::{event_page, invalid_arg, pack_event, rejected};
use examples_proto::{Currency, FundsWithdrawn, WithdrawFunds};

use crate::state::PlayerState;

fn withdraw_funds_guard(state: &PlayerState) -> CommandResult<()> {
    if !state.exists() {
        return Err(rejected("Player does not exist"));
    }
    Ok(())
}

fn withdraw_funds_validate(cmd: &WithdrawFunds, state: &PlayerState) -> CommandResult<i64> {
    let amount = cmd.amount.as_ref().map(|c| c.amount).unwrap_or(0);
    if amount <= 0 {
        return Err(invalid_arg("amount must be positive"));
    }
    if amount > state.available_balance() {
        return Err(rejected("insufficient available balance"));
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
