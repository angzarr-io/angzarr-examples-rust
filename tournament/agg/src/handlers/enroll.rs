//! EnrollPlayer command handler.

use angzarr_client::proto::EventBook;
use angzarr_client::{event_page, pack_event, CommandRejectedError, CommandResult};
use examples_proto::{EnrollPlayer, TournamentEnrollmentRejected, TournamentPlayerEnrolled};

use crate::state::TournamentState;

fn guard(state: &TournamentState) -> CommandResult<()> {
    if !state.exists() {
        return Err(CommandRejectedError::new("Tournament does not exist"));
    }
    Ok(())
}

fn validate(cmd: &EnrollPlayer, state: &TournamentState) -> Result<(), String> {
    if cmd.player_root.is_empty() {
        return Err("player_root is required".to_string());
    }

    if !state.is_registration_open() {
        return Err("Registration is not open".to_string());
    }

    if !state.has_capacity() {
        return Err("Tournament is full".to_string());
    }

    let player_root_hex = hex::encode(&cmd.player_root);
    if state.is_player_registered(&player_root_hex) {
        return Err("Player is already registered".to_string());
    }

    Ok(())
}

pub fn handle_enroll_player(
    cmd: EnrollPlayer,
    state: &TournamentState,
    seq: u32,
) -> CommandResult<EventBook> {
        guard(state)?;

    // Validate and produce appropriate event
    match validate(&cmd, state) {
        Ok(()) => {
            let event = TournamentPlayerEnrolled {
                player_root: cmd.player_root,
                reservation_id: cmd.reservation_id,
                fee_paid: state.buy_in,
                starting_stack: state.starting_stack,
                registration_number: (state.registered_players.len() + 1) as i32,
                enrolled_at: Some(angzarr_client::now()),
            };
            let event_any = pack_event(&event, "examples.TournamentPlayerEnrolled");
            Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
        }
        Err(reason) => {
            let event = TournamentEnrollmentRejected {
                player_root: cmd.player_root,
                reservation_id: cmd.reservation_id,
                reason,
                rejected_at: Some(angzarr_client::now()),
            };
            let event_any = pack_event(&event, "examples.TournamentEnrollmentRejected");
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
    use crate::state::{apply_created, apply_player_enrolled, apply_registration_opened};
    use angzarr_client::proto::event_page::Payload;
    use examples_proto::{GameVariant, RegistrationOpened, TournamentCreated};
    use prost::Message;

    fn open_tournament() -> TournamentState {
        let mut s = TournamentState::default();
        apply_created(
            &mut s,
            TournamentCreated {
                name: "Spring Classic".into(),
                game_variant: GameVariant::TexasHoldem as i32,
                buy_in: 500,
                starting_stack: 10_000,
                max_players: 2,
                min_players: 2,
                scheduled_start: None,
                rebuy_config: None,
                addon_config: None,
                blind_structure: vec![],
                created_at: None,
            },
        );
        apply_registration_opened(&mut s, RegistrationOpened { opened_at: None });
        s
    }

    #[test]
    fn handle_enroll_player_success_emits_enrolled() {
        let state = open_tournament();
        let cmd = EnrollPlayer {
            player_root: vec![1, 2],
            reservation_id: vec![0x1a],
        };
        let book = handle_enroll_player(cmd, &state, 1).expect("ok");
        let any = match book.pages[0].payload.as_ref() {
            Some(Payload::Event(a)) => a,
            _ => panic!(),
        };
        assert!(any.type_url.ends_with("examples.TournamentPlayerEnrolled"));
        let decoded = TournamentPlayerEnrolled::decode(any.value.as_slice()).unwrap();
        assert_eq!(decoded.fee_paid, 500);
        assert_eq!(decoded.starting_stack, 10_000);
    }

    #[test]
    fn handle_enroll_player_rejects_when_no_tournament() {
        let state = TournamentState::default();
        let cmd = EnrollPlayer {
            player_root: vec![1],
            reservation_id: vec![],
        };
        let err = handle_enroll_player(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("does not exist"));
    }

    #[test]
    fn handle_enroll_player_emits_rejection_when_registration_closed() {
        // Tournament exists but registration has not been opened yet
        let mut s = TournamentState::default();
        apply_created(
            &mut s,
            TournamentCreated {
                name: "X".into(),
                game_variant: GameVariant::TexasHoldem as i32,
                buy_in: 100,
                starting_stack: 1000,
                max_players: 2,
                min_players: 2,
                scheduled_start: None,
                rebuy_config: None,
                addon_config: None,
                blind_structure: vec![],
                created_at: None,
            },
        );
        let cmd = EnrollPlayer {
            player_root: vec![1],
            reservation_id: vec![0xaa],
        };
        let book = handle_enroll_player(cmd, &s, 1).expect("ok");
        let any = match book.pages[0].payload.as_ref() {
            Some(Payload::Event(a)) => a,
            _ => panic!(),
        };
        assert!(any.type_url.ends_with("examples.TournamentEnrollmentRejected"));
        let decoded = TournamentEnrollmentRejected::decode(any.value.as_slice()).unwrap();
        assert!(decoded.reason.contains("Registration"));
    }

    #[test]
    fn handle_enroll_player_emits_rejection_when_already_registered() {
        let mut s = open_tournament();
        apply_player_enrolled(
            &mut s,
            TournamentPlayerEnrolled {
                player_root: vec![1, 2],
                reservation_id: vec![],
                fee_paid: 500,
                starting_stack: 10_000,
                registration_number: 1,
                enrolled_at: None,
            },
        );
        let cmd = EnrollPlayer {
            player_root: vec![1, 2],
            reservation_id: vec![0xaa],
        };
        let book = handle_enroll_player(cmd, &s, 1).expect("ok");
        let any = match book.pages[0].payload.as_ref() {
            Some(Payload::Event(a)) => a,
            _ => panic!(),
        };
        let decoded = TournamentEnrollmentRejected::decode(any.value.as_slice()).unwrap();
        assert!(decoded.reason.contains("already registered"));
    }
}
