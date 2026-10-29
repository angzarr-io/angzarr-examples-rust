//! The two policies between the table and the player bot (policies.proto,
//! decision 14.38), framework-free: each trait has one method per event it
//! handles, a generated event in and the command (with the root it is for)
//! out. A harness adapter (the angzarr-embedded `#[policy]` files in
//! `table-embedded`, or any other) delegates to them.
//!
//! Both are stateless and name their target root from the event they handle
//! (decision 14.36).

use examples_proto::v1::{
    DecideMove, DoubleDown, Hit, MoveAction, MoveDecided, Stand, TurnStarted,
};

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
}

/// The table's command for a decided move.
#[derive(Debug, Clone, PartialEq)]
pub enum TableMove {
    Hit(Hit),
    Stand(Stand),
    DoubleDown(DoubleDown),
}

/// A MoveDecided no command answers: a player bug.
#[derive(Debug, Clone, PartialEq)]
pub enum NoMove {
    /// MOVE_ACTION_UNSPECIFIED, or an action this table does not know.
    UnknownAction(i32),
}

/// policy-player-table.
pub trait PlayerTable {
    /// Hit, Stand or DoubleDown{seat} for root = `event.table_root`.
    fn move_decided(&self, event: &MoveDecided) -> Result<To<TableMove>, NoMove>;
}

/// The policies' one implementation.
#[derive(Debug, Clone, Copy, Default)]
pub struct Policies;

impl TablePlayer for Policies {
    fn turn_started(&self, table_root: &[u8], event: &TurnStarted) -> To<DecideMove> {
        let hand = event.hand.clone().unwrap_or_default();
        To {
            root: event.player_root.to_vec(),
            command: DecideMove {
                table_root: table_root.to_vec().into(),
                round: event.round,
                seat: event.seat,
                cards: hand.cards,
                total: hand.total,
                soft: hand.soft,
                dealer_up: event.dealer_up,
                can_double: event.can_double,
                stack: event.stack,
                wager: event.wager,
            },
        }
    }
}

impl PlayerTable for Policies {
    fn move_decided(&self, event: &MoveDecided) -> Result<To<TableMove>, NoMove> {
        let seat = event.seat;
        let command = match MoveAction::try_from(event.action) {
            Ok(MoveAction::Hit) => TableMove::Hit(Hit { seat }),
            Ok(MoveAction::Stand) => TableMove::Stand(Stand { seat }),
            Ok(MoveAction::DoubleDown) => TableMove::DoubleDown(DoubleDown { seat }),
            Ok(MoveAction::Unspecified) | Err(_) => {
                return Err(NoMove::UnknownAction(event.action))
            }
        };
        Ok(To {
            root: event.table_root.to_vec(),
            command,
        })
    }
}
