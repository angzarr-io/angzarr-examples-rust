//! Rebuy orchestration command handlers.

use angzarr_client::proto::EventBook;
use angzarr_client::{event_page, pack_event, CommandRejectedError, CommandResult};
use examples_proto::{
    ConfirmRebuyFee, InitiateRebuy, RebuyFeeConfirmed, RebuyFeeReleased, RebuyRequested,
    ReleaseRebuyFee,
};
use uuid::Uuid;

use crate::state::PlayerState;

// --- InitiateRebuy ---

fn guard_initiate(state: &PlayerState) -> CommandResult<()> {
    if !state.exists() {
        return Err(CommandRejectedError::new("Player does not exist"));
    }
    Ok(())
}

fn validate_initiate(cmd: &InitiateRebuy) -> CommandResult<()> {
    if cmd.tournament_root.is_empty() {
        return Err(CommandRejectedError::new("tournament_root is required"));
    }
    if cmd.table_root.is_empty() {
        return Err(CommandRejectedError::new("table_root is required"));
    }
    Ok(())
}

pub fn handle_initiate_rebuy(
    cmd: InitiateRebuy,
    state: &PlayerState,
    seq: u32,
) -> CommandResult<EventBook> {
    guard_initiate(state)?;
    validate_initiate(&cmd)?;

    let reservation_id = Uuid::new_v4().as_bytes().to_vec();

    let event = RebuyRequested {
        reservation_id,
        tournament_root: cmd.tournament_root,
        table_root: cmd.table_root,
        seat: cmd.seat,
        fee: None,
        requested_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.RebuyRequested");

    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}

// --- ConfirmRebuyFee ---

fn guard_confirm(state: &PlayerState) -> CommandResult<()> {
    if !state.exists() {
        return Err(CommandRejectedError::new("Player does not exist"));
    }
    Ok(())
}

fn validate_confirm(cmd: &ConfirmRebuyFee, state: &PlayerState) -> CommandResult<()> {
    if cmd.reservation_id.is_empty() {
        return Err(CommandRejectedError::new("reservation_id is required"));
    }

    let reservation_hex = hex::encode(&cmd.reservation_id);
    if !state.pending_rebuys.contains_key(&reservation_hex) {
        return Err(CommandRejectedError::new(
            "No pending rebuy with this reservation_id",
        ));
    }

    Ok(())
}

pub fn handle_confirm_rebuy_fee(
    cmd: ConfirmRebuyFee,
    state: &PlayerState,
    seq: u32,
) -> CommandResult<EventBook> {
    guard_confirm(state)?;
    validate_confirm(&cmd, state)?;

    let reservation_hex = hex::encode(&cmd.reservation_id);
    let pending = state.pending_rebuys.get(&reservation_hex).unwrap();

    let event = RebuyFeeConfirmed {
        reservation_id: cmd.reservation_id,
        tournament_root: pending.tournament_root.clone(),
        fee: Some(examples_proto::Currency {
            amount: pending.fee,
            currency_code: "CHIPS".to_string(),
        }),
        chips_added: pending.chips_to_add,
        confirmed_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.RebuyFeeConfirmed");

    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}

// --- ReleaseRebuyFee ---

fn guard_release(state: &PlayerState) -> CommandResult<()> {
    if !state.exists() {
        return Err(CommandRejectedError::new("Player does not exist"));
    }
    Ok(())
}

fn validate_release(cmd: &ReleaseRebuyFee, state: &PlayerState) -> CommandResult<()> {
    if cmd.reservation_id.is_empty() {
        return Err(CommandRejectedError::new("reservation_id is required"));
    }

    let reservation_hex = hex::encode(&cmd.reservation_id);
    if !state.pending_rebuys.contains_key(&reservation_hex) {
        return Err(CommandRejectedError::new(
            "No pending rebuy with this reservation_id",
        ));
    }

    Ok(())
}

pub fn handle_release_rebuy_fee(
    cmd: ReleaseRebuyFee,
    state: &PlayerState,
    seq: u32,
) -> CommandResult<EventBook> {
    guard_release(state)?;
    validate_release(&cmd, state)?;

    let event = RebuyFeeReleased {
        reservation_id: cmd.reservation_id,
        reason: cmd.reason,
        released_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.RebuyFeeReleased");

    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}
