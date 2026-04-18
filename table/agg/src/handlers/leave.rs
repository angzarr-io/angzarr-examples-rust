//! LeaveTable command handler.

use angzarr_client::proto::EventBook;
use angzarr_client::{event_page, pack_event, CommandRejectedError, CommandResult};
use examples_proto::{LeaveTable, PlayerLeft};

use crate::state::{SeatState, TableState};

fn guard(state: &TableState) -> CommandResult<()> {
    if !state.exists() {
        return Err(CommandRejectedError::new("Table does not exist"));
    }
    if state.status == "in_hand" {
        return Err(CommandRejectedError::new("Cannot leave during a hand"));
    }
    Ok(())
}

fn validate<'a>(cmd: &LeaveTable, state: &'a TableState) -> CommandResult<(i32, &'a SeatState)> {
    if cmd.player_root.is_empty() {
        return Err(CommandRejectedError::new("player_root is required"));
    }

    let seat_position = state
        .find_seat_position_by_player(&cmd.player_root)
        .ok_or_else(|| CommandRejectedError::new("Player not seated at table"))?;

    let seat = state.seats.get(&seat_position).unwrap();

    Ok((seat_position, seat))
}

fn compute(cmd: &LeaveTable, seat_position: i32, seat: &SeatState) -> PlayerLeft {
    PlayerLeft {
        player_root: cmd.player_root.clone(),
        seat_position,
        chips_cashed_out: seat.stack,
        left_at: Some(angzarr_client::now()),
    }
}

pub fn handle_leave_table(
    cmd: LeaveTable,
    state: &TableState,
    seq: u32,
) -> CommandResult<EventBook> {
    guard(state)?;
    let (seat_position, seat) = validate(&cmd, state)?;

    let event = compute(&cmd, seat_position, seat);
    let event_any = pack_event(&event, "examples.PlayerLeft");

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
    use examples_proto::{GameVariant, PlayerJoined, TableCreated};
    use prost::Message;

    fn seated_state() -> TableState {
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
        apply_player_joined(
            &mut s,
            PlayerJoined {
                player_root: vec![1, 2, 3],
                seat_position: 4,
                buy_in_amount: 400,
                stack: 400,
                joined_at: None,
            },
        );
        s
    }

    #[test]
    fn handle_leave_table_success_emits_player_left() {
        let state = seated_state();
        let cmd = LeaveTable {
            player_root: vec![1, 2, 3],
        };
        let book = handle_leave_table(cmd, &state, 2).expect("ok");
        let any = match book.pages[0].payload.as_ref() {
            Some(Payload::Event(a)) => a,
            _ => panic!(),
        };
        assert!(any.type_url.ends_with("examples.PlayerLeft"));
        let decoded = PlayerLeft::decode(any.value.as_slice()).unwrap();
        assert_eq!(decoded.seat_position, 4);
        assert_eq!(decoded.chips_cashed_out, 400);
    }

    #[test]
    fn handle_leave_table_rejects_when_no_table() {
        let state = TableState::default();
        let cmd = LeaveTable {
            player_root: vec![1],
        };
        let err = handle_leave_table(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("does not exist"));
    }

    #[test]
    fn handle_leave_table_rejects_during_hand() {
        let mut state = seated_state();
        state.status = "in_hand".into();
        let cmd = LeaveTable {
            player_root: vec![1, 2, 3],
        };
        let err = handle_leave_table(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("during a hand"));
    }

    #[test]
    fn handle_leave_table_rejects_empty_player_root() {
        let state = seated_state();
        let cmd = LeaveTable {
            player_root: vec![],
        };
        let err = handle_leave_table(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("player_root"));
    }

    #[test]
    fn handle_leave_table_rejects_player_not_seated() {
        let state = seated_state();
        let cmd = LeaveTable {
            player_root: vec![0xff, 0xff],
        };
        let err = handle_leave_table(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("not seated"));
    }
}
