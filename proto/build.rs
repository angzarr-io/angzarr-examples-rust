//! Generates the blackjack example types from the angzarr-project submodule.
//!
//! The protos are read in place: the submodule pin is the spec contract, so
//! there is no staging copy. Only the files the Rust components use so far
//! are compiled; add a file here when a component needs its messages.
//!
//! `table.proto` imports `io/angzarr/v1/options.proto` for its component
//! options. prost skips `extend` declarations and generates only the listed
//! files, so nothing from `io.angzarr.v1` is generated here; the wire types
//! come from the angzarr-proto crate.
//!
//! Requires `protoc` on PATH (or `PROTOC` set).

use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?);
    let proto_root = manifest_dir
        .parent()
        .ok_or("proto crate should be in the workspace")?
        .join("angzarr-project")
        .join("proto");
    let examples = proto_root.join("io/angzarr/examples/v1");

    let protos = [
        examples.join("cards.proto"),
        examples.join("table.proto"),
        examples.join("player.proto"),
    ];
    println!("cargo:rerun-if-env-changed=PROTOC");
    for proto in &protos {
        println!("cargo:rerun-if-changed={}", proto.display());
    }

    let mut config = prost_build::Config::new();
    // Type names give every message its `/io.angzarr.examples.v1.X` type URL
    // for packing into `Any`. `bytes` fields are `bytes::Bytes`, as in
    // angzarr-proto.
    config.enable_type_names().bytes(["."]);
    config.compile_protos(&protos, &[proto_root])?;
    Ok(())
}
