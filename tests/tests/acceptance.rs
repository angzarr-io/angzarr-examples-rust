//! Acceptance test runner for poker_game.feature and sync_modes.feature.
//!
//! Default mode (no env vars): InProcessClient — calls handler functions
//! directly against an in-memory event store. Fast and hermetic.
//!
//! gRPC mode (requires --features acceptance-test): bootstraps a cluster
//! (kind by default; override via CLUSTER_PROVIDER=external|skaffold) by
//! shelling out to `tests/scripts/bootstrap-cluster.sh`, then sends commands
//! to the deployed coordinator sidecars via tonic. Subscribes to
//! EventStreamService by per-scenario correlation_id and rebuilds aggregate
//! state by calling EventQueryService.GetEventBook.
//!
//!   cargo test -p poker-tests --test acceptance --features acceptance-test
//!
//! To run against an already-running cluster, skip bootstrap:
//!   CLUSTER_PROVIDER=external PLAYER_URL=... TABLE_URL=... HAND_URL=... \
//!     STREAM_URL=... cargo test ... --features acceptance-test

#[path = "acceptance/mod.rs"]
mod acceptance;

use acceptance::world::AcceptanceWorld;
use cucumber::{World, WriterExt};
use futures::FutureExt;

#[cfg(feature = "acceptance-test")]
mod bootstrap {
    use std::process::{Command, Stdio};

    pub struct ClusterGuard {
        provider: String,
        script: String,
    }

    impl Drop for ClusterGuard {
        fn drop(&mut self) {
            let status = Command::new("bash")
                .arg(&self.script)
                .arg(&self.provider)
                .arg("down")
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .status();
            if let Err(e) = status {
                eprintln!("[bootstrap] teardown failed: {e}");
            }
        }
    }

    /// Shell out to bootstrap-cluster.sh, parse KEY=VALUE lines from stdout,
    /// and set them as process env vars. Returns a guard that tears down on
    /// drop (unless CLUSTER_KEEP=1).
    pub fn bring_up() -> Option<ClusterGuard> {
        let provider = std::env::var("CLUSTER_PROVIDER").unwrap_or_else(|_| "kind".to_string());
        let script = "tests/scripts/bootstrap-cluster.sh".to_string();

        eprintln!("[bootstrap] provider={provider} -> {script} up");
        let out = Command::new("bash")
            .arg(&script)
            .arg(&provider)
            .arg("up")
            .stderr(Stdio::inherit())
            .output()
            .unwrap_or_else(|e| panic!("failed to exec bootstrap script: {e}"));
        if !out.status.success() {
            panic!(
                "bootstrap {} up failed (exit={:?})",
                provider, out.status.code()
            );
        }
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            if let Some((k, v)) = line.split_once('=') {
                let k = k.trim();
                let v = v.trim();
                if !k.is_empty() {
                    std::env::set_var(k, v);
                }
            }
        }
        Some(ClusterGuard { provider, script })
    }
}

#[tokio::main]
async fn main() {
    // Force the `steps` module to be linked so step attrs register.
    let _ = std::any::TypeId::of::<AcceptanceWorld>();

    // In gRPC mode, bootstrap the cluster before running features. The guard
    // tears it down on drop. In default (InProcess) mode this is a no-op.
    #[cfg(feature = "acceptance-test")]
    let _cluster_guard = bootstrap::bring_up();

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
