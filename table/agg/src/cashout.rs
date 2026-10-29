//! The cash-out identity (table.proto, PlayerCashedOut; EU-1461):
//! UUIDv5(namespace = the table root read as a UUID, name = "cashout/" +
//! the decimal sequence of the cash-out event), as 16 bytes, so replay and
//! redelivery always produce the same id.
//!
//! UUIDv5 (RFC 9562) is SHA-1 over the namespace and the name, here
//! written out rather than taken from the `uuid` crate: agg-table has no
//! dependency beyond the generated types, and adding one is a decision for
//! its owner (the `uuid` crate, with `v5`, is already in the workspace's
//! graph through angzarr-proto).

/// The cash-out id for the event at `sequence` in the table at
/// `table_root`.
pub fn cashout_id(table_root: &[u8; 16], sequence: u64) -> [u8; 16] {
    let _ = (table_root, sequence);
    todo!()
}

/// UUIDv5 of `name` in `namespace`.
pub fn uuid_v5(namespace: &[u8; 16], name: &[u8]) -> [u8; 16] {
    let _ = (namespace, name);
    todo!()
}

/// SHA-1 (FIPS 180-4) of `message`.
pub fn sha1(message: &[u8]) -> [u8; 20] {
    let _ = message;
    todo!()
}

#[cfg(test)]
#[path = "cashout.test.rs"]
mod cashout_tests;
