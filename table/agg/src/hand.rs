//! A hand's value (AHR-2), exactly as `cards.proto` defines it.

use examples_proto::v1::Card;

/// A hand's value: its total, whether an Ace counts 11 in it (soft), and
/// whether it is a blackjack (exactly two cards totalling 21).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Value {
    pub total: u32,
    pub soft: bool,
    pub blackjack: bool,
}

impl Value {
    /// Over 21.
    pub fn busted(self) -> bool {
        self.total > 21
    }
}

/// The value of `cards`: the hard total counts every Ace as 1; the hand is
/// soft, and worth hard + 10, when it holds an Ace and hard + 10 <= 21.
pub fn value(cards: &[Card]) -> Value {
    let hard: u32 = cards.iter().map(|c| c.rank.min(10)).sum();
    let ace = cards.iter().any(|c| c.rank == 1);
    let soft = ace && hard + 10 <= 21;
    let total = if soft { hard + 10 } else { hard };
    Value {
        total,
        soft,
        blackjack: cards.len() == 2 && total == 21,
    }
}

#[cfg(test)]
#[path = "hand.test.rs"]
mod hand_tests;
