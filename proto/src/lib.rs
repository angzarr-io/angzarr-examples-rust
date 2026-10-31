//! The blackjack example's generated layer (`io.angzarr.examples.v1`):
//!
//! - [`v1`]: the wire types, prost's, with angzarr's settings: cards, the
//!   table domain and the player bot (`player.proto`, whose DecideMove and
//!   MoveDecided the table's policies translate);
//! - [`domain`]: the markup domain layer over them: each command, event and
//!   typed refusal (`TableRefused`, `SeatNotConfirmable`) as the domain type
//!   the markup takes, each component's command and event sets, and the
//!   handlers' context and failure types.
//!
//! Module nesting mirrors the proto package so prost's `super::` references
//! resolve; [`v1`] is the short path.

pub mod io {
    pub mod angzarr {
        pub mod examples {
            pub mod v1 {
                include!(concat!(env!("OUT_DIR"), "/io.angzarr.examples.v1.rs"));
            }
        }
    }
}

pub use io::angzarr::examples::v1;

pub mod domain {
    include!(concat!(
        env!("OUT_DIR"),
        "/io.angzarr.examples.v1.domain.rs"
    ));
}
