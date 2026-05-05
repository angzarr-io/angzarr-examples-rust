//! Integration tests for the Player -> Table saga router.

use angzarr_client::proto::page_header::SequenceType;
use angzarr_client::proto::{
    event_page, Cover, EventBook, EventPage, SagaHandleRequest, Uuid,
};
use angzarr_client::router::{Built, Handler, HandlerConfig, Router};
use angzarr_client::{full_type_url, Kind};
use examples_proto::{PlayerReturningToPlay, PlayerSatIn, PlayerSatOut, PlayerSittingOut};
use prost::Message;
use prost_types::Any;
use saga_player_table::PlayerTableSaga;

fn build() -> angzarr_client::router::runtime::SagaRouter {
    match Router::new("saga-player-table")
        .with_handler(|| PlayerTableSaga)
        .build()
        .expect("router builds")
    {
        Built::Saga(sr) => sr,
        other => panic!("expected Saga, got {:?}", other),
    }
}

fn pack<M: Message>(msg: &M, full_name: &str) -> Any {
    Any {
        type_url: format!("type.googleapis.com/{full_name}"),
        value: msg.encode_to_vec(),
    }
}

fn saga_request(any: Any, player_root: Vec<u8>) -> SagaHandleRequest {
    SagaHandleRequest {
        source: Some(EventBook {
            cover: Some(Cover {
                domain: "player".to_string(),
                root: Some(Uuid { value: player_root }),
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
    let cfg = PlayerTableSaga.config();
    assert_eq!(cfg.kind(), Kind::Saga);
    match cfg {
        HandlerConfig::Saga {
            name,
            source,
            target,
            handled,
            ..
        } => {
            assert_eq!(name, "saga-player-table");
            assert_eq!(source, "player");
            assert_eq!(target, "table");
            assert!(handled.iter().any(|u| u.ends_with("PlayerSittingOut")));
            assert!(handled.iter().any(|u| u.ends_with("PlayerReturningToPlay")));
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
fn dispatch_player_sitting_out_emits_player_sat_out_fact_to_table() {
    let router = build();
    let table_root = vec![0xAA; 16];
    let player_root = vec![0x33; 16];
    let event = PlayerSittingOut {
        table_root: table_root.clone(),
        sat_out_at: None,
    };
    let any = pack(&event, "angzarr_client.proto.examples.PlayerSittingOut");
    let resp = router
        .dispatch(saga_request(any, player_root.clone()))
        .expect("dispatch ok");

    // Saga emits no commands — the propagation is a fact (event), not a command.
    assert!(resp.commands.is_empty(), "expected no commands");
    assert_eq!(resp.events.len(), 1, "expected one fact emitted");

    let fact = &resp.events[0];
    let cover = fact.cover.as_ref().expect("cover set");
    assert_eq!(cover.domain, "table");
    assert_eq!(
        cover.root.as_ref().expect("root set").value,
        table_root,
        "fact targets the table aggregate by table_root"
    );

    let payload = match fact.pages[0]
        .payload
        .as_ref()
        .expect("page payload")
    {
        event_page::Payload::Event(any) => any,
        _ => panic!("expected inline event payload"),
    };
    assert_eq!(payload.type_url, full_type_url::<PlayerSatOut>());
    let decoded = PlayerSatOut::decode(&*payload.value).expect("decode PlayerSatOut");
    assert_eq!(decoded.sat_out_at, None);
    // player_root must be lifted from the source EventBook's cover.root.value
    // — this is what the macro now plumbs through via the `source_cover` arg.
    assert_eq!(decoded.player_root, player_root);

    // ExternalDeferred header carries the dedup key built from the player root.
    let header = fact.pages[0].header.as_ref().expect("header set");
    match header.sequence_type.as_ref().expect("sequence_type set") {
        SequenceType::ExternalDeferred(ext) => {
            assert_eq!(
                ext.external_id,
                format!("sitout-{}", hex::encode(&player_root))
            );
        }
        other => panic!("expected ExternalDeferred header, got {:?}", other),
    }
}

#[test]
fn dispatch_player_returning_to_play_emits_player_sat_in_fact_to_table() {
    let router = build();
    let table_root = vec![0xBB; 16];
    let player_root = vec![0x44; 16];
    let event = PlayerReturningToPlay {
        table_root: table_root.clone(),
        sat_in_at: None,
    };
    let any = pack(&event, "angzarr_client.proto.examples.PlayerReturningToPlay");
    let resp = router
        .dispatch(saga_request(any, player_root.clone()))
        .expect("dispatch ok");

    assert!(resp.commands.is_empty());
    assert_eq!(resp.events.len(), 1);

    let fact = &resp.events[0];
    let cover = fact.cover.as_ref().expect("cover set");
    assert_eq!(cover.domain, "table");
    assert_eq!(cover.root.as_ref().expect("root set").value, table_root);

    let payload = match fact.pages[0]
        .payload
        .as_ref()
        .expect("page payload")
    {
        event_page::Payload::Event(any) => any,
        _ => panic!("expected inline event payload"),
    };
    assert_eq!(payload.type_url, full_type_url::<PlayerSatIn>());
    let decoded = PlayerSatIn::decode(&*payload.value).expect("decode PlayerSatIn");
    assert_eq!(decoded.player_root, player_root);
}
