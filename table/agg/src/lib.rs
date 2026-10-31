//! The blackjack table aggregate: [`Table`], implementing the table's
//! generated interface, [`table_aggregate::TableAggregate`] (scaffolded by
//! `angzarr scaffold rust`, one method per command and per event), over the
//! generated domain layer ([`domain`], examples-proto's). Its state is the
//! spec's `TableState`; its refusal, the spec's `TableRefused`, a reason
//! code from table.proto and the values it names.
//!
//! - [`create_table`]: CreateTable's decision, TableCreated and the first
//!   ShoeShuffled, and the two appliers that open the table
//!   ([`apply_table_created`], [`apply_shoe_shuffled`]);
//! - [`shoe`]: the deterministic shuffle pinned in `cards.proto`;
//! - [`value`]: a hand's value;
//! - [`cashout`]: the cash-out id.
//!
//! table-embedded's generated adapter carries the markup and calls the
//! interface on [`Table`].

// The interface's error, `Failed<TableRefused>`, is the generated layer's
// (angzarr-embedded's `Failed` over the spec's refusal): its size is not
// this crate's to choose.
#![allow(clippy::result_large_err)]

pub mod cashout;
mod create_table;
mod failure;
mod hand;
pub mod shoe;
mod state;
mod table;
pub mod table_aggregate;
#[cfg(any(test, feature = "test-support"))]
pub mod test_support;

pub use create_table::create_table;
pub use examples_proto::domain;
pub use hand::{value, Value};
pub use state::{apply_shoe_shuffled, apply_table_created};
pub use table::Table;
