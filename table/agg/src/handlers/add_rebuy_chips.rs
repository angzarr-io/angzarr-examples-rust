//! AddRebuyChips command handler for PM-orchestrated rebuy flow.

use angzarr_client::proto::EventBook;
use angzarr_client::CommandResult;
use examples_proto::{AddRebuyChips, RebuyChipsAdded};
use examples_utils::{event_page, pack_event, reject};

use crate::errors::{
    AmountMustBePositive, PlayerNotSeated, PlayerRootRequired, SeatPositionMismatch, TableNotFound,
};
use crate::state::TableState;

fn guard(state: &TableState) -> CommandResult<()> {
    if !state.exists() {
        return Err(reject(TableNotFound));
    }
    Ok(())
}

fn validate(cmd: &AddRebuyChips, state: &TableState) -> CommandResult<i64> {
    if cmd.player_root.is_empty() {
        return Err(reject(PlayerRootRequired));
    }

    if cmd.amount <= 0 {
        return Err(reject(AmountMustBePositive { value: cmd.amount }));
    }

    // Find the player's seat
    let seat_opt = state.find_seat_by_player(&cmd.player_root);
    if seat_opt.is_none() {
        return Err(reject(PlayerNotSeated));
    }

    let seat = seat_opt.unwrap();
    if seat.position != cmd.seat {
        return Err(reject(SeatPositionMismatch {
            expected: seat.position as i64,
            got: cmd.seat as i64,
        }));
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
