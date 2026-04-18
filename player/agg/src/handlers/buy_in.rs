//! Buy-in orchestration command handlers.

use angzarr_client::proto::EventBook;
use angzarr_client::{event_page, pack_event, CommandRejectedError, CommandResult};
use examples_proto::{
    BuyInConfirmed, BuyInRequested, BuyInReservationReleased, ConfirmBuyIn, InitiateBuyIn,
    ReleaseBuyIn,
};
use uuid::Uuid;

use crate::state::PlayerState;

// --- InitiateBuyIn ---

fn guard_initiate(state: &PlayerState) -> CommandResult<()> {
    if !state.exists() {
        return Err(CommandRejectedError::new("Player does not exist"));
    }
    Ok(())
}

fn validate_initiate(cmd: &InitiateBuyIn, state: &PlayerState) -> CommandResult<()> {
    if cmd.table_root.is_empty() {
        return Err(CommandRejectedError::new("table_root is required"));
    }

    let amount = cmd.amount.as_ref().map(|c| c.amount).unwrap_or(0);
    if amount <= 0 {
        return Err(CommandRejectedError::invalid_argument(
            "amount must be positive",
        ));
    }

    if amount > state.available_balance() {
        return Err(CommandRejectedError::new("Insufficient funds"));
    }

    Ok(())
}

