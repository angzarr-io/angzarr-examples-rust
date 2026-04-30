//! AddRebuyChips command handler for PM-orchestrated rebuy flow.

use angzarr_client::proto::EventBook;
use angzarr_client::CommandResult;
use examples_utils::{event_page, invalid_arg, pack_event, rejected};
use examples_proto::{AddRebuyChips, RebuyChipsAdded};

use crate::state::TableState;

fn guard(state: &TableState) -> CommandResult<()> {
    if !state.exists() {
        return Err(rejected("Table does not exist"));
    }
    Ok(())
}

fn validate(cmd: &AddRebuyChips, state: &TableState) -> CommandResult<i64> {
    if cmd.player_root.is_empty() {
        return Err(rejected("player_root is required"));
    }

    if cmd.amount <= 0 {
        return Err(invalid_arg("amount must be positive"));
    }

    // Find the player's seat
    let seat_opt = state.find_seat_by_player(&cmd.player_root);
    if seat_opt.is_none() {
        return Err(rejected(
            "Player is not seated at this table",
        ));
    }

    let seat = seat_opt.unwrap();
    if seat.position != cmd.seat {
        return Err(rejected("Seat position mismatch"));
    }

    // Calculate new stack
    let new_stack = seat.stack + cmd.amount;

    Ok(new_stack)
}

pub fn handle_add_rebuy_chips(
    cmd: AddRebuyChips,
    state: &TableState,
    seq: u32,
) -> CommandResult<EventBook> {
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

    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}
