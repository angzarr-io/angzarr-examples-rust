//! TransferFunds command handler.

use angzarr_client::proto::EventBook;
use angzarr_client::CommandResult;
use examples_utils::{event_page, invalid_arg, pack_event, rejected};
use examples_proto::{Currency, FundsTransferred, TransferFunds};

use crate::state::PlayerState;

fn transfer_funds_guard(state: &PlayerState) -> CommandResult<()> {
    if !state.exists() {
        return Err(rejected("Player does not exist"));
    }
    Ok(())
}

fn transfer_funds_validate(cmd: &TransferFunds) -> CommandResult<i64> {
    let amount = cmd.amount.as_ref().map(|c| c.amount).unwrap_or(0);
    if amount == 0 {
        return Err(invalid_arg("amount must be non-zero"));
    }
    Ok(amount)
}

fn transfer_funds_compute(
    cmd: &TransferFunds,
    state: &PlayerState,
    amount: i64,
) -> FundsTransferred {
    let new_balance = state.bankroll + amount;
    FundsTransferred {
        from_player_root: cmd.from_player_root.clone(),
        to_player_root: state.player_id.as_bytes().to_vec(),
        amount: cmd.amount.clone(),
        hand_root: cmd.hand_root.clone(),
        reason: cmd.reason.clone(),
        new_balance: Some(Currency {
            amount: new_balance,
            currency_code: "CHIPS".to_string(),
        }),
        transferred_at: Some(angzarr_client::now()),
    }
}

pub fn handle_transfer_funds(
    cmd: TransferFunds,
    state: &PlayerState,
    seq: u32,
) -> CommandResult<EventBook> {
    transfer_funds_guard(state)?;
    let amount = transfer_funds_validate(&cmd)?;

    let event = transfer_funds_compute(&cmd, state, amount);
    let event_any = pack_event(&event, "examples.FundsTransferred");

    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}
