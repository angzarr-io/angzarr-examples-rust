//! Hand domain upcaster library.
//!
//! Passthrough upcaster — exists so the unified `Router::build()` accepts an
//! upcaster handler for the hand domain. Replace the placeholder
//! `#[upcasts(from = X, to = Y)]` method with real transformations as schemas
//! evolve.

use angzarr_client::router::{Built, Router, UpcasterRouter};
use angzarr_client::upcaster;
use examples_proto::CardsDealt;

struct HandUpcaster;

#[upcaster(name = "upcaster-hand", domain = "hand")]
impl HandUpcaster {
    #[upcasts(from = CardsDealt, to = CardsDealt)]
    fn passthrough(old: CardsDealt) -> CardsDealt {
        old
    }
}

pub fn build_router() -> UpcasterRouter {
    match Router::new("upcaster-hand")
        .with_handler(|| HandUpcaster)
        .build()
        .expect("failed to build upcaster router")
    {
        Built::Upcaster(r) => r,
        _ => panic!("expected Upcaster variant"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use angzarr_client::proto::{event_page, page_header, EventPage, PageHeader, UpcastRequest};
    use prost_types::Any;

    #[test]
    fn passthrough_preserves_unrecognized_events() {
        let router = build_router();
        let event = Any {
            type_url: "type.googleapis.com/examples.SomeOtherEvent".to_string(),
            value: vec![1, 2, 3],
        };
        let page = EventPage {
            payload: Some(event_page::Payload::Event(event.clone())),
            header: Some(PageHeader {
                sequence_type: Some(page_header::SequenceType::Sequence(0)),
            }),
            created_at: None,
            no_commit: false,
            cascade_id: None,
        };
        let result = router
            .dispatch(UpcastRequest {
                domain: "hand".to_string(),
                events: vec![page],
            })
            .expect("dispatch")
            .events;
        assert_eq!(result.len(), 1);
        if let Some(event_page::Payload::Event(e)) = &result[0].payload {
            assert_eq!(e.value, event.value);
        } else {
            panic!("expected event");
        }
    }
}
