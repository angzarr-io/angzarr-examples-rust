fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Example protos root - from buf export
    // Run: buf export buf.build/angzarr/examples -o examples-proto
    // Build scripts run from crate dir, so we go up to workspace root
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let workspace_root = std::path::Path::new(&manifest_dir)
        .parent()
        .expect("proto crate should be in workspace");

    let proto_root = std::env::var("EXAMPLES_PROTO_ROOT").unwrap_or_else(|_| {
        workspace_root
            .join("examples-proto")
            .to_string_lossy()
            .to_string()
    });

    println!("cargo:rerun-if-changed={}", proto_root);

    let protos = vec![
        format!("{}/examples/poker_types.proto", proto_root),
        format!("{}/examples/player.proto", proto_root),
        format!("{}/examples/table.proto", proto_root),
        format!("{}/examples/hand.proto", proto_root),
        format!("{}/examples/ai_sidecar.proto", proto_root),
        // Orchestration protos
        format!("{}/examples/orchestration.proto", proto_root),
        format!("{}/examples/buy_in.proto", proto_root),
        format!("{}/examples/tournament.proto", proto_root),
        format!("{}/examples/registration.proto", proto_root),
        format!("{}/examples/rebuy.proto", proto_root),
    ];

    let mut prost_config = prost_build::Config::new();
    prost_config.enable_type_names();

    tonic_prost_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_with_config(prost_config, &protos, &[proto_root])?;

    Ok(())
}
