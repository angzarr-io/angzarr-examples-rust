//! Player domain upcaster library.
//!
//! Transforms old event versions to current versions during replay.
//! Currently a passthrough — the registered handler is a noop that
//! exists only so the unified `Router::build()` accepts the upcaster
//! (the builder rejects empty routers). Replace the placeholder
//! `#[upcasts(from = X, to = Y)]` method with real transformations as
//! schemas evolve.

use angzarr_client::router::{Built, Router, UpcasterRouter};
use angzarr_client::upcaster;
use examples_proto::PlayerRegistered;

struct PlayerUpcaster;

#[upcaster(name = "upcaster-player", domain = "player")]
impl PlayerUpcaster {
    #[upcasts(from = PlayerRegistered, to = PlayerRegistered)]
    fn passthrough(old: PlayerRegistered) -> PlayerRegistered {
        old
    }
}

// docs:start:upcaster_router
/// Build the upcaster router for player domain.
pub fn build_router() -> UpcasterRouter {
    match Router::new("upcaster-player")
        .with_handler(|| PlayerUpcaster)
        .build()
        .expect("failed to build upcaster router")
    {
        Built::Upcaster(r) => r,
        _ => panic!("expected Upcaster variant"),
    }
}
// docs:end:upcaster_router

#[cfg(test)]
mod tests {
    use super::*;
    use angzarr_client::proto::{event_page, page_header, EventPage, PageHeader, UpcastRequest};
    use prost_types::Any;

    fn dispatch(events: Vec<EventPage>) -> Vec<EventPage> {
        let router = build_router();
        router
            .dispatch(UpcastRequest {
                domain: "player".to_string(),
                events,
            })
            .expect("upcast dispatch should succeed")
            .events
    }

    /// Test that events without registered transformations pass through unchanged.
    #[test]
    fn test_passthrough_no_transformation() {
        let event = Any {
            type_url: "type.googleapis.com/examples.SomeOtherEvent".to_string(),
            value: vec![1, 2, 3, 4],
        };

        let page = EventPage {
            payload: Some(event_page::Payload::Event(event.clone())),
            header: Some(PageHeader {
                sequence_type: Some(page_header::SequenceType::Sequence(1)),
            }),
            created_at: None,
            no_commit: false,
            cascade_id: None,
        };

        let result = dispatch(vec![page]);

        assert_eq!(result.len(), 1);
        if let Some(event_page::Payload::Event(e)) = &result[0].payload {
            assert_eq!(e.type_url, event.type_url);
            assert_eq!(e.value, event.value);
        } else {
            panic!("Expected event payload");
        }
    }

    /// Test that multiple events are processed in order.
    #[test]
    fn test_multiple_events_preserve_order() {
        let events: Vec<EventPage> = (0..5)
            .map(|i| EventPage {
                payload: Some(event_page::Payload::Event(Any {
                    type_url: format!("type.googleapis.com/examples.Event{}", i),
                    value: vec![i as u8],
                })),
                header: Some(PageHeader {
                    sequence_type: Some(page_header::SequenceType::Sequence(i)),
                }),
                created_at: None,
                no_commit: false,
                cascade_id: None,
            })
            .collect();

        let result = dispatch(events);

        assert_eq!(result.len(), 5);
        for (i, page) in result.iter().enumerate() {
            if let Some(event_page::Payload::Event(e)) = &page.payload {
                assert_eq!(
                    e.type_url,
                    format!("type.googleapis.com/examples.Event{}", i)
                );
            }
        }
    }
}
