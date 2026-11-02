//! The blackjack table with wallets and buy-ins on angzarr-embedded,
//! angzarr's in-app runtime.
//!
//! - [`domain`]: the generated domain layer (examples-proto's);
//! - [`adapters`]: the markup of every component, generated from the spec's
//!   protos (`build.rs`), each handler delegating to its interface on its
//!   implementation: the table (agg-table), the wallet (agg-wallet), the
//!   buy-in process manager (pm-buy-in), its four policies
//!   (buy-in-policies) and policy-table-wallet (wallet-policies).
//!
//! [`register`] registers them all. The player bot and its two policies
//! are table-embedded's: a table played by bots is seated by a driver, with
//! no buy-in (bot_play.feature).

pub use examples_proto::domain;

/// Each component's implementation, as the generated adapters name it.
pub mod table_aggregate_impl {
    pub use agg_table::Table as TableAggregateImpl;
}
pub mod wallet_aggregate_impl {
    pub use agg_wallet::WalletAggregateImpl;
}
pub mod buy_in_process_manager_impl {
    pub use pm_buy_in::BuyInProcessManagerImpl;
}
pub mod buy_in_wallet_policy_impl {
    pub use buy_in_policies::BuyInWalletPolicyImpl;
}
pub mod buy_in_table_policy_impl {
    pub use buy_in_policies::BuyInTablePolicyImpl;
}
pub mod table_buy_in_policy_impl {
    pub use buy_in_policies::TableBuyInPolicyImpl;
}
pub mod wallet_buy_in_policy_impl {
    pub use buy_in_policies::WalletBuyInPolicyImpl;
}
pub mod table_wallet_policy_impl {
    pub use wallet_policies::TableWalletPolicyImpl;
}

// The components' angzarr-embedded markup, generated from the protos: the
// module `adapters`, carrying its own allowances.
include!(concat!(env!("OUT_DIR"), "/angzarr-adapters.rs"));

/// The generated registration.
mod generated {
    include!(concat!(env!("OUT_DIR"), "/angzarr-register.rs"));
}

pub use generated::register;

#[cfg(test)]
#[path = "buy_in.test.rs"]
mod buy_in_tests;
#[cfg(test)]
#[path = "wallet_aggregate.test.rs"]
mod wallet_aggregate_tests;
