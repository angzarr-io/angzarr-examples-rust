//! Integration tests for the Table -> Player saga router.

use std::collections::HashMap;

use angzarr_client::proto::{event_page, Cover, EventBook, EventPage, SagaHandleRequest};
use angzarr_client::router::{Built, Handler, HandlerConfig, Router};
use angzarr_client::{full_type_url, Kind};
use examples_proto::HandEnded;
use prost::Message;
use prost_types::Any;
use saga_table_player::TablePlayerSaga;

fn build() -> angzarr_client::router::runtime::SagaRouter {
    match Router::new("saga-table-player")
        .with_handler(|| TablePlayerSaga)
        .build()
        .expect("router builds")
    {
        Built::Saga(sr) => sr,
        other => panic!("expected Saga, got {:?}", other),
    }
}

fn hand_ended_request() -> SagaHandleRequest {
    let mut stack_changes: HashMap<String, i64> = HashMap::new();
    stack_changes.insert(hex::encode([0xAA_u8; 16]), 100);

    let event = HandEnded {
        hand_root: vec![0xBB; 16],
        results: vec![],
        stack_changes,
        ended_at: None,
    };
    let any = Any {
        type_url: full_type_url::<HandEnded>(),
        value: event.encode_to_vec(),
    };
    SagaHandleRequest {
        source: Some(EventBook {
            cover: Some(Cover {
                domain: "table".to_string(),
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
    let cfg = TablePlayerSaga.config();
    assert_eq!(cfg.kind(), Kind::Saga);
    match cfg {
        HandlerConfig::Saga {
            name,
            source,
            target,
            handled,
            ..
        } => {
            assert_eq!(name, "saga-table-player");
            assert_eq!(source, "table");
            assert_eq!(target, "player");
            assert!(handled.iter().any(|u| u.ends_with("HandEnded")));
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
fn dispatch_hand_ended_emits_release_funds_commands() {
    let router = build();
    let resp = router.dispatch(hand_ended_request()).expect("dispatch ok");
    assert_eq!(resp.commands.len(), 1);
    assert_eq!(resp.commands[0].cover.as_ref().unwrap().domain, "player");
}
