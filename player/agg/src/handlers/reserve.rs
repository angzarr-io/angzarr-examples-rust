//! ReserveFunds command handler.
//!
//! DOC: This file is referenced in docs/docs/examples/aggregates.mdx
//!      Update documentation when making changes to handler patterns.

use angzarr_client::proto::EventBook;
use angzarr_client::CommandResult;
use examples_proto::{Currency, FundsReserved, ReserveFunds};
use examples_utils::{event_page, pack_event, reject};

use crate::errors::{
    AmountMustBePositive, FundsAlreadyReservedForTable, InsufficientFunds, PlayerNotFound,
};
use crate::state::PlayerState;

// docs:start:reserve_funds_imp
fn reserve_funds_guard(state: &PlayerState) -> CommandResult<()> {
    if !state.exists() {
        return Err(reject(PlayerNotFound));
    }
    Ok(())
}

fn reserve_funds_validate(cmd: &ReserveFunds, state: &PlayerState) -> CommandResult<i64> {
    let amount = cmd.amount.as_ref().map(|c| c.amount).unwrap_or(0);
    if amount <= 0 {
        return Err(reject(AmountMustBePositive { value: amount }));
    }
    if amount > state.available_balance() {
        return Err(reject(InsufficientFunds {
            requested: amount,
            available: state.available_balance(),
        }));
    }

    let key_hex = hex::encode(&cmd.key);
    if state.table_reservations.contains_key(&key_hex) {
        return Err(reject(FundsAlreadyReservedForTable {
            table_root_hex: key_hex,
        }));
    }

    Ok(amount)
}

fn reserve_funds_compute(cmd: &ReserveFunds, state: &PlayerState, amount: i64) -> FundsReserved {
    let new_reserved = state.reserved_funds + amount;
    let new_available = state.bankroll - new_reserved;

    FundsReserved {
        amount: cmd.amount.clone(),
        key: cmd.key.clone(),
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
