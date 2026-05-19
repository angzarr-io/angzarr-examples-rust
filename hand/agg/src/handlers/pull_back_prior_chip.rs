//! PullBackPriorChip handler — TDA Rule 46B.
//!
//! Pulling back a prior chip while facing a raise binds the player to
//! call or raise. The handler records the pull-back on the player
//! state via an emitted `PriorChipPulledBack` event; downstream fold
//! attempts within the same hand are rejected by player-action with
//! code `BOUND_TO_CALL_OR_RAISE` (enforcement deferred to the action
//! handler; this handler just records the event).

use angzarr_client::proto::EventBook;
use angzarr_client::CommandResult;
use examples_proto::{PriorChipPulledBack, PullBackPriorChip};
use examples_utils::{event_page, pack_event, reject};

use crate::errors::{
    ChipsPulledMustBePositive, HandAlreadyComplete, HandNotDealt, PlayerHasFolded, PlayerNotInHand,
    PlayerRootRequired,
};
use crate::state::HandState;

fn guard(state: &HandState) -> CommandResult<()> {
    if !state.exists() {
        return Err(reject(HandNotDealt));
    }
    if state.is_complete() {
        return Err(reject(HandAlreadyComplete));
    }
    Ok(())
}

pub fn handle_pull_back_prior_chip(
    cmd: PullBackPriorChip,
    state: &HandState,
    seq: u32,
) -> CommandResult<EventBook> {
    guard(state)?;
    if cmd.player_root.is_empty() {
        return Err(reject(PlayerRootRequired));
    }
    if cmd.chips_pulled <= 0 {
        return Err(reject(ChipsPulledMustBePositive {
            value: cmd.chips_pulled,
        }));
    }
    let player = state
        .get_player(&cmd.player_root)
        .ok_or_else(|| reject(PlayerNotInHand))?;
    if player.has_folded {
        return Err(reject(PlayerHasFolded));
    }

    let event = PriorChipPulledBack {
        player_root: cmd.player_root,
        chips_pulled: cmd.chips_pulled,
        pulled_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.PriorChipPulledBack");
    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{apply_cards_dealt, new_hand_state};
    use examples_proto::{Card, CardsDealt, GameVariant, PlayerInHand};

    fn dealt_state() -> HandState {
        let mut s = new_hand_state();
        apply_cards_dealt(
            &mut s,
            CardsDealt {
                table_root: vec![0xab],
                hand_number: 1,
                game_variant: GameVariant::TexasHoldem as i32,
                player_cards: vec![],
                dealer_position: 0,
                players: vec![PlayerInHand {
                    player_root: vec![1],
                    position: 0,
                    stack: 500,
                    ..Default::default()
                }],
                dealt_at: None,
                remaining_deck: vec![Card { suit: 0, rank: 2 }; 10],
                ..Default::default()
            },
        );
        s
    }

    #[test]
    fn rejects_zero_chips() {
        let s = dealt_state();
        let err = handle_pull_back_prior_chip(
            PullBackPriorChip {
                player_root: vec![1],
                chips_pulled: 0,
            },
            &s,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "CHIPS_PULLED_MUST_BE_POSITIVE");
    }

    #[test]
    fn rejects_unknown_player() {
        let s = dealt_state();
        let err = handle_pull_back_prior_chip(
            PullBackPriorChip {
                player_root: vec![0xff],
                chips_pulled: 50,
            },
            &s,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "PLAYER_NOT_IN_HAND");
    }

    #[test]
    fn rejects_folded_player() {
        let mut s = dealt_state();
        s.get_player_mut(&[1]).unwrap().has_folded = true;
        let err = handle_pull_back_prior_chip(
            PullBackPriorChip {
                player_root: vec![1],
                chips_pulled: 50,
            },
            &s,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "PLAYER_HAS_FOLDED");
    }

    #[test]
    fn rejects_when_hand_complete() {
        let mut s = dealt_state();
        s.status = "complete".into();
        let err = handle_pull_back_prior_chip(
            PullBackPriorChip {
                player_root: vec![1],
                chips_pulled: 50,
            },
            &s,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "HAND_ALREADY_COMPLETE");
    }

    #[test]
    fn emits_prior_chip_pulled_back_event() {
        let s = dealt_state();
        let book = handle_pull_back_prior_chip(
            PullBackPriorChip {
                player_root: vec![1],
                chips_pulled: 50,
            },
            &s,
            1,
        )
        .expect("ok");
        assert_eq!(book.pages.len(), 1);
    }
}
