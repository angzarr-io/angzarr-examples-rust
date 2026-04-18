//! Registration Process Manager — coordinates tournament registration flows.

use angzarr_client::router::{Built, Router};
use angzarr_client::run_process_manager_server;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use pmg_registration::RegistrationPm;

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let router = Router::new("pmg-registration")
        .with_handler(|| RegistrationPm)
        .build()
        .expect("failed to build pm router");

    let Built::ProcessManager(pr) = router else {
        panic!("expected ProcessManager variant");
    };

    run_process_manager_server("pmg-registration", 50393, pr)
        .await
        .expect("Process manager failed");
}
