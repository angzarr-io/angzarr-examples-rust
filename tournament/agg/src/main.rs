//! Tournament bounded context gRPC server.

use angzarr_client::router::{Built, Router};
use angzarr_client::run_command_handler_server;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use agg_tournament::TournamentAggregate;

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let router = Router::new("agg-tournament")
        .with_handler(|| TournamentAggregate)
        .build()
        .expect("failed to build router");

    let Built::CommandHandler(ch) = router else {
        panic!("expected CommandHandler variant");
    };

    run_command_handler_server("tournament", 50004, ch)
        .await
        .expect("Server failed");
}
