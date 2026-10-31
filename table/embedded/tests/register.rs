//! What `register` registers and how the remote player is named, without
//! the Python bot (`tests/remote_player.rs`, ignored, plays a hand with it).

use angzarr_embedded::next::markup::{CommandTarget, EventSource};
use angzarr_embedded::next::Registry;
use table_embedded::{register, Player};

/// The table, its generated registration, and both policies, in order;
/// the player is no domain here, the host's child.
#[test]
fn register_registers_the_table_and_both_policies() {
    let mut registry = Registry::new();
    register(&mut registry);
    assert_eq!(registry.domains().collect::<Vec<_>>(), ["table"]);
    assert_eq!(
        registry.policy_names().collect::<Vec<_>>(),
        ["policy-table-player", "policy-player-table"]
    );
}

/// The player is sent DecideMove and read for MoveDecided, by full name.
#[test]
fn the_player_is_named_by_its_command_and_event() {
    assert_eq!(<Player as CommandTarget>::DOMAIN, "player");
    assert_eq!(
        <Player as CommandTarget>::command_types(),
        ["io.angzarr.examples.v1.DecideMove"]
    );
    assert_eq!(<Player as EventSource>::DOMAIN, "player");
    assert_eq!(
        <Player as EventSource>::event_types(),
        ["io.angzarr.examples.v1.MoveDecided"]
    );
}
