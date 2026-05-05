//! Shim helpers used across examples-rust crates.
//!
//! Restore call-site shapes that angzarr-client used to expose at its
//! crate root before audits #57 / #76 trimmed them. Keeping the shims
//! local lets the examples migrate at one site (this file) instead of
//! at every handler.

pub mod error_shapes;
pub mod errors;
pub use errors::{reject, CommandError, ErrorStatus};

use angzarr_client::proto::{
    event_page as proto_event_page, page_header::SequenceType, EventPage, PageHeader,
};
use angzarr_client::{now, type_url, CommandRejectedError};
use prost::{Message, Name};
use prost_types::Any;

/// Pack a proto message into an `Any` whose `type_url` is the canonical
/// Angzarr URL derived from `M::full_name()`. The legacy `type_name`
/// argument is retained for source compatibility but ignored — the
/// proto's own full name is the source of truth so renames in the .proto
/// schema (e.g. `examples.X` → `angzarr_client.proto.examples.X`) propagate
/// without touching every call site.
pub fn pack_event<M: Message + Name>(msg: &M, _type_name: &str) -> Any {
    Any {
        type_url: type_url(&M::full_name()),
        value: msg.encode_to_vec(),
    }
}

/// Build an `EventPage` carrying `event` at `seq`, stamped with the
/// current timestamp and committed (no 2PC, no cascade).
pub fn event_page(seq: u32, event: Any) -> EventPage {
    EventPage {
        header: Some(PageHeader {
            sync_mode: None,
            sequence_type: Some(SequenceType::Sequence(seq)),
        }),
        created_at: Some(now()),
        payload: Some(proto_event_page::Payload::Event(event)),
        no_commit: false,
        cascade_id: None,
    }
}

/// FAILED_PRECONDITION rejection with the legacy single-message shape.
///
/// Callers used to write `CommandRejectedError::new(msg)`; the new
/// inventory-bound API requires `(code, message, details)`. Until a
/// proper error inventory exists for the examples, route everything
/// through this shim with a generic code.
pub fn rejected(message: &'static str) -> CommandRejectedError {
    CommandRejectedError::precondition_failed(
        "EXAMPLE_REJECTED",
        message,
        std::iter::empty::<(&str, &str)>(),
    )
}

/// INVALID_ARGUMENT rejection with the legacy single-message shape.
///
/// Mirrors `rejected` but emits `status_code = INVALID_ARGUMENT`, which
/// some BDD scenarios assert on. Use for input-shape validation
/// (positive amounts, non-empty fields, etc.); use [`rejected`] for
/// state-machine guards.
pub fn invalid_arg(message: &'static str) -> CommandRejectedError {
    CommandRejectedError::invalid_argument(
        "EXAMPLE_INVALID_ARGUMENT",
        message,
        std::iter::empty::<(&str, &str)>(),
    )
}
