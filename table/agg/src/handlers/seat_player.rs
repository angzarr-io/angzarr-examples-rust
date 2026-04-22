//! SeatPlayer command handler for PM-orchestrated buy-in flow.

use angzarr_client::proto::EventBook;
use angzarr_client::{event_page, pack_event, CommandRejectedError, CommandResult};
use examples_proto::{PlayerSeated, SeatPlayer, SeatingRejected};

use crate::state::TableState;

fn guard(state: &TableState) -> CommandResult<()> {
    if !state.exists() {
        return Err(CommandRejectedError::new("Table does not exist"));
    }
    Ok(())
}

fn validate(cmd: &SeatPlayer, state: &TableState) -> Result<i32, String> {
    if cmd.player_root.is_empty() {
        return Err("player_root is required".to_string());
    }

    if state.find_seat_by_player(&cmd.player_root).is_some() {
        return Err("Player already seated".to_string());
    }

    if cmd.amount < state.min_buy_in {
        return Err(format!("Buy-in must be at least {}", state.min_buy_in));
    }
    if cmd.amount > state.max_buy_in {
        return Err("Buy-in above maximum".to_string());
    }

    let seat_position = if cmd.seat >= 0 && cmd.seat < state.max_players {
        if state.seats.contains_key(&cmd.seat) {
            return Err("Seat is occupied".to_string());
        }
        cmd.seat
    } else if cmd.seat == -1 {
        // Any available seat
        state
            .next_available_seat()
            .ok_or_else(|| "Table is full".to_string())?
    } else {
        return Err("Invalid seat position".to_string());
    };

    Ok(seat_position)
}

pub fn handle_seat_player(
    cmd: SeatPlayer,
    state: &TableState,
    seq: u32,
) -> CommandResult<EventBook> {
    guard(state)?;

    // Unlike JoinTable, SeatPlayer produces success or rejection event (not error)
    match validate(&cmd, state) {
        Ok(seat_position) => {
            let event = PlayerSeated {
                player_root: cmd.player_root,
                reservation_id: cmd.reservation_id,
                seat_position,
                stack: cmd.amount,
                seated_at: Some(angzarr_client::now()),
            };
            let event_any = pack_event(&event, "examples.PlayerSeated");
            Ok(EventBook {
                pages: vec![event_page(seq, event_any)],
                ..Default::default()
            })
        }
        Err(reason) => {
            let event = SeatingRejected {
                player_root: cmd.player_root,
                reservation_id: cmd.reservation_id,
                requested_seat: cmd.seat,
                reason,
                rejected_at: Some(angzarr_client::now()),
            };
            let event_any = pack_event(&event, "examples.SeatingRejected");
            Ok(EventBook {
                pages: vec![event_page(seq, event_any)],
                ..Default::default()
            })
        }
    }
}
