//! Unit-tier runner for poker_game.feature and sync_modes.feature.
//!
//! These scenarios describe full in-process integration through the stack
//! (aggregates, sagas, PMs, projectors) and run against `InProcessClient`
//! only. Cluster-only concerns live in `tests/tests/acceptance.rs`.
//!
//!   cargo test -p poker-tests --test poker_game_unit

#[path = "acceptance/mod.rs"]
mod acceptance;

use acceptance::world::AcceptanceWorld;
use cucumber::{World, WriterExt};

#[tokio::main]
async fn main() {
    let _ = std::any::TypeId::of::<AcceptanceWorld>();

    AcceptanceWorld::cucumber()
        .with_writer(
            cucumber::writer::Basic::stdout()
                .summarized()
                .assert_normalized(),
        )
        .run("features/example/unit/poker_game.feature")
        .await;

    AcceptanceWorld::cucumber()
        .with_writer(
            cucumber::writer::Basic::stdout()
                .summarized()
                .assert_normalized(),
        )
        .run("features/example/unit/sync_modes.feature")
        .await;
}
