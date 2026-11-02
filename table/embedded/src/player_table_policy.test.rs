//! policy-player-table's mapping, its handler called directly: the commands
//! it returns, each with the root it names (bot_play.feature, EU-1603,
//! EU-1604).

use angzarr_diagnostic::{Arg, Diagnostic};
use angzarr_embedded::markup::PolicyContext;
use angzarr_embedded::v1::{Cover, EventRef, Uuid};
use angzarr_embedded::{pack, Any, Timestamp, WireError};
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
fn decided(action: MoveAction) -> v1::MoveDecided {
    v1::MoveDecided {
        table_root: vec![TABLE; 16],
        round: 1,
        seat: 0,
        action: action.into(),
        cards: vec![card(6, Suit::Clubs), card(5, Suit::Hearts)],
        total: 11,
        soft: false,
        dealer_up: Some(card(6, Suit::Diamonds)),
    }
}

/// The MoveDecided the policy is handed: the wire's, as the domain's.
fn domain(decided: v1::MoveDecided) -> MoveDecided {
    MoveDecided::try_from(decided).expect("a MoveDecided")
}

fn table_command(action: MoveAction) -> Vec<(Uuid, Any)> {
    PlayerTablePolicy
        .move_decided(&domain(decided(action)), &context("player", ALICE))
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
    let decision = domain(v1::MoveDecided {
        table_root: vec![0x7b; 16],
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
fn eu_1604_a_decision_with_no_action_is_not_a_move_decided() {
    // The spec: a player bug, dead-lettered and alerted. A MoveDecided with
    // MOVE_ACTION_UNSPECIFIED does not decode as the domain's, so the
    // policy never sees it: angzarr-embedded fails the step as a bug,
    // dead-lettered by the decoding's code and args.
    let error = MoveDecided::try_from(decided(MoveAction::Unspecified))
        .expect_err("no move is a bug, not a decision");
    assert!(
        matches!(error, WireError::UnknownEnum { value: 0, .. }),
        "{error:?}"
    );
    let args = error.args();
    assert!(
        matches!(
            args.as_slice(),
            [
                ("message", Arg::Str(message)),
                ("field", Arg::Str(field)),
                ("value", Arg::Int(0)),
            ] if message == "io.angzarr.examples.v1.MoveDecided" && field == "action"
        ),
        "the action it decided, MOVE_ACTION_UNSPECIFIED: {args:?}"
    );
}

// ================================================================ a refused move

/// The table's refusal of `command`, typed with `reason` when one is given.
fn refusal_of<C>(
    command: C,
    reason: Option<v1::table_refused::Reason>,
) -> angzarr_embedded::markup::Refusal<C> {
    use angzarr_embedded::v1::{refusal_response, EventBook, RefusalResponse};
    let response = RefusalResponse {
        reason: reason.map(|r| {
            refusal_response::Reason::Refusal(pack(&v1::TableRefused {
                reason: r.into(),
                seat: Some(0),
                ..v1::TableRefused::default()
            }))
        }),
    };
    angzarr_embedded::markup::Refusal::new(EventBook::default(), command, response)
}

/// The player's fact of the refusal of `action` at seat 0, for `reason`.
fn move_refused(action: MoveAction, reason: &str) -> crate::domain::PlayerAggregateFact {
    crate::domain::RecordMoveRefused::try_from(v1::RecordMoveRefused {
        seat: 0,
        action: action.into(),
        reason: reason.to_string(),
    })
    .expect("a RecordMoveRefused")
    .into()
}

/// The table refusing a move the player decided (it never does in play)
/// is translated into the player's RecordMoveRefused, with the table's
/// code (angzarr decision 14.76).
#[test]
fn a_refused_move_is_the_players_record_of_it() {
    use angzarr_embedded::markup::CommandContext;
    use v1::table_refused::Reason;

    use crate::domain::{DoubleDown, Hit, Stand};

    let ctx = CommandContext::default();
    let not_your_turn = Some(Reason::NotYourTurn);
    let hit = refusal_of(Hit { seat: 0 }, not_your_turn);
    assert_eq!(
        PlayerTablePolicy
            .hit_refused(&hit, &ctx)
            .expect("translated"),
        move_refused(MoveAction::Hit, "NOT_YOUR_TURN")
    );
    let stand = refusal_of(Stand { seat: 0 }, Some(Reason::NoRoundInProgress));
    assert_eq!(
        PlayerTablePolicy
            .stand_refused(&stand, &ctx)
            .expect("translated"),
        move_refused(MoveAction::Stand, "NO_ROUND_IN_PROGRESS")
    );
    let double = refusal_of(DoubleDown { seat: 0 }, Some(Reason::DoubleNotAllowed));
    assert_eq!(
        PlayerTablePolicy
            .double_down_refused(&double, &ctx)
            .expect("translated"),
        move_refused(MoveAction::DoubleDown, "DOUBLE_NOT_ALLOWED")
    );
    // No typed refusal: the move refused, no code.
    let untyped = refusal_of(Hit { seat: 0 }, None);
    assert_eq!(
        PlayerTablePolicy
            .hit_refused(&untyped, &ctx)
            .expect("translated"),
        move_refused(MoveAction::Hit, "")
    );
}
