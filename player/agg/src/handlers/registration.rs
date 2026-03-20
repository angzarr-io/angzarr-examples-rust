//! Tournament registration orchestration command handlers.

use angzarr_client::proto::{CommandBook, EventBook};
use angzarr_client::{new_event_book, pack_event, CommandRejectedError, CommandResult, UnpackAny};
use examples_proto::{
    ConfirmRegistrationFee, InitiateTournamentRegistration, RegistrationFeeConfirmed,
    RegistrationFeeReleased, RegistrationRequested, ReleaseRegistrationFee,
};
use prost_types::Any;
use uuid::Uuid;

use crate::state::PlayerState;

// --- InitiateTournamentRegistration ---

fn guard_initiate(state: &PlayerState) -> CommandResult<()> {
    if !state.exists() {
        return Err(CommandRejectedError::new("Player does not exist"));
    }
    Ok(())
}

fn validate_initiate(cmd: &InitiateTournamentRegistration) -> CommandResult<()> {
    if cmd.tournament_root.is_empty() {
        return Err(CommandRejectedError::new("tournament_root is required"));
    }
    Ok(())
}

pub fn handle_initiate_tournament_registration(
    command_book: &CommandBook,
    command_any: &Any,
    state: &PlayerState,
    seq: u32,
) -> CommandResult<EventBook> {
    let cmd: InitiateTournamentRegistration = command_any
        .unpack()
        .map_err(|e| CommandRejectedError::new(format!("Failed to decode command: {}", e)))?;

    guard_initiate(state)?;
    validate_initiate(&cmd)?;

    // Generate reservation_id for this registration flow
    let reservation_id = Uuid::new_v4().as_bytes().to_vec();

    // Note: The fee will be looked up from Tournament state by the PM
    // For now, we emit the event and the PM will validate/set the fee
    let event = RegistrationRequested {
        reservation_id,
        tournament_root: cmd.tournament_root,
        fee: None, // PM will fill this in from Tournament state
        requested_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.RegistrationRequested");

    Ok(new_event_book(command_book, seq, event_any))
}

// --- ConfirmRegistrationFee ---

fn guard_confirm(state: &PlayerState) -> CommandResult<()> {
    if !state.exists() {
        return Err(CommandRejectedError::new("Player does not exist"));
    }
    Ok(())
}

fn validate_confirm(cmd: &ConfirmRegistrationFee, state: &PlayerState) -> CommandResult<()> {
    if cmd.reservation_id.is_empty() {
        return Err(CommandRejectedError::new("reservation_id is required"));
    }

    let reservation_hex = hex::encode(&cmd.reservation_id);
    if !state.pending_registrations.contains_key(&reservation_hex) {
        return Err(CommandRejectedError::new(
            "No pending registration with this reservation_id",
        ));
    }

    Ok(())
}

pub fn handle_confirm_registration_fee(
    command_book: &CommandBook,
    command_any: &Any,
    state: &PlayerState,
    seq: u32,
) -> CommandResult<EventBook> {
    let cmd: ConfirmRegistrationFee = command_any
        .unpack()
        .map_err(|e| CommandRejectedError::new(format!("Failed to decode command: {}", e)))?;

    guard_confirm(state)?;
    validate_confirm(&cmd, state)?;

    let reservation_hex = hex::encode(&cmd.reservation_id);
    let pending = state.pending_registrations.get(&reservation_hex).unwrap();

    let event = RegistrationFeeConfirmed {
        reservation_id: cmd.reservation_id,
        tournament_root: pending.tournament_root.clone(),
        fee: Some(examples_proto::Currency {
            amount: pending.fee,
            currency_code: "CHIPS".to_string(),
        }),
        confirmed_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.RegistrationFeeConfirmed");

    Ok(new_event_book(command_book, seq, event_any))
}

// --- ReleaseRegistrationFee ---

fn guard_release(state: &PlayerState) -> CommandResult<()> {
    if !state.exists() {
        return Err(CommandRejectedError::new("Player does not exist"));
    }
    Ok(())
}

fn validate_release(cmd: &ReleaseRegistrationFee, state: &PlayerState) -> CommandResult<()> {
    if cmd.reservation_id.is_empty() {
        return Err(CommandRejectedError::new("reservation_id is required"));
    }

    let reservation_hex = hex::encode(&cmd.reservation_id);
    if !state.pending_registrations.contains_key(&reservation_hex) {
        return Err(CommandRejectedError::new(
            "No pending registration with this reservation_id",
        ));
    }

    Ok(())
}

pub fn handle_release_registration_fee(
    command_book: &CommandBook,
    command_any: &Any,
    state: &PlayerState,
    seq: u32,
) -> CommandResult<EventBook> {
    let cmd: ReleaseRegistrationFee = command_any
        .unpack()
        .map_err(|e| CommandRejectedError::new(format!("Failed to decode command: {}", e)))?;

    guard_release(state)?;
    validate_release(&cmd, state)?;

    let event = RegistrationFeeReleased {
        reservation_id: cmd.reservation_id,
        reason: cmd.reason,
        released_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.RegistrationFeeReleased");

    Ok(new_event_book(command_book, seq, event_any))
}
