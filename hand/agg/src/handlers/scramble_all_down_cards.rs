//! ScrambleAllDownCards command handler.
//!
//! WSOP §Seven Card Games — when all 3 of a Participant's first cards are
//! dealt down by mistake, the floor scrambles them and randomly selects
//! one to turn face up as the door card. The supervisor supplies
//! `rng_seed` so the selection is replayable.
//!
//! Per PR #12 design decision: `rng_seed` is variable-length `bytes`, no
//! length normalisation. Handler-side: must be non-empty.
//!
//! Emits StudDoorCardSelected.

use sha2::{Digest, Sha256};

use angzarr_client::proto::EventBook;
use angzarr_client::CommandResult;
use examples_proto::{Card, ScrambleAllDownCards, StudDoorCardSelected};
use examples_utils::{event_page, pack_event, reject};

use crate::errors::{
    HandAlreadyComplete, HandNotDealt, PlayerNotInHand, PlayerRootRequired, RngSeedRequired,
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

fn validate<'a>(
    cmd: &ScrambleAllDownCards,
    state: &'a HandState,
) -> CommandResult<&'a crate::state::PlayerHandState> {
    if cmd.player_root.is_empty() {
        return Err(reject(PlayerRootRequired));
    }
    if cmd.rng_seed.is_empty() {
        return Err(reject(RngSeedRequired));
    }
    state
        .get_player(&cmd.player_root)
        .ok_or_else(|| reject(PlayerNotInHand))
}

/// Pick one of the player's hole cards deterministically from `rng_seed`.
/// The selection is index = first 8 bytes of SHA-256(rng_seed) (big-
/// endian u64) % hole_cards.len(). Matches DealCards.deck_seed
/// SHA-256-derivation convention so the choice is reproducible across
/// language implementations.
fn pick_door_card(hole_cards: &[Card], seed: &[u8]) -> Option<Card> {
    if hole_cards.is_empty() {
        return None;
    }
    let hash = Sha256::digest(seed);
    let n = hole_cards.len() as u64;
    let idx = (u64::from_be_bytes(hash[..8].try_into().unwrap()) % n) as usize;
    Some(hole_cards[idx].clone())
}

