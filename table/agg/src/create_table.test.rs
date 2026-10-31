use examples_proto::domain::{Card, CreateTable, Failed, ShoeShuffled, TableCreated};
use examples_proto::v1::{self, table_state::Phase, TableState};

use super::*;
use crate::state::{apply_shoe_shuffled, apply_table_created};
use crate::test_support::cards;

/// Table "Main" from `table.feature`: 3 seats, one deck, bets 10 to 100,
/// buy-ins 100 to 1000, shoe seed 42.
fn create_main() -> CreateTable {
    CreateTable {
        name: "Main".into(),
        min_bet: 10,
        max_bet: 100,
        min_buy_in: 100,
        max_buy_in: 1000,
        seats: 3,
        decks: 1,
        shoe_seed: 42,
    }
}

fn fold((created, shuffled): &(TableCreated, ShoeShuffled)) -> TableState {
    let mut state = TableState::default();
    apply_table_created(&mut state, created);
    apply_shoe_shuffled(&mut state, shuffled);
    state
}

/// The cards as the state holds them.
fn wire(cards: &[Card]) -> Vec<v1::Card> {
    cards.iter().cloned().map(v1::Card::from).collect()
}

/// The refusal a decision failed with.
#[track_caller]
fn refusal<T: std::fmt::Debug>(decided: Result<T, Failed<TableRefused>>) -> TableRefused {
    match decided {
        Err(Failed::Rejected { rejection, .. }) => rejection,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn eu_1440_creating_a_table_shuffles_its_first_shoe_from_the_seed() {
    let (created, shuffled) =
        create_table(&TableState::default(), &create_main()).expect("created");

    assert_eq!(
        created,
        TableCreated {
            name: "Main".into(),
            min_bet: 10,
            max_bet: 100,
            min_buy_in: 100,
            max_buy_in: 1000,
            seats: 3,
            decks: 1,
        }
    );
    assert_eq!(
        (shuffled.shoe_number, shuffled.seed, shuffled.decks),
        (1, 42, 1)
    );
    assert_eq!(wire(&shuffled.cards), crate::shoe::shuffle(42, 1));
    assert_eq!(
        wire(&shuffled.cards)[..12],
        cards("7♣ 3♥ K♣ A♠ 2♠ A♣ A♥ K♦ 10♦ 5♣ 8♠ J♥")[..]
    );
}

#[test]
fn eu_1440_the_created_table_is_open_with_empty_seats_and_shoe_one() {
    let state = fold(&create_table(&TableState::default(), &create_main()).expect("created"));

    assert_eq!(state.name, "Main");
    assert_eq!(state.seats, 3);
    assert_eq!(state.phase(), Phase::Idle);
    assert!(state.seated.is_empty());
    assert_eq!((state.shoe_number, state.shoe_seed), (1, 42));
    assert_eq!(
        state.shoe[..12],
        cards("7♣ 3♥ K♣ A♠ 2♠ A♣ A♥ K♦ 10♦ 5♣ 8♠ J♥")[..]
    );
}

#[test]
fn eu_1440_the_shoe_holds_every_deck_the_table_plays_with() {
    let cmd = CreateTable {
        decks: 6,
        shoe_seed: 7,
        ..create_main()
    };
    let (_, ShoeShuffled { decks, cards, .. }) =
        create_table(&TableState::default(), &cmd).expect("created");
    assert_eq!(decks, 6);
    assert_eq!(wire(&cards), crate::shoe::shuffle(7, 6));
}

/// EU-1441 examples: seats, decks, min_bet, max_bet, min_buy_in, max_buy_in.
const INVALID_CONFIGS: &[(i32, i32, i64, i64, i64, i64)] = &[
    (0, 1, 10, 100, 100, 1000),
    (8, 1, 10, 100, 100, 1000),
    (3, 0, 10, 100, 100, 1000),
    (3, 9, 10, 100, 100, 1000),
    (3, 1, 100, 10, 100, 1000),
    (3, 1, 11, 100, 100, 1000),
    (3, 1, 10, 100, 1000, 100),
    (3, 1, 10, 100, 5, 1000),
];

/// House-rule boundaries beyond the EU-1441 examples (`table.proto`
/// CreateTable): bets and buy-ins must be positive, and max_bet even.
const INVALID_BOUNDARIES: &[(i32, i32, i64, i64, i64, i64)] = &[
    (3, 1, 0, 100, 100, 1000),
    (3, 1, -10, 100, 100, 1000),
    (3, 1, 10, 101, 100, 1000),
    (3, 1, 10, 100, 0, 0),
    (-1, 1, 10, 100, 100, 1000),
    (3, -1, 10, 100, 100, 1000),
];

fn with_config(row: (i32, i32, i64, i64, i64, i64)) -> CreateTable {
    let (seats, decks, min_bet, max_bet, min_buy_in, max_buy_in) = row;
    CreateTable {
        seats,
        decks,
        min_bet,
        max_bet,
        min_buy_in,
        max_buy_in,
        ..create_main()
    }
}

#[test]
fn eu_1441_a_table_configuration_outside_the_house_rules_is_refused() {
    for &row in INVALID_CONFIGS.iter().chain(INVALID_BOUNDARIES) {
        let refused = refusal(create_table(&TableState::default(), &with_config(row)));
        assert_eq!(
            refused.reason,
            TableRefusedReason::InvalidTableConfig,
            "config {row:?}"
        );
    }
}

#[test]
fn eu_1441_a_configuration_on_the_house_rule_limits_is_accepted() {
    for &row in &[
        (1, 1, 10, 100, 100, 1000),
        (7, 8, 10, 100, 100, 1000),
        (3, 1, 2, 2, 100, 1000),
        (3, 1, 10, 100, 500, 500),
        (3, 1, 100, 100, 100, 1000),
    ] {
        let created = create_table(&TableState::default(), &with_config(row));
        assert!(created.is_ok(), "config {row:?}: {created:?}");
    }
}

#[test]
fn eu_1442_a_table_cannot_be_created_twice() {
    let existing = fold(&create_table(&TableState::default(), &create_main()).expect("created"));

    let refused = refusal(create_table(&existing, &create_main()));

    assert_eq!(
        refused,
        TableRefused {
            name: Some("Main".into()),
            ..crate::failure::refused(TableRefusedReason::TableAlreadyExists)
        }
    );
}

/// Decision 14.37: a rejection is a key and its args, never English. A
/// broken house rule is named by a key of its own.
#[test]
fn eu_1441_an_invalid_configuration_names_the_rule_it_breaks_by_a_key() {
    for &(row, rule) in &[
        ((0, 1, 10, 100, 100, 1000), "seats"),
        ((3, 9, 10, 100, 100, 1000), "decks"),
        ((3, 1, 0, 100, 100, 1000), "bet_limits"),
        ((3, 1, 11, 101, 100, 1000), "even_bets"),
        ((3, 1, 10, 100, 2, 1000), "buy_in_limits"),
    ] {
        let refused = refusal(create_table(&TableState::default(), &with_config(row)));
        assert_eq!(
            refused,
            TableRefused {
                rule: Some(rule.into()),
                ..crate::failure::refused(TableRefusedReason::InvalidTableConfig)
            },
            "config {row:?}"
        );
    }
}
