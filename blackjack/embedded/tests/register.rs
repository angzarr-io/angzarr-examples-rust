//! What `register` registers: the table, the wallet and the buy-in process
//! manager, and the five policies between them; no player bot (a table with
//! buy-ins is not seated by a driver, bot_play.feature).

use angzarr_embedded::Registry;
use blackjack_embedded::register;

#[test]
fn register_registers_the_three_domains_and_their_five_policies() {
    let mut registry = Registry::new();
    register(&mut registry).unwrap();
    let catalog = registry.catalog();
    assert_eq!(
        catalog.domains().collect::<Vec<_>>(),
        ["table", "wallet", "buy-in"]
    );
    assert_eq!(
        catalog.policy_names().collect::<Vec<_>>(),
        [
            "policy-buy-in-wallet",
            "policy-buy-in-table",
            "policy-table-buy-in",
            "policy-wallet-buy-in",
            "policy-table-wallet",
        ]
    );
}

/// The wallet reads its older deposits through its upcaster: the
/// registry's check finds its chain whole.
#[test]
fn the_wallets_upcasting_chain_checks() {
    let mut registry = Registry::new();
    register(&mut registry).unwrap();
    assert!(registry.upcaster("wallet").is_some());
    registry.check().expect("every upcasting chain finishes");
}
