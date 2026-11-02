//! The buy-in process manager (buy_in.proto): a buy-in holds the seat, then
//! the funds, then confirms the seat, then spends the funds, tracked under
//! the conversation it started in. [`BuyInProcessManagerImpl`] implements
//! the PM's generated interface, [`domain::BuyInProcessManager`], over the
//! generated domain layer; its state is the spec's `BuyInState`.
//!
//! blackjack-embedded's generated adapter carries the markup and calls the
//! interface on [`BuyInProcessManagerImpl`].

pub use examples_proto::domain;
pub use examples_proto::v1 as wire;

mod buy_in_process_manager_impl;

pub use buy_in_process_manager_impl::{
    compensation, BuyInProcessManagerImpl, Failure, HOLD_FUNDS, HOLD_SEAT, LATE_HOLD,
};
