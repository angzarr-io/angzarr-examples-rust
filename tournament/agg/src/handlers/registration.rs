//! Registration command handlers.

use angzarr_client::proto::EventBook;
use angzarr_client::{event_page, pack_event, CommandRejectedError, CommandResult};
use examples_proto::{CloseRegistration, OpenRegistration, RegistrationClosed, RegistrationOpened};

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
    _cmd: OpenRegistration,
    state: &TournamentState,
    seq: u32,
) -> CommandResult<EventBook> {
    guard_open(state)?;

    let event = RegistrationOpened {
        opened_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.RegistrationOpened");

    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
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
    _cmd: CloseRegistration,
    state: &TournamentState,
    seq: u32,
) -> CommandResult<EventBook> {
    guard_close(state)?;

    let event = RegistrationClosed {
        total_registrations: state.registered_players.len() as i32,
        closed_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.RegistrationClosed");

    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}
