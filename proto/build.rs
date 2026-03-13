fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Example protos root - from buf export or sibling repo
    // Build scripts run from crate dir (proto/), so relative path goes up twice
    let proto_root = std::env::var("EXAMPLES_PROTO_ROOT")
        .unwrap_or_else(|_| "../../angzarr-examples-proto/proto".to_string());

    println!("cargo:rerun-if-changed={}", proto_root);

    let protos = vec![
        format!("{}/examples/poker_types.proto", proto_root),
        format!("{}/examples/player.proto", proto_root),
        format!("{}/examples/table.proto", proto_root),
        format!("{}/examples/hand.proto", proto_root),
        format!("{}/examples/ai_sidecar.proto", proto_root),
    ];

    let mut prost_config = prost_build::Config::new();
    prost_config.enable_type_names();

    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos_with_config(prost_config, &protos, &[&proto_root])?;

    Ok(())
}
