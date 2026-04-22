//! BuyIn PM event handlers.
//!
//! Helper functions that build ProcessManagerHandleResponse bundles for each
//! source event. The live PM wiring lives in `main.rs` under `#[process_manager]`.

use angzarr_client::proto::command_page::Payload as CommandPayload;
use angzarr_client::proto::{
    page_header::SequenceType, CommandBook, CommandPage, Cover, EventBook, MergeStrategy,
    PageHeader, ProcessManagerHandleResponse, Uuid as ProtoUuid,
};
use angzarr_client::{pack_event, CommandResult};
use examples_proto::{
    BuyInCompleted, BuyInFailed, BuyInInitiated, BuyInPhase, BuyInRequested, ConfirmBuyIn,
    Currency, OrchestrationFailure, PlayerSeated, ReleaseBuyIn, SeatPlayer, SeatingRejected,
};
use prost::Message;
use prost_types::Any;

/// Translate Player-domain BuyInRequested → Table SeatPlayer command.
pub fn handle_buy_in_requested(
    event: BuyInRequested,
) -> CommandResult<ProcessManagerHandleResponse> {
    let amount = event.amount.as_ref().map(|c| c.amount).unwrap_or(0);
    let reservation_id = event.reservation_id.clone();
    let table_root = event.table_root.clone();
    // Player root is derived from the request's seat+reservation — not carried
    // on the event proto. Leave empty; downstream aggregate flow keys off
    // reservation_id.
    let player_root: Vec<u8> = Vec::new();

    let seat_player = SeatPlayer {
        player_root: player_root.clone(),
        reservation_id: reservation_id.clone(),
        seat: event.seat,
        amount,
    };

    let command = make_command_book("table", &table_root, "examples.SeatPlayer", &seat_player);

    let pm_event = BuyInInitiated {
        player_root,
        table_root,
        reservation_id,
        seat: event.seat,
        amount: Some(Currency {
            amount,
            currency_code: "USD".to_string(),
        }),
        phase: BuyInPhase::BuyInSeating as i32,
        initiated_at: Some(angzarr_client::now()),
    };
    let pm_event_book = make_pm_event_book(pack_event(&pm_event, "examples.BuyInInitiated"));

    Ok(ProcessManagerHandleResponse {
        commands: vec![command],
        process_events: Some(pm_event_book),
        facts: vec![],
    })
}

/// Translate Table PlayerSeated → Player ConfirmBuyIn command.
pub fn handle_player_seated(event: PlayerSeated) -> CommandResult<ProcessManagerHandleResponse> {
    let confirm = ConfirmBuyIn {
        reservation_id: event.reservation_id.clone(),
    };
    let command = make_command_book(
        "player",
        &event.player_root,
        "examples.ConfirmBuyIn",
        &confirm,
    );

    let pm_event = BuyInCompleted {
        player_root: event.player_root.clone(),
        table_root: vec![],
        reservation_id: event.reservation_id,
        seat: event.seat_position,
        amount: Some(Currency {
            amount: event.stack,
            currency_code: "USD".to_string(),
        }),
        completed_at: Some(angzarr_client::now()),
    };
    let pm_event_book = make_pm_event_book(pack_event(&pm_event, "examples.BuyInCompleted"));

    Ok(ProcessManagerHandleResponse {
        commands: vec![command],
        process_events: Some(pm_event_book),
        facts: vec![],
    })
}

/// Translate Table SeatingRejected → Player ReleaseBuyIn command.
pub fn handle_seating_rejected(
    event: SeatingRejected,
) -> CommandResult<ProcessManagerHandleResponse> {
    let release = ReleaseBuyIn {
        reservation_id: event.reservation_id.clone(),
        reason: event.reason.clone(),
    };
    let command = make_command_book(
        "player",
        &event.player_root,
        "examples.ReleaseBuyIn",
        &release,
    );

    let pm_event = BuyInFailed {
        player_root: event.player_root.clone(),
        table_root: vec![],
        reservation_id: event.reservation_id,
        failure: Some(OrchestrationFailure {
            code: "SEATING_REJECTED".to_string(),
            message: event.reason,
            failed_at_phase: "SEATING".to_string(),
            failed_at: Some(angzarr_client::now()),
        }),
    };
    let pm_event_book = make_pm_event_book(pack_event(&pm_event, "examples.BuyInFailed"));

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
    use angzarr_client::proto::event_page::Payload;
    use angzarr_client::proto::EventPage;

    EventBook {
        cover: None,
        pages: vec![EventPage {
            header: Some(PageHeader {
                sequence_type: Some(SequenceType::Sequence(0)),
            }),
            created_at: Some(angzarr_client::now()),
            no_commit: false,
            cascade_id: None,
            payload: Some(Payload::Event(event)),
        }],
        snapshot: None,
        next_sequence: 0,
    }
}

