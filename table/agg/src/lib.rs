//! The blackjack table aggregate's domain logic, framework-free.
//!
//! Everything here is a pure function over the generated
//! `io.angzarr.examples.v1` types, so a handler adapter can wire it into a
//! command-handler framework without rewriting it:
//!
//! - a decision takes the current [`TableState`] and a command and returns
//!   the events it emits, or a [`Rejection`] ([`create_table`]);
//! - an applier folds one event into the state ([`apply`], or the
//!   per-event [`apply_table_created`] and [`apply_shoe_shuffled`]);
//! - [`shoe`] is the deterministic shuffle pinned in `cards.proto`;
//! - [`TableLogic`] is all of it behind one trait, one method per command
//!   and per event, which [`Table`] implements over `TableState`: what a
//!   harness adapter delegates to.
//!
//! [`TableState`]: examples_proto::v1::TableState

pub mod cashout;
mod create_table;
mod hand;
mod rejection;
pub mod shoe;
mod state;
mod table;
#[cfg(any(test, feature = "test-support"))]
pub mod test_support;

pub use create_table::create_table;
pub use hand::{value, Value};
pub use rejection::{Rejection, RejectionCode, StatusCode};
pub use state::{apply, apply_shoe_shuffled, apply_table_created, TableEvent};
pub use table::{AfterAction, ShoeRanOut, Table, TableLogic};
