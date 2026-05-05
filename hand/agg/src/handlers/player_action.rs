//! PlayerAction command handler.

use angzarr_client::proto::EventBook;
use angzarr_client::CommandResult;
use examples_proto::{ActionTaken, ActionType, PlayerAction};
use examples_utils::{event_page, pack_event, reject};

use crate::errors::{
    BetBelowMinRaise, BetExceedsStack, CannotBetOverExistingBet, CannotCheckWithBet,
    CannotRaiseWithoutBet, HandAlreadyComplete, HandNotDealt, InvalidAction, NotInBettingPhase,
    NothingToCall, PlayerHasFolded, PlayerIsAllIn, PlayerNotInHand, PlayerRootRequired,
    RaiseBelowMin, RaiseExceedsStack,
};
use crate::raise_tracking::min_raise_to;
use crate::state::{HandState, PlayerHandState};

/// Validated action result containing computed values.
struct ValidatedAction {
    chips_put_in: i64,
    event_amount: i64,
    final_action: ActionType,
}

fn guard(state: &HandState) -> CommandResult<()> {
    if !state.exists() {
        return Err(reject(HandNotDealt));
    }
    if state.is_complete() {
        return Err(reject(HandAlreadyComplete));
    }
    if state.status != "betting" {
        return Err(reject(NotInBettingPhase));
    }
    Ok(())
}

fn validate<'a>(
    cmd: &PlayerAction,
    state: &'a HandState,
) -> CommandResult<(&'a PlayerHandState, ValidatedAction)> {
    if cmd.player_root.is_empty() {
        return Err(reject(PlayerRootRequired));
    }

    let player = state
        .get_player(&cmd.player_root)
        .ok_or_else(|| reject(PlayerNotInHand))?;

    if player.has_folded {
        return Err(reject(PlayerHasFolded));
    }
    if player.is_all_in {
        return Err(reject(PlayerIsAllIn));
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
                return Err(reject(CannotCheckWithBet));
            }
            chips_put_in = 0;
            event_amount = 0;
        }
        ActionType::Call => {
            if amount_to_call <= 0 {
                return Err(reject(NothingToCall));
            }
            chips_put_in = amount_to_call.min(player.stack);
            event_amount = chips_put_in;
        }
        ActionType::Bet => {
            if state.current_bet > 0 {
                return Err(reject(CannotBetOverExistingBet));
            }
            if cmd.amount < state.min_raise {
                return Err(reject(BetBelowMinRaise {
                    got: cmd.amount,
                    bound: state.min_raise,
                }));
            }
            if cmd.amount > player.stack {
                return Err(reject(BetExceedsStack {
                    got: cmd.amount,
                    bound: player.stack,
                }));
            }
            chips_put_in = cmd.amount;
            event_amount = chips_put_in;
        }
        ActionType::Raise => {
            if state.current_bet <= 0 {
                return Err(reject(CannotRaiseWithoutBet));
            }
            if cmd.amount > player.bet_this_round + player.stack {
                return Err(reject(RaiseExceedsStack {
                    got: cmd.amount,
                    bound: player.stack,
                }));
            }
            let to_put_in = cmd.amount - player.bet_this_round;
            // Conventional poker rule: a raise below the minimum is allowed
            // when the player is going all-in (to_put_in == stack). Python
            // mirrors this with `raise_amount < min_raise and to_put_in < stack`.
            let going_all_in = to_put_in >= player.stack;
            let min_target = min_raise_to(state.current_bet, state.min_raise);
            if cmd.amount < min_target && !going_all_in {
                return Err(reject(RaiseBelowMin {
                    got: cmd.amount,
                    bound: state.min_raise,
                }));
            }
            chips_put_in = to_put_in.min(player.stack);
            event_amount = chips_put_in;
        }
        ActionType::AllIn => {
            chips_put_in = player.stack;
            event_amount = chips_put_in;
        }
        _ => {
            return Err(reject(InvalidAction { got: cmd.action }));
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
