//! Hand-for-hand (TDA Rule 12) command handlers.
//!
//! The HandForHand bubble flow has four commands at the tournament
//! aggregate. They mirror the Python aggregate's handlers in
//! `examples-python/main/tournament/agg/handlers.py:1089..1295` and
//! preserve the same emit-then-apply semantics so state replay sees
//! progress between calls.

use angzarr_client::proto::EventBook;
use angzarr_client::CommandResult;
use examples_proto::{
    EnterHandForHand, HandForHandHandRecorded, HandForHandRoundComplete, HandForHandStarted,
    PlayerMovedTables, RecordHandForHandHand, RecordHandForHandRoundComplete,
    RecordTableHandComplete,
};
use examples_utils::{event_page, pack_event, reject};

use crate::errors::{
    AlreadyInHandForHand, NotInHandForHand, TableNotActiveInHandForHand, TournamentNotFound,
    TournamentNotRunning,
};
use crate::state::TournamentState;

const DEFAULT_CLOCK_DEDUCTION_SECONDS: i32 = 120; // TDA RP-8C default
const MAX_CLOCK_DEDUCTION_SECONDS: i32 = 180; // TDA RP-8B/8C cap

fn guard_running(state: &TournamentState) -> CommandResult<()> {
    if !state.exists() {
        return Err(reject(TournamentNotFound));
    }
    if !state.is_running() {
        return Err(reject(TournamentNotRunning));
    }
    Ok(())
}

// --- EnterHandForHand ---

pub fn handle_enter_hand_for_hand(
    cmd: EnterHandForHand,
    state: &TournamentState,
    seq: u32,
) -> CommandResult<EventBook> {
    guard_running(state)?;
    if state.is_hand_for_hand() {
        return Err(reject(AlreadyInHandForHand));
    }

    let event = HandForHandStarted {
        started_at: Some(angzarr_client::now()),
        active_table_roots: cmd.active_table_roots,
    };
    let event_any = pack_event(&event, "examples.HandForHandStarted");
    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}

// --- RecordTableHandComplete ---

pub fn handle_record_table_hand_complete(
    cmd: RecordTableHandComplete,
    state: &TournamentState,
    seq: u32,
) -> CommandResult<EventBook> {
    guard_running(state)?;
    if !state.is_hand_for_hand() {
        return Err(reject(NotInHandForHand));
    }
    if !state.hand_for_hand_active_tables.contains(&cmd.table_root) {
        return Err(reject(TableNotActiveInHandForHand {
            table_root_hex: hex::encode(&cmd.table_root),
        }));
    }

    let now_ts = angzarr_client::now();
    // Always emit the per-table progress receipt so state replays see
    // prior tables already removed from `hand_for_hand_pending_tables`.
    // Mirrors the cpp tournament aggregate's choice of `PlayerMovedTables`
    // for this slot (`examples-cpp/.../advanced_handlers.cpp:handle_record_table_hand_complete`):
    // only `from_table_root` is populated; the rest of the fields stay
    // default because this emit conveys H4H pending-set progress, not a
    // player rebalance. The applier `apply_player_moved_tables` discards
    // `from_table_root` from `hand_for_hand_pending_tables` so the next
    // command sees the prior table's removal.
    let recorded = PlayerMovedTables {
        from_table_root: cmd.table_root.clone(),
        moved_at: Some(now_ts),
        ..Default::default()
    };
    let mut pages = vec![event_page(
        seq,
        pack_event(&recorded, "examples.PlayerMovedTables"),
    )];

    // Compute whether this discard closes the round. Mirror Python's
    // local-apply-then-check pattern: take a working copy of pending.
    let mut pending = state.hand_for_hand_pending_tables.clone();
    pending.remove(&cmd.table_root);
    if pending.is_empty() {
        let next_round = state.hand_for_hand_round + 1;
        let round_complete = HandForHandRoundComplete {
            round_number: next_round,
            completed_at: Some(now_ts),
        };
        pages.push(event_page(
            seq,
            pack_event(&round_complete, "examples.HandForHandRoundComplete"),
        ));
    }

    Ok(EventBook {
        pages,
        ..Default::default()
    })
}

// --- RecordHandForHandRoundComplete ---

