//! Acceptance test runner for poker_game.feature and sync_modes.feature.
//!
//! Default mode (no env vars): InProcessClient — calls handler functions
//! directly against an in-memory event store. Fast and hermetic.
//!
//! gRPC mode (requires --features acceptance-test and PLAYER_URL env var):
//! sends commands to the deployed coordinator sidecars via tonic. Subscribes
//! to EventStreamService by per-scenario correlation_id and rebuilds
//! aggregate state by calling EventQueryService.GetEventBook.
//!
//! Before running in gRPC mode, stand up the cluster:
//!   skaffold dev
//! or
//!   kubectl apply -f standalone.yaml
//! then export PLAYER_URL / TABLE_URL / HAND_URL / STREAM_URL and invoke:
//!   cargo test -p poker-tests --test acceptance --features acceptance-test

#[path = "acceptance/mod.rs"]
mod acceptance;

use acceptance::world::AcceptanceWorld;
use cucumber::{World, WriterExt};
use futures::FutureExt;

#[tokio::main]
async fn main() {
    // Force the `steps` module to be linked so step attrs register.
    let _ = std::any::TypeId::of::<AcceptanceWorld>();

    AcceptanceWorld::cucumber()
        .before(|_feature, _rule, _scenario, world| {
            async move {
                let cid = uuid::Uuid::new_v4().to_string();
                world.correlation_id = cid.clone();
                world.client.set_correlation(&cid);
            }
            .boxed_local()
        })
        .after(|_feature, _rule, _scenario, _event, world| {
            async move {
                if let Some(w) = world {
                    w.client.close();
                }
            }
            .boxed_local()
        })
        .with_writer(
            cucumber::writer::Basic::stdout()
                .summarized()
                .assert_normalized(),
        )
        .run("features/example/acceptance/poker_game.feature")
        .await;

    AcceptanceWorld::cucumber()
        .before(|_feature, _rule, _scenario, world| {
            async move {
                let cid = uuid::Uuid::new_v4().to_string();
                world.correlation_id = cid.clone();
                world.client.set_correlation(&cid);
            }
            .boxed_local()
        })
        .after(|_feature, _rule, _scenario, _event, world| {
            async move {
                if let Some(w) = world {
                    w.client.close();
                }
            }
            .boxed_local()
        })
        .with_writer(
            cucumber::writer::Basic::stdout()
                .summarized()
                .assert_normalized(),
        )
        .run("features/example/acceptance/sync_modes.feature")
        .await;
}
