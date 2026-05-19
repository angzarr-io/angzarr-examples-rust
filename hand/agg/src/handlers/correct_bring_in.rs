//! CorrectBringIn command handler.
//!
//! WSOP §Seven Card Games / Robert's §SC Stud #5 — bring-in posted by
//! the wrong player on 3rd street; the floor reassigns to the correct
//! low-up-card player and refunds the wager. Emits BringInCorrected.

use angzarr_client::proto::EventBook;
use angzarr_client::CommandResult;
use examples_proto::{BringInCorrected, CorrectBringIn};
use examples_utils::{event_page, pack_event, reject};

use crate::errors::{
    CorrectRootRequired, HandAlreadyComplete, HandNotDealt, IncorrectAndCorrectRootIdentical,
    IncorrectRootRequired, OutsideCorrectionWindow, PlayerNotInHand,
    ReturnedAmountMustBeNonNegative,
};
use crate::state::HandState;

fn guard(state: &HandState) -> CommandResult<()> {
    if !state.exists() {
        return Err(reject(HandNotDealt));
    }
    if state.is_complete() {
        return Err(reject(HandAlreadyComplete));
    }
    // WSOP §SC Stud #5 / Robert's #5 — the window closes the moment the
    // next-to-act player acts. `bring_in_correction_window_open` is
    // opened by `apply_blind_posted` for blind_type="bring_in" and
    // cleared by `apply_action_taken`.
    if !state.bring_in_correction_window_open {
        return Err(reject(OutsideCorrectionWindow));
    }
    Ok(())
}

fn validate(cmd: &CorrectBringIn, state: &HandState) -> CommandResult<()> {
    if cmd.incorrect_root.is_empty() {
        return Err(reject(IncorrectRootRequired));
    }
    if cmd.correct_root.is_empty() {
        return Err(reject(CorrectRootRequired));
    }
    if cmd.incorrect_root == cmd.correct_root {
        return Err(reject(IncorrectAndCorrectRootIdentical));
    }
    if state.get_player(&cmd.incorrect_root).is_none() {
        return Err(reject(PlayerNotInHand));
    }
    if state.get_player(&cmd.correct_root).is_none() {
        return Err(reject(PlayerNotInHand));
    }
    if cmd.returned_amount < 0 {
        return Err(reject(ReturnedAmountMustBeNonNegative {
            value: cmd.returned_amount,
        }));
    }
    Ok(())
}

pub fn handle_correct_bring_in(
    cmd: CorrectBringIn,
    state: &HandState,
    seq: u32,
) -> CommandResult<EventBook> {
    guard(state)?;
    validate(&cmd, state)?;

    let event = BringInCorrected {
        incorrect_root: cmd.incorrect_root,
        correct_root: cmd.correct_root,
        returned_amount: cmd.returned_amount,
        corrected_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.BringInCorrected");
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
                game_variant: GameVariant::SevenCardStud as i32,
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
        // Open the bring-in correction window (a bring-in post would
        // normally do this via `apply_blind_posted`).
        state.bring_in_correction_window_open = true;
        state
    }

    #[test]
    fn rejects_when_hand_not_dealt() {
        let state = new_hand_state();
        let err = handle_correct_bring_in(
            CorrectBringIn {
                incorrect_root: vec![1],
                correct_root: vec![2],
                returned_amount: 50,
            },
            &state,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "HAND_NOT_DEALT");
    }

    #[test]
    fn rejects_when_incorrect_root_missing() {
        let state = dealt_state();
        let err = handle_correct_bring_in(
            CorrectBringIn {
                incorrect_root: vec![],
                correct_root: vec![2],
                returned_amount: 0,
            },
            &state,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "INCORRECT_ROOT_REQUIRED");
    }

    #[test]
    fn rejects_when_correct_root_missing() {
        let state = dealt_state();
        let err = handle_correct_bring_in(
            CorrectBringIn {
                incorrect_root: vec![1],
                correct_root: vec![],
                returned_amount: 0,
            },
            &state,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "CORRECT_ROOT_REQUIRED");
    }

    #[test]
    fn rejects_when_roots_identical() {
        let state = dealt_state();
        let err = handle_correct_bring_in(
            CorrectBringIn {
                incorrect_root: vec![1],
                correct_root: vec![1],
                returned_amount: 0,
            },
            &state,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "INCORRECT_AND_CORRECT_ROOT_IDENTICAL");
    }

    #[test]
    fn rejects_when_incorrect_player_unknown() {
        let state = dealt_state();
        let err = handle_correct_bring_in(
            CorrectBringIn {
                incorrect_root: vec![99],
                correct_root: vec![1],
                returned_amount: 0,
            },
            &state,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "PLAYER_NOT_IN_HAND");
    }

    #[test]
    fn rejects_when_correct_player_unknown() {
        let state = dealt_state();
        let err = handle_correct_bring_in(
            CorrectBringIn {
                incorrect_root: vec![1],
                correct_root: vec![99],
                returned_amount: 0,
            },
            &state,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "PLAYER_NOT_IN_HAND");
    }

    #[test]
    fn rejects_when_returned_amount_negative() {
        let state = dealt_state();
        let err = handle_correct_bring_in(
            CorrectBringIn {
                incorrect_root: vec![1],
                correct_root: vec![2],
                returned_amount: -5,
            },
            &state,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "RETURNED_AMOUNT_MUST_BE_NON_NEGATIVE");
    }

    #[test]
    fn zero_returned_amount_accepted() {
        let state = dealt_state();
        let book = handle_correct_bring_in(
            CorrectBringIn {
                incorrect_root: vec![1],
                correct_root: vec![2],
                returned_amount: 0,
            },
            &state,
            13,
        )
        .expect("ok");
        assert_eq!(book.pages.len(), 1);
    }

    #[test]
    fn rejects_when_correction_window_closed() {
        let mut state = dealt_state();
        state.bring_in_correction_window_open = false;
        let err = handle_correct_bring_in(
            CorrectBringIn {
                incorrect_root: vec![1],
                correct_root: vec![2],
                returned_amount: 50,
            },
            &state,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "OUTSIDE_CORRECTION_WINDOW");
    }

    #[test]
    fn emits_bring_in_corrected_with_both_roots_and_amount() {
        let state = dealt_state();
        let book = handle_correct_bring_in(
            CorrectBringIn {
                incorrect_root: vec![1],
                correct_root: vec![2],
                returned_amount: 25,
            },
            &state,
            14,
        )
        .expect("ok");
        let any = match book.pages[0].payload.as_ref().unwrap() {
            angzarr_client::proto::event_page::Payload::Event(any) => any,
            _ => panic!(),
        };
        let evt: BringInCorrected = prost::Message::decode(any.value.as_slice()).expect("decode");
        assert_eq!(evt.incorrect_root, vec![1u8]);
        assert_eq!(evt.correct_root, vec![2u8]);
        assert_eq!(evt.returned_amount, 25);
    }
}
