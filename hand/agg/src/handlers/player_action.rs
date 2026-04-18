//! PlayerAction command handler.

use angzarr_client::proto::EventBook;
use angzarr_client::{event_page, pack_event, CommandRejectedError, CommandResult};
use examples_proto::{ActionTaken, ActionType, PlayerAction};

use crate::state::{HandState, PlayerHandState};

/// Validated action result containing computed values.
struct ValidatedAction {
    chips_put_in: i64,
    event_amount: i64,
    final_action: ActionType,
}

fn guard(state: &HandState) -> CommandResult<()> {
    if !state.exists() {
        return Err(CommandRejectedError::new("Hand does not exist"));
    }
    if state.is_complete() {
        return Err(CommandRejectedError::new("Hand already complete"));
    }
    if state.status != "betting" {
        return Err(CommandRejectedError::new("Not in betting phase"));
    }
    Ok(())
}

fn validate<'a>(
    cmd: &PlayerAction,
    state: &'a HandState,
) -> CommandResult<(&'a PlayerHandState, ValidatedAction)> {
    let player = state
        .get_player(&cmd.player_root)
        .ok_or_else(|| CommandRejectedError::new("Player not in hand"))?;

    if player.has_folded {
        return Err(CommandRejectedError::new("Player has folded"));
    }
    if player.is_all_in {
        return Err(CommandRejectedError::new("Player is all-in"));
    }

    let action = ActionType::try_from(cmd.action).unwrap_or_default();
    let amount_to_call = state.current_bet - player.bet_this_round;
    let chips_put_in: i64;
    let event_amount: i64;

    match action {
        ActionType::Fold => {
            chips_put_in = 0;
            event_amount = 0;
        }
        ActionType::Check => {
            if amount_to_call > 0 {
                return Err(CommandRejectedError::invalid_argument(
                    "Cannot check, must call or fold",
                ));
            }
            chips_put_in = 0;
            event_amount = 0;
        }
        ActionType::Call => {
            if amount_to_call <= 0 {
                return Err(CommandRejectedError::new("Nothing to call"));
            }
            chips_put_in = amount_to_call.min(player.stack);
            event_amount = chips_put_in;
        }
        ActionType::Bet => {
            if state.current_bet > 0 {
                return Err(CommandRejectedError::invalid_argument(
                    "Cannot bet, use raise",
                ));
            }
            if cmd.amount < state.min_raise {
                return Err(CommandRejectedError::invalid_argument(format!(
                    "Bet must be at least {}",
                    state.min_raise
                )));
            }
            chips_put_in = cmd.amount.min(player.stack);
            event_amount = chips_put_in;
        }
        ActionType::Raise => {
            if state.current_bet <= 0 {
                return Err(CommandRejectedError::invalid_argument(
                    "Cannot raise, use bet",
                ));
            }
            let raise_amount = cmd.amount - state.current_bet;
            if raise_amount < state.min_raise {
                return Err(CommandRejectedError::invalid_argument(
                    "Raise below minimum",
                ));
            }
            let to_put_in = cmd.amount - player.bet_this_round;
            chips_put_in = to_put_in.min(player.stack);
            event_amount = cmd.amount;
        }
        ActionType::AllIn => {
            chips_put_in = player.stack;
            event_amount = chips_put_in;
        }
        _ => {
            return Err(CommandRejectedError::new("Unknown action"));
        }
    }

    let final_action = if chips_put_in == player.stack && chips_put_in > 0 {
        ActionType::AllIn
    } else {
        action
    };

    Ok((
        player,
        ValidatedAction {
            chips_put_in,
            event_amount,
            final_action,
        },
    ))
}

fn compute(
    cmd: &PlayerAction,
    state: &HandState,
    player: &PlayerHandState,
    validated: &ValidatedAction,
) -> ActionTaken {
    let new_stack = player.stack - validated.chips_put_in;
    let new_pot = state.total_pot() + validated.chips_put_in;
    let player_total_bet = player.bet_this_round + validated.chips_put_in;
    let new_current_bet = state.current_bet.max(player_total_bet);

    ActionTaken {
        player_root: cmd.player_root.clone(),
        action: validated.final_action as i32,
        amount: validated.event_amount,
        player_stack: new_stack,
        pot_total: new_pot,
        amount_to_call: new_current_bet,
        action_at: Some(angzarr_client::now()),
    }
}

pub fn handle_player_action(
    cmd: PlayerAction,
    state: &HandState,
    seq: u32,
) -> CommandResult<EventBook> {
        guard(state)?;
    let (player, validated) = validate(&cmd, state)?;

    let event = compute(&cmd, state, player, &validated);
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
    use angzarr_client::proto::event_page::Payload;
    use examples_proto::{CardsDealt, GameVariant, PlayerInHand};
    use prost::Message;

    fn betting_state() -> HandState {
        let mut state = new_hand_state();
        apply_cards_dealt(
            &mut state,
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
                    },
                    PlayerInHand {
                        player_root: vec![2],
                        position: 1,
                        stack: 500,
                    },
                ],
                dealt_at: None,
                remaining_deck: vec![],
            },
        );
        state
    }

    #[test]
    fn handle_player_action_fold_success() {
        let state = betting_state();
        let cmd = PlayerAction {
            player_root: vec![1],
            action: ActionType::Fold as i32,
            amount: 0,
        };
        let book = handle_player_action(cmd, &state, 1).expect("ok");
        let any = match book.pages[0].payload.as_ref() {
            Some(Payload::Event(a)) => a,
            _ => panic!(),
        };
        assert!(any.type_url.ends_with("examples.ActionTaken"));
        let decoded = ActionTaken::decode(any.value.as_slice()).unwrap();
        assert_eq!(decoded.action, ActionType::Fold as i32);
        assert_eq!(decoded.player_stack, 500);
    }

    #[test]
    fn handle_player_action_rejects_check_when_bet_pending() {
        let mut state = betting_state();
        state.current_bet = 10;
        let cmd = PlayerAction {
            player_root: vec![1],
            action: ActionType::Check as i32,
            amount: 0,
        };
        let err = handle_player_action(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("check"));
    }

    #[test]
    fn handle_player_action_rejects_when_hand_does_not_exist() {
        let state = new_hand_state();
        let cmd = PlayerAction {
            player_root: vec![1],
            action: ActionType::Fold as i32,
            amount: 0,
        };
        let err = handle_player_action(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("does not exist"));
    }
}
