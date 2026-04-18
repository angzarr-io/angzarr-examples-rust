//! BuyIn Process Manager — coordinates buy-in flows across Player ↔ Table.

use angzarr_client::router::{Built, Router};
use angzarr_client::run_process_manager_server;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use pmg_buy_in::BuyInPm;

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let router = Router::new("pmg-buy-in")
        .with_handler(|| BuyInPm)
        .build()
        .expect("failed to build pm router");

    let Built::ProcessManager(pr) = router else {
        panic!("expected ProcessManager variant");
    };

    run_process_manager_server("pmg-buy-in", 50392, pr)
        .await
        .expect("Process manager failed");
}
