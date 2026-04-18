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

#[cfg(test)]
mod tests {
    use super::*;

    // ---------- helpers ----------

    /// Extract the inner Any from the first page's Command payload.
    fn first_command_any(book: &CommandBook) -> &Any {
        let page = book.pages.first().expect("command book has no pages");
        match page.payload.as_ref().expect("missing payload") {
            CommandPayload::Command(a) => a,
            _ => panic!("expected inline Command payload"),
        }
    }

    fn cover_domain(book: &CommandBook) -> &str {
        book.cover
            .as_ref()
            .expect("command book missing cover")
            .domain
            .as_str()
    }

    fn cover_root(book: &CommandBook) -> Vec<u8> {
        book.cover
            .as_ref()
            .expect("command book missing cover")
            .root
            .as_ref()
            .expect("cover missing root")
            .value
            .clone()
    }

    fn first_event_any(book: &EventBook) -> Any {
        let page = book.pages.first().expect("event book has no pages");
        match page.payload.as_ref().expect("missing payload") {
            EventPayload::Event(a) => a.clone(),
            _ => panic!("expected Event payload"),
        }
    }

    // ---------- handle_rebuy_requested ----------

    #[test]
    fn handle_rebuy_requested_emits_process_rebuy_and_rebuy_initiated() {
        let reservation_id = vec![0xAA, 0xBB];
        let tournament_root = vec![0x10, 0x20];
        let table_root = vec![0x30, 0x40];
        let event = RebuyRequested {
            reservation_id: reservation_id.clone(),
            tournament_root: tournament_root.clone(),
            table_root: table_root.clone(),
            seat: 5,
            fee: Some(Currency {
                amount: 250,
                currency_code: "USD".to_string(),
            }),
            requested_at: None,
        };

        let resp = handle_rebuy_requested(event).expect("handler returned Err");

        // commands
        assert_eq!(resp.commands.len(), 1);
        let cmd = &resp.commands[0];
        assert_eq!(cover_domain(cmd), "tournament");
        assert_eq!(cover_root(cmd), tournament_root);

        let any = first_command_any(cmd);
        assert!(
            any.type_url.ends_with("examples.ProcessRebuy"),
            "unexpected type_url: {}",
            any.type_url
        );

        let decoded = ProcessRebuy::decode(any.value.as_slice())
            .expect("failed to decode ProcessRebuy");
        assert_eq!(decoded.reservation_id, reservation_id);
        assert!(decoded.player_root.is_empty());

        // facts
        assert!(resp.facts.is_empty());

        // process_events
        let pm_book = resp.process_events.expect("expected process_events Some");
        let pm_any = first_event_any(&pm_book);
        assert!(
            pm_any.type_url.ends_with("examples.RebuyInitiated"),
            "unexpected pm type_url: {}",
            pm_any.type_url
        );
        let initiated = RebuyInitiated::decode(pm_any.value.as_slice())
            .expect("failed to decode RebuyInitiated");
        assert_eq!(initiated.reservation_id, reservation_id);
        assert_eq!(initiated.tournament_root, tournament_root);
        assert_eq!(initiated.table_root, table_root);
        assert_eq!(initiated.seat, 5);
        assert_eq!(initiated.phase(), RebuyPhase::RebuyApproving);
        let fee = initiated.fee.as_ref().expect("fee missing");
        assert_eq!(fee.amount, 250);
        assert_eq!(fee.currency_code, "USD");
    }

    // ---------- handle_rebuy_processed ----------

    #[test]
    fn handle_rebuy_processed_uses_state_table_root_and_seat() {
        let reservation_id = vec![0xCA, 0xFE];
        let player_root = vec![0xDE, 0xAD];
        let table_root = vec![0x77, 0x88, 0x99];
        let seat = 9;

        let state = RebuyState {
            reservation_id: reservation_id.clone(),
            player_root: player_root.clone(),
            table_root: table_root.clone(),
            seat,
            ..RebuyState::default()
        };

        let event = RebuyProcessed {
            player_root: player_root.clone(),
            reservation_id: reservation_id.clone(),
            rebuy_cost: 250,
            chips_added: 1500,
            rebuy_count: 1,
            processed_at: None,
        };

        let resp = handle_rebuy_processed(event, &state).expect("handler returned Err");

        assert_eq!(resp.commands.len(), 1);
        let cmd = &resp.commands[0];
        assert_eq!(cover_domain(cmd), "table");
        // Critical: cover root comes from state.table_root.
        assert_eq!(cover_root(cmd), table_root);

        let any = first_command_any(cmd);
        assert!(
            any.type_url.ends_with("examples.AddRebuyChips"),
            "unexpected type_url: {}",
            any.type_url
        );
        let decoded = AddRebuyChips::decode(any.value.as_slice())
            .expect("failed to decode AddRebuyChips");
        assert_eq!(decoded.reservation_id, reservation_id);
        assert_eq!(decoded.player_root, player_root);
        // Critical: seat came from state, not the event.
        assert_eq!(decoded.seat, seat);
        assert_eq!(decoded.amount, 1500);

        // facts + process_events contract
        assert!(resp.facts.is_empty());
        assert!(
            resp.process_events.is_none(),
            "handle_rebuy_processed must not emit PM events"
        );
    }

