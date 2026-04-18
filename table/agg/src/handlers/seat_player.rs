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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{apply_player_seated, apply_table_created};
    use angzarr_client::proto::event_page::Payload;
    use examples_proto::{GameVariant, TableCreated};
    use prost::Message;

    fn created_state() -> TableState {
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
                max_players: 3,
                action_timeout_seconds: 30,
                created_at: None,
            },
        );
        s
    }

    fn first_event(book: &EventBook) -> &prost_types::Any {
        match book.pages[0].payload.as_ref() {
            Some(Payload::Event(a)) => a,
            _ => panic!("no event payload"),
        }
    }

    #[test]
    fn handle_seat_player_success_emits_player_seated() {
        let state = created_state();
        let cmd = SeatPlayer {
            player_root: vec![1, 2, 3],
            reservation_id: vec![0xaa],
            seat: 1,
            amount: 500,
        };
        let book = handle_seat_player(cmd, &state, 5).expect("ok");
        let any = first_event(&book);
        assert!(any.type_url.ends_with("examples.PlayerSeated"));
        let decoded = PlayerSeated::decode(any.value.as_slice()).unwrap();
        assert_eq!(decoded.seat_position, 1);
        assert_eq!(decoded.stack, 500);
    }

    #[test]
    fn handle_seat_player_rejects_when_no_table_with_command_error() {
        let state = TableState::default();
        let cmd = SeatPlayer {
            player_root: vec![1],
            reservation_id: vec![0xaa],
            seat: 0,
            amount: 500,
        };
        let err = handle_seat_player(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("does not exist"));
    }

    #[test]
    fn handle_seat_player_emits_seating_rejected_on_empty_player_root() {
        let state = created_state();
        let cmd = SeatPlayer {
            player_root: vec![],
            reservation_id: vec![0xaa],
            seat: 0,
            amount: 500,
        };
        let book = handle_seat_player(cmd, &state, 1).expect("ok");
        let any = first_event(&book);
        assert!(any.type_url.ends_with("examples.SeatingRejected"));
        let decoded = SeatingRejected::decode(any.value.as_slice()).unwrap();
        assert!(decoded.reason.contains("player_root"));
    }

    #[test]
    fn handle_seat_player_emits_seating_rejected_when_already_seated() {
        let mut state = created_state();
        apply_player_seated(
            &mut state,
            PlayerSeated {
                player_root: vec![7],
                reservation_id: vec![],
                seat_position: 1,
                stack: 500,
                seated_at: None,
            },
        );
        let cmd = SeatPlayer {
            player_root: vec![7],
            reservation_id: vec![0xaa],
            seat: 2,
            amount: 500,
        };
        let book = handle_seat_player(cmd, &state, 1).expect("ok");
        let any = first_event(&book);
        assert!(any.type_url.ends_with("examples.SeatingRejected"));
        let decoded = SeatingRejected::decode(any.value.as_slice()).unwrap();
        assert!(decoded.reason.contains("already seated"));
    }

    #[test]
    fn handle_seat_player_emits_seating_rejected_on_invalid_seat() {
        let state = created_state();
        let cmd = SeatPlayer {
            player_root: vec![1],
            reservation_id: vec![0xaa],
            seat: -5,
            amount: 500,
        };
        let book = handle_seat_player(cmd, &state, 1).expect("ok");
        let any = first_event(&book);
        let decoded = SeatingRejected::decode(any.value.as_slice()).unwrap();
        assert!(decoded.reason.contains("Invalid seat"));
    }

    #[test]
    fn handle_seat_player_emits_seating_rejected_when_no_available_seats() {
        let mut state = created_state();
        // Fill all 3 seats
        for (i, r) in (0..3).zip([vec![1u8], vec![2], vec![3]]) {
            apply_player_seated(
                &mut state,
                PlayerSeated {
                    player_root: r,
                    reservation_id: vec![],
                    seat_position: i,
                    stack: 500,
                    seated_at: None,
                },
            );
        }
        let cmd = SeatPlayer {
            player_root: vec![9],
            reservation_id: vec![0xaa],
            seat: -1,
            amount: 500,
        };
        let book = handle_seat_player(cmd, &state, 1).expect("ok");
        let any = first_event(&book);
        let decoded = SeatingRejected::decode(any.value.as_slice()).unwrap();
        assert!(decoded.reason.contains("Table is full"));
    }
}
