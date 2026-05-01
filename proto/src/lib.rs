//! Generated protobuf types for poker examples.
//!
//! Re-exports `angzarr_client.proto.examples` at the crate root so call
//! sites keep using `examples_proto::PlayerRegistered` even though the
//! upstream proto package is `angzarr_client.proto.examples`.

mod inner {
    tonic::include_proto!("angzarr_client.proto.examples");
}

pub use inner::*;
