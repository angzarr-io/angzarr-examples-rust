//! The two policies between the table and the player bot (policies.proto,
//! decision 14.38), framework-free: each trait has one method per event it
//! handles, an event of the generated domain layer in and the command (with
//! the root it is for) out. A harness adapter (the angzarr-embedded
//! `#[policy]` files in `table-embedded`, or any other) delegates to them.
//!
//! Both are stateless and name their target root from the event they handle
//! (decision 14.36).

use examples_proto::domain::{
    DecideMove, DoubleDown, Hit, MoveAction, MoveDecided, RecordMoveRefused, Stand,
    TableStateRefusal, TurnStarted,
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

/// policy-player-table.
pub trait PlayerTable {
    /// Hit, Stand or DoubleDown{seat} for root = `event.table_root`. Every
    /// action is one of the three: a MoveDecided with
    /// MOVE_ACTION_UNSPECIFIED, or an action the table does not know, is not
    /// a domain value (a player bug the harness reports as it decodes it).
    fn move_decided(&self, event: &MoveDecided) -> To<TableMove>;

    /// The table refused `command`, a move this policy sent (angzarr
    /// decision 14.89): the player's RecordMoveRefused for its seat and
    /// action, with the refusal's type name.
    fn move_refused(&self, command: &TableMove, refusal: &TableStateRefusal) -> RecordMoveRefused;
}

/// A refusal's name, its type's (`NotYourTurn`), as the player records it.
fn name(refusal: &TableStateRefusal) -> &'static str {
    use TableStateRefusal as R;
    match refusal {
        R::TableAlreadyExists(_) => "TableAlreadyExists",
        R::InvalidTableConfig(_) => "InvalidTableConfig",
        R::TableNotFound(_) => "TableNotFound",
        R::SeatTaken(_) => "SeatTaken",
        R::PlayerAlreadySeated(_) => "PlayerAlreadySeated",
        R::SeatOutOfRange(_) => "SeatOutOfRange",
        R::BuyInOutOfRange(_) => "BuyInOutOfRange",
        R::NotSeated(_) => "NotSeated",
        R::WagerInPlay(_) => "WagerInPlay",
        R::RoundInProgress(_) => "RoundInProgress",
        R::AlreadyBet(_) => "AlreadyBet",
        R::InsufficientStack(_) => "InsufficientStack",
        R::BetOutOfRange(_) => "BetOutOfRange",
        R::BetNotEven(_) => "BetNotEven",
        R::NoBets(_) => "NoBets",
        R::NoRoundInProgress(_) => "NoRoundInProgress",
        R::NotYourTurn(_) => "NotYourTurn",
        R::DoubleNotAllowed(_) => "DoubleNotAllowed",
        R::SeatNotConfirmable(_) => "SeatNotConfirmable",
    }
}

/// The policies' one implementation.
#[derive(Debug, Clone, Copy, Default)]
pub struct Policies;

impl TablePlayer for Policies {
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
    fn move_refused(&self, command: &TableMove, refusal: &TableStateRefusal) -> RecordMoveRefused {
        let (seat, action) = match command {
            TableMove::Hit(hit) => (hit.seat, MoveAction::Hit),
            TableMove::Stand(stand) => (stand.seat, MoveAction::Stand),
            TableMove::DoubleDown(double) => (double.seat, MoveAction::DoubleDown),
        };
        RecordMoveRefused {
            seat,
            action,
            reason: name(refusal).to_string(),
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
