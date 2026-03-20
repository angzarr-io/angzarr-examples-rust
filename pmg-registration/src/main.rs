//! RegistrationOrchestrator Process Manager - coordinates tournament registration flows.
//!
//! This PM handles the synchronous cascade flow for tournament registration:
//! 1. Player emits RegistrationRequested
//! 2. PM checks Tournament state (registration open, capacity)
//! 3. PM emits EnrollPlayer command to Tournament
//! 4. Tournament emits TournamentPlayerEnrolled or TournamentEnrollmentRejected
//! 5. PM emits ConfirmRegistrationFee or ReleaseRegistrationFee to Player

use angzarr_client::{run_process_manager_server, ProcessManagerRouter};
use pmg_registration::{RegistrationPmHandler, RegistrationState};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let router =
        ProcessManagerRouter::new("pmg-registration", "pmg-registration", |_| RegistrationState::default())
            .domain("player", RegistrationPmHandler)
            .domain("tournament", RegistrationPmHandler);

    run_process_manager_server("pmg-registration", 50393, router)
        .await
        .expect("Process manager failed");
}
