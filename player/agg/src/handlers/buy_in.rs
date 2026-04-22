//! Buy-in orchestration command handlers.

use angzarr_client::proto::EventBook;
use angzarr_client::{event_page, pack_event, CommandRejectedError, CommandResult};
use examples_proto::{
    BuyInConfirmed, BuyInRequested, BuyInReservationReleased, ConfirmBuyIn, InitiateBuyIn,
    ReleaseBuyIn,
};
use uuid::Uuid;

use crate::state::PlayerState;

// --- InitiateBuyIn ---

fn guard_initiate(state: &PlayerState) -> CommandResult<()> {
    if !state.exists() {
        return Err(CommandRejectedError::new("Player does not exist"));
    }
    Ok(())
}

fn validate_initiate(cmd: &InitiateBuyIn, state: &PlayerState) -> CommandResult<()> {
    if cmd.table_root.is_empty() {
        return Err(CommandRejectedError::new("table_root is required"));
    }

    let amount = cmd.amount.as_ref().map(|c| c.amount).unwrap_or(0);
    if amount <= 0 {
        return Err(CommandRejectedError::invalid_argument(
            "amount must be positive",
        ));
    }

    if amount > state.available_balance() {
        return Err(CommandRejectedError::new("Insufficient funds"));
    }

    Ok(())
}

pub fn handle_initiate_buy_in(
    cmd: InitiateBuyIn,
    state: &PlayerState,
    seq: u32,
) -> CommandResult<EventBook> {
    guard_initiate(state)?;
    validate_initiate(&cmd, state)?;

    let reservation_id = Uuid::new_v4().as_bytes().to_vec();

    let event = BuyInRequested {
        reservation_id,
        table_root: cmd.table_root,
        seat: cmd.seat,
        amount: cmd.amount,
        requested_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.BuyInRequested");

    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}

// --- ConfirmBuyIn ---

fn guard_confirm(state: &PlayerState) -> CommandResult<()> {
    if !state.exists() {
        return Err(CommandRejectedError::new("Player does not exist"));
    }
    Ok(())
}

fn validate_confirm(cmd: &ConfirmBuyIn, state: &PlayerState) -> CommandResult<()> {
    if cmd.reservation_id.is_empty() {
        return Err(CommandRejectedError::new("reservation_id is required"));
    }

    // Check that this reservation exists in pending buy-ins
    let reservation_hex = hex::encode(&cmd.reservation_id);
    if !state.pending_buy_ins.contains_key(&reservation_hex) {
        return Err(CommandRejectedError::new(
            "No pending buy-in with this reservation_id",
        ));
    }

    Ok(())
}

pub fn handle_confirm_buy_in(
    cmd: ConfirmBuyIn,
    state: &PlayerState,
    seq: u32,
) -> CommandResult<EventBook> {
    guard_confirm(state)?;
    validate_confirm(&cmd, state)?;

    let reservation_hex = hex::encode(&cmd.reservation_id);
    let pending = state.pending_buy_ins.get(&reservation_hex).unwrap();

    let event = BuyInConfirmed {
        reservation_id: cmd.reservation_id,
        table_root: pending.table_root.clone(),
        seat: pending.seat,
        amount: Some(examples_proto::Currency {
            amount: pending.amount,
            currency_code: "CHIPS".to_string(),
        }),
        confirmed_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.BuyInConfirmed");

    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}

// --- ReleaseBuyIn ---

fn guard_release(state: &PlayerState) -> CommandResult<()> {
    if !state.exists() {
        return Err(CommandRejectedError::new("Player does not exist"));
    }
    Ok(())
}

fn validate_release(cmd: &ReleaseBuyIn, state: &PlayerState) -> CommandResult<()> {
    if cmd.reservation_id.is_empty() {
        return Err(CommandRejectedError::new("reservation_id is required"));
    }

    let reservation_hex = hex::encode(&cmd.reservation_id);
    if !state.pending_buy_ins.contains_key(&reservation_hex) {
        return Err(CommandRejectedError::new(
            "No pending buy-in with this reservation_id",
        ));
    }

    Ok(())
}

pub fn handle_release_buy_in(
    cmd: ReleaseBuyIn,
    state: &PlayerState,
    seq: u32,
) -> CommandResult<EventBook> {
    guard_release(state)?;
    validate_release(&cmd, state)?;

    let event = BuyInReservationReleased {
        reservation_id: cmd.reservation_id,
        reason: cmd.reason,
        released_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.BuyInReservationReleased");

    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}

