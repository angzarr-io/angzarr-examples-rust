//! policy-player-table's mapping, its handler called directly: the commands
//! it returns, each with the root it names (bot_play.feature, EU-1603,
//! EU-1604).

use angzarr_embedded::next::markup::PolicyContext;
use angzarr_embedded::next::v1::{Cover, EventRef, Uuid};
use angzarr_embedded::next::{pack, Any, Timestamp};
use examples_proto::v1::{self, MoveAction};

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

fn decided(action: MoveAction) -> MoveDecided {
    MoveDecided(v1::MoveDecided {
        table_root: vec![TABLE; 16].into(),
        round: 1,
        seat: 0,
        action: action.into(),
    })
}

fn table_command(action: MoveAction) -> Vec<(Uuid, Any)> {
    PlayerTablePolicy
        .move_decided(&decided(action), &context("player", ALICE))
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
    });

    let commands = PlayerTablePolicy.move_decided(&decision, &context("player", ALICE));

    assert_eq!(
        commands.orders(),
        [(root(0x7b), pack(&v1::Stand { seat: 2 }))]
    );
}

#[test]
fn eu_1604_a_decision_with_no_action_sends_no_command() {
    // The spec: a player bug, dead-lettered and alerted. A markup policy
    // handler is infallible, so what it can pin is that nothing is sent;
    // the dead letter is an open question for angzarr-embedded.
    assert_eq!(table_command(MoveAction::Unspecified), []);
}
