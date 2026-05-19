//! Penalty handlers — IssuePenalty / DecrementPenalty / DisqualifyPlayer
//! per TDA Rule 71 + WSOP Rule 113/114.

use angzarr_client::proto::EventBook;
use angzarr_client::CommandResult;
use examples_proto::{
    DecrementPenalty, DisqualifyPlayer, IssuePenalty, PenaltyIssued, PenaltyRoundsDecremented,
    PlayerDisqualified,
};
use examples_utils::{event_page, pack_event, reject};

use crate::errors::{
    InvalidPenaltyType, PlayerNotOnPenalty, PlayerNotRegistered, PlayerRootRequired,
    TournamentNotFound, TournamentNotRunning,
};
use crate::state::TournamentState;

fn guard_running(state: &TournamentState) -> CommandResult<()> {
    if !state.exists() {
        return Err(reject(TournamentNotFound));
    }
    if !state.is_running() {
        return Err(reject(TournamentNotRunning));
    }
    Ok(())
}

const PENALTY_TYPES: &[&str] = &[
    "VERBAL_WARNING",
    "MISSED_HAND",
    "MISSED_ROUND",
    "DISQUALIFIED",
];

/// Compute the `missed_hands` field per TDA Rule 71:
/// - VERBAL_WARNING / DISQUALIFIED → 0
/// - MISSED_HAND → 1
/// - MISSED_ROUND → `rounds * table_size`
pub(crate) fn missed_hands(penalty_type: &str, rounds: i32, table_size: i32) -> i32 {
    match penalty_type {
        "MISSED_HAND" => 1,
        "MISSED_ROUND" => rounds.max(0) * table_size.max(0),
        _ => 0,
    }
}

// --- IssuePenalty ---

