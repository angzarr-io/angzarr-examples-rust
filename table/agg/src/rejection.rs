//! Why the table refuses a command: the `ErrorInfo.reason` code and the gRPC
//! status class `table.proto` assigns it.

/// The gRPC status class a rejection is reported with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusCode {
    /// The command is well formed but the table's state refuses it.
    FailedPrecondition,
    /// The command itself is malformed or out of range.
    InvalidArgument,
}

/// A rejection reason, as listed in `table.proto`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RejectionCode {
    /// CreateTable on a table that already exists (EU-1442).
    TableAlreadyExists,
    /// CreateTable with a configuration outside the house rules (EU-1441).
    InvalidTableConfig,
    /// Any other command before the table is created.
    TableNotFound,
    /// RequestSeat for a seat that is held or taken (EU-1445).
    SeatTaken,
    /// RequestSeat from a player already seated here (EU-1448).
    PlayerAlreadySeated,
    /// A bet on an empty seat (EU-1474).
    NotSeated,
    /// A bet once the round is dealt, or a deal during one (EU-1475).
    RoundInProgress,
    /// A second bet on one seat in one round (EU-1472).
    AlreadyBet,
    /// A bet or a double the stack cannot cover (EU-1473, EU-1488).
    InsufficientStack,
    /// DealRound with no bet placed (EU-1476).
    NoBets,
    /// A turn action between rounds (EU-1489).
    NoRoundInProgress,
    /// A turn action from a seat not on turn (EU-1482).
    NotYourTurn,
    /// DoubleDown on more than two cards (EU-1487).
    DoubleNotAllowed,
    /// RequestSeat for a seat the table does not have (EU-1446).
    SeatOutOfRange,
    /// A buy-in outside `[min_buy_in, max_buy_in]` (EU-1447).
    BuyInOutOfRange,
    /// A bet outside `[min_bet, max_bet]` (EU-1471).
    BetOutOfRange,
    /// An odd bet (EU-1471).
    BetNotEven,
    /// LeaveTable while the seat has a wager in the round (EU-1462).
    WagerInPlay,
}

impl RejectionCode {
    /// Every code, for exhaustive checks.
    pub const ALL: [RejectionCode; 18] = [
        Self::TableAlreadyExists,
        Self::InvalidTableConfig,
        Self::TableNotFound,
        Self::SeatTaken,
        Self::PlayerAlreadySeated,
        Self::NotSeated,
        Self::RoundInProgress,
        Self::AlreadyBet,
        Self::InsufficientStack,
        Self::NoBets,
        Self::NoRoundInProgress,
        Self::NotYourTurn,
        Self::DoubleNotAllowed,
        Self::SeatOutOfRange,
        Self::BuyInOutOfRange,
        Self::BetOutOfRange,
        Self::BetNotEven,
        Self::WagerInPlay,
    ];
}

impl RejectionCode {
    /// The `ErrorInfo.reason` string, e.g. `"TABLE_ALREADY_EXISTS"`.
    pub fn reason(self) -> &'static str {
        match self {
            Self::TableAlreadyExists => "TABLE_ALREADY_EXISTS",
            Self::InvalidTableConfig => "INVALID_TABLE_CONFIG",
            Self::TableNotFound => "TABLE_NOT_FOUND",
            Self::SeatTaken => "SEAT_TAKEN",
            Self::PlayerAlreadySeated => "PLAYER_ALREADY_SEATED",
            Self::NotSeated => "NOT_SEATED",
            Self::RoundInProgress => "ROUND_IN_PROGRESS",
            Self::AlreadyBet => "ALREADY_BET",
            Self::InsufficientStack => "INSUFFICIENT_STACK",
            Self::NoBets => "NO_BETS",
            Self::NoRoundInProgress => "NO_ROUND_IN_PROGRESS",
            Self::NotYourTurn => "NOT_YOUR_TURN",
            Self::DoubleNotAllowed => "DOUBLE_NOT_ALLOWED",
            Self::SeatOutOfRange => "SEAT_OUT_OF_RANGE",
            Self::BuyInOutOfRange => "BUY_IN_OUT_OF_RANGE",
            Self::BetOutOfRange => "BET_OUT_OF_RANGE",
            Self::BetNotEven => "BET_NOT_EVEN",
            Self::WagerInPlay => "WAGER_IN_PLAY",
        }
    }

    /// The gRPC status class.
    pub fn status(self) -> StatusCode {
        match self {
            Self::InvalidTableConfig
            | Self::SeatOutOfRange
            | Self::BuyInOutOfRange
            | Self::BetOutOfRange
            | Self::BetNotEven => StatusCode::InvalidArgument,
            Self::TableAlreadyExists
            | Self::TableNotFound
            | Self::SeatTaken
            | Self::PlayerAlreadySeated
            | Self::NotSeated
            | Self::RoundInProgress
            | Self::AlreadyBet
            | Self::InsufficientStack
            | Self::NoBets
            | Self::NoRoundInProgress
            | Self::NotYourTurn
            | Self::DoubleNotAllowed
            | Self::WagerInPlay => StatusCode::FailedPrecondition,
        }
    }
}

/// A refused command: the reason code and a human-readable message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rejection {
    pub code: RejectionCode,
    pub message: String,
}

impl Rejection {
    pub fn new(code: RejectionCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

#[cfg(test)]
#[path = "rejection.test.rs"]
mod rejection_tests;
