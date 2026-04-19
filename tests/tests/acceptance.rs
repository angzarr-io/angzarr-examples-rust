//! Cluster acceptance runner for features/example/acceptance/*.feature.
//!
//! gRPC only. Scenarios here assert on behaviour that is only meaningful
//! against a deployed cluster (wire latency, coordinator restart durability,
//! inter-coordinator routing, projector lag). Build with the feature flag:
//!
//!   cargo test -p poker-tests --test acceptance --features acceptance-test
//!
//! Default mode auto-bootstraps a `kind` cluster via
//! `tests/scripts/bootstrap-cluster.sh`. Override:
//!
//!   CLUSTER_PROVIDER=external PLAYER_URL=... TABLE_URL=... HAND_URL=... \
//!     STREAM_URL=... cargo test -p poker-tests --test acceptance \
//!     --features acceptance-test
//!
//! The in-process path is NOT supported here — these scenarios require a
//! live cluster to mean anything.

#[path = "acceptance/mod.rs"]
mod acceptance;

#[cfg(feature = "acceptance-test")]
use acceptance::world::AcceptanceWorld;
#[cfg(feature = "acceptance-test")]
use cucumber::{World, WriterExt};
#[cfg(feature = "acceptance-test")]
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

    pub fn bring_up() -> Option<ClusterGuard> {
        let provider = std::env::var("CLUSTER_PROVIDER").unwrap_or_else(|_| "kind".to_string());
        if provider == "external" {
            let urls = ["PLAYER_URL", "TABLE_URL", "HAND_URL", "STREAM_URL"];
            let missing: Vec<&&str> =
                urls.iter().filter(|k| std::env::var(k).is_err()).collect();
            if !missing.is_empty() {
                panic!(
                    "CLUSTER_PROVIDER=external requires {:?} to be set",
                    missing
                );
            }
            return None;
        }

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
                provider,
                out.status.code()
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
        if std::env::var("PLAYER_URL").is_err() {
            panic!("bootstrap completed but PLAYER_URL was not exported");
        }
        Some(ClusterGuard { provider, script })
    }
}

#[cfg(feature = "acceptance-test")]
#[tokio::main]
async fn main() {
    let _ = std::any::TypeId::of::<AcceptanceWorld>();

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
        .run("features/example/acceptance/cluster.feature")
        .await;
}

#[cfg(not(feature = "acceptance-test"))]
fn main() {
    eprintln!(
        "cluster acceptance tests require `--features acceptance-test`; \
         skipping. See tests/tests/acceptance.rs docstring for details."
    );
}
