//! DeclareAction handler — TDA Rule 49 (verbal action is binding).
//!
//! A player declares an action verbally without chips moving. The
//! handler resolves the chip amount (min legal raise, full stack,
//! current bet) and emits `ActionTaken` as if the chips had been
//! pushed. The actual side-effects are produced by the existing
//! `apply_action_taken` event applier.

use angzarr_client::proto::EventBook;
use angzarr_client::CommandResult;
use examples_proto::{ActionTaken, ActionType, DeclareAction};
use examples_utils::{event_page, pack_event, reject};

use crate::errors::{
    HandAlreadyComplete, HandNotDealt, InvalidAction, PlayerHasFolded, PlayerNotInHand,
    PlayerRootRequired, VerbalActionUnknown,
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

/// Resolve a declared chip amount.
/// - CALL: amount_to_call (current_bet - bet_this_round)
/// - RAISE with `amount == 0`: minimum legal raise = current_bet + min_raise
/// - RAISE with explicit amount: that amount
/// - ALL_IN: player's full remaining stack
/// - FOLD / CHECK: 0
pub(crate) fn resolve_declared_amount(
    action: ActionType,
    explicit_amount: i64,
    current_bet: i64,
    bet_this_round: i64,
    min_raise: i64,
    stack: i64,
) -> i64 {
    match action {
        ActionType::Fold | ActionType::Check => 0,
        ActionType::Call => (current_bet - bet_this_round).max(0).min(stack),
        ActionType::Raise => {
            if explicit_amount > 0 {
                explicit_amount.min(stack)
            } else {
                (current_bet + min_raise).min(stack)
            }
        }
        ActionType::AllIn => stack,
        ActionType::Bet => {
            if explicit_amount > 0 {
                explicit_amount.min(stack)
            } else {
                min_raise.min(stack)
            }
        }
        _ => 0,
    }
}

pub fn handle_declare_action(
    cmd: DeclareAction,
    state: &HandState,
    seq: u32,
) -> CommandResult<EventBook> {
    guard(state)?;
    if cmd.player_root.is_empty() {
        return Err(reject(PlayerRootRequired));
    }
    let player = state
        .get_player(&cmd.player_root)
        .ok_or_else(|| reject(PlayerNotInHand))?;
    if player.has_folded {
        return Err(reject(PlayerHasFolded));
    }
    let Ok(action) = ActionType::try_from(cmd.action) else {
        return Err(reject(VerbalActionUnknown {
            got: cmd.verbal.clone(),
        }));
    };
    if matches!(action, ActionType::ActionUnspecified) {
        return Err(reject(InvalidAction { got: cmd.action }));
    }

    let resolved = resolve_declared_amount(
        action,
        cmd.amount,
        state.current_bet,
        player.bet_this_round,
        state.min_raise,
        player.stack,
    );
    let new_stack = (player.stack - resolved).max(0);
    let event = ActionTaken {
        player_root: cmd.player_root,
        action: cmd.action,
        amount: resolved,
        player_stack: new_stack,
        pot_total: state.total_pot() + resolved,
        amount_to_call: (state.current_bet - player.bet_this_round).max(0),
        action_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.ActionTaken");
    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{apply_cards_dealt, new_hand_state};
    use examples_proto::{ActionType, Card, CardsDealt, GameVariant, PlayerInHand};

    fn dealt_state() -> HandState {
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
                remaining_deck: vec![Card { suit: 0, rank: 2 }; 10],
                ..Default::default()
            },
        );
        s
    }

    #[test]
    fn fold_resolves_to_zero() {
        assert_eq!(
            resolve_declared_amount(ActionType::Fold, 0, 50, 0, 50, 500),
            0
        );
    }

    #[test]
    fn check_resolves_to_zero() {
        assert_eq!(
            resolve_declared_amount(ActionType::Check, 0, 0, 0, 0, 500),
            0
        );
    }

    #[test]
    fn call_resolves_to_difference_capped_by_stack() {
        assert_eq!(
            resolve_declared_amount(ActionType::Call, 0, 100, 25, 0, 500),
            75
        );
        // Capped at stack
        assert_eq!(
            resolve_declared_amount(ActionType::Call, 0, 1000, 0, 0, 50),
            50
        );
    }

    #[test]
    fn raise_without_explicit_amount_uses_current_plus_min_raise() {
        assert_eq!(
            resolve_declared_amount(ActionType::Raise, 0, 100, 0, 50, 500),
            150
        );
    }

    #[test]
    fn raise_with_explicit_amount_uses_it_capped_by_stack() {
        assert_eq!(
            resolve_declared_amount(ActionType::Raise, 200, 50, 0, 25, 500),
            200
        );
        assert_eq!(
            resolve_declared_amount(ActionType::Raise, 1000, 50, 0, 25, 200),
            200
        );
    }

    #[test]
    fn all_in_resolves_to_full_stack() {
        assert_eq!(
            resolve_declared_amount(ActionType::AllIn, 0, 100, 0, 50, 350),
            350
        );
    }

    #[test]
    fn rejects_when_player_folded() {
        let mut s = dealt_state();
        s.get_player_mut(&[1]).unwrap().has_folded = true;
        let err = handle_declare_action(
            DeclareAction {
                player_root: vec![1],
                action: ActionType::Call as i32,
                amount: 0,
                verbal: "call".into(),
            },
            &s,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "PLAYER_HAS_FOLDED");
    }

    #[test]
    fn emits_action_taken_event() {
        let s = dealt_state();
        let book = handle_declare_action(
            DeclareAction {
                player_root: vec![1],
                action: ActionType::Call as i32,
                amount: 0,
                verbal: "call".into(),
            },
            &s,
            1,
        )
        .expect("ok");
        assert_eq!(book.pages.len(), 1);
    }

    fn decode_action_taken(page: &angzarr_client::proto::EventPage) -> ActionTaken {
        let payload = page.payload.as_ref().expect("payload set");
        let event_any = match payload {
            angzarr_client::proto::event_page::Payload::Event(any) => any,
            _ => panic!("expected event payload"),
        };
        use prost::Message;
        ActionTaken::decode(event_any.value.as_slice()).expect("decode action taken")
    }

    #[test]
    fn handle_declare_action_resolved_amount_reduces_player_stack() {
        // Kill `player.stack - resolved` mutation (line 98) by
        // asserting the actual player_stack field.
        let mut s = dealt_state();
        s.current_bet = 100;
        let book = handle_declare_action(
            DeclareAction {
                player_root: vec![1],
                action: ActionType::Call as i32,
                amount: 0,
                verbal: "call".into(),
            },
            &s,
            1,
        )
        .expect("ok");
        let event = decode_action_taken(&book.pages[0]);
        // player has stack=500, bet_this_round=0, current_bet=100 => resolves to 100
        assert_eq!(event.amount, 100);
        assert_eq!(
            event.player_stack, 400,
            "stack must decrease by resolved amount"
        );
    }

    #[test]
    fn handle_declare_action_pot_total_increases_by_resolved_amount() {
        // Kill `state.total_pot() + resolved` mutations (line 104).
        let mut s = dealt_state();
        s.current_bet = 75;
        // Seed pot with a previous bet
        s.pots[0].amount = 200;
        let book = handle_declare_action(
            DeclareAction {
                player_root: vec![1],
                action: ActionType::Call as i32,
                amount: 0,
                verbal: "call".into(),
            },
            &s,
            1,
        )
        .expect("ok");
        let event = decode_action_taken(&book.pages[0]);
        assert_eq!(event.amount, 75);
        assert_eq!(event.pot_total, 275, "pot must increase by resolved amount");
    }

    #[test]
    fn handle_declare_action_amount_to_call_equals_diff() {
        // Kill `current_bet - bet_this_round` mutation (line 105).
        let mut s = dealt_state();
        s.current_bet = 150;
        s.get_player_mut(&[1]).unwrap().bet_this_round = 25;
        let book = handle_declare_action(
            DeclareAction {
                player_root: vec![1],
                action: ActionType::Check as i32,
                amount: 0,
                verbal: "check".into(),
            },
            &s,
            1,
        )
        .expect("ok");
        let event = decode_action_taken(&book.pages[0]);
        assert_eq!(
            event.amount_to_call, 125,
            "must equal current_bet - bet_this_round"
        );
    }

    #[test]
    fn resolve_declared_amount_bet_returns_min_raise_when_explicit_zero() {
        // Kill "delete match arm ActionType::Bet" by exercising it.
        let resolved = resolve_declared_amount(ActionType::Bet, 0, 0, 0, 100, 500);
        assert_eq!(resolved, 100, "Bet with amount=0 uses min_raise");
    }

    #[test]
    fn resolve_declared_amount_bet_returns_explicit_when_given() {
        let resolved = resolve_declared_amount(ActionType::Bet, 250, 0, 0, 100, 500);
        assert_eq!(resolved, 250);
    }

    #[test]
    fn resolve_declared_amount_bet_capped_by_stack() {
        let resolved = resolve_declared_amount(ActionType::Bet, 9999, 0, 0, 100, 250);
        assert_eq!(resolved, 250);
    }

    #[test]
    fn resolve_declared_amount_fold_check_returns_zero_exact() {
        // Kill "delete match arm Fold | Check" by asserting both
        // return exactly 0 even with non-zero state inputs.
        assert_eq!(
            resolve_declared_amount(ActionType::Fold, 999, 100, 0, 100, 500),
            0
        );
        assert_eq!(
            resolve_declared_amount(ActionType::Check, 999, 100, 0, 100, 500),
            0
        );
    }

    #[test]
    fn resolve_declared_amount_raise_branch_explicit_positive_only() {
        // Kill `explicit_amount > 0` → `>=` / `==` / `<` mutations
        // (line 56). With explicit=0 the function takes the
        // current_bet+min_raise branch; with explicit>0 it uses
        // the explicit value. Three discriminating cases:
        // explicit=0 → 100+50 = 150
        // explicit=1 → 1 (not 150)
        // explicit=-1 → 100+50 = 150 (negative treated as no-explicit)
        assert_eq!(
            resolve_declared_amount(ActionType::Raise, 0, 100, 0, 50, 500),
            150
        );
        assert_eq!(
            resolve_declared_amount(ActionType::Raise, 1, 100, 0, 50, 500),
            1
        );
        // Negative explicit hits the fallback branch (since `> 0` is false).
        assert_eq!(
            resolve_declared_amount(ActionType::Raise, -1, 100, 0, 50, 500),
            150
        );
    }
}
