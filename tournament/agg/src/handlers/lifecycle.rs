//! Tournament lifecycle command handlers.

use angzarr_client::proto::EventBook;
use angzarr_client::{CommandRejectedError, CommandResult};
use examples_utils::{event_page, pack_event, rejected};
use examples_proto::{
    AdvanceBlindLevel, BlindLevelAdvanced, CompleteTournament, EliminatePlayer, PauseTournament,
    PlayerEliminated, ResumeTournament, StartTournament, TournamentCompleted, TournamentPaused,
    TournamentResumed, TournamentStarted, TournamentStatus,
};

use crate::state::TournamentState;

// --- AdvanceBlindLevel ---

fn guard_advance(state: &TournamentState) -> CommandResult<()> {
    if !state.exists() {
        return Err(rejected("Tournament does not exist"));
    }
    if !state.is_running() {
        return Err(rejected("Tournament is not running"));
    }
    Ok(())
}

pub fn handle_advance_blind_level(
    _cmd: AdvanceBlindLevel,
    state: &TournamentState,
    seq: u32,
) -> CommandResult<EventBook> {
    guard_advance(state)?;

    let next_level = state.current_level + 1;

    let (small_blind, big_blind, ante) = if (next_level as usize) <= state.blind_structure.len() {
        let level = &state.blind_structure[(next_level - 1) as usize];
        (level.small_blind, level.big_blind, level.ante)
    } else if let Some(last_level) = state.blind_structure.last() {
        (
            last_level.small_blind,
            last_level.big_blind,
            last_level.ante,
        )
    } else {
        return Err(rejected("No blind structure defined"));
    };

    let event = BlindLevelAdvanced {
        level: next_level,
        small_blind,
        big_blind,
        ante,
        advanced_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.BlindLevelAdvanced");

    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}

// --- EliminatePlayer ---

fn guard_eliminate(state: &TournamentState) -> CommandResult<()> {
    if !state.exists() {
        return Err(rejected("Tournament does not exist"));
    }
    if !state.is_running() {
        return Err(rejected("Tournament is not running"));
    }
    Ok(())
}

pub fn handle_eliminate_player(
    cmd: EliminatePlayer,
    state: &TournamentState,
    seq: u32,
) -> CommandResult<EventBook> {
    guard_eliminate(state)?;

    let player_root_hex = hex::encode(&cmd.player_root);
    if !state.is_player_registered(&player_root_hex) {
        return Err(rejected(
            "Player is not registered in this tournament",
        ));
    }

    let finish_position = state.players_remaining;
    let payout = 0i64;

    let event = PlayerEliminated {
        player_root: cmd.player_root,
        finish_position,
        hand_root: cmd.hand_root,
        payout,
        eliminated_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.PlayerEliminated");

    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}

// --- PauseTournament ---

fn guard_pause(state: &TournamentState) -> CommandResult<()> {
    if !state.exists() {
        return Err(rejected("Tournament does not exist"));
    }
    if state.status == TournamentStatus::TournamentPaused {
        return Err(rejected("Tournament is already paused"));
    }
    if !state.is_running() {
        return Err(rejected("Tournament is not running"));
    }
    Ok(())
}

pub fn handle_pause_tournament(
    cmd: PauseTournament,
    state: &TournamentState,
    seq: u32,
) -> CommandResult<EventBook> {
    guard_pause(state)?;

    let event = TournamentPaused {
        reason: cmd.reason,
        paused_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.TournamentPaused");

    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}

// --- ResumeTournament ---

fn guard_resume(state: &TournamentState) -> CommandResult<()> {
    if !state.exists() {
        return Err(rejected("Tournament does not exist"));
    }
    if state.status != TournamentStatus::TournamentPaused {
        return Err(rejected("Tournament is not paused"));
    }
    Ok(())
}

pub fn handle_resume_tournament(
    _cmd: ResumeTournament,
    state: &TournamentState,
    seq: u32,
) -> CommandResult<EventBook> {
    guard_resume(state)?;

    let event = TournamentResumed {
        resumed_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.TournamentResumed");

    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}

// --- StartTournament ---

fn guard_start(state: &TournamentState) -> CommandResult<()> {
    if !state.exists() {
        return Err(rejected("Tournament does not exist"));
    }
    if !state.is_registration_open() {
        return Err(rejected("Registration is not open"));
    }
    if (state.registered_players.len() as i32) < state.min_players {
        return Err(rejected("Not enough players to start"));
    }
    Ok(())
}

pub fn handle_start_tournament(
    _cmd: StartTournament,
    state: &TournamentState,
    seq: u32,
) -> CommandResult<EventBook> {
    guard_start(state)?;

    let event = TournamentStarted {
        total_players: state.registered_players.len() as i32,
        tables_created: 0,
        total_prize_pool: state.total_prize_pool,
        started_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.TournamentStarted");

    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}

// --- CompleteTournament ---

fn guard_complete(state: &TournamentState) -> CommandResult<()> {
    if !state.exists() {
        return Err(rejected("Tournament does not exist"));
    }
    if state.status == TournamentStatus::TournamentCompleted {
        return Err(rejected("Tournament is already completed"));
    }
    if state.status != TournamentStatus::TournamentRunning
        && state.status != TournamentStatus::TournamentPaused
    {
        return Err(rejected(
            "Tournament must be running or paused to complete",
        ));
    }
    Ok(())
}

pub fn handle_complete_tournament(
    cmd: CompleteTournament,
    state: &TournamentState,
    seq: u32,
) -> CommandResult<EventBook> {
    guard_complete(state)?;

    let event = TournamentCompleted {
        winner_root: cmd.winner_root,
        total_prize_pool: state.total_prize_pool,
        results: vec![],
        completed_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.TournamentCompleted");

    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}
