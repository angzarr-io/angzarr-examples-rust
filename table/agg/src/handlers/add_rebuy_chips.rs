//! AddRebuyChips command handler for PM-orchestrated rebuy flow.

use angzarr_client::proto::EventBook;
use angzarr_client::{event_page, pack_event, CommandRejectedError, CommandResult};
use examples_proto::{AddRebuyChips, RebuyChipsAdded};

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
        return Err(CommandRejectedError::invalid_argument(
            "amount must be positive",
        ));
    }

    // Find the player's seat
    let seat_opt = state.find_seat_by_player(&cmd.player_root);
    if seat_opt.is_none() {
        return Err(CommandRejectedError::new(
            "Player is not seated at this table",
        ));
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
                table_name: "T".into(),
                game_variant: GameVariant::TexasHoldem as i32,
                small_blind: 5,
                big_blind: 10,
                min_buy_in: 100,
                max_buy_in: 1000,
                max_players: 4,
                action_timeout_seconds: 30,
                created_at: None,
            },
        );
        apply_player_joined(
            &mut s,
            PlayerJoined {
                player_root: vec![0xa1],
                seat_position: 2,
                buy_in_amount: 200,
                stack: 200,
                joined_at: None,
            },
        );
        s
    }

    #[test]
    fn handle_add_rebuy_chips_success_emits_rebuy_chips_added() {
        let state = seated_state();
        let cmd = AddRebuyChips {
            player_root: vec![0xa1],
            reservation_id: vec![0x11],
            seat: 2,
            amount: 300,
        };
        let book = handle_add_rebuy_chips(cmd, &state, 1).expect("ok");
        let any = match book.pages[0].payload.as_ref() {
            Some(Payload::Event(a)) => a,
            _ => panic!(),
        };
        assert!(any.type_url.ends_with("examples.RebuyChipsAdded"));
        let decoded = RebuyChipsAdded::decode(any.value.as_slice()).unwrap();
        assert_eq!(decoded.amount, 300);
        assert_eq!(decoded.new_stack, 500);
        assert_eq!(decoded.seat, 2);
    }

    #[test]
    fn handle_add_rebuy_chips_rejects_when_no_table() {
        let state = TableState::default();
        let cmd = AddRebuyChips {
            player_root: vec![1],
            reservation_id: vec![],
            seat: 0,
            amount: 100,
        };
        let err = handle_add_rebuy_chips(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("does not exist"));
    }

    #[test]
    fn handle_add_rebuy_chips_rejects_empty_player_root() {
        let state = seated_state();
        let cmd = AddRebuyChips {
            player_root: vec![],
            reservation_id: vec![],
            seat: 0,
            amount: 100,
        };
        let err = handle_add_rebuy_chips(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("player_root"));
    }

    #[test]
    fn handle_add_rebuy_chips_rejects_non_positive_amount() {
        let state = seated_state();
        let cmd = AddRebuyChips {
            player_root: vec![0xa1],
            reservation_id: vec![],
            seat: 2,
            amount: 0,
        };
        let err = handle_add_rebuy_chips(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("positive"));
    }

    #[test]
    fn handle_add_rebuy_chips_rejects_unseated_player() {
        let state = seated_state();
        let cmd = AddRebuyChips {
            player_root: vec![0xff],
            reservation_id: vec![],
            seat: 0,
            amount: 100,
        };
        let err = handle_add_rebuy_chips(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("not seated"));
    }

    #[test]
    fn handle_add_rebuy_chips_rejects_seat_mismatch() {
        let state = seated_state();
        let cmd = AddRebuyChips {
            player_root: vec![0xa1],
            reservation_id: vec![],
            seat: 3, // player actually sits at 2
            amount: 100,
        };
        let err = handle_add_rebuy_chips(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("Seat position mismatch"));
    }
}
