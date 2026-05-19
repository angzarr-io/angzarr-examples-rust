//! DealStudStreet command handler.
//!
//! Stud street advance — dealer deals 4th/5th/6th street up-cards for
//! every active player. Emits StudStreetDealt with the per-player upcards.

use angzarr_client::proto::EventBook;
use angzarr_client::CommandResult;
use examples_proto::{DealStudStreet, StudStreet, StudStreetDealt};
use examples_utils::{event_page, pack_event, reject};

use crate::errors::{HandAlreadyComplete, HandNotDealt, InvalidStudStreet, NoUpCardsProvided};
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

fn validate(cmd: &DealStudStreet) -> CommandResult<()> {
    // Unspecified (0) and any value not in the StudStreet enum is invalid.
    match StudStreet::try_from(cmd.street) {
        Ok(StudStreet::Unspecified) | Err(_) => {
            return Err(reject(InvalidStudStreet { got: cmd.street }));
        }
        Ok(_) => {}
    }
    if cmd.up_cards.is_empty() {
        return Err(reject(NoUpCardsProvided));
    }
    Ok(())
}

pub fn handle_deal_stud_street(
    cmd: DealStudStreet,
    state: &HandState,
    seq: u32,
) -> CommandResult<EventBook> {
    guard(state)?;
    validate(&cmd)?;

    let event = StudStreetDealt {
        street: cmd.street,
        up_cards: cmd.up_cards,
        dealt_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.StudStreetDealt");
    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{apply_cards_dealt, new_hand_state};
    use examples_proto::{Card, CardsDealt, GameVariant, PlayerInHand, PlayerUpCards, Rank, Suit};

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

    fn sample_up_cards() -> Vec<PlayerUpCards> {
        vec![PlayerUpCards {
            player_root: vec![1],
            up_cards: vec![Card {
                suit: Suit::Hearts as i32,
                rank: Rank::Ace as i32,
            }],
        }]
    }

    #[test]
    fn rejects_when_hand_not_dealt() {
        let state = new_hand_state();
        let err = handle_deal_stud_street(
            DealStudStreet {
                street: StudStreet::FourthStreet as i32,
                up_cards: sample_up_cards(),
            },
            &state,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "HAND_NOT_DEALT");
    }

    #[test]
    fn rejects_when_street_unspecified() {
        let state = dealt_state();
        let err = handle_deal_stud_street(
            DealStudStreet {
                street: StudStreet::Unspecified as i32,
                up_cards: sample_up_cards(),
            },
            &state,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "INVALID_STUD_STREET");
    }

    #[test]
    fn rejects_when_street_out_of_range() {
        let state = dealt_state();
        let err = handle_deal_stud_street(
            DealStudStreet {
                street: 99,
                up_cards: sample_up_cards(),
            },
            &state,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "INVALID_STUD_STREET");
    }

    #[test]
    fn rejects_when_up_cards_empty() {
        let state = dealt_state();
        let err = handle_deal_stud_street(
            DealStudStreet {
                street: StudStreet::FourthStreet as i32,
                up_cards: vec![],
            },
            &state,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "NO_UP_CARDS_PROVIDED");
    }

    #[test]
    fn emits_stud_street_dealt_preserving_street_and_cards() {
        let state = dealt_state();
        let cards = sample_up_cards();
        let book = handle_deal_stud_street(
            DealStudStreet {
                street: StudStreet::FourthStreet as i32,
                up_cards: cards.clone(),
            },
            &state,
            12,
        )
        .expect("ok");
        assert_eq!(book.pages.len(), 1);
        let any = match book.pages[0].payload.as_ref().unwrap() {
            angzarr_client::proto::event_page::Payload::Event(any) => any,
            _ => panic!("expected event"),
        };
        let evt: StudStreetDealt = prost::Message::decode(any.value.as_slice()).expect("decode");
        assert_eq!(evt.street, StudStreet::FourthStreet as i32);
        assert_eq!(evt.up_cards.len(), 1);
        assert_eq!(evt.up_cards[0].player_root, vec![1u8]);
        assert!(evt.dealt_at.is_some());
    }
}
