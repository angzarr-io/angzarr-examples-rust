//! The table's commands, events and refusal objects, and the player bot's
//! DecideMove and MoveDecided, as markup domain types: what the scaffold's
//! `use crate::domain::*` names.
//!
//! HAND-WRITTEN, not generated: angzarr-build's `markup_domain` is meant to
//! generate this module into `OUT_DIR`, but it refuses table.proto
//! (`SeatOutcome.Outcome` and `TableState.Phase` are nested enums; it asks
//! for them at the top level), and before that it refuses any proto set
//! whose events name a component outside the set (LedgerProjection,
//! TableBuyInPolicy, TableWalletPolicy), one of which (LedgerProjection) is
//! a projector, a kind it does not emit. Replace this file with the
//! generated module once it can be.
//!
//! The markup's handlers never take a wire type: each type here is a
//! newtype over the generated prost message it travels as (its type URL is
//! the message's), with that message as its only field, so agg-table's
//! pure functions read it unchanged.

use std::fmt;

use angzarr_embedded::next::markup::DomainMessage;
use angzarr_embedded::next::WireError;
use angzarr_embedded_diagnostic::{Arg, Diagnostic};
use examples_proto::v1;

macro_rules! domain_messages {
    ($($(#[$doc:meta])* $name:ident),* $(,)?) => {$(
        $(#[$doc])*
        #[derive(Clone, Debug, PartialEq)]
        pub struct $name(pub v1::$name);

        impl DomainMessage for $name {
            type Wire = v1::$name;

            fn to_wire(&self) -> v1::$name {
                self.0.clone()
            }

            fn from_wire(wire: v1::$name) -> Result<Self, WireError> {
                Ok(Self(wire))
            }
        }
    )*};
}

domain_messages!(
    /// Open the table and shuffle its first shoe.
    CreateTable,
    /// Hold a seat for a buy-in.
    RequestSeat,
    /// Seat the player on a held seat (from policy-buy-in-table, or sent
    /// directly by a driver with no buy-in process).
    ConfirmSeat,
    /// Wager on the next round.
    PlaceBet,
    /// Deal the round to every wagered seat.
    DealRound,
    /// The seat on turn draws a card.
    Hit,
    /// The seat on turn stands.
    Stand,
    /// The seat on turn doubles its wager and draws exactly one card.
    DoubleDown,
    /// Cash out the whole stack and free the seat.
    LeaveTable,
    /// Free a held seat.
    ReleaseSeat,
    PlayerCashedOut,
    SeatReleased,
    TableCreated,
    ShoeShuffled,
    SeatHeld,
    PlayerSeated,
    BetPlaced,
    RoundDealt,
    /// A decision point: the seat on turn, with what it may see.
    TurnStarted,
    CardDealt,
    HandStood,
    HandDoubled,
    DealerPlayed,
    RoundSettled,
    /// ConfirmSeat's typed refusal (its `refuses`): the hold is gone, or
    /// the player took another seat meanwhile.
    SeatNotConfirmable,
    /// The player domain's command: decide the move of the seat on turn
    /// (player.proto; policy-table-player sends it).
    DecideMove,
    /// The player domain's event: the move it decided (policy-player-table
    /// reads it).
    MoveDecided,
);

impl fmt::Display for SeatNotConfirmable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "the seat cannot be confirmed: {}", self.0.reason)
    }
}

impl Diagnostic for SeatNotConfirmable {
    fn code(&self) -> &'static str {
        "examples-table-seat-not-confirmable"
    }

    fn args(&self) -> Vec<(&'static str, Arg)> {
        vec![("reason", self.0.reason.as_str().into())]
    }
}
