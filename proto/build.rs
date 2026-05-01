fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Read protos directly from the angzarr-project submodule. No
    // buf-export + cp staging — the submodule pin IS the spec contract.
    //
    // Proto files declare `package angzarr_client.proto.examples`, so
    // the generated rust module is `angzarr_client::proto::examples`.
    // `proto/src/lib.rs` re-exports it at the crate root so call sites
    // keep using `examples_proto::*`.
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let workspace_root = std::path::Path::new(&manifest_dir)
        .parent()
        .expect("proto crate should be in workspace");
    let proto_root = workspace_root.join("angzarr-project").join("proto");
    let proto_root_str = proto_root.to_string_lossy().to_string();
    let examples_dir = proto_root.join("angzarr_client/proto/examples");

    println!("cargo:rerun-if-changed={}", examples_dir.display());

    let names = [
        "poker_types.proto",
        "player.proto",
        "table.proto",
        "hand.proto",
        "ai_sidecar.proto",
        // Orchestration protos
        "orchestration.proto",
        "buy_in.proto",
        "tournament.proto",
        "registration.proto",
        "rebuy.proto",
    ];
    let protos: Vec<String> = names
        .iter()
        .map(|n| examples_dir.join(n).to_string_lossy().to_string())
        .collect();

    let mut prost_config = prost_build::Config::new();
    prost_config.enable_type_names();

    tonic_prost_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_with_config(prost_config, &protos, &[proto_root_str])?;

    Ok(())
}
