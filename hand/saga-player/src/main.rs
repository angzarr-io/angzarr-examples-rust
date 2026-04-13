//! Saga: Hand -> Player
//!
//! Reacts to PotAwarded events from Hand domain.
//! Sends DepositFunds commands to Player domain.
//!
//! This saga is a pure translator - it receives source events and produces
//! commands without knowing destination state. The framework handles:
//! - Sequence assignment (via angzarr_deferred)
//! - Idempotency checking
//! - Delivery retry on sequence conflicts

use angzarr_client::{run_saga_server, SagaRouter};
use saga_hand_player::HandPlayerSagaHandler;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let router = SagaRouter::new("saga-hand-player", "hand", HandPlayerSagaHandler);

    run_saga_server("saga-hand-player", 50014, router)
        .await
        .expect("Server failed");
}