pub fn handle_initiate_buy_in(
    cmd: InitiateBuyIn,
    state: &PlayerState,
    seq: u32,
) -> CommandResult<EventBook> {
    guard_initiate(state)?;
    validate_initiate(&cmd, state)?;

    let reservation_id = Uuid::new_v4().as_bytes().to_vec();

    let event = BuyInRequested {
        reservation_id,
        table_root: cmd.table_root,
        seat: cmd.seat,
        amount: cmd.amount,
        requested_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.BuyInRequested");

    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}

#[cfg(test)]
mod initiate_tests {
    use super::*;
    use crate::state::{apply_deposited, apply_registered};
    use angzarr_client::proto::event_page::Payload;
    use examples_proto::{Currency, FundsDeposited, PlayerRegistered, PlayerType};
    use prost::Message;

    fn currency(amount: i64) -> Currency {
        Currency {
            amount,
            currency_code: "CHIPS".into(),
        }
    }

    fn funded(bankroll: i64) -> PlayerState {
        let mut s = PlayerState::default();
        apply_registered(
            &mut s,
            PlayerRegistered {
                display_name: "A".into(),
                email: "a@x".into(),
                player_type: PlayerType::Human as i32,
                ai_model_id: String::new(),
                registered_at: None,
            },
        );
        apply_deposited(
            &mut s,
            FundsDeposited {
                amount: Some(currency(bankroll)),
                new_balance: Some(currency(bankroll)),
                deposited_at: None,
            },
        );
        s
    }

    #[test]
    fn initiate_buy_in_success_emits_buy_in_requested() {
        let state = funded(1000);
        let cmd = InitiateBuyIn {
            table_root: vec![1, 2, 3],
            seat: 2,
            amount: Some(currency(300)),
        };
        let book = handle_initiate_buy_in(cmd, &state, 1).expect("ok");
        assert_eq!(book.pages.len(), 1);
        let any = match book.pages[0].payload.as_ref() {
            Some(Payload::Event(a)) => a,
            _ => panic!(),
        };
        assert!(any.type_url.ends_with("examples.BuyInRequested"));
        let decoded = BuyInRequested::decode(any.value.as_slice()).unwrap();
        assert_eq!(decoded.seat, 2);
        assert_eq!(decoded.amount.unwrap().amount, 300);
        assert!(!decoded.reservation_id.is_empty());
    }

    #[test]
    fn initiate_buy_in_rejects_empty_table_root() {
        let state = funded(1000);
        let cmd = InitiateBuyIn {
            table_root: vec![],
            seat: 0,
            amount: Some(currency(100)),
        };
        let err = handle_initiate_buy_in(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("table_root"));
    }

    #[test]
    fn initiate_buy_in_rejects_non_positive_amount() {
        let state = funded(1000);
        let cmd = InitiateBuyIn {
            table_root: vec![1],
            seat: 0,
            amount: Some(currency(0)),
        };
        let err = handle_initiate_buy_in(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("positive"));
    }

    #[test]
    fn initiate_buy_in_rejects_insufficient_funds() {
        let state = funded(50);
        let cmd = InitiateBuyIn {
            table_root: vec![1],
            seat: 0,
            amount: Some(currency(500)),
        };
        let err = handle_initiate_buy_in(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("Insufficient"));
    }

    #[test]
    fn initiate_buy_in_rejects_when_no_player() {
        let state = PlayerState::default();
        let cmd = InitiateBuyIn {
            table_root: vec![1],
            seat: 0,
            amount: Some(currency(100)),
        };
        let err = handle_initiate_buy_in(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("does not exist"));
    }
}

// --- ConfirmBuyIn ---

fn guard_confirm(state: &PlayerState) -> CommandResult<()> {
    if !state.exists() {
        return Err(CommandRejectedError::new("Player does not exist"));
    }
    Ok(())
}

fn validate_confirm(cmd: &ConfirmBuyIn, state: &PlayerState) -> CommandResult<()> {
    if cmd.reservation_id.is_empty() {
        return Err(CommandRejectedError::new("reservation_id is required"));
    }

    // Check that this reservation exists in pending buy-ins
    let reservation_hex = hex::encode(&cmd.reservation_id);
    if !state.pending_buy_ins.contains_key(&reservation_hex) {
        return Err(CommandRejectedError::new(
            "No pending buy-in with this reservation_id",
        ));
    }

    Ok(())
}

pub fn handle_confirm_buy_in(
    cmd: ConfirmBuyIn,
    state: &PlayerState,
    seq: u32,
) -> CommandResult<EventBook> {
    guard_confirm(state)?;
    validate_confirm(&cmd, state)?;

    let reservation_hex = hex::encode(&cmd.reservation_id);
    let pending = state.pending_buy_ins.get(&reservation_hex).unwrap();

    let event = BuyInConfirmed {
        reservation_id: cmd.reservation_id,
        table_root: pending.table_root.clone(),
        seat: pending.seat,
        amount: Some(examples_proto::Currency {
            amount: pending.amount,
            currency_code: "CHIPS".to_string(),
        }),
        confirmed_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.BuyInConfirmed");

    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}

// --- ReleaseBuyIn ---

fn guard_release(state: &PlayerState) -> CommandResult<()> {
    if !state.exists() {
        return Err(CommandRejectedError::new("Player does not exist"));
    }
    Ok(())
}

fn validate_release(cmd: &ReleaseBuyIn, state: &PlayerState) -> CommandResult<()> {
    if cmd.reservation_id.is_empty() {
        return Err(CommandRejectedError::new("reservation_id is required"));
    }

    let reservation_hex = hex::encode(&cmd.reservation_id);
    if !state.pending_buy_ins.contains_key(&reservation_hex) {
        return Err(CommandRejectedError::new(
            "No pending buy-in with this reservation_id",
        ));
    }

    Ok(())
}

pub fn handle_release_buy_in(
    cmd: ReleaseBuyIn,
    state: &PlayerState,
    seq: u32,
) -> CommandResult<EventBook> {
    guard_release(state)?;
    validate_release(&cmd, state)?;

    let event = BuyInReservationReleased {
        reservation_id: cmd.reservation_id,
        reason: cmd.reason,
        released_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.BuyInReservationReleased");

    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}

#[cfg(test)]
mod confirm_release_tests {
    use super::*;
    use crate::state::{apply_buy_in_requested, apply_deposited, apply_registered};
    use angzarr_client::proto::event_page::Payload;
    use examples_proto::{Currency, FundsDeposited, PlayerRegistered, PlayerType};
    use prost::Message;

    fn currency(amount: i64) -> Currency {
        Currency {
            amount,
            currency_code: "CHIPS".into(),
        }
    }

    fn with_pending_buy_in(reservation_id: Vec<u8>) -> PlayerState {
        let mut s = PlayerState::default();
        apply_registered(
            &mut s,
            PlayerRegistered {
                display_name: "A".into(),
                email: "a@x".into(),
                player_type: PlayerType::Human as i32,
                ai_model_id: String::new(),
                registered_at: None,
            },
        );
        apply_deposited(
            &mut s,
            FundsDeposited {
                amount: Some(currency(1000)),
                new_balance: Some(currency(1000)),
                deposited_at: None,
            },
        );
        apply_buy_in_requested(
            &mut s,
            BuyInRequested {
                reservation_id,
                table_root: vec![0xaa],
                seat: 1,
                amount: Some(currency(200)),
                requested_at: None,
            },
        );
        s
    }

    #[test]
    fn confirm_buy_in_success_emits_event() {
        let rid = vec![1, 2, 3];
        let state = with_pending_buy_in(rid.clone());
        let cmd = ConfirmBuyIn {
            reservation_id: rid.clone(),
        };
        let book = handle_confirm_buy_in(cmd, &state, 4).expect("ok");
        assert_eq!(book.pages.len(), 1);
        let any = match book.pages[0].payload.as_ref() {
            Some(Payload::Event(a)) => a,
            _ => panic!(),
        };
        assert!(any.type_url.ends_with("examples.BuyInConfirmed"));
        let decoded = BuyInConfirmed::decode(any.value.as_slice()).unwrap();
        assert_eq!(decoded.reservation_id, rid);
        assert_eq!(decoded.amount.unwrap().amount, 200);
        assert_eq!(decoded.seat, 1);
    }

    #[test]
    fn confirm_buy_in_rejects_empty_reservation_id() {
        let state = with_pending_buy_in(vec![1]);
        let cmd = ConfirmBuyIn {
            reservation_id: vec![],
        };
        let err = handle_confirm_buy_in(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("reservation_id"));
    }

    #[test]
    fn confirm_buy_in_rejects_unknown_reservation() {
        let state = with_pending_buy_in(vec![1]);
        let cmd = ConfirmBuyIn {
            reservation_id: vec![9, 9, 9],
        };
        let err = handle_confirm_buy_in(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("No pending buy-in"));
    }

    #[test]
    fn confirm_buy_in_rejects_when_no_player() {
        let state = PlayerState::default();
        let cmd = ConfirmBuyIn {
            reservation_id: vec![1],
        };
        let err = handle_confirm_buy_in(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("does not exist"));
    }

    #[test]
    fn release_buy_in_success_emits_event() {
        let rid = vec![5, 5];
        let state = with_pending_buy_in(rid.clone());
        let cmd = ReleaseBuyIn {
            reservation_id: rid.clone(),
            reason: "user cancel".into(),
        };
        let book = handle_release_buy_in(cmd, &state, 7).expect("ok");
        assert_eq!(book.pages.len(), 1);
        let any = match book.pages[0].payload.as_ref() {
            Some(Payload::Event(a)) => a,
            _ => panic!(),
        };
        assert!(any.type_url.ends_with("examples.BuyInReservationReleased"));
        let decoded = BuyInReservationReleased::decode(any.value.as_slice()).unwrap();
        assert_eq!(decoded.reservation_id, rid);
        assert_eq!(decoded.reason, "user cancel");
    }

    #[test]
    fn release_buy_in_rejects_empty_reservation_id() {
        let state = with_pending_buy_in(vec![1]);
        let cmd = ReleaseBuyIn {
            reservation_id: vec![],
            reason: "x".into(),
        };
        let err = handle_release_buy_in(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("reservation_id"));
    }

    #[test]
    fn release_buy_in_rejects_unknown_reservation() {
        let state = with_pending_buy_in(vec![1]);
        let cmd = ReleaseBuyIn {
            reservation_id: vec![0xff, 0xff],
            reason: "x".into(),
        };
        let err = handle_release_buy_in(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("No pending buy-in"));
    }
}
