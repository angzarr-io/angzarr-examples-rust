//! Registration command handlers.

use angzarr_client::proto::{CommandBook, EventBook};
use angzarr_client::{new_event_book, pack_event, CommandRejectedError, CommandResult, UnpackAny};
use examples_proto::{CloseRegistration, OpenRegistration, RegistrationClosed, RegistrationOpened};
use prost_types::Any;

use crate::state::TournamentState;

// --- OpenRegistration ---

fn guard_open(state: &TournamentState) -> CommandResult<()> {
    if !state.exists() {
        return Err(CommandRejectedError::new("Tournament does not exist"));
    }
    if state.is_registration_open() {
        return Err(CommandRejectedError::new("Registration is already open"));
    }
    if state.is_running() {
        return Err(CommandRejectedError::new(
            "Cannot open registration for a running tournament",
        ));
    }
    Ok(())
}

pub fn handle_open_registration(
    command_book: &CommandBook,
    command_any: &Any,
    state: &TournamentState,
    seq: u32,
) -> CommandResult<EventBook> {
    let _cmd: OpenRegistration = command_any
        .unpack()
        .map_err(|e| CommandRejectedError::new(format!("Failed to decode command: {}", e)))?;

    guard_open(state)?;

    let event = RegistrationOpened {
        opened_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.RegistrationOpened");

    Ok(new_event_book(command_book, seq, event_any))
}

// --- CloseRegistration ---

fn guard_close(state: &TournamentState) -> CommandResult<()> {
    if !state.exists() {
        return Err(CommandRejectedError::new("Tournament does not exist"));
    }
    if !state.is_registration_open() {
        return Err(CommandRejectedError::new("Registration is not open"));
    }
    Ok(())
}

pub fn handle_close_registration(
    command_book: &CommandBook,
    command_any: &Any,
    state: &TournamentState,
    seq: u32,
) -> CommandResult<EventBook> {
    let _cmd: CloseRegistration = command_any
        .unpack()
        .map_err(|e| CommandRejectedError::new(format!("Failed to decode command: {}", e)))?;

    guard_close(state)?;

    let event = RegistrationClosed {
        total_registrations: state.registered_players.len() as i32,
        closed_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.RegistrationClosed");

    Ok(new_event_book(command_book, seq, event_any))
}
