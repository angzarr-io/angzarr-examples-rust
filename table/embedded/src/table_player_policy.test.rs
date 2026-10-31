//! policy-table-player's mapping, its handler called directly: the commands
//! it returns, each with the root it names (bot_play.feature, EU-1601). The
//! loop through a player (EU-1605..1609) needs the Python bot and is not
//! pinned here.

use agg_table::test_support::{card, cards};
use angzarr_embedded::next::markup::PolicyContext;
use angzarr_embedded::next::v1::{Cover, EventRef, Uuid};
use angzarr_embedded::next::{pack, Timestamp};
use examples_proto::v1::{self, SeatHand};

use super::TablePlayerPolicy;
use crate::domain::TurnStarted;

const ALICE: u8 = 0xa1;
const TABLE: u8 = 0x7a;

fn root(b: u8) -> Uuid {
    Uuid::from_bytes([b; 16])
}

/// The context of event 7 of the `domain` stream at `stream_root`.
fn context(domain: &str, stream_root: u8) -> PolicyContext {
    PolicyContext::new(
        EventRef {
            stream: Some(Cover {
                domain: domain.to_string(),
                root: Some(root(stream_root)),
                edition: None,
            }),
            sequence: 7,
        },
        root(0xcc),
        Timestamp {
            seconds: 1_000,
            nanos: 0,
        },
    )
}

// ================================================================ policy-table-player

/// The TurnStarted the policy is handed: the wire's, as the domain's.
fn domain(turn: v1::TurnStarted) -> TurnStarted {
    TurnStarted::try_from(turn).expect("a TurnStarted")
}

/// Seat 0's turn at table "Main" in EU-1601: 2♣ 4♣ (a hard 6) against 9♦,
/// may double, stack 480, wager 20.
fn alices_turn() -> v1::TurnStarted {
    v1::TurnStarted {
        round: 1,
        seat: 0,
        player_root: vec![ALICE; 16].into(),
        hand: Some(SeatHand {
            seat: 0,
            cards: cards("2♣ 4♣"),
            total: 6,
            soft: false,
            blackjack: false,
        }),
        dealer_up: Some(card("9♦")),
        can_double: true,
        stack: 480,
        wager: 20,
    }
}

#[test]
fn eu_1601_the_table_to_player_translator_asks_the_seats_player_to_decide() {
    let Ok(commands) =
        TablePlayerPolicy.turn_started(&domain(alices_turn()), &context("table", TABLE));

    let decide = v1::DecideMove {
        table_root: vec![TABLE; 16].into(),
        round: 1,
        seat: 0,
        cards: cards("2♣ 4♣"),
        total: 6,
        soft: false,
        dealer_up: Some(card("9♦")),
        can_double: true,
        stack: 480,
        wager: 20,
    };
    // Addressed to Alice's player root, as the TurnStarted names it; the
    // table root is the TurnStarted's own stream's.
    assert_eq!(commands.orders(), [(root(ALICE), pack(&decide))]);
}

#[test]
fn eu_1601_the_request_follows_the_turn_field_for_field() {
    let turn = v1::TurnStarted {
        round: 3,
        seat: 2,
        player_root: vec![0xb0; 16].into(),
        hand: Some(SeatHand {
            seat: 2,
            cards: cards("A♣ 6♣ 2♥"),
            total: 19,
            soft: true,
            blackjack: false,
        }),
        dealer_up: Some(card("K♠")),
        can_double: false,
        stack: 10,
        wager: 30,
    };

    let Ok(commands) = TablePlayerPolicy.turn_started(&domain(turn), &context("table", 0x7b));

    let decide = v1::DecideMove {
        table_root: vec![0x7b; 16].into(),
        round: 3,
        seat: 2,
        cards: cards("A♣ 6♣ 2♥"),
        total: 19,
        soft: true,
        dealer_up: Some(card("K♠")),
        can_double: false,
        stack: 10,
        wager: 30,
    };
    assert_eq!(commands.orders(), [(root(0xb0), pack(&decide))]);
}
