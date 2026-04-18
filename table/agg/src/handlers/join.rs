//! JoinTable command handler.

use angzarr_client::proto::EventBook;
use angzarr_client::{event_page, pack_event, CommandRejectedError, CommandResult};
use examples_proto::{JoinTable, PlayerJoined};

use crate::state::TableState;

fn guard(state: &TableState) -> CommandResult<()> {
    if !state.exists() {
        return Err(CommandRejectedError::new("Table does not exist"));
    }
    Ok(())
}

fn validate(cmd: &JoinTable, state: &TableState) -> CommandResult<i32> {
    if cmd.player_root.is_empty() {
        return Err(CommandRejectedError::new("player_root is required"));
    }

    if state.find_seat_by_player(&cmd.player_root).is_some() {
        return Err(CommandRejectedError::new("Player already seated"));
    }

    if cmd.buy_in_amount < state.min_buy_in {
        return Err(CommandRejectedError::invalid_argument(format!(
            "Buy-in must be at least {}",
            state.min_buy_in
        )));
    }
    if cmd.buy_in_amount > state.max_buy_in {
        return Err(CommandRejectedError::invalid_argument(
            "Buy-in above maximum",
        ));
    }

    let seat_position = if cmd.preferred_seat >= 0 && cmd.preferred_seat < state.max_players {
        if state.seats.contains_key(&cmd.preferred_seat) {
            return Err(CommandRejectedError::new("Seat is occupied"));
        }
        cmd.preferred_seat
    } else {
        state
            .next_available_seat()
            .ok_or_else(|| CommandRejectedError::new("Table is full"))?
    };

    Ok(seat_position)
}

fn compute(cmd: &JoinTable, seat_position: i32) -> PlayerJoined {
    PlayerJoined {
        player_root: cmd.player_root.clone(),
        seat_position,
        buy_in_amount: cmd.buy_in_amount,
        stack: cmd.buy_in_amount,
        joined_at: Some(angzarr_client::now()),
    }
}

pub fn handle_join_table(
    cmd: JoinTable,
    state: &TableState,
    seq: u32,
) -> CommandResult<EventBook> {
    guard(state)?;
    let seat_position = validate(&cmd, state)?;

    let event = compute(&cmd, seat_position);
    let event_any = pack_event(&event, "examples.PlayerJoined");

    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{apply_player_joined, apply_table_created};
    use angzarr_client::proto::event_page::Payload;
    use examples_proto::{GameVariant, TableCreated};
    use prost::Message;

    fn created_state() -> TableState {
        let mut s = TableState::default();
        apply_table_created(
            &mut s,
            TableCreated {
                table_name: "T1".into(),
                game_variant: GameVariant::TexasHoldem as i32,
                small_blind: 5,
                big_blind: 10,
                min_buy_in: 100,
                max_buy_in: 1000,
                max_players: 6,
                action_timeout_seconds: 30,
                created_at: None,
            },
        );
        s
    }

    #[test]
    fn handle_join_table_success_at_preferred_seat() {
        let state = created_state();
        let cmd = JoinTable {
            player_root: vec![1, 2, 3],
            preferred_seat: 3,
            buy_in_amount: 500,
        };
        let book = handle_join_table(cmd, &state, 1).expect("ok");
        let any = match book.pages[0].payload.as_ref() {
            Some(Payload::Event(a)) => a,
            _ => panic!(),
        };
        assert!(any.type_url.ends_with("examples.PlayerJoined"));
        let decoded = PlayerJoined::decode(any.value.as_slice()).unwrap();
        assert_eq!(decoded.seat_position, 3);
        assert_eq!(decoded.stack, 500);
        assert_eq!(decoded.buy_in_amount, 500);
    }

    #[test]
    fn handle_join_table_assigns_next_available_seat_with_negative_preferred() {
        let state = created_state();
        let cmd = JoinTable {
            player_root: vec![4, 5],
            preferred_seat: -1,
            buy_in_amount: 200,
        };
        let book = handle_join_table(cmd, &state, 1).expect("ok");
        let any = match book.pages[0].payload.as_ref() {
            Some(Payload::Event(a)) => a,
            _ => panic!(),
        };
        let decoded = PlayerJoined::decode(any.value.as_slice()).unwrap();
        assert_eq!(decoded.seat_position, 0);
    }

    #[test]
    fn handle_join_table_rejects_when_no_table() {
        let state = TableState::default();
        let cmd = JoinTable {
            player_root: vec![1],
            preferred_seat: 0,
            buy_in_amount: 100,
        };
        let err = handle_join_table(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("does not exist"));
    }

    #[test]
    fn handle_join_table_rejects_empty_player_root() {
        let state = created_state();
        let cmd = JoinTable {
            player_root: vec![],
            preferred_seat: 0,
            buy_in_amount: 500,
        };
        let err = handle_join_table(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("player_root"));
    }

    #[test]
    fn handle_join_table_rejects_already_seated_player() {
        let mut state = created_state();
        apply_player_joined(
            &mut state,
            PlayerJoined {
                player_root: vec![7, 7],
                seat_position: 2,
                buy_in_amount: 500,
                stack: 500,
                joined_at: None,
            },
        );
        let cmd = JoinTable {
            player_root: vec![7, 7],
            preferred_seat: 3,
            buy_in_amount: 500,
        };
        let err = handle_join_table(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("already seated"));
    }

    #[test]
    fn handle_join_table_rejects_buy_in_below_min() {
        let state = created_state();
        let cmd = JoinTable {
            player_root: vec![1],
            preferred_seat: 0,
            buy_in_amount: 50, // min is 100
        };
        let err = handle_join_table(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("at least"));
    }

    #[test]
    fn handle_join_table_rejects_buy_in_above_max() {
        let state = created_state();
        let cmd = JoinTable {
            player_root: vec![1],
            preferred_seat: 0,
            buy_in_amount: 5000, // max is 1000
        };
        let err = handle_join_table(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("above maximum"));
    }

    #[test]
    fn handle_join_table_rejects_occupied_preferred_seat() {
        let mut state = created_state();
        apply_player_joined(
            &mut state,
            PlayerJoined {
                player_root: vec![1],
                seat_position: 2,
                buy_in_amount: 500,
                stack: 500,
                joined_at: None,
            },
        );
        let cmd = JoinTable {
            player_root: vec![2],
            preferred_seat: 2,
            buy_in_amount: 500,
        };
        let err = handle_join_table(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("occupied"));
    }
}
