//! The table's state, folded from its events.
//!
//! The state is the spec's `TableState` message itself. Appliers never
//! fail: an event is a fact the table already decided.

use examples_proto::domain::{ShoeShuffled, TableCreated};
use examples_proto::v1::{table_state::Phase, Card, TableState};

/// Whether the table has been created.
pub(crate) fn table_exists(state: &TableState) -> bool {
    state.phase() != Phase::Unspecified
}

/// The table opens with its configuration, no one seated and no wager.
pub fn apply_table_created(state: &mut TableState, event: &TableCreated) {
    state.name.clone_from(&event.name);
    state.min_bet = event.min_bet;
    state.max_bet = event.max_bet;
    state.min_buy_in = event.min_buy_in;
    state.max_buy_in = event.max_buy_in;
    state.seats = event.seats;
    state.decks = event.decks;
    state.set_phase(Phase::Idle);
}

/// A new shoe replaces the undealt cards.
pub fn apply_shoe_shuffled(state: &mut TableState, event: &ShoeShuffled) {
    state.shoe_number = event.shoe_number;
    state.shoe_seed = event.seed;
    state.shoe = event.cards.iter().cloned().map(Card::from).collect();
}

#[cfg(test)]
#[path = "state.test.rs"]
mod state_tests;
