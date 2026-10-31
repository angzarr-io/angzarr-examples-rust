//! The blackjack table on angzarr-embedded, angzarr's in-app runtime: the
//! table aggregate and the two policies between it and the player bot.
//!
//! - [`domain`]: the generated domain layer (examples-proto's), every
//!   command, event and typed refusal as the type the markup takes;
//! - [`adapters`]: the table's markup, generated from table.proto
//!   (`build.rs`), every handler and applier delegating to its interface,
//!   agg-table's [`table_aggregate::TableAggregate`], on its
//!   implementation, agg-table's `Table`; [`TableAggregate`] is the domain
//!   registered;
//! - [`TablePlayerPolicy`] and [`PlayerTablePolicy`]: TurnStarted to the
//!   player bot's DecideMove, and its MoveDecided back to Hit, Stand or
//!   DoubleDown, over table-player-policies;
//! - [`Player`]: the player bot's domain as the policies name it, its
//!   command set and its event set: the bot is Python, run as a child
//!   process by angzarr-embedded-host (`tests/remote_player.rs` plays a
//!   hand with it).
//!
//! [`register`] registers the table and both policies, beside the remote
//! player (`angzarr_embedded_host::run(config, register)`).

pub use agg_table::table_aggregate;
pub use examples_proto::domain;

/// The table's implementation, as the generated adapter names it.
pub mod table_aggregate_impl {
    pub use agg_table::Table as TableAggregateImpl;
}

/// The table's angzarr-embedded markup, generated from table.proto. Its
/// handlers' error is the generated layer's `Failed`, whose size is not
/// this crate's to choose.
#[allow(clippy::result_large_err)]
pub mod adapters {
    include!(concat!(env!("OUT_DIR"), "/angzarr-adapters.rs"));
}

/// The table's generated registration.
mod generated {
    include!(concat!(env!("OUT_DIR"), "/angzarr-register.rs"));
}

mod player;
mod player_table_policy;
mod table_player_policy;

pub use adapters::TableAggregate;
pub use player::Player;
pub use player_table_policy::PlayerTablePolicy;
pub use table_player_policy::TablePlayerPolicy;

/// Registers the table (its generated registration) and both policies. The
/// player domain is the child's, registered by the host.
pub fn register(registry: &mut angzarr_embedded::next::Registry) {
    generated::register(registry);
    registry.policy(TablePlayerPolicy).policy(PlayerTablePolicy);
}

#[cfg(test)]
#[path = "table_aggregate.test.rs"]
mod table_aggregate_tests;
