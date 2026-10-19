//! Generated protobuf types for the blackjack example
//! (`io.angzarr.examples.v1`): cards and the table domain.
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
