//! Rebuy orchestration command handlers.

use angzarr_client::proto::EventBook;
use angzarr_client::{event_page, pack_event, CommandRejectedError, CommandResult};
use examples_proto::{
    ConfirmRebuyFee, InitiateRebuy, RebuyFeeConfirmed, RebuyFeeReleased, RebuyRequested,
    ReleaseRebuyFee,
};
use uuid::Uuid;

use crate::state::PlayerState;

// --- InitiateRebuy ---

fn guard_initiate(state: &PlayerState) -> CommandResult<()> {
    if !state.exists() {
        return Err(CommandRejectedError::new("Player does not exist"));
    }
    Ok(())
}

fn validate_initiate(cmd: &InitiateRebuy) -> CommandResult<()> {
    if cmd.tournament_root.is_empty() {
        return Err(CommandRejectedError::new("tournament_root is required"));
    }
    if cmd.table_root.is_empty() {
        return Err(CommandRejectedError::new("table_root is required"));
    }
    Ok(())
}

pub fn handle_initiate_rebuy(
    cmd: InitiateRebuy,
    state: &PlayerState,
    seq: u32,
) -> CommandResult<EventBook> {
    guard_initiate(state)?;
    validate_initiate(&cmd)?;

    let reservation_id = Uuid::new_v4().as_bytes().to_vec();

    let event = RebuyRequested {
        reservation_id,
        tournament_root: cmd.tournament_root,
        table_root: cmd.table_root,
        seat: cmd.seat,
        fee: None,
        requested_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.RebuyRequested");

    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}

// --- ConfirmRebuyFee ---

fn guard_confirm(state: &PlayerState) -> CommandResult<()> {
    if !state.exists() {
        return Err(CommandRejectedError::new("Player does not exist"));
    }
    Ok(())
}

fn validate_confirm(cmd: &ConfirmRebuyFee, state: &PlayerState) -> CommandResult<()> {
    if cmd.reservation_id.is_empty() {
        return Err(CommandRejectedError::new("reservation_id is required"));
    }

    let reservation_hex = hex::encode(&cmd.reservation_id);
    if !state.pending_rebuys.contains_key(&reservation_hex) {
        return Err(CommandRejectedError::new(
            "No pending rebuy with this reservation_id",
        ));
    }

    Ok(())
}

pub fn handle_confirm_rebuy_fee(
    cmd: ConfirmRebuyFee,
    state: &PlayerState,
    seq: u32,
) -> CommandResult<EventBook> {
    guard_confirm(state)?;
    validate_confirm(&cmd, state)?;

    let reservation_hex = hex::encode(&cmd.reservation_id);
    let pending = state.pending_rebuys.get(&reservation_hex).unwrap();

    let event = RebuyFeeConfirmed {
        reservation_id: cmd.reservation_id,
        tournament_root: pending.tournament_root.clone(),
        fee: Some(examples_proto::Currency {
            amount: pending.fee,
            currency_code: "CHIPS".to_string(),
        }),
        chips_added: pending.chips_to_add,
        confirmed_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.RebuyFeeConfirmed");

    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}

// --- ReleaseRebuyFee ---

fn guard_release(state: &PlayerState) -> CommandResult<()> {
    if !state.exists() {
        return Err(CommandRejectedError::new("Player does not exist"));
    }
    Ok(())
}

fn validate_release(cmd: &ReleaseRebuyFee, state: &PlayerState) -> CommandResult<()> {
    if cmd.reservation_id.is_empty() {
        return Err(CommandRejectedError::new("reservation_id is required"));
    }

    let reservation_hex = hex::encode(&cmd.reservation_id);
    if !state.pending_rebuys.contains_key(&reservation_hex) {
        return Err(CommandRejectedError::new(
            "No pending rebuy with this reservation_id",
        ));
    }

    Ok(())
}

pub fn handle_release_rebuy_fee(
    cmd: ReleaseRebuyFee,
    state: &PlayerState,
    seq: u32,
) -> CommandResult<EventBook> {
    guard_release(state)?;
    validate_release(&cmd, state)?;

    let event = RebuyFeeReleased {
        reservation_id: cmd.reservation_id,
        reason: cmd.reason,
        released_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.RebuyFeeReleased");

    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{apply_rebuy_requested, apply_registered};
    use angzarr_client::proto::event_page::Payload;
    use examples_proto::{Currency, PlayerRegistered, PlayerType};
    use prost::Message;

    fn currency(amount: i64) -> Currency {
        Currency {
            amount,
            currency_code: "CHIPS".into(),
        }
    }

    fn registered() -> PlayerState {
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
        s
    }

    fn with_pending_rebuy(rid: Vec<u8>) -> PlayerState {
        let mut s = registered();
        apply_rebuy_requested(
            &mut s,
            RebuyRequested {
                reservation_id: rid,
                tournament_root: vec![0xa1],
                table_root: vec![0xa2],
                seat: 3,
                fee: Some(currency(200)),
                requested_at: None,
            },
        );
        s
    }

    #[test]
    fn initiate_rebuy_success_emits_event_with_reservation_id() {
        let state = registered();
        let cmd = InitiateRebuy {
            tournament_root: vec![0x01],
            table_root: vec![0x02],
            seat: 5,
        };
        let book = handle_initiate_rebuy(cmd, &state, 2).expect("ok");
        assert_eq!(book.pages.len(), 1);
        let any = match book.pages[0].payload.as_ref() {
            Some(Payload::Event(a)) => a,
            _ => panic!(),
        };
        assert!(any.type_url.ends_with("examples.RebuyRequested"));
        let decoded = RebuyRequested::decode(any.value.as_slice()).unwrap();
        assert_eq!(decoded.tournament_root, vec![0x01]);
        assert_eq!(decoded.table_root, vec![0x02]);
        assert_eq!(decoded.seat, 5);
        assert!(!decoded.reservation_id.is_empty());
    }

    #[test]
    fn initiate_rebuy_rejects_empty_tournament_root() {
        let state = registered();
        let cmd = InitiateRebuy {
            tournament_root: vec![],
            table_root: vec![0x02],
            seat: 0,
        };
        let err = handle_initiate_rebuy(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("tournament_root"));
    }

    #[test]
    fn initiate_rebuy_rejects_empty_table_root() {
        let state = registered();
        let cmd = InitiateRebuy {
            tournament_root: vec![0x01],
            table_root: vec![],
            seat: 0,
        };
        let err = handle_initiate_rebuy(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("table_root"));
    }

    #[test]
    fn initiate_rebuy_rejects_when_no_player() {
        let state = PlayerState::default();
        let cmd = InitiateRebuy {
            tournament_root: vec![1],
            table_root: vec![1],
            seat: 0,
        };
        let err = handle_initiate_rebuy(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("does not exist"));
    }

    #[test]
    fn confirm_rebuy_fee_success_emits_event() {
        let rid = vec![7, 7];
        let state = with_pending_rebuy(rid.clone());
        let cmd = ConfirmRebuyFee {
            reservation_id: rid.clone(),
        };
        let book = handle_confirm_rebuy_fee(cmd, &state, 3).expect("ok");
        let any = match book.pages[0].payload.as_ref() {
            Some(Payload::Event(a)) => a,
            _ => panic!(),
        };
        assert!(any.type_url.ends_with("examples.RebuyFeeConfirmed"));
        let decoded = RebuyFeeConfirmed::decode(any.value.as_slice()).unwrap();
        assert_eq!(decoded.reservation_id, rid);
        assert_eq!(decoded.fee.unwrap().amount, 200);
    }

    #[test]
    fn confirm_rebuy_fee_rejects_unknown_reservation() {
        let state = with_pending_rebuy(vec![1]);
        let cmd = ConfirmRebuyFee {
            reservation_id: vec![0xef],
        };
        let err = handle_confirm_rebuy_fee(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("No pending rebuy"));
    }

    #[test]
    fn confirm_rebuy_fee_rejects_empty_reservation_id() {
        let state = with_pending_rebuy(vec![1]);
        let cmd = ConfirmRebuyFee {
            reservation_id: vec![],
        };
        let err = handle_confirm_rebuy_fee(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("reservation_id"));
    }

    #[test]
    fn release_rebuy_fee_success_emits_event() {
        let rid = vec![9];
        let state = with_pending_rebuy(rid.clone());
        let cmd = ReleaseRebuyFee {
            reservation_id: rid.clone(),
            reason: "timeout".into(),
        };
        let book = handle_release_rebuy_fee(cmd, &state, 4).expect("ok");
        let any = match book.pages[0].payload.as_ref() {
            Some(Payload::Event(a)) => a,
            _ => panic!(),
        };
        assert!(any.type_url.ends_with("examples.RebuyFeeReleased"));
        let decoded = RebuyFeeReleased::decode(any.value.as_slice()).unwrap();
        assert_eq!(decoded.reservation_id, rid);
        assert_eq!(decoded.reason, "timeout");
    }

    #[test]
    fn release_rebuy_fee_rejects_empty_reservation_id() {
        let state = with_pending_rebuy(vec![1]);
        let cmd = ReleaseRebuyFee {
            reservation_id: vec![],
            reason: "x".into(),
        };
        let err = handle_release_rebuy_fee(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("reservation_id"));
    }
}
