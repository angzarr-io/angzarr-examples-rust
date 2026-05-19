//! DealStudCommunityCard command handler.
//!
//! TDA RP-10H — short-stub fallback on 7th street. When the stub plus
//! remaining burns cannot service individual deals, a single face-up
//! community card is dealt and shared by all active players.
//! Emits StudCommunityCardDealt.

use angzarr_client::proto::EventBook;
use angzarr_client::CommandResult;
use examples_proto::{DealStudCommunityCard, StudCommunityCardDealt, StudStreet};
use examples_utils::{event_page, pack_event, reject};

use crate::errors::{
    HandAlreadyComplete, HandNotDealt, InvalidStudStreet, NoSharedWithProvided,
    NotEnoughCardsInDeck,
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

fn validate(cmd: &DealStudCommunityCard, state: &HandState) -> CommandResult<()> {
    match StudStreet::try_from(cmd.street) {
        Ok(StudStreet::Unspecified) | Err(_) => {
            return Err(reject(InvalidStudStreet { got: cmd.street }));
        }
        Ok(_) => {}
    }
    if cmd.shared_with.is_empty() {
        return Err(reject(NoSharedWithProvided));
    }
    // One card consumed from the stub.
    if state.remaining_deck.is_empty() {
        return Err(reject(NotEnoughCardsInDeck {
            requested: 1,
            available: 0,
        }));
    }
    Ok(())
}

pub fn handle_deal_stud_community_card(
    cmd: DealStudCommunityCard,
    state: &HandState,
    seq: u32,
) -> CommandResult<EventBook> {
    guard(state)?;
    validate(&cmd, state)?;

    let card = state.remaining_deck.first().cloned();

    let event = StudCommunityCardDealt {
        card,
        street: cmd.street,
        shared_with: cmd.shared_with,
        dealt_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.StudCommunityCardDealt");
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

    fn dealt_state_with_deck(remaining: Vec<Card>) -> HandState {
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
                remaining_deck: remaining,
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
        let err = handle_deal_stud_community_card(
            DealStudCommunityCard {
                street: StudStreet::SeventhStreet as i32,
                shared_with: vec![vec![1]],
            },
            &state,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "HAND_NOT_DEALT");
    }

    #[test]
    fn rejects_when_street_unspecified() {
        let state = dealt_state_with_deck(vec![ace()]);
        let err = handle_deal_stud_community_card(
            DealStudCommunityCard {
                street: StudStreet::Unspecified as i32,
                shared_with: vec![vec![1]],
            },
            &state,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "INVALID_STUD_STREET");
    }

    #[test]
    fn rejects_when_shared_with_empty() {
        let state = dealt_state_with_deck(vec![ace()]);
        let err = handle_deal_stud_community_card(
            DealStudCommunityCard {
                street: StudStreet::SeventhStreet as i32,
                shared_with: vec![],
            },
            &state,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "NO_SHARED_WITH_PROVIDED");
    }

    #[test]
    fn rejects_when_deck_empty() {
        let state = dealt_state_with_deck(vec![]);
        let err = handle_deal_stud_community_card(
            DealStudCommunityCard {
                street: StudStreet::SeventhStreet as i32,
                shared_with: vec![vec![1]],
            },
            &state,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "NOT_ENOUGH_CARDS_IN_DECK");
    }

    #[test]
    fn emits_stud_community_card_dealt_with_first_deck_card() {
        let state = dealt_state_with_deck(vec![ace()]);
        let book = handle_deal_stud_community_card(
            DealStudCommunityCard {
                street: StudStreet::SeventhStreet as i32,
                shared_with: vec![vec![1], vec![2]],
            },
            &state,
            14,
        )
        .expect("ok");
        assert_eq!(book.pages.len(), 1);
        let any = match book.pages[0].payload.as_ref().unwrap() {
            angzarr_client::proto::event_page::Payload::Event(any) => any,
            _ => panic!("expected event"),
        };
        let evt: StudCommunityCardDealt =
            prost::Message::decode(any.value.as_slice()).expect("decode");
        assert_eq!(evt.shared_with.len(), 2);
        assert_eq!(evt.street, StudStreet::SeventhStreet as i32);
        assert_eq!(evt.card.unwrap().rank, Rank::Ace as i32);
    }
}
