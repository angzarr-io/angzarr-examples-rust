//! ReplaceSeventhStreetCard command handler.
//!
//! TDA RP-10B — 7th-street card exposed by dealer with action remaining.
//! The original is removed from play and a replacement is dealt face
//! down. Emits SeventhStreetCardReplaced. (Per proto docs: if no betting
//! action remains, this would be rejected with FAILED_PRECONDITION; that
//! state-derived guard is captured here via the standard hand-not-dealt
//! / hand-complete pair, awaiting richer phase tracking in a follow-up.)

use angzarr_client::proto::EventBook;
use angzarr_client::CommandResult;
use examples_proto::{ReplaceSeventhStreetCard, SeventhStreetCardReplaced, StudStreet};
use examples_utils::{event_page, pack_event, reject};

use crate::errors::{
    HandAlreadyComplete, HandNotDealt, NotSeventhStreet, OriginalCardRequired, PlayerNotInHand,
    PlayerRootRequired, ReplacementCardRequired,
};
use crate::state::HandState;

fn guard(state: &HandState) -> CommandResult<()> {
    if !state.exists() {
        return Err(reject(HandNotDealt));
    }
    if state.is_complete() {
        return Err(reject(HandAlreadyComplete));
    }
    // TDA RP-10B — only valid on 7th street. While the hand may not
    // have entered stud-street tracking yet (current_stud_street ==
    // STUD_STREET_UNSPECIFIED), the proto spec is explicit that any
    // non-7th street is a rejection. If the hand is mid-stud and on a
    // different street, reject. STUD_STREET_UNSPECIFIED is permissive
    // for non-stud variants / pre-stud-street state per the Python
    // canonical (`current_stud_street and current_stud_street != ...`).
    if state.current_stud_street != StudStreet::Unspecified
        && state.current_stud_street != StudStreet::SeventhStreet
    {
        return Err(reject(NotSeventhStreet));
    }
    Ok(())
}

fn validate(cmd: &ReplaceSeventhStreetCard, state: &HandState) -> CommandResult<()> {
    if cmd.player_root.is_empty() {
        return Err(reject(PlayerRootRequired));
    }
    if state.get_player(&cmd.player_root).is_none() {
        return Err(reject(PlayerNotInHand));
    }
    if cmd.original_card.is_none() {
        return Err(reject(OriginalCardRequired));
    }
    if cmd.replacement_card.is_none() {
        return Err(reject(ReplacementCardRequired));
    }
    Ok(())
}

pub fn handle_replace_seventh_street_card(
    cmd: ReplaceSeventhStreetCard,
    state: &HandState,
    seq: u32,
) -> CommandResult<EventBook> {
    guard(state)?;
    validate(&cmd, state)?;

    let event = SeventhStreetCardReplaced {
        player_root: cmd.player_root,
        original_card: cmd.original_card,
        replacement_card: cmd.replacement_card,
        replaced_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.SeventhStreetCardReplaced");
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
                game_variant: GameVariant::SevenCardStud as i32,
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

    fn card(rank: Rank) -> Card {
        Card {
            suit: Suit::Hearts as i32,
            rank: rank as i32,
        }
    }

    #[test]
    fn rejects_when_hand_not_dealt() {
        let state = new_hand_state();
        let err = handle_replace_seventh_street_card(
            ReplaceSeventhStreetCard {
                player_root: vec![1],
                original_card: Some(card(Rank::King)),
                replacement_card: Some(card(Rank::Ace)),
            },
            &state,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "HAND_NOT_DEALT");
    }

    #[test]
    fn rejects_when_player_root_missing() {
        let state = dealt_state();
        let err = handle_replace_seventh_street_card(
            ReplaceSeventhStreetCard {
                player_root: vec![],
                original_card: Some(card(Rank::King)),
                replacement_card: Some(card(Rank::Ace)),
            },
            &state,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "PLAYER_ROOT_REQUIRED");
    }

    #[test]
    fn rejects_when_player_unknown() {
        let state = dealt_state();
        let err = handle_replace_seventh_street_card(
            ReplaceSeventhStreetCard {
                player_root: vec![99],
                original_card: Some(card(Rank::King)),
                replacement_card: Some(card(Rank::Ace)),
            },
            &state,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "PLAYER_NOT_IN_HAND");
    }

    #[test]
    fn rejects_when_original_missing() {
        let state = dealt_state();
        let err = handle_replace_seventh_street_card(
            ReplaceSeventhStreetCard {
                player_root: vec![1],
                original_card: None,
                replacement_card: Some(card(Rank::Ace)),
            },
            &state,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "ORIGINAL_CARD_REQUIRED");
    }

    #[test]
    fn rejects_when_replacement_missing() {
        let state = dealt_state();
        let err = handle_replace_seventh_street_card(
            ReplaceSeventhStreetCard {
                player_root: vec![1],
                original_card: Some(card(Rank::King)),
                replacement_card: None,
            },
            &state,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "REPLACEMENT_CARD_REQUIRED");
    }

    #[test]
    fn rejects_when_not_on_seventh_street() {
        let mut state = dealt_state();
        state.current_stud_street = StudStreet::FourthStreet;
        let err = handle_replace_seventh_street_card(
            ReplaceSeventhStreetCard {
                player_root: vec![1],
                original_card: Some(card(Rank::King)),
                replacement_card: Some(card(Rank::Ace)),
            },
            &state,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "NOT_SEVENTH_STREET");
    }

    #[test]
    fn accepts_when_on_seventh_street() {
        let mut state = dealt_state();
        state.current_stud_street = StudStreet::SeventhStreet;
        let book = handle_replace_seventh_street_card(
            ReplaceSeventhStreetCard {
                player_root: vec![1],
                original_card: Some(card(Rank::King)),
                replacement_card: Some(card(Rank::Ace)),
            },
            &state,
            2,
        )
        .expect("ok");
        assert_eq!(book.pages.len(), 1);
    }

    #[test]
    fn emits_seventh_street_card_replaced_carrying_both_cards() {
        let state = dealt_state();
        let book = handle_replace_seventh_street_card(
            ReplaceSeventhStreetCard {
                player_root: vec![1],
                original_card: Some(card(Rank::King)),
                replacement_card: Some(card(Rank::Ace)),
            },
            &state,
            11,
        )
        .expect("ok");
        assert_eq!(book.pages.len(), 1);
        let any = match book.pages[0].payload.as_ref().unwrap() {
            angzarr_client::proto::event_page::Payload::Event(any) => any,
            _ => panic!(),
        };
        let evt: SeventhStreetCardReplaced =
            prost::Message::decode(any.value.as_slice()).expect("decode");
        assert_eq!(evt.original_card.unwrap().rank, Rank::King as i32);
        assert_eq!(evt.replacement_card.unwrap().rank, Rank::Ace as i32);
    }
}
