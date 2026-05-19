//! CorrectIllegalBet handler — TDA Rule 52B.
//!
//! Operator-issued correction of an illegal pot-limit overbet or
//! declared underraise. Reduces every caller's contribution on the
//! current street to ``corrected_amount`` and emits
//! ``UnderbetCorrected`` with per-player adjustments computed from
//! the current `bet_this_round` values.

use angzarr_client::proto::EventBook;
use angzarr_client::CommandResult;
use examples_proto::{CorrectIllegalBet, UnderbetAdjustment, UnderbetCorrected};
use examples_utils::{event_page, pack_event, reject};

use crate::errors::{
    CorrectedAmountMustBePositive, CorrectionReasonRequired, HandAlreadyComplete, HandNotDealt,
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

/// Build the per-player adjustment for a single player whose bet
/// exceeded `corrected_amount`. Returns `None` when the player's bet
/// is at-or-under the corrected value (no refund needed).
pub(crate) fn adjustment_for_player(
    player_root: &[u8],
    bet_this_round: i64,
    corrected_amount: i64,
) -> Option<UnderbetAdjustment> {
    if bet_this_round <= corrected_amount {
        return None;
    }
    let refund = bet_this_round - corrected_amount;
    Some(UnderbetAdjustment {
        player_root: player_root.to_vec(),
        prior_contribution: bet_this_round,
        new_contribution: corrected_amount,
        refund_to_stack: refund,
    })
}

pub fn handle_correct_illegal_bet(
    cmd: CorrectIllegalBet,
    state: &HandState,
    seq: u32,
) -> CommandResult<EventBook> {
    guard(state)?;
    if cmd.reason.is_empty() {
        return Err(reject(CorrectionReasonRequired));
    }
    if cmd.corrected_amount <= 0 {
        return Err(reject(CorrectedAmountMustBePositive {
            value: cmd.corrected_amount,
        }));
    }

    // Build adjustments — iterate by player_root for deterministic
    // output ordering (HashMap iteration order is not stable).
    let mut players: Vec<_> = state.players.values().collect();
    players.sort_by(|a, b| hex::encode(&a.player_root).cmp(&hex::encode(&b.player_root)));
    let adjustments: Vec<UnderbetAdjustment> = players
        .into_iter()
        .filter(|p| !p.has_folded)
        .filter_map(|p| {
            adjustment_for_player(&p.player_root, p.bet_this_round, cmd.corrected_amount)
        })
        .collect();

    let event = UnderbetCorrected {
        reason: cmd.reason,
        corrected_amount: cmd.corrected_amount,
        adjustments,
        corrected_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.UnderbetCorrected");
    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{apply_cards_dealt, new_hand_state};
    use examples_proto::{Card, CardsDealt, GameVariant, PlayerInHand};

    fn dealt_state_with_bets(bet_a: i64, bet_b: i64) -> HandState {
        let mut s = new_hand_state();
        apply_cards_dealt(
            &mut s,
            CardsDealt {
                table_root: vec![0xab],
                hand_number: 1,
                game_variant: GameVariant::TexasHoldem as i32,
                player_cards: vec![],
                dealer_position: 0,
                players: vec![
                    PlayerInHand {
                        player_root: vec![0x01],
                        position: 0,
                        stack: 500,
                        ..Default::default()
                    },
                    PlayerInHand {
                        player_root: vec![0x02],
                        position: 1,
                        stack: 500,
                        ..Default::default()
                    },
                ],
                dealt_at: None,
                remaining_deck: vec![Card { suit: 0, rank: 2 }; 10],
                ..Default::default()
            },
        );
        s.get_player_mut(&[0x01]).unwrap().bet_this_round = bet_a;
        s.get_player_mut(&[0x02]).unwrap().bet_this_round = bet_b;
        s
    }

    #[test]
    fn rejects_empty_reason() {
        let s = dealt_state_with_bets(0, 0);
        let err = handle_correct_illegal_bet(
            CorrectIllegalBet {
                reason: String::new(),
                corrected_amount: 100,
            },
            &s,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "CORRECTION_REASON_REQUIRED");
    }

    #[test]
    fn rejects_zero_corrected_amount() {
        let s = dealt_state_with_bets(0, 0);
        let err = handle_correct_illegal_bet(
            CorrectIllegalBet {
                reason: "PL_ILLEGAL_OVERBET".into(),
                corrected_amount: 0,
            },
            &s,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "CORRECTED_AMOUNT_MUST_BE_POSITIVE");
    }

    #[test]
    fn adjustment_skipped_when_bet_at_or_under_corrected() {
        assert!(adjustment_for_player(&[1], 50, 50).is_none());
        assert!(adjustment_for_player(&[1], 25, 50).is_none());
    }

    #[test]
    fn adjustment_refunds_excess_when_bet_above_corrected() {
        let adj = adjustment_for_player(&[1], 200, 50).expect("adjustment");
        assert_eq!(adj.prior_contribution, 200);
        assert_eq!(adj.new_contribution, 50);
        assert_eq!(adj.refund_to_stack, 150);
    }

    #[test]
    fn emits_correction_event() {
        let s = dealt_state_with_bets(200, 100);
        let book = handle_correct_illegal_bet(
            CorrectIllegalBet {
                reason: "PL_ILLEGAL_OVERBET".into(),
                corrected_amount: 50,
            },
            &s,
            1,
        )
        .expect("ok");
        assert_eq!(book.pages.len(), 1);
    }

    fn decode_correction(page: &angzarr_client::proto::EventPage) -> UnderbetCorrected {
        let payload = page.payload.as_ref().expect("payload set");
        let any = match payload {
            angzarr_client::proto::event_page::Payload::Event(any) => any,
            _ => panic!("expected event"),
        };
        use prost::Message;
        UnderbetCorrected::decode(any.value.as_slice()).expect("decode")
    }

    #[test]
    fn folded_players_are_excluded_from_adjustments() {
        // Kill `delete !` mutation on `filter(|p| !p.has_folded)`.
        // Without the `!`, folded players would receive
        // adjustments. Set player 0x01 with bet=200 and FOLDED;
        // set player 0x02 with bet=200 and NOT folded. Correct
        // to 50; expect only player 0x02 in the adjustments.
        let mut s = dealt_state_with_bets(200, 200);
        s.get_player_mut(&[0x01]).unwrap().has_folded = true;
        let book = handle_correct_illegal_bet(
            CorrectIllegalBet {
                reason: "PL_ILLEGAL_OVERBET".into(),
                corrected_amount: 50,
            },
            &s,
            1,
        )
        .expect("ok");
        let event = decode_correction(&book.pages[0]);
        assert_eq!(
            event.adjustments.len(),
            1,
            "only non-folded player should receive an adjustment"
        );
        assert_eq!(
            event.adjustments[0].player_root,
            vec![0x02],
            "folded player 0x01 must be excluded; only 0x02 remains"
        );
    }
}
