//! Integration tests for the Hand -> Table saga router.

use angzarr_client::proto::{
    command_page, event_page, Cover, EventBook, EventPage, SagaHandleRequest, Uuid,
};
use angzarr_client::router::{Built, Handler, HandlerConfig, Router};
use angzarr_client::{full_type_url, Kind};
use examples_proto::{EndHand, HandComplete};
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

fn hand_complete_request(hand_root: Vec<u8>) -> SagaHandleRequest {
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
                root: Some(Uuid { value: hand_root }),
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
    let hand_root = vec![0x77; 16];
    let resp = router
        .dispatch(hand_complete_request(hand_root.clone()))
        .expect("dispatch ok");
    assert_eq!(resp.commands.len(), 1);
    assert_eq!(resp.commands[0].cover.as_ref().unwrap().domain, "table");

    // The emitted EndHand should carry hand_root pulled from the source
    // EventBook's cover.root — the macro now plumbs source_cover through to
    // the saga handler, matching Python.
    let cmd_any = match resp.commands[0].pages[0]
        .payload
        .as_ref()
        .expect("page payload")
    {
        command_page::Payload::Command(any) => any,
        _ => panic!("expected inline command payload"),
    };
    let end_hand = EndHand::decode(&*cmd_any.value).expect("decode EndHand");
    assert_eq!(end_hand.hand_root, hand_root);
}
