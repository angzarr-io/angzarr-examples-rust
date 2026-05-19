//! Saga: tournament -> tables (hand-for-hand fan-out) gRPC server.

use angzarr_client::router::{Built, Router};
use angzarr_client::run_saga_server;
use saga_table_tournament_h4h::TableTournamentH4hSaga;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let router = Router::new("saga-table-tournament-h4h")
        .with_handler(|| TableTournamentH4hSaga)
        .build()
        .expect("failed to build saga router");

    let Built::Saga(sr) = router else {
        panic!("expected Saga variant");
    };

    run_saga_server(sr, 50014).await.expect("Server failed");
}
