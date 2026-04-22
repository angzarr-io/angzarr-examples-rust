//! Process Manager: Hand Flow (OO/Tier 5 example).

use angzarr_client::router::{Built, Router};
use angzarr_client::run_process_manager_server;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use hand_flow_oo::HandFlowPm;

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    println!("Starting Hand Flow process manager");

    let router = Router::new("hand-flow")
        .with_handler(|| HandFlowPm)
        .build()
        .expect("failed to build pm router");

    let Built::ProcessManager(pr) = router else {
        panic!("expected ProcessManager variant");
    };

    run_process_manager_server("hand-flow", 50092, pr)
        .await
        .expect("Server failed");
}
