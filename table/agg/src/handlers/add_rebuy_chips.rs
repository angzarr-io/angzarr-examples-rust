//! AddRebuyChips command handler for PM-orchestrated rebuy flow.

use angzarr_client::proto::{CommandBook, EventBook};
use angzarr_client::{new_event_book, pack_event, CommandRejectedError, CommandResult, UnpackAny};
use examples_proto::{AddRebuyChips, RebuyChipsAdded};
use prost_types::Any;

use crate::state::TableState;

fn guard(state: &TableState) -> CommandResult<()> {
    if !state.exists() {
        return Err(CommandRejectedError::new("Table does not exist"));
    }
    Ok(())
}

fn validate(cmd: &AddRebuyChips, state: &TableState) -> CommandResult<i64> {
    if cmd.player_root.is_empty() {
        return Err(CommandRejectedError::new("player_root is required"));
    }

    if cmd.amount <= 0 {
        return Err(CommandRejectedError::new("amount must be positive"));
    }

    // Find the player's seat
    let seat_opt = state.find_seat_by_player(&cmd.player_root);
    if seat_opt.is_none() {
        return Err(CommandRejectedError::new("Player is not seated at this table"));
    }

    let seat = seat_opt.unwrap();
    if seat.position != cmd.seat {
        return Err(CommandRejectedError::new("Seat position mismatch"));
    }

    // Calculate new stack
    let new_stack = seat.stack + cmd.amount;

    Ok(new_stack)
}

pub fn handle_add_rebuy_chips(
    command_book: &CommandBook,
    command_any: &Any,
    state: &TableState,
    seq: u32,
) -> CommandResult<EventBook> {
    let cmd: AddRebuyChips = command_any
        .unpack()
        .map_err(|e| CommandRejectedError::new(format!("Failed to decode command: {}", e)))?;

    guard(state)?;
    let new_stack = validate(&cmd, state)?;

    let event = RebuyChipsAdded {
        player_root: cmd.player_root,
        reservation_id: cmd.reservation_id,
        seat: cmd.seat,
        amount: cmd.amount,
        new_stack,
        added_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.RebuyChipsAdded");

    Ok(new_event_book(command_book, seq, event_any))
}
