//! The blackjack table aggregate: [`Table`], implementing the table's
//! generated interface, [`domain::TableStateInterface`] (examples-proto's
//! generated domain layer's, one method per command and per event), over
//! that domain layer ([`domain`]). Its state is the
//! spec's `TableState`; each refusal is a typed message of table.proto's.
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

pub mod cashout;
mod create_table;
mod failure;
mod hand;
pub mod shoe;
mod state;
mod table;
#[cfg(any(test, feature = "test-support"))]
pub mod test_support;

pub use create_table::create_table;
pub use examples_proto::domain;
pub use hand::{value, Value};
pub use state::{apply_shoe_shuffled, apply_table_created};
pub use table::Table;
