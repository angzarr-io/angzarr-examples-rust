//! policy-player-table's mapping, its handler called directly: the commands
//! it returns, each with the root it names (bot_play.feature, EU-1603,
//! EU-1604).

use angzarr_embedded::next::markup::PolicyContext;
use angzarr_embedded::next::v1::{Cover, EventRef, Uuid};
use angzarr_embedded::next::{pack, Any, Timestamp};
use angzarr_embedded_diagnostic::{Arg, Diagnostic};
use examples_proto::v1::{self, MoveAction, Suit};

use super::PlayerTablePolicy;
use crate::domain::MoveDecided;

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

// ================================================================ policy-player-table

fn card(rank: u32, suit: Suit) -> v1::Card {
    v1::Card {
        rank,
        suit: suit.into(),
    }
}

/// A decision on a hard 11 (six, five) against a dealer's six: a hand on
/// which a hit, a stand and a double down are all moves a player makes.
fn decided(action: MoveAction) -> MoveDecided {
    MoveDecided(v1::MoveDecided {
        table_root: vec![TABLE; 16].into(),
        round: 1,
        seat: 0,
        action: action.into(),
        cards: vec![card(6, Suit::Clubs), card(5, Suit::Hearts)],
        total: 11,
        soft: false,
        dealer_up: Some(card(6, Suit::Diamonds)),
    })
}

fn table_command(action: MoveAction) -> Vec<(Uuid, Any)> {
    PlayerTablePolicy
        .move_decided(&decided(action), &context("player", ALICE))
        .expect("a move")
        .orders()
        .to_vec()
}

#[test]
fn eu_1603_the_player_to_table_translator_turns_hit_into_hit() {
    assert_eq!(
        table_command(MoveAction::Hit),
        [(root(TABLE), pack(&v1::Hit { seat: 0 }))]
    );
}

#[test]
fn eu_1603_the_player_to_table_translator_turns_stand_into_stand() {
    assert_eq!(
        table_command(MoveAction::Stand),
        [(root(TABLE), pack(&v1::Stand { seat: 0 }))]
    );
}

#[test]
fn eu_1603_the_player_to_table_translator_turns_double_down_into_double_down() {
    assert_eq!(
        table_command(MoveAction::DoubleDown),
        [(root(TABLE), pack(&v1::DoubleDown { seat: 0 }))]
    );
}

#[test]
fn eu_1603_the_command_names_the_seat_and_table_the_decision_names() {
    let decision = MoveDecided(v1::MoveDecided {
        table_root: vec![0x7b; 16].into(),
        round: 4,
        seat: 2,
        action: MoveAction::Stand.into(),
        cards: vec![card(10, Suit::Spades), card(7, Suit::Hearts)],
        total: 17,
        soft: false,
        dealer_up: Some(card(9, Suit::Clubs)),
    });

    let commands = PlayerTablePolicy
        .move_decided(&decision, &context("player", ALICE))
        .expect("a move");

    assert_eq!(
        commands.orders(),
        [(root(0x7b), pack(&v1::Stand { seat: 2 }))]
    );
}

#[test]
fn eu_1604_a_decision_with_no_action_is_the_policys_error() {
    // The spec: a player bug, dead-lettered and alerted. The handler's
    // error is what angzarr-embedded dead-letters, by its code and args.
    let error = PlayerTablePolicy
        .move_decided(&decided(MoveAction::Unspecified), &context("player", ALICE))
        .err()
        .expect("no move is a bug, not an empty answer");
    assert_eq!(error.code(), "policy-player-table-unknown-action");
    let args = error.args();
    assert!(
        matches!(args.as_slice(), [("action", Arg::Int(0))]),
        "the action it decided, MOVE_ACTION_UNSPECIFIED: {args:?}"
    );
}
