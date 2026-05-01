//! Integration tests for the Hand -> Table saga router.

use angzarr_client::proto::{event_page, Cover, EventBook, EventPage, SagaHandleRequest};
use angzarr_client::router::{Built, Handler, HandlerConfig, Router};
use angzarr_client::{full_type_url, Kind};
use examples_proto::HandComplete;
use prost::Message;
use prost_types::Any;
use saga_hand_table::HandTableSaga;

fn build() -> angzarr_client::router::runtime::SagaRouter {
    match Router::new("saga-hand-table")
        .with_handler(|| HandTableSaga)
        .build()
        .expect("router builds")
    {
        Built::Saga(sr) => sr,
        other => panic!("expected Saga, got {:?}", other),
    }
}

fn hand_complete_request() -> SagaHandleRequest {
    let event = HandComplete {
        table_root: vec![0xAA; 16],
        hand_number: 1,
        winners: vec![],
        final_stacks: vec![],
        completed_at: None,
    };
    let any = Any {
        type_url: full_type_url::<HandComplete>(),
        value: event.encode_to_vec(),
    };
    SagaHandleRequest {
        source: Some(EventBook {
            cover: Some(Cover {
                domain: "hand".to_string(),
                ..Default::default()
            }),
            pages: vec![EventPage {
                payload: Some(event_page::Payload::Event(any)),
                ..Default::default()
            }],
            ..Default::default()
        }),
        ..Default::default()
    }
}

#[test]
fn config_exposes_saga_metadata() {
    let cfg = HandTableSaga.config();
    assert_eq!(cfg.kind(), Kind::Saga);
    match cfg {
        HandlerConfig::Saga {
            name,
            source,
            target,
            handled,
            ..
        } => {
            assert_eq!(name, "saga-hand-table");
            assert_eq!(source, "hand");
            assert_eq!(target, "table");
            assert!(handled.iter().any(|u| u.ends_with("HandComplete")));
        }
        other => panic!("expected Saga, got {:?}", other),
    }
}

#[test]
fn router_builds_as_saga() {
    let router = build();
    assert_eq!(router.handler_count(), 1);
}

#[test]
fn dispatch_hand_complete_emits_end_hand_command() {
    let router = build();
    let resp = router
        .dispatch(hand_complete_request())
        .expect("dispatch ok");
    assert_eq!(resp.commands.len(), 1);
    assert_eq!(resp.commands[0].cover.as_ref().unwrap().domain, "table");
}
