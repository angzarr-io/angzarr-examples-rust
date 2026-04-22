//! Saga: Table -> Hand gRPC server.

use angzarr_client::router::{Built, Router};
use angzarr_client::run_saga_server;
use saga_table_hand::TableHandSaga;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    // docs:start:event_router
    let router = Router::new("saga-table-hand")
        .with_handler(|| TableHandSaga)
        .build()
        .expect("failed to build saga router");

    let Built::Saga(sr) = router else {
        panic!("expected Saga variant");
    };
    // docs:end:event_router

    run_saga_server("saga-table-hand", 50011, sr)
        .await
        .expect("Server failed");
}
