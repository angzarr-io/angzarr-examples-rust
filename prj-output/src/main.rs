//! Projector: Output gRPC server.

use angzarr_client::router::{Built, Router};
use angzarr_client::{run_projector_server, ProjectorHandler};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use prj_output::OutputProjector;

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    println!("Starting Output projector");

    let router = Router::new("prj-output")
        .with_handler(|| OutputProjector)
        .build()
        .expect("failed to build projector router");

    let Built::Projector(pr) = router else {
        panic!("expected Projector variant");
    };

    run_projector_server("output", 50391, ProjectorHandler::new(pr))
        .await
        .expect("Server failed");
}
