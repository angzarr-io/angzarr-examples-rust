use super::*;
use crate::test_support::cards;

/// EU-1479: an ace counts eleven unless that would bust the hand.
#[test]
fn eu_1479_an_ace_counts_eleven_unless_that_would_bust_the_hand() {
    // (cards, total, soft, blackjack)
    let cases = [
        ("K♣ Q♦", 20, false, false),
        ("A♣ 6♣", 17, true, false),
        ("A♣ A♦", 12, true, false),
        ("A♣ 6♣ 10♥", 17, false, false),
        ("A♣ A♦ 9♥", 21, true, false),
        ("A♣ K♦", 21, true, true),
        ("7♣ 4♦ K♥", 21, false, false),
    ];
    for (text, total, soft, blackjack) in cases {
        assert_eq!(
            value(&cards(text)),
            Value {
                total,
                soft,
                blackjack
            },
            "{text}"
        );
    }
}

#[test]
fn a_hand_over_21_is_busted() {
    assert!(value(&cards("10♣ 6♥ K♣")).busted());
    assert!(!value(&cards("10♣ A♥ K♣")).busted());
}
