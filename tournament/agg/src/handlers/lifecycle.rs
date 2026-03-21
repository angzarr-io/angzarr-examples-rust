//! Tournament lifecycle command handlers.

use angzarr_client::proto::{CommandBook, EventBook};
use angzarr_client::{new_event_book, pack_event, CommandRejectedError, CommandResult, UnpackAny};
use examples_proto::{
    AdvanceBlindLevel, BlindLevelAdvanced, EliminatePlayer, PauseTournament, PlayerEliminated,
    ResumeTournament, TournamentPaused, TournamentResumed, TournamentStatus,
};
use prost_types::Any;

use crate::state::TournamentState;

// --- AdvanceBlindLevel ---

fn guard_advance(state: &TournamentState) -> CommandResult<()> {
    if !state.exists() {
        return Err(CommandRejectedError::new("Tournament does not exist"));
    }
    if !state.is_running() {
        return Err(CommandRejectedError::new("Tournament is not running"));
    }
    Ok(())
}

pub fn handle_advance_blind_level(
    command_book: &CommandBook,
    command_any: &Any,
    state: &TournamentState,
    seq: u32,
) -> CommandResult<EventBook> {
    let _cmd: AdvanceBlindLevel = command_any
        .unpack()
        .map_err(|e| CommandRejectedError::new(format!("Failed to decode command: {}", e)))?;

    guard_advance(state)?;

    let next_level = state.current_level + 1;

    // Get blind values from structure, or use last level if we've exceeded structure
    let (small_blind, big_blind, ante) = if (next_level as usize) <= state.blind_structure.len() {
        let level = &state.blind_structure[(next_level - 1) as usize];
        (level.small_blind, level.big_blind, level.ante)
    } else if let Some(last_level) = state.blind_structure.last() {
        // Stay at last level
        (
            last_level.small_blind,
            last_level.big_blind,
            last_level.ante,
        )
    } else {
        return Err(CommandRejectedError::new("No blind structure defined"));
    };

    let event = BlindLevelAdvanced {
        level: next_level,
        small_blind,
        big_blind,
        ante,
        advanced_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.BlindLevelAdvanced");

    Ok(new_event_book(command_book, seq, event_any))
}

// --- EliminatePlayer ---

fn guard_eliminate(state: &TournamentState) -> CommandResult<()> {
    if !state.exists() {
        return Err(CommandRejectedError::new("Tournament does not exist"));
    }
    if !state.is_running() {
        return Err(CommandRejectedError::new("Tournament is not running"));
    }
    Ok(())
}

pub fn handle_eliminate_player(
    command_book: &CommandBook,
    command_any: &Any,
    state: &TournamentState,
    seq: u32,
) -> CommandResult<EventBook> {
    let cmd: EliminatePlayer = command_any
        .unpack()
        .map_err(|e| CommandRejectedError::new(format!("Failed to decode command: {}", e)))?;

    guard_eliminate(state)?;

    let player_root_hex = hex::encode(&cmd.player_root);
    if !state.is_player_registered(&player_root_hex) {
        return Err(CommandRejectedError::new(
            "Player is not registered in this tournament",
        ));
    }

    // Calculate finish position (players remaining after elimination)
    let finish_position = state.players_remaining;

    // TODO: Calculate payout based on prize structure
    let payout = 0i64;

    let event = PlayerEliminated {
        player_root: cmd.player_root,
        finish_position,
        hand_root: cmd.hand_root,
        payout,
        eliminated_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.PlayerEliminated");

    Ok(new_event_book(command_book, seq, event_any))
}

// --- PauseTournament ---

fn guard_pause(state: &TournamentState) -> CommandResult<()> {
    if !state.exists() {
        return Err(CommandRejectedError::new("Tournament does not exist"));
    }
    if state.status == TournamentStatus::TournamentPaused {
        return Err(CommandRejectedError::new("Tournament is already paused"));
    }
    if !state.is_running() {
        return Err(CommandRejectedError::new("Tournament is not running"));
    }
    Ok(())
}

pub fn handle_pause_tournament(
    command_book: &CommandBook,
    command_any: &Any,
    state: &TournamentState,
    seq: u32,
) -> CommandResult<EventBook> {
    let cmd: PauseTournament = command_any
        .unpack()
        .map_err(|e| CommandRejectedError::new(format!("Failed to decode command: {}", e)))?;

    guard_pause(state)?;

    let event = TournamentPaused {
        reason: cmd.reason,
        paused_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.TournamentPaused");

    Ok(new_event_book(command_book, seq, event_any))
}

// --- ResumeTournament ---

fn guard_resume(state: &TournamentState) -> CommandResult<()> {
    if !state.exists() {
        return Err(CommandRejectedError::new("Tournament does not exist"));
    }
    if state.status != TournamentStatus::TournamentPaused {
        return Err(CommandRejectedError::new("Tournament is not paused"));
    }
    Ok(())
}

pub fn handle_resume_tournament(
    command_book: &CommandBook,
    command_any: &Any,
    state: &TournamentState,
    seq: u32,
) -> CommandResult<EventBook> {
    let _cmd: ResumeTournament = command_any
        .unpack()
        .map_err(|e| CommandRejectedError::new(format!("Failed to decode command: {}", e)))?;

    guard_resume(state)?;

    let event = TournamentResumed {
        resumed_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.TournamentResumed");

    Ok(new_event_book(command_book, seq, event_any))
}
