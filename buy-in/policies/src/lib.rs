//! The buy-in's four policies (buy_in.proto), each implementing its
//! generated interface over the generated domain layer:
//!
//! - [`TableBuyInPolicyImpl`] and [`WalletBuyInPolicyImpl`]: what the table
//!   and the wallet report, as facts for the buy-in of the conversation
//!   their event belongs to (the PM's root is that correlation);
//! - [`BuyInWalletPolicyImpl`] and [`BuyInTablePolicyImpl`]: the buy-in's
//!   steps, as what the wallet and the table declare (HoldFunds and
//!   ConfirmSeat commands; CaptureFunds, ReleaseHold and ReleaseSeat facts),
//!   every refusal translated (angzarr decision 14.76): HoldFunds' into the
//!   buy-in's FundsHoldRefused, ConfirmSeat's into its
//!   SeatConfirmationRefused, any command they never send a bug.
//!
//! blackjack-embedded's generated adapters carry the markup and call them.

pub use examples_proto::domain;

mod buy_in_table_policy_impl;
mod buy_in_wallet_policy_impl;
mod never_sent;
mod table_buy_in_policy_impl;
mod wallet_buy_in_policy_impl;

pub use buy_in_table_policy_impl::BuyInTablePolicyImpl;
pub use buy_in_wallet_policy_impl::{BuyInWalletPolicyImpl, HOLD_REFUSED_UNTYPED};
pub use never_sent::NEVER_SENT;
pub use table_buy_in_policy_impl::TableBuyInPolicyImpl;
pub use wallet_buy_in_policy_impl::WalletBuyInPolicyImpl;
