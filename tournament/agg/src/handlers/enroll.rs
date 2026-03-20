//! EnrollPlayer command handler.

use angzarr_client::proto::{CommandBook, EventBook};
use angzarr_client::{new_event_book, pack_event, CommandRejectedError, CommandResult, UnpackAny};
use examples_proto::{EnrollPlayer, TournamentEnrollmentRejected, TournamentPlayerEnrolled};
use prost_types::Any;

use crate::state::TournamentState;

fn guard(state: &TournamentState) -> CommandResult<()> {
    if !state.exists() {
        return Err(CommandRejectedError::new("Tournament does not exist"));
    }
    Ok(())
}

fn validate(cmd: &EnrollPlayer, state: &TournamentState) -> Result<(), String> {
    if cmd.player_root.is_empty() {
        return Err("player_root is required".to_string());
    }

    if !state.is_registration_open() {
        return Err("Registration is not open".to_string());
    }

    if !state.has_capacity() {
        return Err("Tournament is full".to_string());
    }

    let player_root_hex = hex::encode(&cmd.player_root);
    if state.is_player_registered(&player_root_hex) {
        return Err("Player is already registered".to_string());
    }

    Ok(())
}

pub fn handle_enroll_player(
    command_book: &CommandBook,
    command_any: &Any,
    state: &TournamentState,
    seq: u32,
) -> CommandResult<EventBook> {
    let cmd: EnrollPlayer = command_any
        .unpack()
        .map_err(|e| CommandRejectedError::new(format!("Failed to decode command: {}", e)))?;

    guard(state)?;

    // Validate and produce appropriate event
    match validate(&cmd, state) {
        Ok(()) => {
            let event = TournamentPlayerEnrolled {
                player_root: cmd.player_root,
                reservation_id: cmd.reservation_id,
                fee_paid: state.buy_in,
                starting_stack: state.starting_stack,
                registration_number: (state.registered_players.len() + 1) as i32,
                enrolled_at: Some(angzarr_client::now()),
            };
            let event_any = pack_event(&event, "examples.TournamentPlayerEnrolled");
            Ok(new_event_book(command_book, seq, event_any))
        }
        Err(reason) => {
            let event = TournamentEnrollmentRejected {
                player_root: cmd.player_root,
                reservation_id: cmd.reservation_id,
                reason,
                rejected_at: Some(angzarr_client::now()),
            };
            let event_any = pack_event(&event, "examples.TournamentEnrollmentRejected");
            Ok(new_event_book(command_book, seq, event_any))
        }
    }
}
