//! DeductReservedFunds command handler.
//!
//! Issued by the reservation PM after a confirmed lifecycle action
//! (BuyInConfirmed / RebuyFeeConfirmed / RegistrationFeeConfirmed). Settles
//! a previously-reserved amount: bankroll AND reserved_funds drop.

use angzarr_client::proto::EventBook;
use angzarr_client::{CommandRejectedError, CommandResult};
use examples_proto::{Currency, DeductReservedFunds, FundsDeducted};
use examples_utils::{event_page, invalid_arg, pack_event, rejected};

use crate::state::PlayerState;

fn deduct_guard(state: &PlayerState) -> CommandResult<()> {
    if !state.exists() {
        return Err(rejected("Player does not exist"));
    }
    Ok(())
}

fn deduct_validate(cmd: &DeductReservedFunds, state: &PlayerState) -> CommandResult<i64> {
    if cmd.key.is_empty() {
        return Err(rejected("key is required"));
    }
    let amount = cmd.amount.as_ref().map(|c| c.amount).unwrap_or(0);
    if amount <= 0 {
        return Err(invalid_arg("amount must be positive"));
    }
    let key_hex = hex::encode(&cmd.key);
    let reserved_for_key = state.table_reservations.get(&key_hex).copied().unwrap_or(0);
    if amount > reserved_for_key {
        return Err(rejected("amount exceeds reserved funds for this key"));
    }
    Ok(amount)
}

fn deduct_compute(cmd: &DeductReservedFunds, state: &PlayerState, amount: i64) -> FundsDeducted {
    let new_reserved = state.reserved_funds - amount;
    let new_balance = state.bankroll - amount;
    FundsDeducted {
        amount: Some(Currency {
            amount,
            currency_code: "CHIPS".to_string(),
        }),
        key: cmd.key.clone(),
        reservation_id: cmd.reservation_id.clone(),
        new_balance: Some(Currency {
            amount: new_balance,
            currency_code: "CHIPS".to_string(),
        }),
        new_reserved_balance: Some(Currency {
            amount: new_reserved,
            currency_code: "CHIPS".to_string(),
        }),
        deducted_at: Some(angzarr_client::now()),
    }
}

pub fn handle_deduct_reserved_funds(
    cmd: DeductReservedFunds,
    state: &PlayerState,
    seq: u32,
) -> CommandResult<EventBook> {
    deduct_guard(state)?;
    let amount = deduct_validate(&cmd, state)?;

    let event = deduct_compute(&cmd, state, amount);
    let event_any = pack_event(&event, "examples.FundsDeducted");

    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}
