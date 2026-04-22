//! Acceptance test runner for poker_game.feature and sync_modes.feature.
//!
//! Default mode (no env vars): InProcessClient — calls handler functions
//! directly against an in-memory event store. Fast and hermetic.
//!
//! gRPC mode (requires --features acceptance-test and PLAYER_URL env var):
//! sends commands to the deployed coordinator sidecars via tonic.

#[path = "acceptance/mod.rs"]
mod acceptance;

use acceptance::world::AcceptanceWorld;
use cucumber::{World, WriterExt};

#[tokio::main]
async fn main() {
    // Force the `steps` module to be linked so step attrs register.
    let _ = std::any::TypeId::of::<AcceptanceWorld>();

    AcceptanceWorld::cucumber()
        .with_writer(
            cucumber::writer::Basic::stdout()
                .summarized()
                .assert_normalized(),
        )
        .run("features/example/acceptance/poker_game.feature")
        .await;

    AcceptanceWorld::cucumber()
        .with_writer(
            cucumber::writer::Basic::stdout()
                .summarized()
                .assert_normalized(),
        )
        .run("features/example/acceptance/sync_modes.feature")
        .await;
}
