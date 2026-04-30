//! Player domain upcaster gRPC server.
//!
//! Transforms old event versions to current versions during replay.
//! This is a passthrough upcaster - no transformations yet.

use angzarr_client::run_upcaster_server;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use upc_player::build_router;

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    // docs:start:upcaster_server
    run_upcaster_server(build_router(), 50401)
        .await
        .expect("Upcaster server failed");
    // docs:end:upcaster_server
}
