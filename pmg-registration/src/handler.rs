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
    let pm_event_book = make_pm_event_book(pack_event(&pm_event, "examples.RegistrationInitiated"));

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
    let pm_event_book = make_pm_event_book(pack_event(&pm_event, "examples.RegistrationCompleted"));

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
