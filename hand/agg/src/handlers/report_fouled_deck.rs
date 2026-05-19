//! ReportFouledDeck command handler.
//!
//! TDA Rule 35E — fouled deck detected. Two cards of the same rank+suit
//! were found, regardless of substantial action. Play stops, all bets are
//! returned, the hand is voided. Emits FouledDeckDetected.

use angzarr_client::proto::EventBook;
use angzarr_client::CommandResult;
use examples_proto::{FouledDeckDetected, ReportFouledDeck};
use examples_utils::{event_page, pack_event, reject};

use crate::errors::{DuplicateCardRequired, HandNotDealt};
use crate::state::HandState;

fn guard(state: &HandState) -> CommandResult<()> {
    if !state.exists() {
        return Err(reject(HandNotDealt));
    }
    Ok(())
}

fn validate(cmd: &ReportFouledDeck) -> CommandResult<()> {
    if cmd.duplicate_card.is_empty() {
        return Err(reject(DuplicateCardRequired));
    }
    Ok(())
}

pub fn handle_report_fouled_deck(
    cmd: ReportFouledDeck,
    state: &HandState,
    seq: u32,
) -> CommandResult<EventBook> {
    guard(state)?;
    validate(&cmd)?;

    let event = FouledDeckDetected {
        duplicate_card: cmd.duplicate_card,
        detected_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.FouledDeckDetected");
    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{apply_cards_dealt, new_hand_state};
    use examples_proto::{CardsDealt, GameVariant, PlayerInHand};

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

    #[test]
    fn rejects_when_hand_not_dealt() {
        let state = new_hand_state();
        let err = handle_report_fouled_deck(
            ReportFouledDeck {
                duplicate_card: "Ah".into(),
            },
            &state,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "HAND_NOT_DEALT");
    }

    #[test]
    fn rejects_when_duplicate_card_missing() {
        let state = dealt_state();
        let err = handle_report_fouled_deck(
            ReportFouledDeck {
                duplicate_card: String::new(),
            },
            &state,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "DUPLICATE_CARD_REQUIRED");
    }

    #[test]
    fn emits_fouled_deck_detected_with_duplicate_card() {
        let state = dealt_state();
        let book = handle_report_fouled_deck(
            ReportFouledDeck {
                duplicate_card: "Ks".into(),
            },
            &state,
            3,
        )
        .expect("ok");
        assert_eq!(book.pages.len(), 1);
    }
}
