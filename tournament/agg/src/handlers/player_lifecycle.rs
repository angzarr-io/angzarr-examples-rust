//! Player lifecycle handlers — simultaneous bust groups (TDA RP-8A),
//! seat redraws (WSOP Rule 67c), absent-player reseats (TDA RP-16),
//! re-entry (TDA Rule 8B), absent-blind ticks (WSOP Rule 36),
//! bounty awards (RP-22 / Rule 39), and no-show detection (Rule 16).

use angzarr_client::proto::EventBook;
use angzarr_client::CommandResult;
use examples_proto::{
    AbsentBlindAdvanced, AdvanceAbsentBlind, AwardBounty, BountyAwarded, DetectNoShow,
    NoShowDetected, PlayerMovedTables, PlayerReEntered, ReEntryPlayer, RecordSimultaneousBusts,
    ReseatAbsentPlayer, SeatRedrawTriggered, SimultaneousBustsRecorded, TriggerSeatRedraw,
};
use examples_utils::{event_page, pack_event, reject};

use crate::errors::{
    AmountMustBePositive, PlayerNotRegistered, PlayerRootRequired, PlayerRootsRequired,
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

// --- RecordSimultaneousBusts ---

pub fn handle_record_simultaneous_busts(
    cmd: RecordSimultaneousBusts,
    state: &TournamentState,
    seq: u32,
) -> CommandResult<EventBook> {
    guard_running(state)?;
    if cmd.player_roots.is_empty() {
        return Err(reject(PlayerRootsRequired));
    }

    let event = SimultaneousBustsRecorded {
        player_roots: cmd.player_roots,
        hand_root: cmd.hand_root,
        same_table: cmd.same_table,
        pre_hand_stacks: cmd.pre_hand_stacks,
        recorded_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.SimultaneousBustsRecorded");
    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}

// --- TriggerSeatRedraw ---

/// Map `tables_remaining` onto the canonical WSOP Rule 67c trigger
/// label. 1 = FINAL_TABLE, 2 = TWO_TABLES, 3 = THREE_TABLES; anything
/// else returns the numeric form so the saga can surface unexpected
/// values rather than silently misclassify them.
pub(crate) fn redraw_trigger_label(tables_remaining: i32) -> String {
    match tables_remaining {
        1 => "FINAL_TABLE".to_string(),
        2 => "TWO_TABLES".to_string(),
        3 => "THREE_TABLES".to_string(),
        other => format!("TABLES_{other}"),
    }
}

pub fn handle_trigger_seat_redraw(
    cmd: TriggerSeatRedraw,
    state: &TournamentState,
    seq: u32,
) -> CommandResult<EventBook> {
    guard_running(state)?;

    let event = SeatRedrawTriggered {
        trigger: redraw_trigger_label(cmd.tables_remaining),
        tables_remaining: cmd.tables_remaining,
        original_field: cmd.original_field,
        triggered_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.SeatRedrawTriggered");
    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}

// --- ReseatAbsentPlayer ---

pub fn handle_reseat_absent_player(
    cmd: ReseatAbsentPlayer,
    state: &TournamentState,
    seq: u32,
) -> CommandResult<EventBook> {
    guard_running(state)?;
    if cmd.player_root.is_empty() {
        return Err(reject(PlayerRootRequired));
    }

    let event = PlayerMovedTables {
        player_root: cmd.player_root,
        from_table_root: cmd.from_table_root,
        to_table_root: cmd.to_table_root,
        to_seat: cmd.to_seat,
        stack: cmd.stack,
        absent_at_move: true,
        moved_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.PlayerMovedTables");
    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}

// --- ReEntryPlayer ---

pub fn handle_re_entry_player(
    cmd: ReEntryPlayer,
    state: &TournamentState,
    seq: u32,
) -> CommandResult<EventBook> {
    guard_running(state)?;
    if cmd.player_root.is_empty() {
        return Err(reject(PlayerRootRequired));
    }

    // Re-entry adds a fresh starting stack. Forfeited chips track what
    // came off the table at bust time; mirror Python semantics by
    // emitting both deltas so the conservation invariant is auditable.
    let event = PlayerReEntered {
        player_root: cmd.player_root,
        chips_forfeited: cmd.chips_forfeited,
        chips_added: state.starting_stack,
        re_entered_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.PlayerReEntered");
    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}

// --- AdvanceAbsentBlind ---

pub fn handle_advance_absent_blind(
    cmd: AdvanceAbsentBlind,
    state: &TournamentState,
    seq: u32,
) -> CommandResult<EventBook> {
    guard_running(state)?;
    if cmd.absent_player_root.is_empty() || cmd.lone_player_root.is_empty() {
        return Err(reject(PlayerRootRequired));
    }

    // Stack delta = small_blind + big_blind transferred from absent
    // player to lone player (WSOP Rule 36).
    let event = AbsentBlindAdvanced {
        table_root: cmd.table_root,
        absent_player_root: cmd.absent_player_root,
        lone_player_root: cmd.lone_player_root,
        stack_delta: cmd.small_blind + cmd.big_blind,
        advanced_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.AbsentBlindAdvanced");
    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}

// --- AwardBounty ---

pub fn handle_award_bounty(
    cmd: AwardBounty,
    state: &TournamentState,
    seq: u32,
) -> CommandResult<EventBook> {
    guard_running(state)?;
    if cmd.eliminator_root.is_empty() || cmd.knocked_out_root.is_empty() {
        return Err(reject(PlayerRootRequired));
    }
    if cmd.amount <= 0 {
        return Err(reject(AmountMustBePositive { value: cmd.amount }));
    }

    let event = BountyAwarded {
        eliminator_root: cmd.eliminator_root,
        knocked_out_root: cmd.knocked_out_root,
        amount: cmd.amount,
        tiebreak_reason: cmd.tiebreak_reason,
        awarded_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.BountyAwarded");
    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}

// --- DetectNoShow ---

pub fn handle_detect_no_show(
    cmd: DetectNoShow,
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

    let chips_to_remove = state
        .registered_players
        .get(&key)
        .map(|r| r.starting_stack)
        .unwrap_or(0);
    let event = NoShowDetected {
        player_root: cmd.player_root,
        chips_removed: chips_to_remove,
        buy_in_held: state.buy_in,
        detected_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.NoShowDetected");
    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{apply_created, apply_player_enrolled};
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
    fn record_simultaneous_busts_requires_at_least_one_player() {
        let s = running_state_with(&[]);
        let err = handle_record_simultaneous_busts(
            RecordSimultaneousBusts {
                player_roots: vec![],
                hand_root: vec![],
                same_table: false,
                pre_hand_stacks: Default::default(),
            },
            &s,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "PLAYER_ROOTS_REQUIRED");
    }

    #[test]
    fn record_simultaneous_busts_emits_recorded_event() {
        let s = running_state_with(&[]);
        let book = handle_record_simultaneous_busts(
            RecordSimultaneousBusts {
                player_roots: vec![vec![0xaa], vec![0xbb]],
                hand_root: vec![0x01],
                same_table: true,
                pre_hand_stacks: Default::default(),
            },
            &s,
            1,
        )
        .expect("ok");
        assert_eq!(book.pages.len(), 1);
    }

    #[test]
    fn trigger_seat_redraw_label_canonical_for_known_remaining() {
        assert_eq!(redraw_trigger_label(1), "FINAL_TABLE");
        assert_eq!(redraw_trigger_label(2), "TWO_TABLES");
        assert_eq!(redraw_trigger_label(3), "THREE_TABLES");
        assert_eq!(redraw_trigger_label(7), "TABLES_7");
    }

    #[test]
    fn trigger_seat_redraw_emits_event() {
        let s = running_state_with(&[]);
        let book = handle_trigger_seat_redraw(
            TriggerSeatRedraw {
                tables_remaining: 2,
                original_field: 100,
            },
            &s,
            1,
        )
        .expect("ok");
        assert_eq!(book.pages.len(), 1);
    }

    #[test]
    fn reseat_absent_player_emits_player_moved_tables() {
        let s = running_state_with(&[]);
        let book = handle_reseat_absent_player(
            ReseatAbsentPlayer {
                player_root: vec![0xaa],
                from_table_root: vec![0x01],
                to_table_root: vec![0x02],
                to_seat: 4,
                stack: 5_000,
            },
            &s,
            1,
        )
        .expect("ok");
        assert_eq!(book.pages.len(), 1);
    }

    #[test]
    fn reseat_absent_player_requires_player_root() {
        let s = running_state_with(&[]);
        let err = handle_reseat_absent_player(
            ReseatAbsentPlayer {
                player_root: vec![],
                from_table_root: vec![],
                to_table_root: vec![],
                to_seat: 0,
                stack: 0,
            },
            &s,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "PLAYER_ROOT_REQUIRED");
    }

    #[test]
    fn re_entry_player_emits_player_re_entered() {
        let s = running_state_with(&[]);
        let book = handle_re_entry_player(
            ReEntryPlayer {
                player_root: vec![0xaa],
                chips_forfeited: 2_500,
            },
            &s,
            1,
        )
        .expect("ok");
        assert_eq!(book.pages.len(), 1);
    }

    #[test]
    fn advance_absent_blind_emits_stack_delta_equal_to_sb_plus_bb() {
        let s = running_state_with(&[]);
        let book = handle_advance_absent_blind(
            AdvanceAbsentBlind {
                table_root: vec![0x01],
                absent_player_root: vec![0xaa],
                lone_player_root: vec![0xbb],
                small_blind: 25,
                big_blind: 50,
            },
            &s,
            1,
        )
        .expect("ok");
        assert_eq!(book.pages.len(), 1);
    }

    fn decode_absent_blind(page: &angzarr_client::proto::EventPage) -> AbsentBlindAdvanced {
        let payload = page.payload.as_ref().expect("payload set");
        let any = match payload {
            angzarr_client::proto::event_page::Payload::Event(any) => any,
            _ => panic!("expected event"),
        };
        use prost::Message;
        AbsentBlindAdvanced::decode(any.value.as_slice()).expect("decode")
    }

    #[test]
    fn advance_absent_blind_stack_delta_is_sb_plus_bb_exact() {
        // Kill `+` -> `-` mutation on `small_blind + big_blind` (line 166).
        let s = running_state_with(&[]);
        let book = handle_advance_absent_blind(
            AdvanceAbsentBlind {
                table_root: vec![0x01],
                absent_player_root: vec![0xaa],
                lone_player_root: vec![0xbb],
                small_blind: 25,
                big_blind: 50,
            },
            &s,
            1,
        )
        .expect("ok");
        let event = decode_absent_blind(&book.pages[0]);
        assert_eq!(event.stack_delta, 75, "stack_delta must equal SB + BB");
    }

    #[test]
    fn advance_absent_blind_requires_both_player_roots() {
        let s = running_state_with(&[]);
        let err = handle_advance_absent_blind(
            AdvanceAbsentBlind {
                table_root: vec![],
                absent_player_root: vec![],
                lone_player_root: vec![],
                small_blind: 0,
                big_blind: 0,
            },
            &s,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "PLAYER_ROOT_REQUIRED");
    }

    #[test]
    fn advance_absent_blind_rejects_when_only_absent_root_missing() {
        // Kill `||` -> `&&` mutation (line 156): `||` requires
        // either side empty to reject; `&&` would require BOTH
        // empty, so a single-side-empty case passes the guard
        // under the mutation and emits an event (or fails
        // differently). This test asserts only-absent-empty
        // rejects; the `lone_player_root: vec![0x01]` is non-empty.
        let s = running_state_with(&[]);
        let err = handle_advance_absent_blind(
            AdvanceAbsentBlind {
                table_root: vec![0x01],
                absent_player_root: vec![],
                lone_player_root: vec![0x02],
                small_blind: 25,
                big_blind: 50,
            },
            &s,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "PLAYER_ROOT_REQUIRED");
    }

    #[test]
    fn advance_absent_blind_rejects_when_only_lone_root_missing() {
        let s = running_state_with(&[]);
        let err = handle_advance_absent_blind(
            AdvanceAbsentBlind {
                table_root: vec![0x01],
                absent_player_root: vec![0x02],
                lone_player_root: vec![],
                small_blind: 25,
                big_blind: 50,
            },
            &s,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "PLAYER_ROOT_REQUIRED");
    }

    #[test]
    fn award_bounty_emits_bounty_awarded() {
        let s = running_state_with(&[]);
        let book = handle_award_bounty(
            AwardBounty {
                eliminator_root: vec![0xaa],
                knocked_out_root: vec![0xbb],
                amount: 1_000,
                tiebreak_reason: String::new(),
            },
            &s,
            1,
        )
        .expect("ok");
        assert_eq!(book.pages.len(), 1);
    }

    #[test]
    fn award_bounty_rejects_non_positive_amount() {
        let s = running_state_with(&[]);
        let err = handle_award_bounty(
            AwardBounty {
                eliminator_root: vec![0xaa],
                knocked_out_root: vec![0xbb],
                amount: 0,
                tiebreak_reason: String::new(),
            },
            &s,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "AMOUNT_MUST_BE_POSITIVE");
    }

    #[test]
    fn award_bounty_rejects_when_only_eliminator_root_missing() {
        // Kill `||` -> `&&` mutation on award_bounty validation
        // (line 184). The `||` makes either-empty trigger rejection;
        // `&&` would let single-empty pass through, emitting an event.
        let s = running_state_with(&[]);
        let err = handle_award_bounty(
            AwardBounty {
                eliminator_root: vec![],
                knocked_out_root: vec![0xbb],
                amount: 100,
                tiebreak_reason: String::new(),
            },
            &s,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "PLAYER_ROOT_REQUIRED");
    }

    #[test]
    fn award_bounty_rejects_when_only_knocked_out_root_missing() {
        let s = running_state_with(&[]);
        let err = handle_award_bounty(
            AwardBounty {
                eliminator_root: vec![0xaa],
                knocked_out_root: vec![],
                amount: 100,
                tiebreak_reason: String::new(),
            },
            &s,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "PLAYER_ROOT_REQUIRED");
    }

    #[test]
    fn detect_no_show_requires_registered_player() {
        let s = running_state_with(&[]);
        let err = handle_detect_no_show(
            DetectNoShow {
                player_root: vec![0xaa],
            },
            &s,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "PLAYER_NOT_REGISTERED");
    }

    #[test]
    fn detect_no_show_emits_event_with_stack_removed() {
        let s = running_state_with(&[b"\xaa".as_ref()]);
        let book = handle_detect_no_show(
            DetectNoShow {
                player_root: vec![0xaa],
            },
            &s,
            1,
        )
        .expect("ok");
        assert_eq!(book.pages.len(), 1);
    }
}
