//! DeclareMisdeal command handler.
//!
//! TDA Rule 35 family — floor declares the hand is a misdeal. The
//! aggregate emits MisdealDeclared; downstream a saga / floor decision
//! drives recovery (RedealHand or hand void).

use angzarr_client::proto::EventBook;
use angzarr_client::CommandResult;
use examples_proto::{DeclareMisdeal, MisdealDeclared};
use examples_utils::{event_page, pack_event, reject};

use crate::errors::{
    HandAlreadyComplete, HandNotDealt, MisdealReasonRequired, PostSubstantialActionMisdeal,
};
use crate::state::HandState;

fn guard(state: &HandState) -> CommandResult<()> {
    if !state.exists() {
        return Err(reject(HandNotDealt));
    }
    if state.is_complete() {
        return Err(reject(HandAlreadyComplete));
    }
    // TDA Rule 35-D — misdeal cannot be declared after substantial
    // action has occurred. The flag is set by `apply_action_taken` once
    // Rule 36's SA criteria are met.
    if state.substantial_action_occurred {
        return Err(reject(PostSubstantialActionMisdeal));
    }
    Ok(())
}

fn validate(cmd: &DeclareMisdeal) -> CommandResult<()> {
    if cmd.reason.is_empty() {
        return Err(reject(MisdealReasonRequired));
    }
    Ok(())
}

pub fn handle_declare_misdeal(
    cmd: DeclareMisdeal,
    state: &HandState,
    seq: u32,
) -> CommandResult<EventBook> {
    guard(state)?;
    validate(&cmd)?;

    let event = MisdealDeclared {
        reason: cmd.reason,
        dealer_button_preserved: cmd.dealer_button_preserved,
        declared_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.MisdealDeclared");
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
        let err = handle_declare_misdeal(
            DeclareMisdeal {
                reason: "EXPOSED_BUTTON_CARD".into(),
                dealer_button_preserved: true,
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
        let err = handle_declare_misdeal(
            DeclareMisdeal {
                reason: "x".into(),
                dealer_button_preserved: false,
            },
            &state,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "HAND_ALREADY_COMPLETE");
    }

    #[test]
    fn rejects_when_reason_missing() {
        let state = dealt_state();
        let err = handle_declare_misdeal(
            DeclareMisdeal {
                reason: String::new(),
                dealer_button_preserved: false,
            },
            &state,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "MISDEAL_REASON_REQUIRED");
    }

    #[test]
    fn rejects_when_substantial_action_occurred() {
        let mut state = dealt_state();
        state.substantial_action_occurred = true;
        let err = handle_declare_misdeal(
            DeclareMisdeal {
                reason: "EXPOSED_BUTTON_CARD".into(),
                dealer_button_preserved: true,
            },
            &state,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "POST_SUBSTANTIAL_ACTION_MISDEAL");
    }

    #[test]
    fn emits_misdeal_declared_with_reason_and_button_flag() {
        let state = dealt_state();
        let book = handle_declare_misdeal(
            DeclareMisdeal {
                reason: "EXPOSED_BUTTON_CARD".into(),
                dealer_button_preserved: true,
            },
            &state,
            7,
        )
        .expect("ok");
        assert_eq!(book.pages.len(), 1);
    }
}
