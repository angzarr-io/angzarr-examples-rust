//! Tournament lifecycle command handlers.

use angzarr_client::proto::EventBook;
use angzarr_client::{event_page, pack_event, CommandRejectedError, CommandResult};
use examples_proto::{
    AdvanceBlindLevel, BlindLevelAdvanced, EliminatePlayer, PauseTournament, PlayerEliminated,
    ResumeTournament, TournamentPaused, TournamentResumed, TournamentStatus,
};

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

    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
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
    cmd: EliminatePlayer,
    state: &TournamentState,
    seq: u32,
) -> CommandResult<EventBook> {
    guard_eliminate(state)?;

    let player_root_hex = hex::encode(&cmd.player_root);
    if !state.is_player_registered(&player_root_hex) {
        return Err(CommandRejectedError::new(
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
        return Err(CommandRejectedError::new("Tournament does not exist"));
    }
    if state.status != TournamentStatus::TournamentPaused {
        return Err(CommandRejectedError::new("Tournament is not paused"));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{apply_created, apply_player_enrolled};
    use angzarr_client::proto::event_page::Payload;
    use examples_proto::{
        BlindLevel, GameVariant, TournamentCreated, TournamentPlayerEnrolled,
    };
    use prost::Message;

    fn running_state_with_levels(levels: Vec<BlindLevel>) -> TournamentState {
        let mut s = TournamentState::default();
        apply_created(
            &mut s,
            TournamentCreated {
                name: "X".into(),
                game_variant: GameVariant::TexasHoldem as i32,
                buy_in: 500,
                starting_stack: 10_000,
                max_players: 2,
                min_players: 2,
                scheduled_start: None,
                rebuy_config: None,
                addon_config: None,
                blind_structure: levels,
                created_at: None,
            },
        );
        s.status = TournamentStatus::TournamentRunning;
        s
    }

    // ---- advance_blind_level ----

    #[test]
    fn handle_advance_blind_level_success_emits_event() {
        let state = running_state_with_levels(vec![
            BlindLevel {
                level: 1,
                small_blind: 25,
                big_blind: 50,
                ante: 0,
                duration_minutes: 20,
            },
            BlindLevel {
                level: 2,
                small_blind: 50,
                big_blind: 100,
                ante: 10,
                duration_minutes: 20,
            },
        ]);
        let book = handle_advance_blind_level(AdvanceBlindLevel {}, &state, 1).expect("ok");
        let any = match book.pages[0].payload.as_ref() {
            Some(Payload::Event(a)) => a,
            _ => panic!(),
        };
        assert!(any.type_url.ends_with("examples.BlindLevelAdvanced"));
        let decoded = BlindLevelAdvanced::decode(any.value.as_slice()).unwrap();
        assert_eq!(decoded.level, 2);
        assert_eq!(decoded.small_blind, 50);
        assert_eq!(decoded.ante, 10);
    }

    #[test]
    fn handle_advance_blind_level_rejects_when_not_running() {
        let mut state = running_state_with_levels(vec![BlindLevel {
            level: 1,
            small_blind: 25,
            big_blind: 50,
            ante: 0,
            duration_minutes: 20,
        }]);
        state.status = TournamentStatus::TournamentCreated;
        let err = handle_advance_blind_level(AdvanceBlindLevel {}, &state, 1).unwrap_err();
        assert!(err.reason.contains("not running"));
    }

    // ---- eliminate_player ----

    #[test]
    fn handle_eliminate_player_success_emits_event() {
        let mut state = running_state_with_levels(vec![]);
        apply_player_enrolled(
            &mut state,
            TournamentPlayerEnrolled {
                player_root: vec![0xaa],
                reservation_id: vec![],
                fee_paid: 500,
                starting_stack: 10_000,
                registration_number: 1,
                enrolled_at: None,
            },
        );
        let cmd = EliminatePlayer {
            player_root: vec![0xaa],
            hand_root: vec![0x11, 0x22],
        };
        let book = handle_eliminate_player(cmd, &state, 1).expect("ok");
        let any = match book.pages[0].payload.as_ref() {
            Some(Payload::Event(a)) => a,
            _ => panic!(),
        };
        assert!(any.type_url.ends_with("examples.PlayerEliminated"));
        let decoded = PlayerEliminated::decode(any.value.as_slice()).unwrap();
        assert_eq!(decoded.hand_root, vec![0x11, 0x22]);
    }

    #[test]
    fn handle_eliminate_player_rejects_unregistered_player() {
        let state = running_state_with_levels(vec![]);
        let cmd = EliminatePlayer {
            player_root: vec![0xaa],
            hand_root: vec![],
        };
        let err = handle_eliminate_player(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("not registered"));
    }

    // ---- pause_tournament ----

    #[test]
    fn handle_pause_tournament_success_emits_event() {
        let state = running_state_with_levels(vec![]);
        let cmd = PauseTournament {
            reason: "dinner break".into(),
        };
        let book = handle_pause_tournament(cmd, &state, 1).expect("ok");
        let any = match book.pages[0].payload.as_ref() {
            Some(Payload::Event(a)) => a,
            _ => panic!(),
        };
        assert!(any.type_url.ends_with("examples.TournamentPaused"));
        let decoded = TournamentPaused::decode(any.value.as_slice()).unwrap();
        assert_eq!(decoded.reason, "dinner break");
    }

    #[test]
    fn handle_pause_tournament_rejects_when_already_paused() {
        let mut state = running_state_with_levels(vec![]);
        state.status = TournamentStatus::TournamentPaused;
        let cmd = PauseTournament {
            reason: "x".into(),
        };
        let err = handle_pause_tournament(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("already paused"));
    }

    // ---- resume_tournament ----

    #[test]
    fn handle_resume_tournament_success_emits_event() {
        let mut state = running_state_with_levels(vec![]);
        state.status = TournamentStatus::TournamentPaused;
        let book = handle_resume_tournament(ResumeTournament {}, &state, 1).expect("ok");
        let any = match book.pages[0].payload.as_ref() {
            Some(Payload::Event(a)) => a,
            _ => panic!(),
        };
        assert!(any.type_url.ends_with("examples.TournamentResumed"));
    }

    #[test]
    fn handle_resume_tournament_rejects_when_not_paused() {
        let state = running_state_with_levels(vec![]);
        let err = handle_resume_tournament(ResumeTournament {}, &state, 1).unwrap_err();
        assert!(err.reason.contains("not paused"));
    }
}
