use examples_proto::domain::{Card, ShoeShuffled, TableCreated};
use examples_proto::v1::table_state::Phase;
use examples_proto::v1::TableState;

use super::*;
use crate::test_support::cards;

fn main_created() -> TableCreated {
    TableCreated {
        name: "Main".into(),
        min_bet: 10,
        max_bet: 100,
        min_buy_in: 100,
        max_buy_in: 1000,
        seats: 3,
        decks: 1,
    }
}

fn shoe_one() -> ShoeShuffled {
    ShoeShuffled {
        shoe_number: 1,
        seed: 42,
        decks: 1,
        cards: cards("7♣ 3♥ K♣")
            .into_iter()
            .map(|c| Card::try_from(c).expect("a card"))
            .collect(),
    }
}

#[test]
fn eu_1440_a_created_table_is_open_with_its_configuration_and_empty_seats() {
    let mut state = TableState::default();
    apply_table_created(&mut state, &main_created());
    assert_eq!(
        state,
        TableState {
            name: "Main".into(),
            min_bet: 10,
            max_bet: 100,
            min_buy_in: 100,
            max_buy_in: 1000,
            seats: 3,
            decks: 1,
            phase: Phase::Idle.into(),
            ..TableState::default()
        }
    );
}

#[test]
fn eu_1440_a_shuffled_shoe_becomes_the_undealt_cards() {
    let mut state = TableState {
        shoe: cards("A♠"),
        ..TableState::default()
    };
    apply_shoe_shuffled(&mut state, &shoe_one());
    assert_eq!(state.shoe_number, 1);
    assert_eq!(state.shoe_seed, 42);
    assert_eq!(state.shoe, cards("7♣ 3♥ K♣"));
}

#[test]
fn eu_1442_a_table_exists_once_created() {
    let mut state = TableState::default();
    assert!(!table_exists(&state));
    apply_table_created(&mut state, &main_created());
    assert!(table_exists(&state));
}
