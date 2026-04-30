//! Pretty Output Projector gRPC server.

use angzarr_client::router::{Built, Router};
use angzarr_client::run_projector_server;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use prj_pretty_output::PrettyOutputProjector;

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    println!("Starting Pretty Output projector");

    // The projector captures a SQLite connection + stdout sink at
    // construction; we build a single instance and hand it to the runtime
    // via a factory that returns a *fresh* instance per dispatch. The
    // runtime documents that projector factories run at most twice per
    // dispatch, so this is fine even though `from_env()` opens the DB.
    let router = Router::new("prj-pretty-output")
        .with_handler(PrettyOutputProjector::from_env)
        .build()
        .expect("failed to build projector router");

    let Built::Projector(pr) = router else {
        panic!("expected Projector variant");
    };

    run_projector_server(pr, 50391)
        .await
        .expect("Server failed");
}
