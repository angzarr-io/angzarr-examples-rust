//! ReleaseFunds command handler.

use angzarr_client::proto::EventBook;
use angzarr_client::CommandResult;
use examples_proto::{Currency, FundsReleased, ReleaseFunds};
use examples_utils::{event_page, pack_event, reject};

use crate::errors::{NoFundsReservedForTable, PlayerNotFound, TableRootRequired};
use crate::state::PlayerState;

fn release_funds_guard(state: &PlayerState) -> CommandResult<()> {
    if !state.exists() {
        return Err(reject(PlayerNotFound));
    }
    Ok(())
}

fn release_funds_validate(cmd: &ReleaseFunds, state: &PlayerState) -> CommandResult<i64> {
    if cmd.key.is_empty() {
        return Err(reject(TableRootRequired));
    }

    let key_hex = hex::encode(&cmd.key);
    match state.table_reservations.get(&key_hex) {
        Some(&amount) if amount > 0 => Ok(amount),
        _ => Err(reject(NoFundsReservedForTable {
            table_root_hex: key_hex,
        })),
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
        key: cmd.key.clone(),
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
