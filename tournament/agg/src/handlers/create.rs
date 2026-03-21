//! CreateTournament command handler.

use angzarr_client::proto::{CommandBook, EventBook};
use angzarr_client::{new_event_book, pack_event, CommandRejectedError, CommandResult, UnpackAny};
use examples_proto::{CreateTournament, TournamentCreated};
use prost_types::Any;

use crate::state::TournamentState;

fn guard(state: &TournamentState) -> CommandResult<()> {
    if state.exists() {
        return Err(CommandRejectedError::new("Tournament already exists"));
    }
    Ok(())
}

fn validate(cmd: &CreateTournament) -> CommandResult<()> {
    if cmd.name.is_empty() {
        return Err(CommandRejectedError::new("name is required"));
    }
    if cmd.buy_in <= 0 {
        return Err(CommandRejectedError::new("buy_in must be positive"));
    }
    if cmd.starting_stack <= 0 {
        return Err(CommandRejectedError::new("starting_stack must be positive"));
    }
    if cmd.max_players < 2 {
        return Err(CommandRejectedError::new("max_players must be at least 2"));
    }
    if cmd.min_players < 2 {
        return Err(CommandRejectedError::new("min_players must be at least 2"));
    }
    if cmd.min_players > cmd.max_players {
        return Err(CommandRejectedError::new(
            "min_players cannot exceed max_players",
        ));
    }
    Ok(())
}

fn compute(cmd: &CreateTournament) -> TournamentCreated {
    TournamentCreated {
        name: cmd.name.clone(),
        game_variant: cmd.game_variant,
        buy_in: cmd.buy_in,
        starting_stack: cmd.starting_stack,
        max_players: cmd.max_players,
        min_players: cmd.min_players,
        scheduled_start: cmd.scheduled_start,
        rebuy_config: cmd.rebuy_config,
        addon_config: cmd.addon_config,
        blind_structure: cmd.blind_structure.clone(),
        created_at: Some(angzarr_client::now()),
    }
}

pub fn handle_create_tournament(
    command_book: &CommandBook,
    command_any: &Any,
    state: &TournamentState,
    seq: u32,
) -> CommandResult<EventBook> {
    let cmd: CreateTournament = command_any
        .unpack()
        .map_err(|e| CommandRejectedError::new(format!("Failed to decode command: {}", e)))?;

    guard(state)?;
    validate(&cmd)?;

    let event = compute(&cmd);
    let event_any = pack_event(&event, "examples.TournamentCreated");

    Ok(new_event_book(command_book, seq, event_any))
}
