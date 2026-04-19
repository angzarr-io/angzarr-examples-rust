//! Acceptance test support modules.
//!
//! Default mode uses `InProcessClient` — calls aggregate handlers directly
//! against an in-memory event store. No cluster required.
//!
//! gRPC mode exercises a deployed cluster end-to-end. Bring it up first:
//!
//!   skaffold dev
//!   # or
//!   kubectl apply -f standalone.yaml
//!
//! Then export the coordinator URLs and run with the feature flag:
//!
//!   cargo test -p poker-tests --test acceptance --features acceptance-test \
//!     PLAYER_URL=http://localhost:1310 \
//!     TABLE_URL=http://localhost:1311 \
//!     HAND_URL=http://localhost:1312 \
//!     STREAM_URL=http://localhost:1340
//!
//! In gRPC mode each scenario allocates a fresh `correlation_id`, subscribes
//! to `EventStreamService` filtered by that id, and rebuilds aggregate state
//! via `EventQueryService.GetEventBook` for state assertions. `within N
//! seconds:` steps poll the received-events buffer until the expected
//! event-types appear or the timeout expires.

pub mod command_client;
pub mod steps;
pub mod world;
