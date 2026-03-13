fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Only build gateway proto when acceptance-test feature is enabled
    #[cfg(feature = "acceptance-test")]
    {
        // Angzarr core protos root - from buf export
        let angzarr_proto_root = std::env::var("ANGZARR_PROTO_ROOT")
            .unwrap_or_else(|_| "../angzarr-proto".to_string());

        // Check if protos exist (CI exports them)
        let gateway_proto = format!("{}/angzarr/gateway.proto", angzarr_proto_root);
        if !std::path::Path::new(&gateway_proto).exists() {
            // In local dev without protos, skip building
            println!(
                "cargo:warning=Gateway proto not found at {}, skipping proto generation",
                gateway_proto
            );
            return Ok(());
        }

        println!("cargo:rerun-if-changed={}", angzarr_proto_root);

        // Map angzarr types to angzarr_client::proto
        // This avoids duplicate type definitions
        let mut config = prost_build::Config::new();
        config.extern_path(".angzarr", "::angzarr_client::proto");

        tonic_build::configure()
            .build_server(false)
            .build_client(true)
            .compile_protos_with_config(
                config,
                &[gateway_proto],
                &[&angzarr_proto_root],
            )?;
    }

    Ok(())
}