    // ---------- handle_rebuy_denied ----------

    #[test]
    fn handle_rebuy_denied_emits_release_fee_and_rebuy_failed() {
        let reservation_id = vec![0x11, 0x22];
        let player_root = vec![0xDE, 0xAD];
        let reason = "insufficient funds".to_string();

        let event = RebuyDenied {
            reservation_id: reservation_id.clone(),
            player_root: player_root.clone(),
            reason: reason.clone(),
            denied_at: None,
        };

        let resp = handle_rebuy_denied(event).expect("handler returned Err");

        assert_eq!(resp.commands.len(), 1);
        let cmd = &resp.commands[0];
        assert_eq!(cover_domain(cmd), "player");
        assert_eq!(cover_root(cmd), player_root);

        let any = first_command_any(cmd);
        assert!(
            any.type_url.ends_with("examples.ReleaseRebuyFee"),
            "unexpected type_url: {}",
            any.type_url
        );
        let decoded = ReleaseRebuyFee::decode(any.value.as_slice())
            .expect("failed to decode ReleaseRebuyFee");
        assert_eq!(decoded.reservation_id, reservation_id);
        assert_eq!(decoded.reason, reason);

        // facts
        assert!(resp.facts.is_empty());

        // process_events
        let pm_book = resp.process_events.expect("expected process_events Some");
        let pm_any = first_event_any(&pm_book);
        assert!(
            pm_any.type_url.ends_with("examples.RebuyFailed"),
            "unexpected pm type_url: {}",
            pm_any.type_url
        );
        let failed = RebuyFailed::decode(pm_any.value.as_slice())
            .expect("failed to decode RebuyFailed");
        assert_eq!(failed.reservation_id, reservation_id);
        assert_eq!(failed.player_root, player_root);
        let failure = failed.failure.as_ref().expect("failure missing");
        assert_eq!(failure.code, "REBUY_DENIED");
        assert_eq!(failure.message, reason);
        assert_eq!(failure.failed_at_phase, "APPROVING");
    }

    // ---------- handle_chips_added ----------

    #[test]
    fn handle_chips_added_emits_confirm_fee_and_rebuy_completed() {
        let reservation_id = vec![0xAB, 0xCD];
        let player_root = vec![0xBE, 0xEF];
        let amount = 2500;

        let event = RebuyChipsAdded {
            player_root: player_root.clone(),
            reservation_id: reservation_id.clone(),
            seat: 3,
            amount,
            new_stack: 12345,
            added_at: None,
        };

        let resp = handle_chips_added(event).expect("handler returned Err");

        assert_eq!(resp.commands.len(), 1);
        let cmd = &resp.commands[0];
        assert_eq!(cover_domain(cmd), "player");
        assert_eq!(cover_root(cmd), player_root);

        let any = first_command_any(cmd);
        assert!(
            any.type_url.ends_with("examples.ConfirmRebuyFee"),
            "unexpected type_url: {}",
            any.type_url
        );
        let decoded = ConfirmRebuyFee::decode(any.value.as_slice())
            .expect("failed to decode ConfirmRebuyFee");
        assert_eq!(decoded.reservation_id, reservation_id);

        // facts
        assert!(resp.facts.is_empty());

        // process_events
        let pm_book = resp.process_events.expect("expected process_events Some");
        let pm_any = first_event_any(&pm_book);
        assert!(
            pm_any.type_url.ends_with("examples.RebuyCompleted"),
            "unexpected pm type_url: {}",
            pm_any.type_url
        );
        let completed = RebuyCompleted::decode(pm_any.value.as_slice())
            .expect("failed to decode RebuyCompleted");
        assert_eq!(completed.reservation_id, reservation_id);
        assert_eq!(completed.player_root, player_root);
        assert_eq!(completed.chips_added, amount);
        let fee = completed.fee.as_ref().expect("fee missing");
        assert_eq!(fee.amount, amount);
        assert_eq!(fee.currency_code, "USD");
    }
}
