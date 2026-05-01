//! Hand Flow Process Manager — orchestrates poker hand phases across domains.

use angzarr_client::router::{Built, Router};
use angzarr_client::run_process_manager_server;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use pmg_hand_flow::HandFlowPm;

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let router = Router::new("pmg-hand-flow")
        .with_handler(|| HandFlowPm)
        .build()
        .expect("failed to build pm router");

    let Built::ProcessManager(pr) = router else {
        panic!("expected ProcessManager variant");
    };

    run_process_manager_server(pr, 50391)
        .await
        .expect("Process manager failed");
}
