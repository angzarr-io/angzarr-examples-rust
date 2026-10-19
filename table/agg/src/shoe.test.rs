use std::collections::HashMap;

use super::*;
use crate::test_support::{card, cards};

/// EU-1520 examples: seed, decks, the first twelve cards, the last card.
const SEED_VECTORS: &[(u64, u32, &str, &str)] = &[
    (1, 1, "7♦ 9♣ 4♦ 8♥ 8♦ 5♠ J♦ 6♦ 9♠ J♣ 5♦ K♦", "7♠"),
    (42, 1, "7♣ 3♥ K♣ A♠ 2♠ A♣ A♥ K♦ 10♦ 5♣ 8♠ J♥", "10♣"),
    (7, 1, "4♦ 5♣ J♥ 3♠ 4♥ 10♥ K♠ 6♦ 4♠ 7♥ 9♦ 2♣", "Q♣"),
    (1, 6, "6♠ A♣ 7♥ 3♣ 6♠ J♥ 3♥ Q♠ Q♣ 6♥ 6♦ 7♦", "7♠"),
    (42, 6, "7♠ K♠ 2♦ Q♣ 7♣ 7♥ 3♣ 7♥ 6♣ 7♦ 7♠ 4♦", "10♣"),
    (7, 6, "A♦ K♣ 4♦ 3♦ 9♣ 6♣ K♦ J♠ Q♠ J♦ A♥ 8♣", "Q♣"),
];

#[test]
fn eu_1520_a_seed_always_produces_the_same_shoe() {
    for &(seed, decks, first_twelve, last) in SEED_VECTORS {
        let shoe = shuffle(seed, decks);
        assert_eq!(
            shoe[..12],
            cards(first_twelve)[..],
            "first 12 cards, seed {seed}, {decks} decks"
        );
        assert_eq!(
            shoe.last(),
            Some(&card(last)),
            "last card, seed {seed}, {decks} decks"
        );
    }
}

#[test]
fn eu_1521_the_complete_one_deck_shoe_for_seed_1() {
    let expected = cards(concat!(
        "7♦ 9♣ 4♦ 8♥ 8♦ 5♠ J♦ 6♦ 9♠ J♣ 5♦ K♦ 3♣ ",
        "2♦ J♥ 5♥ 7♥ 4♥ 2♣ A♦ 6♠ K♣ K♥ 10♥ 10♠ J♠ ",
        "10♦ 8♠ Q♣ 4♠ 6♣ 7♣ A♠ 5♣ A♥ K♠ 6♥ Q♠ Q♥ ",
        "Q♦ 9♦ 3♦ 3♠ A♣ 4♣ 2♥ 8♣ 10♣ 3♥ 2♠ 9♥ 7♠",
    ));
    assert_eq!(shuffle(1, 1), expected);
}

/// Counts each distinct card in `shoe`.
fn census(shoe: &[Card]) -> HashMap<(u32, i32), u32> {
    let mut counts = HashMap::new();
    for c in shoe {
        *counts.entry((c.rank, c.suit)).or_insert(0) += 1;
    }
    counts
}

#[test]
fn eu_1522_a_shuffled_shoe_holds_every_card_once_per_deck() {
    for &(seed, decks, len) in &[(1u64, 1u32, 52usize), (42, 6, 312), (7, 8, 416)] {
        let shoe = shuffle(seed, decks);
        assert_eq!(shoe.len(), len, "seed {seed}, {decks} decks");
        let counts = census(&shoe);
        assert_eq!(counts.len(), 52, "distinct cards, seed {seed}");
        for rank in 1..=13 {
            for suit in [Suit::Clubs, Suit::Diamonds, Suit::Hearts, Suit::Spades] {
                assert_eq!(
                    counts.get(&(rank, suit as i32)),
                    Some(&decks),
                    "rank {rank} {suit:?}, seed {seed}, {decks} decks"
                );
            }
        }
    }
}

#[test]
fn eu_1522_the_fresh_shoe_is_the_canonical_deck_repeated() {
    let shoe = fresh_shoe(2);
    assert_eq!(shoe.len(), 104);
    assert_eq!(shoe[0], card("A♣"));
    assert_eq!(shoe[12], card("K♣"));
    assert_eq!(shoe[13], card("A♦"));
    assert_eq!(shoe[26], card("A♥"));
    assert_eq!(shoe[51], card("K♠"));
    assert_eq!(shoe[52], card("A♣"));
    assert_eq!(shoe[103], card("K♠"));
}

#[test]
fn eu_1523_each_new_shoe_seed_follows_from_the_previous_seed() {
    for &(seed, next) in &[
        (1u64, 10451216379200822465u64),
        (42, 13679457532755275413),
        (7, 7191089600892374487),
    ] {
        assert_eq!(next_seed(seed), next, "seed {seed}");
    }
}

#[test]
fn eu_1524_the_random_source_matches_the_splitmix64_reference() {
    for &(seed, expected) in &[
        (
            1234567u64,
            [
                6457827717110365317u64,
                3203168211198807973,
                9817491932198370423,
            ],
        ),
        (
            1,
            [
                10451216379200822465,
                13757245211066428519,
                17911839290282890590,
            ],
        ),
    ] {
        let mut rng = SplitMix64::new(seed);
        let drawn = [rng.next_u64(), rng.next_u64(), rng.next_u64()];
        assert_eq!(drawn, expected, "seed {seed}");
    }
}

#[test]
fn eu_1525_a_bounded_draw_discards_values_that_would_bias_the_result() {
    let mut rng = SplitMix64::new(1);
    let drawn: Vec<u64> = (0..5).map(|_| rng.below(9223372036854775809)).collect();
    assert_eq!(
        drawn,
        [
            1227844342346046656,
            4533873174211652710,
            8688467253428114781,
            4849545566009754239,
            6960854651289091236,
        ]
    );
}

#[test]
fn eu_1525_a_bounded_draw_keeps_a_value_equal_to_the_threshold() {
    // The threshold for bound n is 2^64 mod n; a raw value equal to it is
    // kept ("r >= threshold"). Seed 1234567's first raw value r1 is below
    // 2^63, so with n = 2^64 - r1 the threshold is exactly r1: the first
    // draw keeps it and returns r1 mod n = r1.
    let r1 = SplitMix64::new(1234567).next_u64();
    let n = r1.wrapping_neg();
    assert_eq!(n.wrapping_neg() % n, r1, "threshold equals r1");
    assert_eq!(SplitMix64::new(1234567).below(n), r1);
}
