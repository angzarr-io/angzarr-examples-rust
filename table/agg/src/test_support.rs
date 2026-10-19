//! The spec's card notation, for tests: rank then suit, `A`, `2`..`10`,
//! `J`, `Q`, `K` and `♣ ♦ ♥ ♠` (`shoe.feature`).

use examples_proto::v1::{Card, Suit};

/// One card written as the spec writes it, e.g. `"10♦"`.
pub fn card(text: &str) -> Card {
    let suit_char = text.chars().last().expect("a card has a suit");
    let rank_text = &text[..text.len() - suit_char.len_utf8()];
    let rank = match rank_text {
        "A" => 1,
        "J" => 11,
        "Q" => 12,
        "K" => 13,
        pips => pips.parse().expect("a pip rank"),
    };
    let suit = match suit_char {
        '♣' => Suit::Clubs,
        '♦' => Suit::Diamonds,
        '♥' => Suit::Hearts,
        '♠' => Suit::Spades,
        other => panic!("not a suit: {other}"),
    };
    Card {
        rank,
        suit: suit.into(),
    }
}

/// Space-separated cards, e.g. `"7♣ 3♥ K♣"`.
pub fn cards(text: &str) -> Vec<Card> {
    text.split_whitespace().map(card).collect()
}
