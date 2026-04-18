//! Registration command handlers.

use angzarr_client::proto::EventBook;
use angzarr_client::{event_page, pack_event, CommandRejectedError, CommandResult};
use examples_proto::{CloseRegistration, OpenRegistration, RegistrationClosed, RegistrationOpened};

use crate::state::TournamentState;

// --- OpenRegistration ---

fn guard_open(state: &TournamentState) -> CommandResult<()> {
    if !state.exists() {
        return Err(CommandRejectedError::new("Tournament does not exist"));
    }
    if state.is_registration_open() {
        return Err(CommandRejectedError::new("Registration is already open"));
    }
    if state.is_running() {
        return Err(CommandRejectedError::new(
            "Cannot open registration for a running tournament",
        ));
    }
    Ok(())
}

pub fn handle_open_registration(
    _cmd: OpenRegistration,
    state: &TournamentState,
    seq: u32,
) -> CommandResult<EventBook> {
        guard_open(state)?;

    let event = RegistrationOpened {
        opened_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.RegistrationOpened");

    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}

// --- CloseRegistration ---

fn guard_close(state: &TournamentState) -> CommandResult<()> {
    if !state.exists() {
        return Err(CommandRejectedError::new("Tournament does not exist"));
    }
    if !state.is_registration_open() {
        return Err(CommandRejectedError::new("Registration is not open"));
    }
    Ok(())
}

pub fn handle_close_registration(
    _cmd: CloseRegistration,
    state: &TournamentState,
    seq: u32,
) -> CommandResult<EventBook> {
        guard_close(state)?;

    let event = RegistrationClosed {
        total_registrations: state.registered_players.len() as i32,
        closed_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.RegistrationClosed");

    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{apply_created, apply_player_enrolled, apply_registration_opened};
    use angzarr_client::proto::event_page::Payload;
    use examples_proto::{
        GameVariant, TournamentCreated, TournamentPlayerEnrolled, TournamentStatus,
    };
    use prost::Message;

    fn created() -> TournamentState {
        let mut s = TournamentState::default();
        apply_created(
            &mut s,
            TournamentCreated {
                name: "X".into(),
                game_variant: GameVariant::TexasHoldem as i32,
                buy_in: 500,
                starting_stack: 10_000,
                max_players: 4,
                min_players: 2,
                scheduled_start: None,
                rebuy_config: None,
                addon_config: None,
                blind_structure: vec![],
                created_at: None,
            },
        );
        s
    }

    #[test]
    fn handle_open_registration_success_emits_event() {
        let state = created();
        let book = handle_open_registration(OpenRegistration {}, &state, 1).expect("ok");
        let any = match book.pages[0].payload.as_ref() {
            Some(Payload::Event(a)) => a,
            _ => panic!(),
        };
        assert!(any.type_url.ends_with("examples.RegistrationOpened"));
    }

    #[test]
    fn handle_open_registration_rejects_when_no_tournament() {
        let state = TournamentState::default();
        let err = handle_open_registration(OpenRegistration {}, &state, 1).unwrap_err();
        assert!(err.reason.contains("does not exist"));
    }

    #[test]
    fn handle_open_registration_rejects_when_already_open() {
        let mut state = created();
        apply_registration_opened(
            &mut state,
            examples_proto::RegistrationOpened { opened_at: None },
        );
        let err = handle_open_registration(OpenRegistration {}, &state, 1).unwrap_err();
        assert!(err.reason.contains("already open"));
    }

    #[test]
    fn handle_open_registration_rejects_when_tournament_running() {
        let mut state = created();
        state.status = TournamentStatus::TournamentRunning;
        let err = handle_open_registration(OpenRegistration {}, &state, 1).unwrap_err();
        assert!(err.reason.contains("running tournament"));
    }

    #[test]
    fn handle_close_registration_success_emits_event_with_count() {
        let mut state = created();
        apply_registration_opened(
            &mut state,
            examples_proto::RegistrationOpened { opened_at: None },
        );
        apply_player_enrolled(
            &mut state,
            TournamentPlayerEnrolled {
                player_root: vec![0x01],
                reservation_id: vec![],
                fee_paid: 500,
                starting_stack: 10_000,
                registration_number: 1,
                enrolled_at: None,
            },
        );
        let book = handle_close_registration(CloseRegistration {}, &state, 1).expect("ok");
        let any = match book.pages[0].payload.as_ref() {
            Some(Payload::Event(a)) => a,
            _ => panic!(),
        };
        assert!(any.type_url.ends_with("examples.RegistrationClosed"));
        let decoded = RegistrationClosed::decode(any.value.as_slice()).unwrap();
        assert_eq!(decoded.total_registrations, 1);
    }

    #[test]
    fn handle_close_registration_rejects_when_not_open() {
        let state = created();
        let err = handle_close_registration(CloseRegistration {}, &state, 1).unwrap_err();
        assert!(err.reason.contains("not open"));
    }
}
