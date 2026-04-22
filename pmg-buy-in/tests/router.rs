//! Integration tests for the BuyIn process manager router.

use std::collections::HashMap;

use angzarr_client::proto::{
    event_page, EventBook, EventPage, ProcessManagerHandleRequest, ProcessManagerHandleResponse,
};
use angzarr_client::router::{Built, Handler, HandlerConfig, Router};
use angzarr_client::{full_type_url, Kind};
use examples_proto::{
    BuyInCompleted, BuyInFailed, BuyInInitiated, BuyInPhase, BuyInPhaseChanged, BuyInRequested,
    Currency, OrchestrationFailure, PlayerSeated, SeatingRejected,
};
use pmg_buy_in::BuyInPm;
use prost::Message;
use prost_types::Any;

fn build() -> angzarr_client::router::runtime::ProcessManagerRouter {
    match Router::new("pmg-buy-in")
        .with_handler(|| BuyInPm)
        .build()
        .expect("router builds")
    {
        Built::ProcessManager(pr) => pr,
        other => panic!("expected ProcessManager, got {:?}", other),
    }
}

fn pack<M: Message + prost::Name>(msg: &M) -> Any {
    Any {
        type_url: full_type_url::<M>(),
        value: msg.encode_to_vec(),
    }
}

/// Build a PM-handle request from a trigger event + prior process events.
fn pm_request(trigger: Any, prior: Vec<Any>) -> ProcessManagerHandleRequest {
    let state_pages: Vec<EventPage> = prior
        .into_iter()
        .map(|a| EventPage {
            payload: Some(event_page::Payload::Event(a)),
            ..Default::default()
        })
        .collect();
    ProcessManagerHandleRequest {
        trigger: Some(EventBook {
            pages: vec![EventPage {
                payload: Some(event_page::Payload::Event(trigger)),
                ..Default::default()
            }],
            ..Default::default()
        }),
        process_state: Some(EventBook {
            pages: state_pages,
            ..Default::default()
        }),
        destination_sequences: HashMap::new(),
    }
}

fn buy_in_requested_event() -> Any {
    pack(&BuyInRequested {
        reservation_id: vec![0x03; 16],
        table_root: vec![0x02; 16],
        seat: 4,
        amount: Some(Currency {
            amount: 1000,
            currency_code: "USD".into(),
        }),
        requested_at: None,
    })
}

fn buy_in_initiated_event() -> Any {
    pack(&BuyInInitiated {
        player_root: vec![0x01; 16],
        table_root: vec![0x02; 16],
        reservation_id: vec![0x03; 16],
        seat: 4,
        amount: Some(Currency {
            amount: 1000,
            currency_code: "USD".into(),
        }),
        phase: BuyInPhase::BuyInRequested as i32,
        initiated_at: None,
    })
}

#[test]
fn config_exposes_buy_in_pm_metadata() {
    let cfg = BuyInPm.config();
    assert_eq!(cfg.kind(), Kind::ProcessManager);
    match cfg {
        HandlerConfig::ProcessManager {
            name,
            pm_domain,
            sources,
            targets,
            handled,
            applies,
            ..
        } => {
            assert_eq!(name, "pmg-buy-in");
            assert_eq!(pm_domain, "pmg-buy-in");
            assert!(sources.contains(&"player".to_string()));
            assert!(sources.contains(&"table".to_string()));
            assert!(targets.contains(&"table".to_string()));
            assert!(targets.contains(&"player".to_string()));
            assert!(handled.iter().any(|u| u.ends_with("BuyInRequested")));
            assert!(applies.iter().any(|u| u.ends_with("BuyInInitiated")));
        }
        other => panic!("expected ProcessManager, got {:?}", other),
    }
}

#[test]
fn router_builds_as_process_manager() {
    let router = build();
    assert_eq!(router.handler_count(), 1);
}

#[test]
fn dispatch_buy_in_requested_emits_seat_player_command() {
    let router = build();
    let resp = router
        .dispatch(pm_request(buy_in_requested_event(), vec![]))
        .expect("dispatch ok");
    assert_eq!(resp.commands.len(), 1);
    assert_eq!(resp.commands[0].cover.as_ref().unwrap().domain, "table");
    assert!(resp.process_events.is_some());
}

// -----------------------------------------------------------------------------
// Handlers — one test per `#[handles]` branch.
// -----------------------------------------------------------------------------

fn run(
    req: ProcessManagerHandleRequest,
) -> Result<ProcessManagerHandleResponse, angzarr_client::ClientError> {
    build().dispatch(req)
}

#[test]
fn dispatch_player_seated_arm() {
    let evt = pack(&PlayerSeated {
        player_root: vec![0x01; 16],
        reservation_id: vec![0x03; 16],
        seat_position: 4,
        stack: 1000,
        seated_at: None,
    });
    let _ = run(pm_request(evt, vec![buy_in_initiated_event()]));
}

#[test]
fn dispatch_seating_rejected_arm() {
    let evt = pack(&SeatingRejected {
        player_root: vec![0x01; 16],
        reservation_id: vec![0x03; 16],
        requested_seat: 4,
        reason: "seat_occupied".into(),
        rejected_at: None,
    });
    let _ = run(pm_request(evt, vec![buy_in_initiated_event()]));
}

// -----------------------------------------------------------------------------
// Applier branches — exercise every `#[applies]` arm during state rebuild.
// -----------------------------------------------------------------------------

fn replay_then_run(
    state_event: Any,
) -> Result<ProcessManagerHandleResponse, angzarr_client::ClientError> {
    // Send BuyInRequested as the trigger, but preload one of each applier event
    // into process_state so the replay pass hits the target applier arm.
    run(pm_request(buy_in_requested_event(), vec![state_event]))
}

#[test]
fn apply_buy_in_initiated_branch() {
    let _ = replay_then_run(buy_in_initiated_event());
}

#[test]
fn apply_buy_in_phase_changed_branch() {
    let _ = replay_then_run(pack(&BuyInPhaseChanged {
        reservation_id: vec![0x03; 16],
        from_phase: BuyInPhase::BuyInRequested as i32,
        to_phase: BuyInPhase::BuyInSeating as i32,
        changed_at: None,
    }));
}

#[test]
fn apply_buy_in_completed_branch() {
    let _ = replay_then_run(pack(&BuyInCompleted {
        player_root: vec![0x01; 16],
        table_root: vec![0x02; 16],
        reservation_id: vec![0x03; 16],
        seat: 4,
        amount: Some(Currency {
            amount: 1000,
            currency_code: "USD".into(),
        }),
        completed_at: None,
    }));
}

#[test]
fn apply_buy_in_failed_branch() {
    let _ = replay_then_run(pack(&BuyInFailed {
        player_root: vec![0x01; 16],
        table_root: vec![0x02; 16],
        reservation_id: vec![0x03; 16],
        failure: Some(OrchestrationFailure {
            code: "seating_rejected".into(),
            message: "seat occupied".into(),
            failed_at_phase: "seating".into(),
            failed_at: None,
        }),
    }));
}
