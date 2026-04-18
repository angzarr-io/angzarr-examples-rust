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

#[cfg(test)]
mod tests {
    use super::*;
    use angzarr_client::proto::event_page::Payload as EventPayload;

    fn command_any(response: &ProcessManagerHandleResponse) -> &Any {
        let page = response.commands[0]
            .pages
            .first()
            .expect("command book should have at least one page");
        match page.payload.as_ref().expect("payload present") {
            CommandPayload::Command(any) => any,
            _ => panic!("expected Command payload"),
        }
    }

    fn pm_event_any(response: &ProcessManagerHandleResponse) -> &Any {
        let event_book = response
            .process_events
            .as_ref()
            .expect("process_events should be Some");
        assert_eq!(
            event_book.pages.len(),
            1,
            "exactly one event page expected"
        );
        let page = &event_book.pages[0];
        match page.payload.as_ref().expect("event payload present") {
            EventPayload::Event(any) => any,
            _ => panic!("expected Event payload"),
        }
    }

    #[test]
    fn handle_buy_in_requested_builds_seat_player_command() {
        let reservation_id = b"res-abc".to_vec();
        let table_root = b"table-root-xyz".to_vec();
        let event = BuyInRequested {
            reservation_id: reservation_id.clone(),
            table_root: table_root.clone(),
            seat: 5,
            amount: Some(Currency {
                amount: 10_000,
                currency_code: "USD".to_string(),
            }),
            requested_at: None,
        };

        let response = handle_buy_in_requested(event).expect("handler should succeed");

        // Commands list
        assert_eq!(response.commands.len(), 1);
        let cover = response.commands[0]
            .cover
            .as_ref()
            .expect("cover should be Some");
        assert_eq!(cover.domain, "table");
        assert_eq!(
            cover.root.as_ref().expect("root uuid").value,
            table_root.clone()
        );

        // Inner type URL
        let any = command_any(&response);
        assert!(
            any.type_url.ends_with("examples.SeatPlayer"),
            "type_url was {}",
            any.type_url
        );

        // Decode inner SeatPlayer
        let seat_player = SeatPlayer::decode(any.value.as_slice()).expect("decode SeatPlayer");
        assert_eq!(seat_player.reservation_id, reservation_id);
        assert_eq!(seat_player.seat, 5);
        assert_eq!(seat_player.amount, 10_000);
        // Handler leaves player_root empty by design (derived downstream).
        assert!(seat_player.player_root.is_empty());

        // process_events: exactly one event page, payload is BuyInInitiated
        let pm_any = pm_event_any(&response);
        assert!(
            pm_any.type_url.ends_with("examples.BuyInInitiated"),
            "pm event type_url was {}",
            pm_any.type_url
        );
        let initiated =
            BuyInInitiated::decode(pm_any.value.as_slice()).expect("decode BuyInInitiated");
        assert_eq!(initiated.reservation_id, reservation_id);
        assert_eq!(initiated.table_root, table_root);
        assert_eq!(initiated.seat, 5);
        assert_eq!(initiated.phase(), BuyInPhase::BuyInSeating);
        let amount = initiated.amount.expect("currency present");
        assert_eq!(amount.amount, 10_000);
        assert_eq!(amount.currency_code, "USD");

        // facts empty
        assert!(response.facts.is_empty());
    }

    #[test]
    fn handle_buy_in_requested_defaults_missing_amount_to_zero() {
        let event = BuyInRequested {
            reservation_id: b"res-1".to_vec(),
            table_root: b"tbl-1".to_vec(),
            seat: -1,
            amount: None,
            requested_at: None,
        };

        let response = handle_buy_in_requested(event).expect("handler should succeed");

        let any = command_any(&response);
        let seat_player = SeatPlayer::decode(any.value.as_slice()).expect("decode SeatPlayer");
        assert_eq!(seat_player.amount, 0);
        assert_eq!(seat_player.seat, -1);

        let pm_any = pm_event_any(&response);
        let initiated =
            BuyInInitiated::decode(pm_any.value.as_slice()).expect("decode BuyInInitiated");
        let inner_amount = initiated.amount.expect("currency present");
        assert_eq!(inner_amount.amount, 0);
        assert_eq!(inner_amount.currency_code, "USD");
    }

