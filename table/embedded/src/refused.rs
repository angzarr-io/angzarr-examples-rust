//! agg-table's [`Rejection`] as the table domain's untyped rejection: a
//! localizable diagnostic, one code per `ErrorInfo.reason`, its args the
//! reason and the values the rejection names (decision 14.37: a key and an
//! argument map, never English).

use std::fmt;

use agg_table::{Rejection, RejectionArg, RejectionCode, ShoeRanOut};
use angzarr_embedded_diagnostic::{Arg, Diagnostic};

/// Why the table refused a command (other than ConfirmSeat's typed
/// [`SeatNotConfirmable`](crate::domain::SeatNotConfirmable)).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableRefused(pub Rejection);

impl TableRefused {
    /// The `ErrorInfo.reason` code `table.proto` lists, e.g.
    /// `"ALREADY_BET"`.
    pub fn reason(&self) -> &'static str {
        self.0.code.reason()
    }
}

impl From<Rejection> for TableRefused {
    fn from(rejection: Rejection) -> Self {
        Self(rejection)
    }
}

/// Its code and args, as every error displays itself (decision 14.37).
impl fmt::Display for TableRefused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        angzarr_embedded_diagnostic::display(self, f)
    }
}

impl Diagnostic for TableRefused {
    fn code(&self) -> &'static str {
        match self.0.code {
            RejectionCode::TableAlreadyExists => "examples-table-table-already-exists",
            RejectionCode::InvalidTableConfig => "examples-table-invalid-table-config",
            RejectionCode::TableNotFound => "examples-table-table-not-found",
            RejectionCode::SeatTaken => "examples-table-seat-taken",
            RejectionCode::PlayerAlreadySeated => "examples-table-player-already-seated",
            RejectionCode::NotSeated => "examples-table-not-seated",
            RejectionCode::RoundInProgress => "examples-table-round-in-progress",
            RejectionCode::AlreadyBet => "examples-table-already-bet",
            RejectionCode::InsufficientStack => "examples-table-insufficient-stack",
            RejectionCode::NoBets => "examples-table-no-bets",
            RejectionCode::NoRoundInProgress => "examples-table-no-round-in-progress",
            RejectionCode::NotYourTurn => "examples-table-not-your-turn",
            RejectionCode::DoubleNotAllowed => "examples-table-double-not-allowed",
            RejectionCode::SeatOutOfRange => "examples-table-seat-out-of-range",
            RejectionCode::BuyInOutOfRange => "examples-table-buy-in-out-of-range",
            RejectionCode::BetOutOfRange => "examples-table-bet-out-of-range",
            RejectionCode::BetNotEven => "examples-table-bet-not-even",
            RejectionCode::WagerInPlay => "examples-table-wager-in-play",
        }
    }

    fn args(&self) -> Vec<(&'static str, Arg)> {
        // The reason travels as an arg: through angzarr-router a caller
        // receives the code and args, not this value.
        let mut args = vec![("reason", self.reason().into())];
        args.extend(self.0.args.iter().map(|(name, value)| {
            let value = match value {
                RejectionArg::Int(n) => Arg::Int(*n),
                RejectionArg::Str(s) => Arg::Str(s.clone()),
            };
            (*name, value)
        }));
        args
    }
}

/// agg-table's [`ShoeRanOut`] as the table's bug: a defect, never a
/// refusal, keyed `examples-table-shoe-ran-out` with its round.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TableBug(pub ShoeRanOut);

/// Its code and args, as every error displays itself (decision 14.37).
impl fmt::Display for TableBug {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        angzarr_embedded_diagnostic::display(self, f)
    }
}

impl Diagnostic for TableBug {
    fn code(&self) -> &'static str {
        "examples-table-shoe-ran-out"
    }

    fn args(&self) -> Vec<(&'static str, Arg)> {
        vec![("round", u64::from(self.0.round).into())]
    }
}
