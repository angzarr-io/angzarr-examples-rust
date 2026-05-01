//! Saga: Hand -> Table gRPC server.

use angzarr_client::router::{Built, Router};
use angzarr_client::run_saga_server;
use saga_hand_table::HandTableSaga;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let router = Router::new("saga-hand-table")
        .with_handler(|| HandTableSaga)
        .build()
        .expect("failed to build saga router");

    let Built::Saga(sr) = router else {
        panic!("expected Saga variant");
    };

    run_saga_server(sr, 50012).await.expect("Server failed");
}
