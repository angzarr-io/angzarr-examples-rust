//! StartActionClock handler — TDA Rule 29.
//!
//! A floor/opponent/idle-detect call to put a player on the action
//! clock. Precondition: the named player must be the seat currently
//! to act (position == `state.action_on_position`). Emits
//! `ActionClockStarted` and pins the action position so subsequent
//! commands have an explicit "whose turn it is" record.

use angzarr_client::proto::EventBook;
use angzarr_client::CommandResult;
use examples_proto::{ActionClockStarted, StartActionClock};
use examples_utils::{event_page, pack_event, reject};

use crate::errors::{
    ClockSecondsMustBePositive, HandAlreadyComplete, HandNotDealt, NotActorsTurn, PlayerNotInHand,
    PlayerRootRequired,
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

fn validate(cmd: &StartActionClock, state: &HandState) -> CommandResult<()> {
    if cmd.player_root.is_empty() {
        return Err(reject(PlayerRootRequired));
    }
    if cmd.seconds == 0 {
        return Err(reject(ClockSecondsMustBePositive {
            value: cmd.seconds as i64,
        }));
    }
    let player = state
        .get_player(&cmd.player_root)
        .ok_or_else(|| reject(PlayerNotInHand))?;
    // Clock can only run on the seat currently to act (TDA Rule 29).
    if player.position != state.action_on_position {
        return Err(reject(NotActorsTurn {
            player_root_hex: hex::encode(&cmd.player_root),
        }));
    }
    Ok(())
}

pub fn handle_start_action_clock(
    cmd: StartActionClock,
    state: &HandState,
    seq: u32,
) -> CommandResult<EventBook> {
    guard(state)?;
    validate(&cmd, state)?;

    let event = ActionClockStarted {
        player_root: cmd.player_root,
        seconds: cmd.seconds,
        started_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.ActionClockStarted");
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

    fn two_player_state() -> HandState {
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
    fn rejects_when_hand_not_dealt() {
        let s = new_hand_state();
        let err = handle_start_action_clock(
            StartActionClock {
                player_root: vec![1],
                seconds: 30,
            },
            &s,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "HAND_NOT_DEALT");
    }

    #[test]
    fn rejects_when_player_root_missing() {
        let s = two_player_state();
        let err = handle_start_action_clock(
            StartActionClock {
                player_root: vec![],
                seconds: 30,
            },
            &s,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "PLAYER_ROOT_REQUIRED");
    }

    #[test]
    fn rejects_zero_seconds() {
        let s = two_player_state();
        let err = handle_start_action_clock(
            StartActionClock {
                player_root: vec![1],
                seconds: 0,
            },
            &s,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "CLOCK_SECONDS_MUST_BE_POSITIVE");
    }

    #[test]
    fn rejects_when_player_not_in_hand() {
        let s = two_player_state();
        let err = handle_start_action_clock(
            StartActionClock {
                player_root: vec![0xff],
                seconds: 30,
            },
            &s,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "PLAYER_NOT_IN_HAND");
    }

    #[test]
    fn rejects_when_player_not_seat_to_act() {
        let mut s = two_player_state();
        s.action_on_position = 1; // not player_root vec![1] (position 0)
        let err = handle_start_action_clock(
            StartActionClock {
                player_root: vec![1],
                seconds: 30,
            },
            &s,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "NOT_ACTORS_TURN");
    }

    #[test]
    fn emits_clock_started_when_preconditions_met() {
        let mut s = two_player_state();
        s.action_on_position = 0;
        let book = handle_start_action_clock(
            StartActionClock {
                player_root: vec![1],
                seconds: 30,
            },
            &s,
            1,
        )
        .expect("ok");
        assert_eq!(book.pages.len(), 1);
    }
}
