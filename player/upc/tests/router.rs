//! Integration tests for the player domain upcaster router.

use angzarr_client::proto::{event_page, page_header, EventPage, PageHeader, UpcastRequest};
use prost_types::Any;
use upc_player::build_router;

fn dispatch(events: Vec<EventPage>) -> Vec<EventPage> {
    build_router()
        .dispatch(UpcastRequest {
            domain: "player".to_string(),
            events,
        })
        .expect("upcast dispatch should succeed")
        .events
}

#[test]
fn handle_upcast_preserves_unregistered_event_payloads() {
    let event = Any {
        type_url: "type.googleapis.com/examples.FundsDeposited".into(),
        value: vec![10, 20, 30],
    };
    let page = EventPage {
        payload: Some(event_page::Payload::Event(event.clone())),
        header: Some(PageHeader {
            sync_mode: None,
            sequence_type: Some(page_header::SequenceType::Sequence(42)),
        }),
        created_at: None,
        no_commit: false,
        cascade_id: None,
    };
    let result = dispatch(vec![page]);
    assert_eq!(result.len(), 1);
    match &result[0].payload {
        Some(event_page::Payload::Event(e)) => {
            assert_eq!(e.type_url, event.type_url);
            assert_eq!(e.value, event.value);
        }
        other => panic!("expected Event payload, got {:?}", other),
    }
}

#[test]
fn handle_upcast_empty_input_returns_empty() {
    let result = dispatch(Vec::new());
    assert!(result.is_empty());
}
