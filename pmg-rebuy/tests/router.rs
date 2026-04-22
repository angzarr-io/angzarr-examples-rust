//! Integration tests for the Rebuy process manager router.

use std::collections::HashMap;

use angzarr_client::proto::{
    event_page, EventBook, EventPage, ProcessManagerHandleRequest, ProcessManagerHandleResponse,
};
use angzarr_client::router::{Built, Handler, HandlerConfig, Router};
use angzarr_client::{full_type_url, Kind};
use examples_proto::{
    Currency, OrchestrationFailure, RebuyChipsAdded, RebuyCompleted, RebuyDenied, RebuyFailed,
    RebuyInitiated, RebuyPhase, RebuyPhaseChanged, RebuyProcessed, RebuyRequested,
};
use pmg_rebuy::RebuyPm;
use prost::Message;
use prost_types::Any;

fn build() -> angzarr_client::router::runtime::ProcessManagerRouter {
    match Router::new("pmg-rebuy")
        .with_handler(|| RebuyPm)
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

fn rebuy_requested_event() -> Any {
    pack(&RebuyRequested {
        reservation_id: vec![0x01; 16],
        tournament_root: vec![0x02; 16],
        table_root: vec![0x03; 16],
        seat: 2,
        fee: Some(Currency {
            amount: 500,
            currency_code: "USD".into(),
        }),
        requested_at: None,
    })
}

fn rebuy_initiated_event() -> Any {
    pack(&RebuyInitiated {
        player_root: vec![0x04; 16],
        tournament_root: vec![0x02; 16],
        table_root: vec![0x03; 16],
        reservation_id: vec![0x01; 16],
        seat: 2,
        fee: Some(Currency {
            amount: 500,
            currency_code: "USD".into(),
        }),
        chips_to_add: 10_000,
        phase: RebuyPhase::RebuyRequested as i32,
        initiated_at: None,
    })
}

#[test]
fn config_exposes_rebuy_pm_metadata() {
    let cfg = RebuyPm.config();
    assert_eq!(cfg.kind(), Kind::ProcessManager);
    match cfg {
        HandlerConfig::ProcessManager {
            name,
            sources,
            targets,
            handled,
            applies,
            ..
        } => {
            assert_eq!(name, "pmg-rebuy");
            for s in ["player", "tournament", "table"] {
                assert!(sources.contains(&s.to_string()), "source {}", s);
            }
            for t in ["tournament", "table", "player"] {
                assert!(targets.contains(&t.to_string()), "target {}", t);
            }
            assert!(handled.iter().any(|u| u.ends_with("RebuyRequested")));
            assert!(handled.iter().any(|u| u.ends_with("RebuyProcessed")));
            assert!(applies.iter().any(|u| u.ends_with("RebuyInitiated")));
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
fn dispatch_rebuy_requested_emits_response() {
    let router = build();
    let _resp = router
        .dispatch(pm_request(rebuy_requested_event(), vec![]))
        .expect("dispatch ok");
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
fn dispatch_rebuy_processed_arm() {
    let evt = pack(&RebuyProcessed {
        player_root: vec![0x04; 16],
        reservation_id: vec![0x01; 16],
        rebuy_cost: 500,
        chips_added: 10_000,
        rebuy_count: 1,
        processed_at: None,
    });
    let _ = run(pm_request(evt, vec![rebuy_initiated_event()]));
}

#[test]
fn dispatch_rebuy_denied_arm() {
    let evt = pack(&RebuyDenied {
        player_root: vec![0x04; 16],
        reservation_id: vec![0x01; 16],
        reason: "max_reached".into(),
        denied_at: None,
    });
    let _ = run(pm_request(evt, vec![rebuy_initiated_event()]));
}

#[test]
fn dispatch_rebuy_chips_added_arm() {
    let evt = pack(&RebuyChipsAdded {
        player_root: vec![0x04; 16],
        reservation_id: vec![0x01; 16],
        seat: 2,
        amount: 10_000,
        new_stack: 20_000,
        added_at: None,
    });
    let _ = run(pm_request(evt, vec![rebuy_initiated_event()]));
}

// -----------------------------------------------------------------------------
// Applier branches.
// -----------------------------------------------------------------------------

fn replay_then_run(
    state_event: Any,
) -> Result<ProcessManagerHandleResponse, angzarr_client::ClientError> {
    run(pm_request(rebuy_requested_event(), vec![state_event]))
}

#[test]
fn apply_rebuy_initiated_branch() {
    let _ = replay_then_run(rebuy_initiated_event());
}

#[test]
fn apply_rebuy_phase_changed_branch() {
    let _ = replay_then_run(pack(&RebuyPhaseChanged {
        reservation_id: vec![0x01; 16],
        from_phase: RebuyPhase::RebuyRequested as i32,
        to_phase: RebuyPhase::RebuyApproving as i32,
        changed_at: None,
    }));
}

#[test]
fn apply_rebuy_completed_branch() {
    let _ = replay_then_run(pack(&RebuyCompleted {
        player_root: vec![0x04; 16],
        tournament_root: vec![0x02; 16],
        table_root: vec![0x03; 16],
        reservation_id: vec![0x01; 16],
        fee: Some(Currency {
            amount: 500,
            currency_code: "USD".into(),
        }),
        chips_added: 10_000,
        completed_at: None,
    }));
}

#[test]
fn apply_rebuy_failed_branch() {
    let _ = replay_then_run(pack(&RebuyFailed {
        player_root: vec![0x04; 16],
        tournament_root: vec![0x02; 16],
        reservation_id: vec![0x01; 16],
        failure: Some(OrchestrationFailure {
            code: "stack_too_high".into(),
            message: "ineligible".into(),
            failed_at_phase: "approving".into(),
            failed_at: None,
        }),
    }));
}
