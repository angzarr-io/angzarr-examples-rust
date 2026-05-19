//! ReportPrematureTurn command handler.
//!
//! TDA RP-5B — premature turn detected. Same procedure as flop. Emits
//! PrematureTurnDetected.

use angzarr_client::proto::EventBook;
use angzarr_client::CommandResult;
use examples_proto::{PrematureTurnDetected, ReportPrematureTurn};
use examples_utils::{event_page, pack_event, reject};

use crate::errors::{HandAlreadyComplete, HandNotDealt};
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

pub fn handle_report_premature_turn(
    _cmd: ReportPrematureTurn,
    state: &HandState,
    seq: u32,
) -> CommandResult<EventBook> {
    guard(state)?;

    let event = PrematureTurnDetected {
        detected_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.PrematureTurnDetected");
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
        let err = handle_report_premature_turn(ReportPrematureTurn {}, &state, 1).unwrap_err();
        assert_eq!(err.code, "HAND_NOT_DEALT");
    }

    #[test]
    fn rejects_when_hand_complete() {
        let mut state = dealt_state();
        state.status = "complete".into();
        let err = handle_report_premature_turn(ReportPrematureTurn {}, &state, 1).unwrap_err();
        assert_eq!(err.code, "HAND_ALREADY_COMPLETE");
    }

    #[test]
    fn emits_premature_turn_detected() {
        let state = dealt_state();
        let book = handle_report_premature_turn(ReportPrematureTurn {}, &state, 9).expect("ok");
        assert_eq!(book.pages.len(), 1);
    }
}
