//! Player domain upcaster library.
//!
//! Transforms old event versions to current versions during replay.
//! Currently a passthrough - add transformations as needed for schema evolution.

use angzarr_client::proto::EventPage;
use angzarr_client::UpcasterRouter;

// docs:start:upcaster_router
/// Build the upcaster router for player domain.
///
/// Currently a passthrough - add transformations as needed for schema evolution.
pub fn build_router() -> UpcasterRouter {
    UpcasterRouter::new("player")
    // Example transformation (uncomment when needed):
    // .on("PlayerRegisteredV1", upcast_player_registered_v1)
}

/// Handle upcasting for player domain events.
///
/// Delegates to the router for any registered transformations.
/// Events without registered transformations pass through unchanged.
pub fn handle_upcast(events: &[EventPage]) -> Vec<EventPage> {
    let router = build_router();
    router.upcast(events)
}
// docs:end:upcaster_router

#[cfg(test)]
mod tests {
    use super::*;
    use angzarr_client::proto::{event_page, page_header, PageHeader};
    use prost_types::Any;

    /// Test that events without registered transformations pass through unchanged.
    #[test]
    fn test_passthrough_no_transformation() {
        let event = Any {
            type_url: "type.googleapis.com/examples.PlayerRegistered".to_string(),
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

        let result = handle_upcast(&[page]);

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

        let result = handle_upcast(&events);

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

    /// Test that the router domain is correctly set.
    #[test]
    fn test_router_domain() {
        let router = build_router();
        assert_eq!(router.domain(), "player");
    }
}