    #[test]
    fn handle_player_seated_builds_confirm_buy_in_command() {
        let reservation_id = b"res-def".to_vec();
        let player_root = b"player-root-abc".to_vec();
        let event = PlayerSeated {
            player_root: player_root.clone(),
            reservation_id: reservation_id.clone(),
            seat_position: 3,
            stack: 7_500,
            seated_at: None,
        };

        let response = handle_player_seated(event).expect("handler should succeed");

        // Commands list
        assert_eq!(response.commands.len(), 1);
        let cover = response.commands[0]
            .cover
            .as_ref()
            .expect("cover should be Some");
        assert_eq!(cover.domain, "player");
        assert_eq!(
            cover.root.as_ref().expect("root uuid").value,
            player_root.clone()
        );

        let any = command_any(&response);
        assert!(
            any.type_url.ends_with("examples.ConfirmBuyIn"),
            "type_url was {}",
            any.type_url
        );

        let confirm = ConfirmBuyIn::decode(any.value.as_slice()).expect("decode ConfirmBuyIn");
        assert_eq!(confirm.reservation_id, reservation_id);

        // process_events: exactly one event page, payload is BuyInCompleted
        let pm_any = pm_event_any(&response);
        assert!(
            pm_any.type_url.ends_with("examples.BuyInCompleted"),
            "pm event type_url was {}",
            pm_any.type_url
        );
        let completed =
            BuyInCompleted::decode(pm_any.value.as_slice()).expect("decode BuyInCompleted");
        assert_eq!(completed.reservation_id, reservation_id);
        assert_eq!(completed.player_root, player_root);
        assert_eq!(completed.seat, 3);
        let amount = completed.amount.expect("currency present");
        assert_eq!(amount.amount, 7_500);
        assert_eq!(amount.currency_code, "USD");

        assert!(response.facts.is_empty());
    }

    #[test]
    fn handle_seating_rejected_builds_release_buy_in_command() {
        let reservation_id = b"res-ghi".to_vec();
        let player_root = b"player-root-xyz".to_vec();
        let event = SeatingRejected {
            player_root: player_root.clone(),
            reservation_id: reservation_id.clone(),
            requested_seat: 2,
            reason: "seat_occupied".to_string(),
            rejected_at: None,
        };

        let response = handle_seating_rejected(event).expect("handler should succeed");

        // Commands list
        assert_eq!(response.commands.len(), 1);
        let cover = response.commands[0]
            .cover
            .as_ref()
            .expect("cover should be Some");
        assert_eq!(cover.domain, "player");
        assert_eq!(
            cover.root.as_ref().expect("root uuid").value,
            player_root.clone()
        );

        let any = command_any(&response);
        assert!(
            any.type_url.ends_with("examples.ReleaseBuyIn"),
            "type_url was {}",
            any.type_url
        );

        let release = ReleaseBuyIn::decode(any.value.as_slice()).expect("decode ReleaseBuyIn");
        assert_eq!(release.reservation_id, reservation_id);
        assert_eq!(release.reason, "seat_occupied");

        // process_events: exactly one event page, payload is BuyInFailed
        let pm_any = pm_event_any(&response);
        assert!(
            pm_any.type_url.ends_with("examples.BuyInFailed"),
            "pm event type_url was {}",
            pm_any.type_url
        );
        let failed = BuyInFailed::decode(pm_any.value.as_slice()).expect("decode BuyInFailed");
        assert_eq!(failed.reservation_id, reservation_id);
        assert_eq!(failed.player_root, player_root);
        let failure = failed.failure.expect("failure payload present");
        assert_eq!(failure.code, "SEATING_REJECTED");
        assert_eq!(failure.message, "seat_occupied");
        assert_eq!(failure.failed_at_phase, "SEATING");

        assert!(response.facts.is_empty());
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

