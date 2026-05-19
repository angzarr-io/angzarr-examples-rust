//! RedealHand command handler.
//!
//! TDA Rule 35C — pre-SA misdeal triggered a redeal. Same button, same
//! blind level, same player roster. Emits HandRedealt. (A fresh CardsDealt
//! sequence follows downstream.)

use angzarr_client::proto::EventBook;
use angzarr_client::CommandResult;
use examples_proto::{HandRedealt, RedealHand};
use examples_utils::{event_page, pack_event, reject};

use crate::errors::{
    BlindAmountMustBePositive, BlindLevelMustBeNonNegative, HandNotDealt, TableRootRequired,
};
use crate::state::HandState;

fn guard(state: &HandState) -> CommandResult<()> {
    if !state.exists() {
        return Err(reject(HandNotDealt));
    }
    Ok(())
}

fn validate(cmd: &RedealHand) -> CommandResult<()> {
    if cmd.table_root.is_empty() {
        return Err(reject(TableRootRequired));
    }
    if cmd.small_blind <= 0 {
        return Err(reject(BlindAmountMustBePositive {
            value: cmd.small_blind,
        }));
    }
    if cmd.big_blind <= 0 {
        return Err(reject(BlindAmountMustBePositive {
            value: cmd.big_blind,
        }));
    }
    if cmd.level < 0 {
        return Err(reject(BlindLevelMustBeNonNegative { value: cmd.level }));
    }
    Ok(())
}

pub fn handle_redeal_hand(
    cmd: RedealHand,
    state: &HandState,
    seq: u32,
) -> CommandResult<EventBook> {
    guard(state)?;
    validate(&cmd)?;

    let event = HandRedealt {
        table_root: cmd.table_root,
        hand_number: cmd.hand_number,
        dealer_position: cmd.dealer_position,
        small_blind: cmd.small_blind,
        big_blind: cmd.big_blind,
        level: cmd.level,
        redealt_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.HandRedealt");
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

    fn good_cmd() -> RedealHand {
        RedealHand {
            table_root: vec![0xab],
            hand_number: 1,
            dealer_position: 0,
            small_blind: 5,
            big_blind: 10,
            level: 3,
        }
    }

    #[test]
    fn rejects_when_hand_not_dealt() {
        let state = new_hand_state();
        let err = handle_redeal_hand(good_cmd(), &state, 1).unwrap_err();
        assert_eq!(err.code, "HAND_NOT_DEALT");
    }

    #[test]
    fn rejects_when_table_root_missing() {
        let state = dealt_state();
        let mut cmd = good_cmd();
        cmd.table_root = vec![];
        let err = handle_redeal_hand(cmd, &state, 1).unwrap_err();
        assert_eq!(err.code, "TABLE_ROOT_REQUIRED");
    }

    #[test]
    fn rejects_when_small_blind_not_positive() {
        let state = dealt_state();
        let mut cmd = good_cmd();
        cmd.small_blind = 0;
        let err = handle_redeal_hand(cmd, &state, 1).unwrap_err();
        assert_eq!(err.code, "BLIND_AMOUNT_MUST_BE_POSITIVE");
    }

    #[test]
    fn rejects_when_big_blind_not_positive() {
        let state = dealt_state();
        let mut cmd = good_cmd();
        cmd.big_blind = -5;
        let err = handle_redeal_hand(cmd, &state, 1).unwrap_err();
        assert_eq!(err.code, "BLIND_AMOUNT_MUST_BE_POSITIVE");
    }

    #[test]
    fn rejects_when_level_negative() {
        let state = dealt_state();
        let mut cmd = good_cmd();
        cmd.level = -1;
        let err = handle_redeal_hand(cmd, &state, 1).unwrap_err();
        assert_eq!(err.code, "BLIND_LEVEL_MUST_BE_NON_NEGATIVE");
    }

    #[test]
    fn level_zero_is_accepted() {
        let state = dealt_state();
        let mut cmd = good_cmd();
        cmd.level = 0;
        let book = handle_redeal_hand(cmd, &state, 2).expect("ok");
        assert_eq!(book.pages.len(), 1);
    }

    #[test]
    fn emits_hand_redealt_preserving_all_fields() {
        let state = dealt_state();
        let cmd = good_cmd();
        let expected_table_root = cmd.table_root.clone();
        let book = handle_redeal_hand(cmd, &state, 5).expect("ok");
        assert_eq!(book.pages.len(), 1);
        // Decode to confirm field preservation
        let any = match book.pages[0].payload.as_ref().unwrap() {
            angzarr_client::proto::event_page::Payload::Event(any) => any,
            _ => panic!("expected event payload"),
        };
        let evt: HandRedealt = prost::Message::decode(any.value.as_slice()).expect("decode");
        assert_eq!(evt.table_root, expected_table_root);
        assert_eq!(evt.hand_number, 1);
        assert_eq!(evt.dealer_position, 0);
        assert_eq!(evt.small_blind, 5);
        assert_eq!(evt.big_blind, 10);
        assert_eq!(evt.level, 3);
        assert!(evt.redealt_at.is_some());
    }
}
