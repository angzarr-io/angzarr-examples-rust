//! Integration tests for the Table -> Hand saga router.

use angzarr_client::proto::{event_page, EventBook, EventPage, SagaHandleRequest};
use angzarr_client::router::{Built, Handler, HandlerConfig, Router};
use angzarr_client::{full_type_url, Kind};
use examples_proto::HandStarted;
use prost::Message;
use prost_types::Any;
use saga_table_hand::TableHandSaga;

fn build() -> angzarr_client::router::runtime::SagaRouter {
    match Router::new("saga-table-hand")
        .with_handler(|| TableHandSaga)
        .build()
        .expect("router builds")
    {
        Built::Saga(sr) => sr,
        other => panic!("expected Saga, got {:?}", other),
    }
}

fn hand_started_request() -> SagaHandleRequest {
    let event = HandStarted {
        hand_root: vec![0xAA; 16],
        hand_number: 1,
        dealer_position: 0,
        small_blind_position: 1,
        big_blind_position: 2,
        active_players: vec![],
        game_variant: 1,
        small_blind: 10,
        big_blind: 20,
        started_at: None,
    };
    let any = Any {
        type_url: full_type_url::<HandStarted>(),
        value: event.encode_to_vec(),
    };
    SagaHandleRequest {
        source: Some(EventBook {
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
    let cfg = TableHandSaga.config();
    assert_eq!(cfg.kind(), Kind::Saga);
    match cfg {
        HandlerConfig::Saga {
            name,
            source,
            target,
            handled,
            ..
        } => {
            assert_eq!(name, "saga-table-hand");
            assert_eq!(source, "table");
            assert_eq!(target, "hand");
            assert!(handled.iter().any(|u| u.ends_with("HandStarted")));
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
fn dispatch_hand_started_emits_deal_cards_command() {
    let router = build();
    let resp = router.dispatch(hand_started_request()).expect("dispatch ok");
    assert_eq!(resp.commands.len(), 1);
    assert_eq!(resp.commands[0].cover.as_ref().unwrap().domain, "hand");
}
