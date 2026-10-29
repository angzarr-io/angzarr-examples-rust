//! The cash-out identity (table.proto, PlayerCashedOut; EU-1461):
//! UUIDv5(namespace = the table root read as a UUID, name = "cashout/" +
//! the decimal sequence of the cash-out event), as 16 bytes, so replay and
//! redelivery always produce the same id.
//!
//! UUIDv5 (RFC 9562) comes from the `uuid` crate's `Uuid::new_v5`.

use uuid::Uuid;

/// The cash-out id for the event at `sequence` in the table at
/// `table_root`.
pub fn cashout_id(table_root: &[u8; 16], sequence: u64) -> [u8; 16] {
    uuid_v5(table_root, format!("cashout/{sequence}").as_bytes())
}

/// UUIDv5 of `name` in `namespace`.
pub fn uuid_v5(namespace: &[u8; 16], name: &[u8]) -> [u8; 16] {
    Uuid::new_v5(&Uuid::from_bytes(*namespace), name).into_bytes()
}

#[cfg(test)]
#[path = "cashout.test.rs"]
mod cashout_tests;
