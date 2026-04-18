//! Tournament registration orchestration command handlers.

use angzarr_client::proto::EventBook;
use angzarr_client::{event_page, pack_event, CommandRejectedError, CommandResult};
use examples_proto::{
    ConfirmRegistrationFee, InitiateTournamentRegistration, RegistrationFeeConfirmed,
    RegistrationFeeReleased, RegistrationRequested, ReleaseRegistrationFee,
};
use uuid::Uuid;

use crate::state::PlayerState;

// --- InitiateTournamentRegistration ---

fn guard_initiate(state: &PlayerState) -> CommandResult<()> {
    if !state.exists() {
        return Err(CommandRejectedError::new("Player does not exist"));
    }
    Ok(())
}

fn validate_initiate(cmd: &InitiateTournamentRegistration) -> CommandResult<()> {
    if cmd.tournament_root.is_empty() {
        return Err(CommandRejectedError::new("tournament_root is required"));
    }
    Ok(())
}

pub fn handle_initiate_tournament_registration(
    cmd: InitiateTournamentRegistration,
    state: &PlayerState,
    seq: u32,
) -> CommandResult<EventBook> {
    guard_initiate(state)?;
    validate_initiate(&cmd)?;

    let reservation_id = Uuid::new_v4().as_bytes().to_vec();

    let event = RegistrationRequested {
        reservation_id,
        tournament_root: cmd.tournament_root,
        fee: None, // PM fills this in from Tournament state
        requested_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.RegistrationRequested");

    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}

// --- ConfirmRegistrationFee ---

fn guard_confirm(state: &PlayerState) -> CommandResult<()> {
    if !state.exists() {
        return Err(CommandRejectedError::new("Player does not exist"));
    }
    Ok(())
}

fn validate_confirm(cmd: &ConfirmRegistrationFee, state: &PlayerState) -> CommandResult<()> {
    if cmd.reservation_id.is_empty() {
        return Err(CommandRejectedError::new("reservation_id is required"));
    }

    let reservation_hex = hex::encode(&cmd.reservation_id);
    if !state.pending_registrations.contains_key(&reservation_hex) {
        return Err(CommandRejectedError::new(
            "No pending registration with this reservation_id",
        ));
    }

    Ok(())
}

pub fn handle_confirm_registration_fee(
    cmd: ConfirmRegistrationFee,
    state: &PlayerState,
    seq: u32,
) -> CommandResult<EventBook> {
    guard_confirm(state)?;
    validate_confirm(&cmd, state)?;

    let reservation_hex = hex::encode(&cmd.reservation_id);
    let pending = state.pending_registrations.get(&reservation_hex).unwrap();

    let event = RegistrationFeeConfirmed {
        reservation_id: cmd.reservation_id,
        tournament_root: pending.tournament_root.clone(),
        fee: Some(examples_proto::Currency {
            amount: pending.fee,
            currency_code: "CHIPS".to_string(),
        }),
        confirmed_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.RegistrationFeeConfirmed");

    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}

// --- ReleaseRegistrationFee ---

fn guard_release(state: &PlayerState) -> CommandResult<()> {
    if !state.exists() {
        return Err(CommandRejectedError::new("Player does not exist"));
    }
    Ok(())
}

fn validate_release(cmd: &ReleaseRegistrationFee, state: &PlayerState) -> CommandResult<()> {
    if cmd.reservation_id.is_empty() {
        return Err(CommandRejectedError::new("reservation_id is required"));
    }

    let reservation_hex = hex::encode(&cmd.reservation_id);
    if !state.pending_registrations.contains_key(&reservation_hex) {
        return Err(CommandRejectedError::new(
            "No pending registration with this reservation_id",
        ));
    }

    Ok(())
}

pub fn handle_release_registration_fee(
    cmd: ReleaseRegistrationFee,
    state: &PlayerState,
    seq: u32,
) -> CommandResult<EventBook> {
    guard_release(state)?;
    validate_release(&cmd, state)?;

    let event = RegistrationFeeReleased {
        reservation_id: cmd.reservation_id,
        reason: cmd.reason,
        released_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.RegistrationFeeReleased");

    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{apply_registered, apply_registration_requested};
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

    fn with_pending_registration(rid: Vec<u8>) -> PlayerState {
        let mut s = registered();
        apply_registration_requested(
            &mut s,
            RegistrationRequested {
                reservation_id: rid,
                tournament_root: vec![0xc1],
                fee: Some(currency(150)),
                requested_at: None,
            },
        );
        s
    }

    #[test]
    fn initiate_registration_success_emits_event_with_reservation_id() {
        let state = registered();
        let cmd = InitiateTournamentRegistration {
            tournament_root: vec![0x99],
        };
        let book = handle_initiate_tournament_registration(cmd, &state, 1).expect("ok");
        let any = match book.pages[0].payload.as_ref() {
            Some(Payload::Event(a)) => a,
            _ => panic!(),
        };
        assert!(any.type_url.ends_with("examples.RegistrationRequested"));
        let decoded = RegistrationRequested::decode(any.value.as_slice()).unwrap();
        assert_eq!(decoded.tournament_root, vec![0x99]);
        assert!(!decoded.reservation_id.is_empty());
    }

    #[test]
    fn initiate_registration_rejects_empty_tournament_root() {
        let state = registered();
        let cmd = InitiateTournamentRegistration {
            tournament_root: vec![],
        };
        let err = handle_initiate_tournament_registration(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("tournament_root"));
    }

    #[test]
    fn initiate_registration_rejects_when_no_player() {
        let state = PlayerState::default();
        let cmd = InitiateTournamentRegistration {
            tournament_root: vec![1],
        };
        let err = handle_initiate_tournament_registration(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("does not exist"));
    }

    #[test]
    fn confirm_registration_fee_success_emits_event() {
        let rid = vec![2, 2];
        let state = with_pending_registration(rid.clone());
        let cmd = ConfirmRegistrationFee {
            reservation_id: rid.clone(),
        };
        let book = handle_confirm_registration_fee(cmd, &state, 3).expect("ok");
        let any = match book.pages[0].payload.as_ref() {
            Some(Payload::Event(a)) => a,
            _ => panic!(),
        };
        assert!(any.type_url.ends_with("examples.RegistrationFeeConfirmed"));
        let decoded = RegistrationFeeConfirmed::decode(any.value.as_slice()).unwrap();
        assert_eq!(decoded.reservation_id, rid);
        assert_eq!(decoded.fee.unwrap().amount, 150);
    }

    #[test]
    fn confirm_registration_fee_rejects_unknown_reservation() {
        let state = with_pending_registration(vec![1]);
        let cmd = ConfirmRegistrationFee {
            reservation_id: vec![0xee],
        };
        let err = handle_confirm_registration_fee(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("No pending registration"));
    }

    #[test]
    fn confirm_registration_fee_rejects_empty_reservation_id() {
        let state = with_pending_registration(vec![1]);
        let cmd = ConfirmRegistrationFee {
            reservation_id: vec![],
        };
        let err = handle_confirm_registration_fee(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("reservation_id"));
    }

    #[test]
    fn release_registration_fee_success_emits_event() {
        let rid = vec![8];
        let state = with_pending_registration(rid.clone());
        let cmd = ReleaseRegistrationFee {
            reservation_id: rid.clone(),
            reason: "cancelled".into(),
        };
        let book = handle_release_registration_fee(cmd, &state, 5).expect("ok");
        let any = match book.pages[0].payload.as_ref() {
            Some(Payload::Event(a)) => a,
            _ => panic!(),
        };
        assert!(any.type_url.ends_with("examples.RegistrationFeeReleased"));
        let decoded = RegistrationFeeReleased::decode(any.value.as_slice()).unwrap();
        assert_eq!(decoded.reservation_id, rid);
        assert_eq!(decoded.reason, "cancelled");
    }

    #[test]
    fn release_registration_fee_rejects_empty_reservation_id() {
        let state = with_pending_registration(vec![1]);
        let cmd = ReleaseRegistrationFee {
            reservation_id: vec![],
            reason: "x".into(),
        };
        let err = handle_release_registration_fee(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("reservation_id"));
    }
}
