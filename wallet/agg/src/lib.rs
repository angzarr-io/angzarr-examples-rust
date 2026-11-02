//! The blackjack wallet aggregate (wallet.proto, decision 14.38): every chip
//! that is not on a table. [`WalletAggregateImpl`] implements the wallet's
//! generated interface, [`domain::WalletAggregate`] (examples-proto's
//! generated domain layer's, one method per command, fact and event), over
//! that domain layer; its state is the spec's `WalletState`.
//!
//! blackjack-embedded's generated adapter carries the markup and calls the
//! interface on [`WalletAggregateImpl`].

// The interface's errors, `Failed<..>`, are the generated layer's: their
// size is not this crate's to choose.
#![allow(clippy::result_large_err)]

pub use examples_proto::domain;
pub use examples_proto::v1 as wire;

pub mod reason;
pub mod upcast;
mod wallet_aggregate_impl;

pub use wallet_aggregate_impl::WalletAggregateImpl;
