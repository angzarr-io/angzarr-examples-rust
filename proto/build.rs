fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Proto generation moved out of build.rs per project_proto_generation_model
    // (cross-language pattern: pre-build trigger lives in `just generate-proto`,
    // never in build-tool integration). build.rs only emits bindings when
    // explicitly opted in via `GENERATE_PROTOS=1` (the `just generate-proto`
    // recipe sets this when it needs to refresh the source-tree bindings).
    //
    // Normal `cargo build` paths skip codegen and consume the pre-emitted
    // `src/generated/*.rs` file via `include!` in `src/lib.rs`. That file is
    // gitignored; lefthook fires `just generate-proto` on post-checkout /
    // post-merge so fresh clones get it automatically.
    println!("cargo:rerun-if-env-changed=GENERATE_PROTOS");
    if std::env::var("GENERATE_PROTOS").is_err() {
        return Ok(());
    }

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

    let out_dir = std::env::var("ANGZARR_PROTO_OUT_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            std::path::PathBuf::from(&manifest_dir)
                .join("src")
                .join("generated")
        });
    std::fs::create_dir_all(&out_dir)?;

    let mut prost_config = prost_build::Config::new();
    prost_config.enable_type_names();

    tonic_prost_build::configure()
        .build_server(true)
        .build_client(true)
        .out_dir(&out_dir)
        .compile_with_config(prost_config, &protos, &[proto_root])?;

    Ok(())
}
