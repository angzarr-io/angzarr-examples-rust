//! ReportExposedStudDowncard command handler.
//!
//! TDA RP-10A — dealer or floor reports that a card meant to be dealt
//! face-down on the initial deal was exposed. The exposed card becomes
//! the player's upcard and the next dealt card (the door card) is dealt
//! face down to compensate. Emits StudDownCardConverted.

use angzarr_client::proto::EventBook;
use angzarr_client::CommandResult;
use examples_proto::{ReportExposedStudDowncard, StudDownCardConverted};
use examples_utils::{event_page, pack_event, reject};

use crate::errors::{
    ExposedCardRequired, HandAlreadyComplete, HandNotDealt, PlayerNotInHand, PlayerRootRequired,
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

fn validate(cmd: &ReportExposedStudDowncard, state: &HandState) -> CommandResult<()> {
    if cmd.player_root.is_empty() {
        return Err(reject(PlayerRootRequired));
    }
    if state.get_player(&cmd.player_root).is_none() {
        return Err(reject(PlayerNotInHand));
    }
    if cmd.exposed_card.is_none() {
        return Err(reject(ExposedCardRequired));
    }
    Ok(())
}

pub fn handle_report_exposed_stud_downcard(
    cmd: ReportExposedStudDowncard,
    state: &HandState,
    seq: u32,
) -> CommandResult<EventBook> {
    guard(state)?;
    validate(&cmd, state)?;

    let event = StudDownCardConverted {
        player_root: cmd.player_root,
        exposed_card: cmd.exposed_card,
        converted_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.StudDownCardConverted");
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

    fn ace() -> Card {
        Card {
            suit: Suit::Hearts as i32,
            rank: Rank::Ace as i32,
        }
    }

    #[test]
    fn rejects_when_hand_not_dealt() {
        let state = new_hand_state();
        let err = handle_report_exposed_stud_downcard(
            ReportExposedStudDowncard {
                player_root: vec![1],
                exposed_card: Some(ace()),
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
        let err = handle_report_exposed_stud_downcard(
            ReportExposedStudDowncard {
                player_root: vec![],
                exposed_card: Some(ace()),
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
        let err = handle_report_exposed_stud_downcard(
            ReportExposedStudDowncard {
                player_root: vec![99],
                exposed_card: Some(ace()),
            },
            &state,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "PLAYER_NOT_IN_HAND");
    }

    #[test]
    fn rejects_when_exposed_card_missing() {
        let state = dealt_state();
        let err = handle_report_exposed_stud_downcard(
            ReportExposedStudDowncard {
                player_root: vec![1],
                exposed_card: None,
            },
            &state,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "EXPOSED_CARD_REQUIRED");
    }

    #[test]
    fn emits_stud_down_card_converted() {
        let state = dealt_state();
        let book = handle_report_exposed_stud_downcard(
            ReportExposedStudDowncard {
                player_root: vec![1],
                exposed_card: Some(ace()),
            },
            &state,
            7,
        )
        .expect("ok");
        assert_eq!(book.pages.len(), 1);
    }
}
