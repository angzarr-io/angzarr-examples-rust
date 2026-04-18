//! Registration PM event handlers.

use angzarr_client::proto::command_page::Payload as CommandPayload;
use angzarr_client::proto::{
    event_page::Payload as EventPayload, page_header::SequenceType, CommandBook, CommandPage,
    Cover, EventBook, EventPage, MergeStrategy, PageHeader, ProcessManagerHandleResponse,
    Uuid as ProtoUuid,
};
use angzarr_client::{pack_event, CommandResult};
use examples_proto::{
    ConfirmRegistrationFee, Currency, EnrollPlayer, OrchestrationFailure, RegistrationCompleted,
    RegistrationFailed, RegistrationInitiated, RegistrationPhase, RegistrationRequested,
    ReleaseRegistrationFee, TournamentEnrollmentRejected, TournamentPlayerEnrolled,
};
use prost::Message;
use prost_types::Any;

pub fn handle_registration_requested(
    event: RegistrationRequested,
) -> CommandResult<ProcessManagerHandleResponse> {
    let fee = event.fee.as_ref().map(|c| c.amount).unwrap_or(0);
    let reservation_id = event.reservation_id.clone();
    let tournament_root = event.tournament_root.clone();
    let player_root: Vec<u8> = Vec::new();

    let enroll = EnrollPlayer {
        player_root: player_root.clone(),
        reservation_id: reservation_id.clone(),
    };
    let command = make_command_book(
        "tournament",
        &tournament_root,
        "examples.EnrollPlayer",
        &enroll,
    );

    let pm_event = RegistrationInitiated {
        player_root,
        tournament_root,
        reservation_id,
        fee: Some(Currency {
            amount: fee,
            currency_code: "USD".to_string(),
        }),
        phase: RegistrationPhase::RegistrationEnrolling as i32,
        initiated_at: Some(angzarr_client::now()),
    };
    let pm_event_book =
        make_pm_event_book(pack_event(&pm_event, "examples.RegistrationInitiated"));

    Ok(ProcessManagerHandleResponse {
        commands: vec![command],
        process_events: Some(pm_event_book),
        facts: vec![],
    })
}

pub fn handle_player_enrolled(
    event: TournamentPlayerEnrolled,
) -> CommandResult<ProcessManagerHandleResponse> {
    let confirm = ConfirmRegistrationFee {
        reservation_id: event.reservation_id.clone(),
    };
    let command = make_command_book(
        "player",
        &event.player_root,
        "examples.ConfirmRegistrationFee",
        &confirm,
    );

    let pm_event = RegistrationCompleted {
        player_root: event.player_root.clone(),
        tournament_root: vec![],
        reservation_id: event.reservation_id,
        fee: Some(Currency {
            amount: event.fee_paid,
            currency_code: "USD".to_string(),
        }),
        starting_stack: event.starting_stack,
        completed_at: Some(angzarr_client::now()),
    };
    let pm_event_book =
        make_pm_event_book(pack_event(&pm_event, "examples.RegistrationCompleted"));

    Ok(ProcessManagerHandleResponse {
        commands: vec![command],
        process_events: Some(pm_event_book),
        facts: vec![],
    })
}

pub fn handle_enrollment_rejected(
    event: TournamentEnrollmentRejected,
) -> CommandResult<ProcessManagerHandleResponse> {
    let release = ReleaseRegistrationFee {
        reservation_id: event.reservation_id.clone(),
        reason: event.reason.clone(),
    };
    let command = make_command_book(
        "player",
        &event.player_root,
        "examples.ReleaseRegistrationFee",
        &release,
    );

    let pm_event = RegistrationFailed {
        player_root: event.player_root.clone(),
        tournament_root: vec![],
        reservation_id: event.reservation_id,
        failure: Some(OrchestrationFailure {
            code: "ENROLLMENT_REJECTED".to_string(),
            message: event.reason,
            failed_at_phase: "ENROLLING".to_string(),
            failed_at: Some(angzarr_client::now()),
        }),
    };
    let pm_event_book = make_pm_event_book(pack_event(&pm_event, "examples.RegistrationFailed"));

    Ok(ProcessManagerHandleResponse {
        commands: vec![command],
        process_events: Some(pm_event_book),
        facts: vec![],
    })
}

fn make_command_book<M: Message>(
    domain: &str,
    root: &[u8],
    type_url: &str,
    message: &M,
) -> CommandBook {
    CommandBook {
        cover: Some(Cover {
            domain: domain.to_string(),
            root: Some(ProtoUuid {
                value: root.to_vec(),
            }),
            correlation_id: uuid::Uuid::new_v4().to_string(),
            edition: None,
        }),
        pages: vec![CommandPage {
            header: Some(PageHeader {
                sequence_type: Some(SequenceType::Sequence(0)),
            }),
            merge_strategy: MergeStrategy::MergeCommutative as i32,
            payload: Some(CommandPayload::Command(Any {
                type_url: angzarr_client::type_url(type_url),
                value: message.encode_to_vec(),
            })),
        }],
    }
}

