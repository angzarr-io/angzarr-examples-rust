//! JoinTable command handler.

use angzarr_client::proto::EventBook;
use angzarr_client::CommandResult;
use examples_proto::{JoinTable, PlayerJoined};
use examples_utils::{event_page, pack_event, reject};

use crate::errors::{
    BuyInAboveMax, BuyInBelowMin, PlayerAlreadySeated, PlayerRootRequired, SeatOccupied,
    TableIsFull, TableNotFound,
};
use crate::state::TableState;

fn guard(state: &TableState) -> CommandResult<()> {
    if !state.exists() {
        return Err(reject(TableNotFound));
    }
    Ok(())
}

fn validate(cmd: &JoinTable, state: &TableState) -> CommandResult<i32> {
    if cmd.player_root.is_empty() {
        return Err(reject(PlayerRootRequired));
    }

    if state.find_seat_by_player(&cmd.player_root).is_some() {
        return Err(reject(PlayerAlreadySeated));
    }

    if cmd.buy_in_amount < state.min_buy_in {
        return Err(reject(BuyInBelowMin {
            got: cmd.buy_in_amount,
            bound: state.min_buy_in,
        }));
    }
    if cmd.buy_in_amount > state.max_buy_in {
        return Err(reject(BuyInAboveMax {
            got: cmd.buy_in_amount,
            bound: state.max_buy_in,
        }));
    }

    let seat_position = if cmd.preferred_seat >= 0 && cmd.preferred_seat < state.max_players {
        if state.seats.contains_key(&cmd.preferred_seat) {
            return Err(reject(SeatOccupied {
                seat: cmd.preferred_seat,
            }));
        }
        cmd.preferred_seat
    } else {
        state
            .next_available_seat()
            .ok_or_else(|| reject(TableIsFull))?
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

pub fn handle_join_table(cmd: JoinTable, state: &TableState, seq: u32) -> CommandResult<EventBook> {
    guard(state)?;
    let seat_position = validate(&cmd, state)?;

    let event = compute(&cmd, seat_position);
    let event_any = pack_event(&event, "examples.PlayerJoined");

    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}
