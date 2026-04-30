//! Generated protobuf types for poker examples.
//!
//! Re-exports the contents of `angzarr_client.proto.examples` at the
//! crate root so callers can write `examples_proto::PlayerRegistered`
//! the same way they did before the canonical proto package rename.

mod inner {
    tonic::include_proto!("angzarr_client.proto.examples");
}

pub use inner::*;