fn make_pm_event_book(event: Any) -> EventBook {
    EventBook {
        cover: None,
        pages: vec![EventPage {
            header: Some(PageHeader {
                sequence_type: Some(SequenceType::Sequence(0)),
            }),
            created_at: Some(angzarr_client::now()),
            no_commit: false,
            cascade_id: None,
            payload: Some(EventPayload::Event(event)),
        }],
        snapshot: None,
        next_sequence: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reservation_bytes() -> Vec<u8> {
        vec![0xAA, 0xBB, 0xCC, 0xDD]
    }

    fn player_bytes() -> Vec<u8> {
        vec![0x11, 0x22, 0x33, 0x44]
    }

    fn tournament_bytes() -> Vec<u8> {
        vec![0x55, 0x66, 0x77, 0x88]
    }

    /// Extract the inner command `Any` from the only page of the only command book
    /// in the response.
    fn extract_command_any(response: &ProcessManagerHandleResponse) -> &Any {
        let book = response.commands.first().expect("expected one command book");
        let page = book.pages.first().expect("expected one command page");
        match page.payload.as_ref().expect("command page has payload") {
            CommandPayload::Command(any) => any,
            other => panic!("unexpected command page payload variant: {:?}", other),
        }
    }

    /// Extract the inner event `Any` from the only page of the process_events book.
    fn extract_pm_event_any(response: &ProcessManagerHandleResponse) -> &Any {
        let book = response
            .process_events
            .as_ref()
            .expect("expected process_events to be Some");
        assert_eq!(
            book.pages.len(),
            1,
            "expected exactly one PM event page, got {}",
            book.pages.len()
        );
        let page = &book.pages[0];
        match page.payload.as_ref().expect("event page has payload") {
            EventPayload::Event(any) => any,
            other => panic!("unexpected event page payload variant: {:?}", other),
        }
    }

    // -------------------------------------------------------------------------
    // handle_registration_requested
    // -------------------------------------------------------------------------

    #[test]
    fn handle_registration_requested_emits_enroll_player_command_and_initiated_event() {
        let event = RegistrationRequested {
            reservation_id: reservation_bytes(),
            tournament_root: tournament_bytes(),
            fee: Some(Currency {
                amount: 2500,
                currency_code: "USD".to_string(),
            }),
            requested_at: None,
        };

        let response = handle_registration_requested(event).expect("handler returned Ok");

        // facts empty
        assert!(
            response.facts.is_empty(),
            "facts should be empty, got {:?}",
            response.facts
        );

        // exactly one command
        assert_eq!(
            response.commands.len(),
            1,
            "expected exactly one command book"
        );

        let book = &response.commands[0];
        let cover = book.cover.as_ref().expect("cover set");
        assert_eq!(cover.domain, "tournament");
        assert_eq!(
            cover.root.as_ref().expect("root set").value,
            tournament_bytes(),
        );
        assert_eq!(book.pages.len(), 1);

        let command_any = extract_command_any(&response);
        assert!(
            command_any.type_url.ends_with("examples.EnrollPlayer"),
            "unexpected command type_url: {}",
            command_any.type_url
        );

        let decoded = EnrollPlayer::decode(command_any.value.as_slice())
            .expect("decode EnrollPlayer payload");
        assert_eq!(decoded.reservation_id, reservation_bytes());
        assert!(
            decoded.player_root.is_empty(),
            "player_root should be empty before enrollment, got {:?}",
            decoded.player_root
        );

        // PM event assertions
        let event_any = extract_pm_event_any(&response);
        assert!(
            event_any
                .type_url
                .ends_with("examples.RegistrationInitiated"),
            "unexpected PM event type_url: {}",
            event_any.type_url
        );
        let pm_event = RegistrationInitiated::decode(event_any.value.as_slice())
            .expect("decode RegistrationInitiated");
        assert_eq!(pm_event.reservation_id, reservation_bytes());
        assert_eq!(pm_event.tournament_root, tournament_bytes());
        assert!(pm_event.player_root.is_empty());
        assert_eq!(
            pm_event.fee.as_ref().expect("fee set").amount,
            2500,
            "fee should match input"
        );
        assert_eq!(pm_event.fee.as_ref().unwrap().currency_code, "USD");
        assert_eq!(
            pm_event.phase,
            RegistrationPhase::RegistrationEnrolling as i32,
        );
        assert!(
            pm_event.initiated_at.is_some(),
            "initiated_at should be stamped"
        );
    }

    #[test]
    fn handle_registration_requested_defaults_missing_fee_to_zero() {
        let event = RegistrationRequested {
            reservation_id: reservation_bytes(),
            tournament_root: tournament_bytes(),
            fee: None,
            requested_at: None,
        };

        let response = handle_registration_requested(event).expect("handler returned Ok");
        let event_any = extract_pm_event_any(&response);
        let pm_event = RegistrationInitiated::decode(event_any.value.as_slice())
            .expect("decode RegistrationInitiated");
        assert_eq!(
            pm_event.fee.as_ref().expect("fee always populated").amount,
            0,
        );
    }

    // -------------------------------------------------------------------------
    // handle_player_enrolled
    // -------------------------------------------------------------------------

    #[test]
    fn handle_player_enrolled_emits_confirm_fee_command_and_completed_event() {
        let event = TournamentPlayerEnrolled {
            player_root: player_bytes(),
            reservation_id: reservation_bytes(),
            fee_paid: 3000,
            starting_stack: 15_000,
            registration_number: 7,
            enrolled_at: None,
        };

        let response = handle_player_enrolled(event).expect("handler returned Ok");

        assert!(response.facts.is_empty());
        assert_eq!(response.commands.len(), 1);

        let book = &response.commands[0];
        let cover = book.cover.as_ref().expect("cover set");
        assert_eq!(cover.domain, "player");
        assert_eq!(
            cover.root.as_ref().expect("root set").value,
            player_bytes(),
        );

        let command_any = extract_command_any(&response);
        assert!(
            command_any
                .type_url
                .ends_with("examples.ConfirmRegistrationFee"),
            "unexpected command type_url: {}",
            command_any.type_url
        );
        let confirm = ConfirmRegistrationFee::decode(command_any.value.as_slice())
            .expect("decode ConfirmRegistrationFee");
        assert_eq!(confirm.reservation_id, reservation_bytes());

        // PM event
        let event_any = extract_pm_event_any(&response);
        assert!(
            event_any
                .type_url
                .ends_with("examples.RegistrationCompleted"),
            "unexpected PM event type_url: {}",
            event_any.type_url
        );
        let pm_event = RegistrationCompleted::decode(event_any.value.as_slice())
            .expect("decode RegistrationCompleted");
        assert_eq!(pm_event.reservation_id, reservation_bytes());
        assert_eq!(pm_event.player_root, player_bytes());
        assert_eq!(
            pm_event.fee.as_ref().expect("fee set").amount,
            3000,
            "fee should propagate from fee_paid"
        );
        assert_eq!(pm_event.fee.as_ref().unwrap().currency_code, "USD");
        assert_eq!(pm_event.starting_stack, 15_000);
        assert!(
            pm_event.completed_at.is_some(),
            "completed_at should be stamped"
        );
    }

    // -------------------------------------------------------------------------
    // handle_enrollment_rejected
    // -------------------------------------------------------------------------

    #[test]
    fn handle_enrollment_rejected_emits_release_fee_command_and_failed_event() {
        let event = TournamentEnrollmentRejected {
            player_root: player_bytes(),
            reservation_id: reservation_bytes(),
            reason: "tournament_full".to_string(),
            rejected_at: None,
        };

        let response = handle_enrollment_rejected(event).expect("handler returned Ok");

        assert!(response.facts.is_empty());
        assert_eq!(response.commands.len(), 1);

        let book = &response.commands[0];
        let cover = book.cover.as_ref().expect("cover set");
        assert_eq!(cover.domain, "player");
        assert_eq!(
            cover.root.as_ref().expect("root set").value,
            player_bytes(),
        );

        let command_any = extract_command_any(&response);
        assert!(
            command_any
                .type_url
                .ends_with("examples.ReleaseRegistrationFee"),
            "unexpected command type_url: {}",
            command_any.type_url
        );
        let release = ReleaseRegistrationFee::decode(command_any.value.as_slice())
            .expect("decode ReleaseRegistrationFee");
        assert_eq!(release.reservation_id, reservation_bytes());
        assert_eq!(release.reason, "tournament_full");

        // PM event
        let event_any = extract_pm_event_any(&response);
        assert!(
            event_any
                .type_url
                .ends_with("examples.RegistrationFailed"),
            "unexpected PM event type_url: {}",
            event_any.type_url
        );
        let pm_event = RegistrationFailed::decode(event_any.value.as_slice())
            .expect("decode RegistrationFailed");
        assert_eq!(pm_event.reservation_id, reservation_bytes());
        assert_eq!(pm_event.player_root, player_bytes());

        let failure = pm_event.failure.as_ref().expect("failure set");
        assert_eq!(failure.code, "ENROLLMENT_REJECTED");
        assert_eq!(failure.message, "tournament_full");
        assert_eq!(failure.failed_at_phase, "ENROLLING");
        assert!(failure.failed_at.is_some(), "failed_at should be stamped");
    }

    #[test]
    fn handle_enrollment_rejected_propagates_custom_reason_verbatim() {
        let event = TournamentEnrollmentRejected {
            player_root: player_bytes(),
            reservation_id: reservation_bytes(),
            reason: "already_registered".to_string(),
            rejected_at: None,
        };

        let response = handle_enrollment_rejected(event).expect("handler returned Ok");

        // command-side reason
        let command_any = extract_command_any(&response);
        let release = ReleaseRegistrationFee::decode(command_any.value.as_slice())
            .expect("decode ReleaseRegistrationFee");
        assert_eq!(release.reason, "already_registered");

        // event-side message
        let event_any = extract_pm_event_any(&response);
        let pm_event = RegistrationFailed::decode(event_any.value.as_slice())
            .expect("decode RegistrationFailed");
        assert_eq!(
            pm_event.failure.as_ref().expect("failure set").message,
            "already_registered"
        );
    }
}