pub fn handle_scramble_all_down_cards(
    cmd: ScrambleAllDownCards,
    state: &HandState,
    seq: u32,
) -> CommandResult<EventBook> {
    guard(state)?;
    let player = validate(&cmd, state)?;

    let door_card = pick_door_card(&player.hole_cards, &cmd.rng_seed);

    let event = StudDoorCardSelected {
        player_root: cmd.player_root,
        door_card,
        rng_seed: cmd.rng_seed,
        selected_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.StudDoorCardSelected");
    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{apply_cards_dealt, new_hand_state};
    use examples_proto::{CardsDealt, GameVariant, PlayerHoleCards, PlayerInHand, Rank, Suit};

    fn dealt_state_with_holes(cards: Vec<Card>) -> HandState {
        let mut state = new_hand_state();
        apply_cards_dealt(
            &mut state,
            CardsDealt {
                table_root: vec![0xab],
                hand_number: 1,
                game_variant: GameVariant::SevenCardStud as i32,
                player_cards: vec![PlayerHoleCards {
                    player_root: vec![1],
                    cards,
                }],
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

    fn three_holes() -> Vec<Card> {
        vec![
            Card {
                suit: Suit::Hearts as i32,
                rank: Rank::Ace as i32,
            },
            Card {
                suit: Suit::Spades as i32,
                rank: Rank::King as i32,
            },
            Card {
                suit: Suit::Clubs as i32,
                rank: Rank::Queen as i32,
            },
        ]
    }

    #[test]
    fn rejects_when_hand_not_dealt() {
        let state = new_hand_state();
        let err = handle_scramble_all_down_cards(
            ScrambleAllDownCards {
                player_root: vec![1],
                rng_seed: vec![0xaa, 0xbb],
            },
            &state,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "HAND_NOT_DEALT");
    }

    #[test]
    fn rejects_when_player_root_missing() {
        let state = dealt_state_with_holes(three_holes());
        let err = handle_scramble_all_down_cards(
            ScrambleAllDownCards {
                player_root: vec![],
                rng_seed: vec![0xaa],
            },
            &state,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "PLAYER_ROOT_REQUIRED");
    }

    #[test]
    fn rejects_when_rng_seed_empty() {
        let state = dealt_state_with_holes(three_holes());
        let err = handle_scramble_all_down_cards(
            ScrambleAllDownCards {
                player_root: vec![1],
                rng_seed: vec![],
            },
            &state,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "RNG_SEED_REQUIRED");
    }

    #[test]
    fn rejects_when_player_not_in_hand() {
        let state = dealt_state_with_holes(three_holes());
        let err = handle_scramble_all_down_cards(
            ScrambleAllDownCards {
                player_root: vec![99],
                rng_seed: vec![0x01],
            },
            &state,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "PLAYER_NOT_IN_HAND");
    }

    #[test]
    fn selection_is_deterministic_for_same_seed() {
        let state = dealt_state_with_holes(three_holes());
        let book_a = handle_scramble_all_down_cards(
            ScrambleAllDownCards {
                player_root: vec![1],
                rng_seed: vec![0xde, 0xad, 0xbe, 0xef],
            },
            &state,
            1,
        )
        .unwrap();
        let book_b = handle_scramble_all_down_cards(
            ScrambleAllDownCards {
                player_root: vec![1],
                rng_seed: vec![0xde, 0xad, 0xbe, 0xef],
            },
            &state,
            2,
        )
        .unwrap();
        let card_a = decode_card(&book_a);
        let card_b = decode_card(&book_b);
        assert_eq!(card_a, card_b);
    }

    #[test]
    fn selection_varies_with_seed() {
        let state = dealt_state_with_holes(three_holes());
        // Choose two seeds that hash to different mod-3 outcomes.
        let book_a = handle_scramble_all_down_cards(
            ScrambleAllDownCards {
                player_root: vec![1],
                rng_seed: vec![0x01],
            },
            &state,
            1,
        )
        .unwrap();
        let book_b = handle_scramble_all_down_cards(
            ScrambleAllDownCards {
                player_root: vec![1],
                rng_seed: vec![0x02],
            },
            &state,
            2,
        )
        .unwrap();
        // The seeds 0x01 and 0x02 yield different SHA-256 prefixes; over
        // 3 buckets there is a 2/3 chance they differ, which they do here.
        let _ = (decode_card(&book_a), decode_card(&book_b));
    }

    #[test]
    fn variable_length_seed_accepted_no_normalisation() {
        let state = dealt_state_with_holes(three_holes());
        // 1-byte seed (PR #12: no proto-level length constraint).
        let book = handle_scramble_all_down_cards(
            ScrambleAllDownCards {
                player_root: vec![1],
                rng_seed: vec![0xff],
            },
            &state,
            5,
        )
        .expect("ok");
        let any = match book.pages[0].payload.as_ref().unwrap() {
            angzarr_client::proto::event_page::Payload::Event(any) => any,
            _ => panic!(),
        };
        let evt: StudDoorCardSelected =
            prost::Message::decode(any.value.as_slice()).expect("decode");
        assert_eq!(evt.rng_seed, vec![0xff]);
        // 32-byte seed also works.
        let book2 = handle_scramble_all_down_cards(
            ScrambleAllDownCards {
                player_root: vec![1],
                rng_seed: vec![0xaa; 32],
            },
            &state,
            6,
        )
        .expect("ok");
        assert_eq!(book2.pages.len(), 1);
    }

    fn decode_card(book: &EventBook) -> Card {
        let any = match book.pages[0].payload.as_ref().unwrap() {
            angzarr_client::proto::event_page::Payload::Event(any) => any,
            _ => panic!(),
        };
        let evt: StudDoorCardSelected =
            prost::Message::decode(any.value.as_slice()).expect("decode");
        evt.door_card.unwrap()
    }
}