pub fn handle_record_round_complete(
    _cmd: RecordHandForHandRoundComplete,
    state: &TournamentState,
    seq: u32,
) -> CommandResult<EventBook> {
    guard_running(state)?;
    if !state.is_hand_for_hand() {
        return Err(reject(NotInHandForHand));
    }
    let next_round = state.hand_for_hand_round + 1;
    let event = HandForHandRoundComplete {
        round_number: next_round,
        completed_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.HandForHandRoundComplete");
    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}

// --- RecordHandForHandHand ---

/// Cap the per-hand level-clock deduction. TDA RP-8C: "whenever possible
/// the clock should be reduced by 2-minutes each hand", capped by
/// RP-8B at 3 minutes. Absent or non-positive `real_seconds` defaults
/// to 120; otherwise the lesser of `real_seconds` and 180 is used.
pub(crate) fn clock_deduction(real_seconds: i32) -> i32 {
    if real_seconds <= 0 {
        DEFAULT_CLOCK_DEDUCTION_SECONDS
    } else {
        real_seconds.min(MAX_CLOCK_DEDUCTION_SECONDS)
    }
}

pub fn handle_record_h4h_hand(
    cmd: RecordHandForHandHand,
    state: &TournamentState,
    seq: u32,
) -> CommandResult<EventBook> {
    guard_running(state)?;
    if !state.is_hand_for_hand() {
        return Err(reject(NotInHandForHand));
    }
    let clock_deducted = clock_deduction(cmd.real_seconds);
    let event = HandForHandHandRecorded {
        real_seconds: cmd.real_seconds,
        clock_seconds_deducted: clock_deducted,
        recorded_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.HandForHandHandRecorded");
    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{apply_created, apply_hand_for_hand_started};
    use examples_proto::{BlindLevel, GameVariant, TournamentCreated, TournamentStatus};

    fn running_state(active_tables: Vec<Vec<u8>>) -> TournamentState {
        let mut s = TournamentState::default();
        apply_created(
            &mut s,
            TournamentCreated {
                name: "Bubble".into(),
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
        if !active_tables.is_empty() {
            apply_hand_for_hand_started(
                &mut s,
                HandForHandStarted {
                    started_at: None,
                    active_table_roots: active_tables,
                },
            );
        }
        s
    }

    #[test]
    fn enter_hand_for_hand_emits_started_event() {
        let s = running_state(vec![]);
        let book = handle_enter_hand_for_hand(
            EnterHandForHand {
                active_table_roots: vec![vec![0xaa], vec![0xbb]],
            },
            &s,
            1,
        )
        .expect("ok");
        assert_eq!(book.pages.len(), 1);
    }

    #[test]
    fn enter_hand_for_hand_rejects_when_not_running() {
        let s = TournamentState::default();
        let err = handle_enter_hand_for_hand(
            EnterHandForHand {
                active_table_roots: vec![],
            },
            &s,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "TOURNAMENT_NOT_FOUND");
    }

    #[test]
    fn enter_hand_for_hand_rejects_when_already_in_h4h() {
        let s = running_state(vec![vec![0x01]]);
        let err = handle_enter_hand_for_hand(
            EnterHandForHand {
                active_table_roots: vec![],
            },
            &s,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "ALREADY_IN_HAND_FOR_HAND");
    }

    #[test]
    fn record_table_hand_emits_only_recorded_when_pending_remains() {
        let s = running_state(vec![vec![0xaa], vec![0xbb]]);
        let book = handle_record_table_hand_complete(
            RecordTableHandComplete {
                table_root: vec![0xaa],
            },
            &s,
            1,
        )
        .expect("ok");
        assert_eq!(book.pages.len(), 1);
    }

    #[test]
    fn record_table_hand_emits_round_complete_on_final_table() {
        let mut s = running_state(vec![vec![0xaa]]);
        // After apply, pending = {0xaa}; recording 0xaa empties it.
        s.hand_for_hand_pending_tables = [vec![0xaa]].iter().cloned().collect();
        let book = handle_record_table_hand_complete(
            RecordTableHandComplete {
                table_root: vec![0xaa],
            },
            &s,
            1,
        )
        .expect("ok");
        assert_eq!(book.pages.len(), 2);
    }

    #[test]
    fn record_table_hand_rejects_unknown_table() {
        let s = running_state(vec![vec![0xaa]]);
        let err = handle_record_table_hand_complete(
            RecordTableHandComplete {
                table_root: vec![0xff],
            },
            &s,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "TABLE_NOT_ACTIVE_IN_HAND_FOR_HAND");
    }

    #[test]
    fn record_round_complete_increments_round() {
        let mut s = running_state(vec![vec![0xaa]]);
        s.hand_for_hand_round = 5;
        let book =
            handle_record_round_complete(RecordHandForHandRoundComplete {}, &s, 1).expect("ok");
        assert_eq!(book.pages.len(), 1);
    }

    fn decode_event<M: prost::Message + Default>(page: &angzarr_client::proto::EventPage) -> M {
        let payload = page.payload.as_ref().expect("payload set");
        let event_any = match payload {
            angzarr_client::proto::event_page::Payload::Event(any) => any,
            _ => panic!("expected event payload"),
        };
        M::decode(event_any.value.as_slice()).expect("decode")
    }

    #[test]
    fn record_table_hand_round_number_equals_prior_plus_one() {
        // Kill `+1 -> -1` / `*1` mutations on the round-number
        // increment in handle_record_table_hand_complete.
        let mut s = running_state(vec![vec![0xaa]]);
        s.hand_for_hand_pending_tables = [vec![0xaa]].iter().cloned().collect();
        s.hand_for_hand_round = 7;
        let book = handle_record_table_hand_complete(
            RecordTableHandComplete {
                table_root: vec![0xaa],
            },
            &s,
            1,
        )
        .expect("ok");
        let round: HandForHandRoundComplete = decode_event(&book.pages[1]);
        assert_eq!(round.round_number, 8);
    }

    #[test]
    fn record_round_complete_round_number_equals_prior_plus_one() {
        // Kill `+1 -> -1` / `*1` mutations on the round-number
        // increment in handle_record_round_complete.
        let mut s = running_state(vec![vec![0xaa]]);
        s.hand_for_hand_round = 11;
        let book =
            handle_record_round_complete(RecordHandForHandRoundComplete {}, &s, 1).expect("ok");
        let round: HandForHandRoundComplete = decode_event(&book.pages[0]);
        assert_eq!(round.round_number, 12);
    }

    #[test]
    fn record_round_complete_rejects_when_not_in_h4h() {
        let s = running_state(vec![]);
        let err =
            handle_record_round_complete(RecordHandForHandRoundComplete {}, &s, 1).unwrap_err();
        assert_eq!(err.code, "NOT_IN_HAND_FOR_HAND");
    }

    #[test]
    fn record_table_hand_emits_player_moved_tables_with_from_table_root() {
        // The per-table progress receipt is `PlayerMovedTables` with
        // only `from_table_root` populated. Asserts the new proto-shape
        // contract introduced when `TableHandCompleteRecorded` was
        // removed from the regenerated bindings.
        let s = running_state(vec![vec![0xaa], vec![0xbb]]);
        let book = handle_record_table_hand_complete(
            RecordTableHandComplete {
                table_root: vec![0xaa],
            },
            &s,
            1,
        )
        .expect("ok");
        let moved: PlayerMovedTables = decode_event(&book.pages[0]);
        assert_eq!(moved.from_table_root, vec![0xaa]);
        // Only from_table_root carries the H4H progress signal; the
        // rest of the fields are default-zero in this emit.
        assert!(moved.player_root.is_empty());
        assert!(moved.to_table_root.is_empty());
        assert_eq!(moved.to_seat, 0);
        assert_eq!(moved.stack, 0);
    }

    #[test]
    fn record_h4h_hand_caps_deduction_at_3_minutes() {
        assert_eq!(clock_deduction(500), MAX_CLOCK_DEDUCTION_SECONDS);
    }

    #[test]
    fn record_h4h_hand_uses_real_seconds_under_cap() {
        assert_eq!(clock_deduction(90), 90);
    }

    #[test]
    fn record_h4h_hand_defaults_when_zero() {
        assert_eq!(clock_deduction(0), DEFAULT_CLOCK_DEDUCTION_SECONDS);
        assert_eq!(clock_deduction(-1), DEFAULT_CLOCK_DEDUCTION_SECONDS);
    }

    #[test]
    fn record_h4h_hand_emits_recorded_event() {
        let s = running_state(vec![vec![0xaa]]);
        let book =
            handle_record_h4h_hand(RecordHandForHandHand { real_seconds: 60 }, &s, 1).expect("ok");
        assert_eq!(book.pages.len(), 1);
    }

    #[test]
    fn record_h4h_hand_rejects_when_not_in_h4h() {
        let s = running_state(vec![]);
        let err =
            handle_record_h4h_hand(RecordHandForHandHand { real_seconds: 60 }, &s, 1).unwrap_err();
        assert_eq!(err.code, "NOT_IN_HAND_FOR_HAND");
    }
}
