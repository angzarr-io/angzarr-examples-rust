//! ReleaseFunds command handler.

use angzarr_client::proto::EventBook;
use angzarr_client::{event_page, pack_event, CommandRejectedError, CommandResult};
use examples_proto::{Currency, FundsReleased, ReleaseFunds};

use crate::state::PlayerState;

fn release_funds_guard(state: &PlayerState) -> CommandResult<()> {
    if !state.exists() {
        return Err(CommandRejectedError::new("Player does not exist"));
    }
    Ok(())
}

fn release_funds_validate(cmd: &ReleaseFunds, state: &PlayerState) -> CommandResult<i64> {
    if cmd.table_root.is_empty() {
        return Err(CommandRejectedError::new("table_root is required"));
    }

    let table_key = hex::encode(&cmd.table_root);
    match state.table_reservations.get(&table_key) {
        Some(&amount) => Ok(amount),
        None => Err(CommandRejectedError::new(
            "No funds reserved for this table",
        )),
    }
}

fn release_funds_compute(cmd: &ReleaseFunds, state: &PlayerState, reserved: i64) -> FundsReleased {
    let new_reserved = state.reserved_funds - reserved;
    let new_available = state.bankroll - new_reserved;

    FundsReleased {
        amount: Some(Currency {
            amount: reserved,
            currency_code: "CHIPS".to_string(),
        }),
        table_root: cmd.table_root.clone(),
        new_available_balance: Some(Currency {
            amount: new_available,
            currency_code: "CHIPS".to_string(),
        }),
        new_reserved_balance: Some(Currency {
            amount: new_reserved,
            currency_code: "CHIPS".to_string(),
        }),
        released_at: Some(angzarr_client::now()),
    }
}

pub fn handle_release_funds(
    cmd: ReleaseFunds,
    state: &PlayerState,
    seq: u32,
) -> CommandResult<EventBook> {
    release_funds_guard(state)?;
    let reserved = release_funds_validate(&cmd, state)?;

    let event = release_funds_compute(&cmd, state, reserved);
    let event_any = pack_event(&event, "examples.FundsReleased");

    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}