pub fn handle_issue_penalty(
    cmd: IssuePenalty,
    state: &TournamentState,
    seq: u32,
) -> CommandResult<EventBook> {
    guard_running(state)?;
    if cmd.player_root.is_empty() {
        return Err(reject(PlayerRootRequired));
    }
    if !PENALTY_TYPES.contains(&cmd.r#type.as_str()) {
        return Err(reject(InvalidPenaltyType {
            value: cmd.r#type.clone(),
        }));
    }

    let computed_missed = missed_hands(&cmd.r#type, cmd.rounds, cmd.table_size);
    let event = PenaltyIssued {
        player_root: cmd.player_root,
        r#type: cmd.r#type,
        rounds: cmd.rounds,
        missed_hands: computed_missed,
        issued_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.PenaltyIssued");
    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}

// --- DecrementPenalty ---

pub fn handle_decrement_penalty(
    cmd: DecrementPenalty,
    state: &TournamentState,
    seq: u32,
) -> CommandResult<EventBook> {
    guard_running(state)?;
    if cmd.player_root.is_empty() {
        return Err(reject(PlayerRootRequired));
    }
    let key = hex::encode(&cmd.player_root);
    let current = state.active_penalties.get(&key).copied().ok_or_else(|| {
        reject(PlayerNotOnPenalty {
            player_root_hex: key.clone(),
        })
    })?;
    let next = (current - 1).max(0);
    let event = PenaltyRoundsDecremented {
        player_root: cmd.player_root,
        rounds_remaining: next,
        decremented_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.PenaltyRoundsDecremented");
    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}

// --- DisqualifyPlayer ---

pub fn handle_disqualify_player(
    cmd: DisqualifyPlayer,
    state: &TournamentState,
    seq: u32,
) -> CommandResult<EventBook> {
    guard_running(state)?;
    if cmd.player_root.is_empty() {
        return Err(reject(PlayerRootRequired));
    }
    let key = hex::encode(&cmd.player_root);
    if !state.is_player_registered(&key) {
        return Err(reject(PlayerNotRegistered {
            player_root_hex: key.clone(),
        }));
    }

    // Chips removed = whatever stack the player had registered with.
    // The aggregate doesn't track running stack changes from per-hand
    // events; mirror Python's "use registered starting stack as the
    // removal magnitude" heuristic.
    let chips_to_remove = state
        .registered_players
        .get(&key)
        .map(|r| r.starting_stack)
        .unwrap_or(0);

    let event = PlayerDisqualified {
        player_root: cmd.player_root,
        reason: cmd.reason,
        chips_removed: chips_to_remove,
        disqualified_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.PlayerDisqualified");
    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{apply_created, apply_penalty_issued, apply_player_enrolled};
    use examples_proto::{
        BlindLevel, GameVariant, TournamentCreated, TournamentPlayerEnrolled, TournamentStatus,
    };

    fn running_state_with(players: &[&[u8]]) -> TournamentState {
        let mut s = TournamentState::default();
        apply_created(
            &mut s,
            TournamentCreated {
                name: "T".into(),
                game_variant: GameVariant::TexasHoldem as i32,
                buy_in: 500,
                starting_stack: 10_000,
                max_players: 9,
                min_players: 2,
                scheduled_start: None,
                rebuy_config: None,
                addon_config: None,
                blind_structure: vec![BlindLevel {
                    level: 1,
                    small_blind: 25,
                    big_blind: 50,
                    ante: 0,
                    duration_minutes: 20,
                }],
                created_at: None,
                ..Default::default()
            },
        );
        s.status = TournamentStatus::TournamentRunning;
        for (i, p) in players.iter().enumerate() {
            apply_player_enrolled(
                &mut s,
                TournamentPlayerEnrolled {
                    player_root: p.to_vec(),
                    reservation_id: vec![],
                    fee_paid: 500,
                    starting_stack: 10_000,
                    registration_number: (i + 1) as i32,
                    enrolled_at: None,
                },
            );
        }
        s
    }

    #[test]
    fn missed_hands_table() {
        assert_eq!(missed_hands("VERBAL_WARNING", 0, 0), 0);
        assert_eq!(missed_hands("DISQUALIFIED", 5, 5), 0);
        assert_eq!(missed_hands("MISSED_HAND", 99, 99), 1);
        assert_eq!(missed_hands("MISSED_ROUND", 2, 9), 18);
        assert_eq!(missed_hands("MISSED_ROUND", -1, 9), 0);
        assert_eq!(missed_hands("MISSED_ROUND", 2, -3), 0);
        assert_eq!(missed_hands("UNKNOWN", 1, 1), 0);
    }

    #[test]
    fn issue_penalty_rejects_unknown_type() {
        let s = running_state_with(&[]);
        let err = handle_issue_penalty(
            IssuePenalty {
                player_root: vec![0xaa],
                r#type: "NONSENSE".into(),
                rounds: 0,
                table_size: 0,
            },
            &s,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "INVALID_PENALTY_TYPE");
    }

    #[test]
    fn issue_penalty_emits_event_with_computed_missed_hands() {
        let s = running_state_with(&[]);
        let book = handle_issue_penalty(
            IssuePenalty {
                player_root: vec![0xaa],
                r#type: "MISSED_ROUND".into(),
                rounds: 1,
                table_size: 9,
            },
            &s,
            1,
        )
        .expect("ok");
        assert_eq!(book.pages.len(), 1);
    }

    #[test]
    fn decrement_penalty_rejects_player_not_on_penalty() {
        let s = running_state_with(&[]);
        let err = handle_decrement_penalty(
            DecrementPenalty {
                player_root: vec![0xaa],
            },
            &s,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "PLAYER_NOT_ON_PENALTY");
    }

    #[test]
    fn decrement_penalty_emits_rounds_decremented() {
        let mut s = running_state_with(&[]);
        apply_penalty_issued(
            &mut s,
            PenaltyIssued {
                player_root: vec![0xaa],
                r#type: "MISSED_ROUND".into(),
                rounds: 3,
                missed_hands: 27,
                issued_at: None,
            },
        );
        let book = handle_decrement_penalty(
            DecrementPenalty {
                player_root: vec![0xaa],
            },
            &s,
            1,
        )
        .expect("ok");
        assert_eq!(book.pages.len(), 1);
    }

    fn decode_decremented(page: &angzarr_client::proto::EventPage) -> PenaltyRoundsDecremented {
        let payload = page.payload.as_ref().expect("payload set");
        let any = match payload {
            angzarr_client::proto::event_page::Payload::Event(any) => any,
            _ => panic!("expected event"),
        };
        use prost::Message;
        PenaltyRoundsDecremented::decode(any.value.as_slice()).expect("decode")
    }

    #[test]
    fn decrement_penalty_rounds_remaining_equals_prior_minus_one() {
        let mut s = running_state_with(&[]);
        apply_penalty_issued(
            &mut s,
            PenaltyIssued {
                player_root: vec![0xaa],
                r#type: "MISSED_ROUND".into(),
                rounds: 5,
                missed_hands: 45,
                issued_at: None,
            },
        );
        let book = handle_decrement_penalty(
            DecrementPenalty {
                player_root: vec![0xaa],
            },
            &s,
            1,
        )
        .expect("ok");
        let event = decode_decremented(&book.pages[0]);
        assert_eq!(event.rounds_remaining, 4, "must be prior (5) - 1");
    }

    #[test]
    fn decrement_penalty_rounds_floored_at_zero() {
        let mut s = running_state_with(&[]);
        apply_penalty_issued(
            &mut s,
            PenaltyIssued {
                player_root: vec![0xaa],
                r#type: "MISSED_HAND".into(),
                rounds: 1,
                missed_hands: 1,
                issued_at: None,
            },
        );
        let book = handle_decrement_penalty(
            DecrementPenalty {
                player_root: vec![0xaa],
            },
            &s,
            1,
        )
        .expect("ok");
        let event = decode_decremented(&book.pages[0]);
        assert_eq!(event.rounds_remaining, 0, "must floor at 0");
    }

    #[test]
    fn disqualify_player_requires_registration() {
        let s = running_state_with(&[]);
        let err = handle_disqualify_player(
            DisqualifyPlayer {
                player_root: vec![0xaa],
                reason: "violation".into(),
            },
            &s,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "PLAYER_NOT_REGISTERED");
    }

    #[test]
    fn disqualify_player_emits_disqualified_event() {
        let s = running_state_with(&[b"\xaa".as_ref()]);
        let book = handle_disqualify_player(
            DisqualifyPlayer {
                player_root: vec![0xaa],
                reason: "violation".into(),
            },
            &s,
            1,
        )
        .expect("ok");
        assert_eq!(book.pages.len(), 1);
    }
}
