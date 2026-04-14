//! Saga: Hand -> Table
//!
//! Reacts to HandComplete events from Hand domain.
//! Sends EndHand commands to Table domain.
//!
//! This saga is a pure translator - it receives source events and produces
//! commands without knowing destination state. The framework handles:
//! - Sequence assignment (via angzarr_deferred)
//! - Idempotency checking
//! - Delivery retry on sequence conflicts

use angzarr_client::{run_saga_server, SagaRouter};
use saga_hand_table::HandTableSagaHandler;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let router = SagaRouter::new("saga-hand-table", "hand", "table", HandTableSagaHandler);

    run_saga_server("saga-hand-table", 50012, router)
        .await
        .expect("Server failed");
}
