//! The two policies between the table and the player bot (policies.proto,
//! decision 14.38), framework-free: each trait has one method per event it
//! handles, an event of the generated domain layer in and the command (with
//! the root it is for) out. A harness adapter (the angzarr-embedded
//! `#[policy]` files in `table-embedded`, or any other) delegates to them.
//!
//! Both are stateless and name their target root from the event they handle
//! (decision 14.36).

use examples_proto::domain::{
    DecideMove, DecideMoveRefused, DoubleDown, Hit, MoveAction, MoveDecided, RecordMoveRefused,
    RecordMoveUndecided, Stand, TableRefused, TurnStarted,
};
use examples_proto::v1;

/// A command for the stream at `root`.
#[derive(Debug, Clone, PartialEq)]
pub struct To<C> {
    pub root: Vec<u8>,
    pub command: C,
}

/// policy-table-player.
pub trait TablePlayer {
    /// The seat's player is asked to decide: DecideMove for root =
    /// `event.player_root`, with the hand, the dealer's up-card,
    /// `can_double`, stack and wager unchanged, and `table_root` the
    /// event's own stream root.
    fn turn_started(&self, table_root: &[u8], event: &TurnStarted) -> To<DecideMove>;

    /// The player refused to decide `command` (angzarr decision 14.76):
    /// the table's RecordMoveUndecided for its round and seat, with the
    /// player's reason (empty when it gave none).
    fn decide_move_refused(
        &self,
        command: &DecideMove,
        refusal: Option<DecideMoveRefused>,
    ) -> RecordMoveUndecided;
}

/// The table's command for a decided move.
#[derive(Debug, Clone, PartialEq)]
pub enum TableMove {
    Hit(Hit),
    Stand(Stand),
    DoubleDown(DoubleDown),
}

/// policy-player-table.
pub trait PlayerTable {
    /// Hit, Stand or DoubleDown{seat} for root = `event.table_root`. Every
    /// action is one of the three: a MoveDecided with
    /// MOVE_ACTION_UNSPECIFIED, or an action the table does not know, is not
    /// a domain value (a player bug the harness reports as it decodes it).
    fn move_decided(&self, event: &MoveDecided) -> To<TableMove>;

    /// The table refused `command`, a move this policy sent (angzarr
    /// decision 14.76): the player's RecordMoveRefused for its seat and
    /// action, with the table's code (empty when it gave no typed refusal).
    fn move_refused(&self, command: &TableMove, refusal: Option<TableRefused>)
        -> RecordMoveRefused;
}

/// A refusal's code, the wire name of its reason without its prefix
/// (`NOT_YOUR_TURN`), as the rejection carries it.
fn code(refusal: &TableRefused) -> String {
    let wire = v1::TableRefused::from(refusal.clone());
    v1::table_refused::Reason::try_from(wire.reason)
        .map(|r| r.as_str_name().trim_start_matches("REASON_").to_string())
        .unwrap_or_default()
}

/// The policies' one implementation.
#[derive(Debug, Clone, Copy, Default)]
pub struct Policies;

impl TablePlayer for Policies {
    fn decide_move_refused(
        &self,
        command: &DecideMove,
        refusal: Option<DecideMoveRefused>,
    ) -> RecordMoveUndecided {
        RecordMoveUndecided {
            round: command.round,
            seat: command.seat,
            reason: refusal.map(|r| r.reason).unwrap_or_default(),
        }
    }

    fn turn_started(&self, table_root: &[u8], event: &TurnStarted) -> To<DecideMove> {
        let hand = event.hand.clone();
        To {
            root: event.player_root.clone(),
            command: DecideMove {
                table_root: table_root.to_vec(),
                round: event.round,
                seat: event.seat,
                cards: hand.cards,
                total: hand.total,
                soft: hand.soft,
                dealer_up: event.dealer_up.clone(),
                can_double: event.can_double,
                stack: event.stack,
                wager: event.wager,
            },
        }
    }
}

impl PlayerTable for Policies {
    fn move_refused(
        &self,
        command: &TableMove,
        refusal: Option<TableRefused>,
    ) -> RecordMoveRefused {
        let (seat, action) = match command {
            TableMove::Hit(hit) => (hit.seat, MoveAction::Hit),
            TableMove::Stand(stand) => (stand.seat, MoveAction::Stand),
            TableMove::DoubleDown(double) => (double.seat, MoveAction::DoubleDown),
        };
        RecordMoveRefused {
            seat,
            action,
            reason: refusal.as_ref().map(code).unwrap_or_default(),
        }
    }

    fn move_decided(&self, event: &MoveDecided) -> To<TableMove> {
        let seat = event.seat;
        let command = match event.action {
            MoveAction::Hit => TableMove::Hit(Hit { seat }),
            MoveAction::Stand => TableMove::Stand(Stand { seat }),
            MoveAction::DoubleDown => TableMove::DoubleDown(DoubleDown { seat }),
        };
        To {
            root: event.table_root.clone(),
            command,
        }
    }
}
