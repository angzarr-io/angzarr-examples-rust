//! Rebuy PM event handlers.

use angzarr_client::proto::command_page::Payload as CommandPayload;
use angzarr_client::proto::{
    event_page::Payload as EventPayload, page_header::SequenceType, CommandBook, CommandPage,
    Cover, EventBook, EventPage, MergeStrategy, PageHeader, ProcessManagerHandleResponse,
    Uuid as ProtoUuid,
};
use angzarr_client::{pack_event, CommandResult};
use examples_proto::{
    AddRebuyChips, ConfirmRebuyFee, Currency, OrchestrationFailure, ProcessRebuy, RebuyChipsAdded,
    RebuyCompleted, RebuyDenied, RebuyFailed, RebuyInitiated, RebuyPhase, RebuyProcessed,
    RebuyRequested, ReleaseRebuyFee,
};
use prost::Message;
use prost_types::Any;

use crate::state::RebuyState;

pub fn handle_rebuy_requested(
    event: RebuyRequested,
) -> CommandResult<ProcessManagerHandleResponse> {
    let fee = event.fee.as_ref().map(|c| c.amount).unwrap_or(0);
    let reservation_id = event.reservation_id.clone();
    let tournament_root = event.tournament_root.clone();
    let table_root = event.table_root.clone();
    let player_root: Vec<u8> = Vec::new();

    let process_rebuy = ProcessRebuy {
        player_root: player_root.clone(),
        reservation_id: reservation_id.clone(),
    };
    let command = make_command_book(
        "tournament",
        &tournament_root,
        "examples.ProcessRebuy",
        &process_rebuy,
    );

    let pm_event = RebuyInitiated {
        player_root,
        tournament_root,
        table_root,
        reservation_id,
        seat: event.seat,
        fee: Some(Currency {
            amount: fee,
            currency_code: "USD".to_string(),
        }),
        chips_to_add: 0,
        phase: RebuyPhase::RebuyApproving as i32,
        initiated_at: Some(angzarr_client::now()),
    };
    let pm_event_book = make_pm_event_book(pack_event(&pm_event, "examples.RebuyInitiated"));

    Ok(ProcessManagerHandleResponse {
        commands: vec![command],
        process_events: Some(pm_event_book),
        facts: vec![],
    })
}

pub fn handle_rebuy_processed(
    event: RebuyProcessed,
    state: &RebuyState,
) -> CommandResult<ProcessManagerHandleResponse> {
    let add_chips = AddRebuyChips {
        player_root: event.player_root.clone(),
        reservation_id: event.reservation_id.clone(),
        seat: state.seat,
        amount: event.chips_added,
    };
    let command = make_command_book(
        "table",
        &state.table_root,
        "examples.AddRebuyChips",
        &add_chips,
    );

    Ok(ProcessManagerHandleResponse {
        commands: vec![command],
        process_events: None,
        facts: vec![],
    })
}

pub fn handle_rebuy_denied(event: RebuyDenied) -> CommandResult<ProcessManagerHandleResponse> {
    let release = ReleaseRebuyFee {
        reservation_id: event.reservation_id.clone(),
        reason: event.reason.clone(),
    };
    let command = make_command_book(
        "player",
        &event.player_root,
        "examples.ReleaseRebuyFee",
        &release,
    );

    let pm_event = RebuyFailed {
        player_root: event.player_root.clone(),
        tournament_root: vec![],
        reservation_id: event.reservation_id,
        failure: Some(OrchestrationFailure {
            code: "REBUY_DENIED".to_string(),
            message: event.reason,
            failed_at_phase: "APPROVING".to_string(),
            failed_at: Some(angzarr_client::now()),
        }),
    };
    let pm_event_book = make_pm_event_book(pack_event(&pm_event, "examples.RebuyFailed"));

    Ok(ProcessManagerHandleResponse {
        commands: vec![command],
        process_events: Some(pm_event_book),
        facts: vec![],
    })
}

pub fn handle_chips_added(
    event: RebuyChipsAdded,
) -> CommandResult<ProcessManagerHandleResponse> {
    let confirm = ConfirmRebuyFee {
        reservation_id: event.reservation_id.clone(),
    };
    let command = make_command_book(
        "player",
        &event.player_root,
        "examples.ConfirmRebuyFee",
        &confirm,
    );

    let pm_event = RebuyCompleted {
        player_root: event.player_root.clone(),
        tournament_root: vec![],
        table_root: vec![],
        reservation_id: event.reservation_id,
        chips_added: event.amount,
        fee: Some(Currency {
            amount: event.amount,
            currency_code: "USD".to_string(),
        }),
        completed_at: Some(angzarr_client::now()),
    };
    let pm_event_book = make_pm_event_book(pack_event(&pm_event, "examples.RebuyCompleted"));

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

