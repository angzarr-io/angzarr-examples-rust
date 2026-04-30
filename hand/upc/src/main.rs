//! Hand domain upcaster gRPC server.

use angzarr_client::run_upcaster_server;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use upc_hand::build_router;

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    run_upcaster_server(build_router(), 50421)
        .await
        .expect("Upcaster server failed");
}
