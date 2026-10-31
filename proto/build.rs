//! Generates the blackjack example's layer from the angzarr-project
//! submodule's protos, read in place (the submodule pin is the spec
//! contract): the wire types (angzarr-build's `wire`, prost with angzarr's
//! settings) and the markup domain layer over them (`markup_domain`), into
//! OUT_DIR. table.proto's siblings (cards, player, policies) join the set;
//! io.angzarr.v1's own types come from angzarr-proto, never generated here.
//!
//! Requires `protoc` on PATH (or `PROTOC` set).

const PROTOS: &[&str] = &[
    "io/angzarr/examples/v1/cards.proto",
    "io/angzarr/examples/v1/table.proto",
    "io/angzarr/examples/v1/player.proto",
    "io/angzarr/examples/v1/policies.proto",
];
const INCLUDES: &[&str] = &["../angzarr-project/proto"];

fn main() -> Result<(), angzarr_build::BuildError> {
    angzarr_build::wire(PROTOS, INCLUDES)?;
    angzarr_build::markup_domain(PROTOS, INCLUDES, &[("io.angzarr.examples.v1", "crate::v1")])
}
