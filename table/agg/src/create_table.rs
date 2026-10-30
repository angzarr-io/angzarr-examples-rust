//! CreateTable: open the table and shuffle its first shoe (AHR-1, AHR-3,
//! AHR-5, AHR-13).

use examples_proto::v1::{CreateTable, ShoeShuffled, TableCreated, TableState};

use crate::rejection::{Rejection, RejectionCode};
use crate::shoe;
use crate::state::{table_exists, TableEvent};

/// Seats a table may have (AHR-1).
const SEATS: std::ops::RangeInclusive<i32> = 1..=7;
/// Decks a shoe may hold (AHR-3).
const DECKS: std::ops::RangeInclusive<i32> = 1..=8;

/// Decide CreateTable against `state`: TableCreated then ShoeShuffled for
/// shoe 1, shuffled from the command's seed.
///
/// # Errors
/// [`TableAlreadyExists`](crate::RejectionCode::TableAlreadyExists) when the
/// table exists; [`InvalidTableConfig`](crate::RejectionCode::InvalidTableConfig)
/// when the configuration breaks the house rules: 1..=7 seats, 1..=8 decks,
/// `0 < min_bet <= max_bet` with both even, `0 < min_buy_in <= max_buy_in`
/// and `min_bet <= min_buy_in`.
pub fn create_table(state: &TableState, cmd: &CreateTable) -> Result<Vec<TableEvent>, Rejection> {
    if table_exists(state) {
        return Err(
            Rejection::new(RejectionCode::TableAlreadyExists).arg("name", state.name.as_str())
        );
    }
    let decks = valid_decks(cmd)?;
    Ok(vec![
        TableEvent::TableCreated(TableCreated {
            name: cmd.name.clone(),
            min_bet: cmd.min_bet,
            max_bet: cmd.max_bet,
            min_buy_in: cmd.min_buy_in,
            max_buy_in: cmd.max_buy_in,
            seats: cmd.seats,
            decks: cmd.decks,
        }),
        TableEvent::ShoeShuffled(ShoeShuffled {
            shoe_number: 1,
            seed: cmd.shoe_seed,
            decks: cmd.decks,
            cards: shoe::shuffle(cmd.shoe_seed, decks),
        }),
    ])
}

/// Check the configuration against the house rules and return the deck
/// count as the shoe takes it.
fn valid_decks(cmd: &CreateTable) -> Result<u32, Rejection> {
    // The rule broken, by a key: `seats` 1 to 7, `decks` 1 to 8,
    // `bet_limits` 0 < min_bet <= max_bet, `even_bets` both even,
    // `buy_in_limits` min_bet <= min_buy_in <= max_buy_in.
    let invalid = |rule: &str| Rejection::new(RejectionCode::InvalidTableConfig).arg("rule", rule);
    if !SEATS.contains(&cmd.seats) {
        return Err(invalid("seats"));
    }
    if !DECKS.contains(&cmd.decks) {
        return Err(invalid("decks"));
    }
    if cmd.min_bet <= 0 || cmd.min_bet > cmd.max_bet {
        return Err(invalid("bet_limits"));
    }
    if cmd.min_bet % 2 != 0 || cmd.max_bet % 2 != 0 {
        return Err(invalid("even_bets"));
    }
    // min_buy_in > 0 follows from 0 < min_bet <= min_buy_in.
    if cmd.min_buy_in < cmd.min_bet || cmd.min_buy_in > cmd.max_buy_in {
        return Err(invalid("buy_in_limits"));
    }
    // Positive, checked above.
    Ok(cmd.decks.unsigned_abs())
}

#[cfg(test)]
#[path = "create_table.test.rs"]
mod create_table_tests;
