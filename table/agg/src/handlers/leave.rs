//! LeaveTable command handler.

use angzarr_client::proto::EventBook;
use angzarr_client::CommandResult;
use examples_proto::{LeaveTable, PlayerLeft};
use examples_utils::{event_page, pack_event, rejected};

use crate::state::{SeatState, TableState};

fn guard(state: &TableState) -> CommandResult<()> {
    if !state.exists() {
        return Err(rejected("Table does not exist"));
    }
    if state.status == "in_hand" {
        return Err(rejected("Cannot leave during a hand"));
    }
    Ok(())
}

fn validate<'a>(cmd: &LeaveTable, state: &'a TableState) -> CommandResult<(i32, &'a SeatState)> {
    if cmd.player_root.is_empty() {
        return Err(rejected("player_root is required"));
    }

    let seat_position = state
        .find_seat_position_by_player(&cmd.player_root)
        .ok_or_else(|| rejected("Player not seated at table"))?;

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
