//! Integration tests for the Registration process manager router.

use std::collections::HashMap;

use angzarr_client::proto::{
    event_page, EventBook, EventPage, ProcessManagerHandleRequest, ProcessManagerHandleResponse,
};
use angzarr_client::router::{Built, Handler, HandlerConfig, Router};
use angzarr_client::{full_type_url, Kind};
use examples_proto::{
    Currency, OrchestrationFailure, RegistrationCompleted, RegistrationFailed,
    RegistrationInitiated, RegistrationPhase, RegistrationPhaseChanged, RegistrationRequested,
    TournamentEnrollmentRejected, TournamentPlayerEnrolled,
};
use pmg_registration::RegistrationPm;
use prost::Message;
use prost_types::Any;

fn build() -> angzarr_client::router::runtime::ProcessManagerRouter {
    match Router::new("pmg-registration")
        .with_handler(|| RegistrationPm)
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

fn registration_requested_event() -> Any {
    pack(&RegistrationRequested {
        reservation_id: vec![0xAA; 16],
        tournament_root: vec![0xBB; 16],
        fee: Some(Currency {
            amount: 500,
            currency_code: "USD".into(),
        }),
        requested_at: None,
    })
}

fn registration_initiated_event() -> Any {
    pack(&RegistrationInitiated {
        player_root: vec![0x01; 16],
        tournament_root: vec![0xBB; 16],
        reservation_id: vec![0xAA; 16],
        fee: Some(Currency {
            amount: 500,
            currency_code: "USD".into(),
        }),
        phase: RegistrationPhase::RegistrationRequested as i32,
        initiated_at: None,
    })
}

#[test]
fn config_exposes_registration_pm_metadata() {
    let cfg = RegistrationPm.config();
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
            assert_eq!(name, "pmg-registration");
            assert!(sources.contains(&"player".into()));
            assert!(sources.contains(&"tournament".into()));
            assert!(targets.contains(&"player".into()));
            assert!(targets.contains(&"tournament".into()));
            assert!(handled.iter().any(|u| u.ends_with("RegistrationRequested")));
            assert!(applies.iter().any(|u| u.ends_with("RegistrationInitiated")));
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
fn dispatch_registration_requested_emits_response() {
    let router = build();
    let _resp = router
        .dispatch(pm_request(registration_requested_event(), vec![]))
        .expect("dispatch ok");
}

// -----------------------------------------------------------------------------
// Handlers — one test per `#[handles]` branch.
// -----------------------------------------------------------------------------

fn run(req: ProcessManagerHandleRequest) -> Result<ProcessManagerHandleResponse, angzarr_client::ClientError> {
    build().dispatch(req)
}

#[test]
fn dispatch_player_enrolled_arm() {
    let evt = pack(&TournamentPlayerEnrolled {
        player_root: vec![0x01; 16],
        reservation_id: vec![0xAA; 16],
        fee_paid: 500,
        starting_stack: 10_000,
        registration_number: 1,
        enrolled_at: None,
    });
    let _ = run(pm_request(evt, vec![registration_initiated_event()]));
}

#[test]
fn dispatch_enrollment_rejected_arm() {
    let evt = pack(&TournamentEnrollmentRejected {
        player_root: vec![0x01; 16],
        reservation_id: vec![0xAA; 16],
        reason: "full".into(),
        rejected_at: None,
    });
    let _ = run(pm_request(evt, vec![registration_initiated_event()]));
}

// -----------------------------------------------------------------------------
// Applier branches — exercise every `#[applies]` arm during state rebuild.
// -----------------------------------------------------------------------------

fn replay_then_run(state_event: Any) -> Result<ProcessManagerHandleResponse, angzarr_client::ClientError> {
    run(pm_request(registration_requested_event(), vec![state_event]))
}

#[test]
fn apply_registration_initiated_branch() {
    let _ = replay_then_run(registration_initiated_event());
}

#[test]
fn apply_registration_phase_changed_branch() {
    let _ = replay_then_run(pack(&RegistrationPhaseChanged {
        reservation_id: vec![0xAA; 16],
        from_phase: RegistrationPhase::RegistrationRequested as i32,
        to_phase: RegistrationPhase::RegistrationEnrolling as i32,
        changed_at: None,
    }));
}

#[test]
fn apply_registration_completed_branch() {
    let _ = replay_then_run(pack(&RegistrationCompleted {
        player_root: vec![0x01; 16],
        tournament_root: vec![0xBB; 16],
        reservation_id: vec![0xAA; 16],
        fee: Some(Currency {
            amount: 500,
            currency_code: "USD".into(),
        }),
        starting_stack: 10_000,
        completed_at: None,
    }));
}

#[test]
fn apply_registration_failed_branch() {
    let _ = replay_then_run(pack(&RegistrationFailed {
        player_root: vec![0x01; 16],
        tournament_root: vec![0xBB; 16],
        reservation_id: vec![0xAA; 16],
        failure: Some(OrchestrationFailure {
            code: "tournament_full".into(),
            message: "no seats".into(),
            failed_at_phase: "enrolling".into(),
            failed_at: None,
        }),
    }));
}
