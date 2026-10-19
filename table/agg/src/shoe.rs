//! The deterministic shoe (house rule AHR-3), exactly as `cards.proto`
//! pins it: a fresh shoe of canonical decks, shuffled by a descending
//! Fisher–Yates driven by SplitMix64 through unbiased bounded draws.
//!
//! Every implementation of the example must deal the same order from the
//! same seed; `shoe.feature` (EU-1520..EU-1525) holds the golden vectors.

use examples_proto::v1::{Card, Suit};

/// Cards in one deck.
pub const CARDS_PER_DECK: u32 = 52;

/// SplitMix64's state increment (the golden-ratio constant).
const GAMMA: u64 = 0x9E37_79B9_7F4A_7C15;

/// The SplitMix64 random source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    /// A source that starts from `seed`.
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// The next raw 64-bit value.
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(GAMMA);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// An unbiased draw in `[0, n)`; `n` must be at least 1. Raw values below
    /// `2^64 mod n` are discarded and redrawn.
    pub fn below(&mut self, n: u64) -> u64 {
        // (2^64 - n) mod n, which equals 2^64 mod n.
        let threshold = n.wrapping_neg() % n;
        loop {
            let r = self.next_u64();
            if r >= threshold {
                return r % n;
            }
        }
    }
}

/// The card at canonical index `index` (`13 * suit + rank - 1`, clubs,
/// diamonds, hearts, spades), taken modulo one deck.
fn card_at(index: u32) -> Card {
    const SUITS: [Suit; 4] = [Suit::Clubs, Suit::Diamonds, Suit::Hearts, Suit::Spades];
    let index = index % CARDS_PER_DECK;
    Card {
        rank: index % 13 + 1,
        suit: SUITS[(index / 13) as usize].into(),
    }
}

/// The unshuffled shoe: the canonical deck, `decks` times over.
pub fn fresh_shoe(decks: u32) -> Vec<Card> {
    (0..CARDS_PER_DECK * decks).map(card_at).collect()
}

/// The shoe of `decks` decks shuffled with `seed`, in deal order (the
/// first card is dealt first).
pub fn shuffle(seed: u64, decks: u32) -> Vec<Card> {
    let mut cards = fresh_shoe(decks);
    let mut rng = SplitMix64::new(seed);
    for i in (1..cards.len()).rev() {
        let j = rng.below(i as u64 + 1) as usize;
        cards.swap(i, j);
    }
    cards
}

/// The seed of the shoe that replaces the shoe shuffled with `seed`: the
/// first value of a fresh SplitMix64 from `seed`.
pub fn next_seed(seed: u64) -> u64 {
    SplitMix64::new(seed).next_u64()
}

#[cfg(test)]
#[path = "shoe.test.rs"]
mod shoe_tests;
