//! ReplaceButtonCard command handler.
//!
//! TDA Rule 37 — button-position card replaced when announced before the
//! button has acted. The original card is returned to the stub and a
//! replacement is dealt. Emits ButtonCardReplaced.

use angzarr_client::proto::EventBook;
use angzarr_client::CommandResult;
use examples_proto::{ButtonCardReplaced, ReplaceButtonCard};
use examples_utils::{event_page, pack_event, reject};

use crate::errors::{
    HandAlreadyComplete, HandNotDealt, NotOnButton, PlayerNotInHand, PlayerRootRequired,
    ReplacementCardRequired,
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

fn validate(cmd: &ReplaceButtonCard, state: &HandState) -> CommandResult<()> {
    if cmd.player_root.is_empty() {
        return Err(reject(PlayerRootRequired));
    }
    if state.get_player(&cmd.player_root).is_none() {
        return Err(reject(PlayerNotInHand));
    }
    if cmd.replacement_card.is_none() {
        return Err(reject(ReplacementCardRequired));
    }
    // TDA Rule 37 — only the button player's card may be replaced via
    // this command. Mismatched roots route through the misdeal path
    // (different floor remedy).
    if let Some(button_player) = state
        .players
        .values()
        .find(|p| p.position == state.dealer_position)
    {
        if button_player.player_root != cmd.player_root {
            return Err(reject(NotOnButton));
        }
    }
    Ok(())
}

pub fn handle_replace_button_card(
    cmd: ReplaceButtonCard,
    state: &HandState,
    seq: u32,
) -> CommandResult<EventBook> {
    guard(state)?;
    validate(&cmd, state)?;

    let event = ButtonCardReplaced {
        player_root: cmd.player_root,
        replacement_card: cmd.replacement_card,
        replaced_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.ButtonCardReplaced");
    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{apply_cards_dealt, new_hand_state};
    use examples_proto::{Card, CardsDealt, GameVariant, PlayerInHand, Rank, Suit};

    fn dealt_state() -> HandState {
        let mut state = new_hand_state();
        apply_cards_dealt(
            &mut state,
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
                remaining_deck: vec![],
                ..Default::default()
            },
        );
        state
    }

    fn ace() -> Card {
        Card {
            suit: Suit::Hearts as i32,
            rank: Rank::Ace as i32,
        }
    }

    #[test]
    fn rejects_when_hand_not_dealt() {
        let state = new_hand_state();
        let err = handle_replace_button_card(
            ReplaceButtonCard {
                player_root: vec![1],
                replacement_card: Some(ace()),
            },
            &state,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "HAND_NOT_DEALT");
    }

    #[test]
    fn rejects_when_hand_complete() {
        let mut state = dealt_state();
        state.status = "complete".into();
        let err = handle_replace_button_card(
            ReplaceButtonCard {
                player_root: vec![1],
                replacement_card: Some(ace()),
            },
            &state,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "HAND_ALREADY_COMPLETE");
    }

    #[test]
    fn rejects_when_player_root_missing() {
        let state = dealt_state();
        let err = handle_replace_button_card(
            ReplaceButtonCard {
                player_root: vec![],
                replacement_card: Some(ace()),
            },
            &state,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "PLAYER_ROOT_REQUIRED");
    }

    #[test]
    fn rejects_when_player_not_in_hand() {
        let state = dealt_state();
        let err = handle_replace_button_card(
            ReplaceButtonCard {
                player_root: vec![99],
                replacement_card: Some(ace()),
            },
            &state,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "PLAYER_NOT_IN_HAND");
    }

    #[test]
    fn rejects_when_replacement_card_missing() {
        let state = dealt_state();
        let err = handle_replace_button_card(
            ReplaceButtonCard {
                player_root: vec![1],
                replacement_card: None,
            },
            &state,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "REPLACEMENT_CARD_REQUIRED");
    }

    #[test]
    fn rejects_when_player_not_on_button() {
        // Two-player state with player 1 @ position 0 (button) and player 2
        // @ position 1. Asking to replace player 2's card must reject.
        let mut state = new_hand_state();
        crate::state::apply_cards_dealt(
            &mut state,
            CardsDealt {
                table_root: vec![0xab],
                hand_number: 1,
                game_variant: GameVariant::TexasHoldem as i32,
                player_cards: vec![],
                dealer_position: 0,
                players: vec![
                    PlayerInHand {
                        player_root: vec![1],
                        position: 0,
                        stack: 500,
                        ..Default::default()
                    },
                    PlayerInHand {
                        player_root: vec![2],
                        position: 1,
                        stack: 500,
                        ..Default::default()
                    },
                ],
                dealt_at: None,
                remaining_deck: vec![],
                ..Default::default()
            },
        );
        let err = handle_replace_button_card(
            ReplaceButtonCard {
                player_root: vec![2],
                replacement_card: Some(ace()),
            },
            &state,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "NOT_ON_BUTTON");
    }

    #[test]
    fn emits_button_card_replaced() {
        let state = dealt_state();
        let book = handle_replace_button_card(
            ReplaceButtonCard {
                player_root: vec![1],
                replacement_card: Some(ace()),
            },
            &state,
            4,
        )
        .expect("ok");
        assert_eq!(book.pages.len(), 1);
    }
}
