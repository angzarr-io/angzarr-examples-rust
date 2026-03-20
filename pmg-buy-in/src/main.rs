//! BuyInOrchestrator Process Manager - coordinates buy-in flows across Player ↔ Table.
//!
//! This PM handles the synchronous cascade flow for buy-ins:
//! 1. Player emits BuyInRequested
//! 2. PM checks Table state (seat availability, buy-in range)
//! 3. PM emits SeatPlayer command to Table
//! 4. Table emits PlayerSeated or SeatingRejected
//! 5. PM emits ConfirmBuyIn or ReleaseBuyIn to Player

use angzarr_client::{run_process_manager_server, ProcessManagerRouter};
use pmg_buy_in::{BuyInPmHandler, BuyInState};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let router = ProcessManagerRouter::new("pmg-buy-in", "pmg-buy-in", |_| BuyInState::default())
        .domain("player", BuyInPmHandler)
        .domain("table", BuyInPmHandler);

    run_process_manager_server("pmg-buy-in", 50392, router)
        .await
        .expect("Process manager failed");
}
