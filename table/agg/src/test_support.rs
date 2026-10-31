//! The spec's card notation, for tests: rank then suit, `A`, `2`..`10`,
//! `J`, `Q`, `K` and `♣ ♦ ♥ ♠` (`shoe.feature`); table "Main"
//! ([`create_main`]); and [`seed_21`], the hand every driver of the table
//! plays to settlement.

use examples_proto::v1::{self, Card, Suit};

pub mod seed_21;

/// Creates table "Main" (round.feature) with its first shoe from `seed`:
/// 3 seats, one deck, bets 10 to 100, buy-ins 100 to 1000.
pub fn create_main(seed: u64) -> v1::CreateTable {
    v1::CreateTable {
        name: "Main".into(),
        min_bet: 10,
        max_bet: 100,
        min_buy_in: 100,
        max_buy_in: 1000,
        seats: 3,
        decks: 1,
        shoe_seed: seed,
    }
}

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
