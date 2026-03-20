//! RebuyOrchestrator Process Manager - coordinates rebuy flows.
//!
//! This PM handles the synchronous cascade flow for tournament rebuys:
//! 1. Player emits RebuyRequested
//! 2. PM checks Tournament state (rebuy window, eligibility) + Table state (seat)
//! 3. PM emits ProcessRebuy command to Tournament
//! 4. Tournament emits RebuyProcessed or RebuyDenied
//! 5. If approved, PM emits AddRebuyChips to Table
//! 6. PM emits ConfirmRebuyFee or ReleaseRebuyFee to Player

use angzarr_client::{run_process_manager_server, ProcessManagerRouter};
use pmg_rebuy::{RebuyPmHandler, RebuyState};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let router = ProcessManagerRouter::new("pmg-rebuy", "pmg-rebuy", |_| RebuyState::default())
        .domain("player", RebuyPmHandler)
        .domain("tournament", RebuyPmHandler)
        .domain("table", RebuyPmHandler);

    run_process_manager_server("pmg-rebuy", 50394, router)
        .await
        .expect("Process manager failed");
}
