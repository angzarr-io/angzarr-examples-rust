//! Integration tests for the Hand -> Player saga router.

use angzarr_client::proto::{event_page, EventBook, EventPage, SagaHandleRequest};
use angzarr_client::router::{Built, Handler, HandlerConfig, Router};
use angzarr_client::{full_type_url, Kind};
use examples_proto::{HandRanking, PotAwarded, PotWinner};
use prost::Message;
use prost_types::Any;
use saga_hand_player::HandPlayerSaga;

fn build() -> angzarr_client::router::runtime::SagaRouter {
    match Router::new("saga-hand-player")
        .with_handler(|| HandPlayerSaga)
        .build()
        .expect("router builds")
    {
        Built::Saga(sr) => sr,
        other => panic!("expected Saga, got {:?}", other),
    }
}

fn pot_awarded_request() -> SagaHandleRequest {
    let event = PotAwarded {
        winners: vec![PotWinner {
            player_root: vec![0xAA; 16],
            amount: 100,
            pot_type: "main".into(),
            winning_hand: Some(HandRanking::default()),
        }],
        awarded_at: None,
    };
    let any = Any {
        type_url: full_type_url::<PotAwarded>(),
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
    let cfg = HandPlayerSaga.config();
    assert_eq!(cfg.kind(), Kind::Saga);
    match cfg {
        HandlerConfig::Saga {
            name,
            source,
            target,
            handled,
            ..
        } => {
            assert_eq!(name, "saga-hand-player");
            assert_eq!(source, "hand");
            assert_eq!(target, "player");
            assert!(handled.iter().any(|u| u.ends_with("PotAwarded")));
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
fn dispatch_pot_awarded_emits_deposit_funds_commands() {
    let router = build();
    let resp = router.dispatch(pot_awarded_request()).expect("dispatch ok");
    assert_eq!(resp.commands.len(), 1);
    assert_eq!(resp.commands[0].cover.as_ref().unwrap().domain, "player");
}
