//! policy-table-wallet (policies.proto, decision 14.69): each settled seat's
//! round result and loyalty points, news its wallet records and never
//! refuses. [`TableWalletPolicyImpl`] implements the policy's generated
//! interface, [`domain::TableWalletPolicy`]; blackjack-embedded's generated
//! adapter carries the markup and calls it.

pub use examples_proto::domain;

mod table_wallet_policy_impl;

pub use table_wallet_policy_impl::TableWalletPolicyImpl;
