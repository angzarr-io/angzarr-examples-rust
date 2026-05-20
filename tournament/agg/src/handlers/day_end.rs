//! End-of-day and mixed-game handlers — StopNewHands (WSOP Rule 125),
//! BagAndTag (WSOP Rule 122), RotateMixedGameVariant (TDA RP-18).

use angzarr_client::proto::EventBook;
use angzarr_client::CommandResult;
use examples_proto::{
    BagAndTag, BagAndTagComplete, GameVariant, MixedGameVariantRotated, NewHandsHalted,
    PlayerBagSnapshot, RotateMixedGameVariant, StopNewHands,
};
use examples_utils::{event_page, pack_event, reject};

use crate::errors::{
    NewHandsAlreadyHalted, NewHandsNotHalted, TournamentNotFound, TournamentNotRunning,
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

// --- StopNewHands ---

pub fn handle_stop_new_hands(
    cmd: StopNewHands,
    state: &TournamentState,
    seq: u32,
) -> CommandResult<EventBook> {
    guard_running(state)?;
    if state.is_new_hands_halted() {
        return Err(reject(NewHandsAlreadyHalted));
    }

    let event = NewHandsHalted {
        effective_at: "AFTER_CURRENT_HAND".into(),
        reason: cmd.reason,
        halted_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.NewHandsHalted");
    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}

// --- BagAndTag ---

pub fn handle_bag_and_tag(
    _cmd: BagAndTag,
    state: &TournamentState,
    seq: u32,
) -> CommandResult<EventBook> {
    guard_running(state)?;
    if !state.is_new_hands_halted() {
        return Err(reject(NewHandsNotHalted));
    }

    // Build per-player snapshots from registered_players. Stack and
    // seat come from the registration record; if a saga keeps live
    // table/seat assignments it would augment this view before emit.
    let mut snapshots: Vec<PlayerBagSnapshot> = state
        .registered_players
        .values()
        .map(|r| PlayerBagSnapshot {
            player_root: r.player_root.clone(),
            stack: r.starting_stack,
            table_root: Vec::new(),
            seat: r.seat_assignment,
        })
        .collect();
    // Deterministic order: lexicographic by player_root_hex.
    snapshots.sort_by_key(|s| hex::encode(&s.player_root));

    let event = BagAndTagComplete {
        snapshots,
        completed_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.BagAndTagComplete");
    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}

// --- RotateMixedGameVariant ---

/// HORSE cycle order per TDA RP-18: H(old'em) → O(maha Hi/Lo) →
/// R(azz) → S(even Card Stud) → E(ven Card Stud Hi/Lo 8 or Better) →
/// repeat. Falls back to TexasHoldem for unknown current variant.
pub(crate) fn next_horse_variant(current: GameVariant) -> GameVariant {
    match current {
        GameVariant::TexasHoldem => GameVariant::OmahaHiLo8b,
        GameVariant::OmahaHiLo8b => GameVariant::Razz,
        GameVariant::Razz => GameVariant::SevenCardStud,
        GameVariant::SevenCardStud => GameVariant::StudHiLo8b,
        GameVariant::StudHiLo8b => GameVariant::TexasHoldem,
        _ => GameVariant::TexasHoldem,
    }
}

pub fn handle_rotate_mixed_game_variant(
    _cmd: RotateMixedGameVariant,
    state: &TournamentState,
    seq: u32,
) -> CommandResult<EventBook> {
    guard_running(state)?;

    let next = next_horse_variant(state.game_variant);
    let event = MixedGameVariantRotated {
        from_variant: state.game_variant as i32,
        to_variant: next as i32,
        rotated_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.MixedGameVariantRotated");
    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{apply_created, apply_new_hands_halted};
    use examples_proto::{BlindLevel, GameVariant, TournamentCreated, TournamentStatus};

    fn running_state(variant: GameVariant) -> TournamentState {
        let mut s = TournamentState::default();
        apply_created(
            &mut s,
            TournamentCreated {
                name: "T".into(),
                game_variant: variant as i32,
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
        s
    }

    #[test]
    fn stop_new_hands_emits_halted_event() {
        let s = running_state(GameVariant::TexasHoldem);
        let book = handle_stop_new_hands(
            StopNewHands {
                reason: "END_OF_DAY".into(),
            },
            &s,
            1,
        )
        .expect("ok");
        assert_eq!(book.pages.len(), 1);
    }

    #[test]
    fn stop_new_hands_rejects_when_already_halted() {
        let mut s = running_state(GameVariant::TexasHoldem);
        apply_new_hands_halted(
            &mut s,
            NewHandsHalted {
                effective_at: "AFTER_CURRENT_HAND".into(),
                reason: "EOD".into(),
                halted_at: None,
            },
        );
        let err = handle_stop_new_hands(
            StopNewHands {
                reason: "END_OF_DAY".into(),
            },
            &s,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "NEW_HANDS_ALREADY_HALTED");
    }

    #[test]
    fn bag_and_tag_requires_halt_first() {
        let s = running_state(GameVariant::TexasHoldem);
        let err = handle_bag_and_tag(BagAndTag {}, &s, 1).unwrap_err();
        assert_eq!(err.code, "NEW_HANDS_NOT_HALTED");
    }

    #[test]
    fn bag_and_tag_emits_complete_after_halt() {
        let mut s = running_state(GameVariant::TexasHoldem);
        apply_new_hands_halted(
            &mut s,
            NewHandsHalted {
                effective_at: "AFTER_CURRENT_HAND".into(),
                reason: "EOD".into(),
                halted_at: None,
            },
        );
        let book = handle_bag_and_tag(BagAndTag {}, &s, 1).expect("ok");
        assert_eq!(book.pages.len(), 1);
    }

    #[test]
    fn horse_cycle_is_h_o_r_s_e_repeat() {
        assert_eq!(
            next_horse_variant(GameVariant::TexasHoldem),
            GameVariant::OmahaHiLo8b
        );
        assert_eq!(
            next_horse_variant(GameVariant::OmahaHiLo8b),
            GameVariant::Razz
        );
        assert_eq!(
            next_horse_variant(GameVariant::Razz),
            GameVariant::SevenCardStud
        );
        assert_eq!(
            next_horse_variant(GameVariant::SevenCardStud),
            GameVariant::StudHiLo8b
        );
        assert_eq!(
            next_horse_variant(GameVariant::StudHiLo8b),
            GameVariant::TexasHoldem
        );
    }

    #[test]
    fn rotate_mixed_game_variant_emits_rotated_event() {
        let s = running_state(GameVariant::TexasHoldem);
        let book = handle_rotate_mixed_game_variant(RotateMixedGameVariant {}, &s, 1).expect("ok");
        assert_eq!(book.pages.len(), 1);
    }
}
