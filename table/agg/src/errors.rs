//! Table-domain command error catalog.
//!
//! Note: `SeatPlayer` denial reasons are *event-payload strings* on
//! `SeatingRejected` events — not structured command rejections — so the PM
//! can compensate by releasing the buy-in reservation. Only *command*
//! rejection sites are catalogued here.

use examples_utils::error_shapes::{
    AggregateAlreadyExists, AggregateNotFound, BoundKind, BoundViolation, ContainerFull,
    EntityKeyedConflict, EntityNotInContainer, FieldRequired, IdentityMismatch,
    InsufficientCapacity, InvalidOperationInState, MustBePositive, PreconditionError,
    RelationViolation, StateAlreadyEntered, StateMismatch, ValidationError, ValueOutOfRange,
};
use examples_utils::{CommandError, ErrorStatus};

pub struct TableAlreadyExists;
impl CommandError for TableAlreadyExists {
    const CODE: &'static str = "TABLE_ALREADY_EXISTS";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "Table already exists";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl PreconditionError for TableAlreadyExists {}
impl AggregateAlreadyExists for TableAlreadyExists {}

pub struct TableNameRequired;
impl CommandError for TableNameRequired {
    const CODE: &'static str = "TABLE_NAME_REQUIRED";
    const STATUS: ErrorStatus = ErrorStatus::InvalidArgument;
    const TEMPLATE: &'static str = "table_name is required";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl ValidationError for TableNameRequired {}
impl FieldRequired for TableNameRequired {}

pub struct SmallBlindMustBePositive {
    pub value: i64,
}
impl CommandError for SmallBlindMustBePositive {
    const CODE: &'static str = "SMALL_BLIND_MUST_BE_POSITIVE";
    const STATUS: ErrorStatus = ErrorStatus::InvalidArgument;
    const TEMPLATE: &'static str = "small_blind must be positive, got {value}";
    fn fields(&self) -> Vec<(&'static str, String)> {
        vec![("value", self.value.to_string())]
    }
}
impl ValidationError for SmallBlindMustBePositive {}
impl MustBePositive for SmallBlindMustBePositive {
    fn value(&self) -> i64 {
        self.value
    }
}

pub struct BigBlindMustExceedSmallBlind {
    pub lhs: i64,
    pub rhs: i64,
}
impl CommandError for BigBlindMustExceedSmallBlind {
    const CODE: &'static str = "BIG_BLIND_MUST_EXCEED_SMALL_BLIND";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "big_blind {lhs} must be >= small_blind {rhs}";
    fn fields(&self) -> Vec<(&'static str, String)> {
        vec![("lhs", self.lhs.to_string()), ("rhs", self.rhs.to_string())]
    }
}
impl PreconditionError for BigBlindMustExceedSmallBlind {}
impl RelationViolation for BigBlindMustExceedSmallBlind {
    fn lhs(&self) -> i64 {
        self.lhs
    }
    fn rhs(&self) -> i64 {
        self.rhs
    }
}

pub struct MinBuyInMustBePositive {
    pub value: i64,
}
impl CommandError for MinBuyInMustBePositive {
    const CODE: &'static str = "MIN_BUY_IN_MUST_BE_POSITIVE";
    const STATUS: ErrorStatus = ErrorStatus::InvalidArgument;
    const TEMPLATE: &'static str = "min_buy_in must be positive, got {value}";
    fn fields(&self) -> Vec<(&'static str, String)> {
        vec![("value", self.value.to_string())]
    }
}
impl ValidationError for MinBuyInMustBePositive {}
impl MustBePositive for MinBuyInMustBePositive {
    fn value(&self) -> i64 {
        self.value
    }
}

pub struct MaxBuyInMustExceedMinBuyIn {
    pub lhs: i64,
    pub rhs: i64,
}
impl CommandError for MaxBuyInMustExceedMinBuyIn {
    const CODE: &'static str = "MAX_BUY_IN_MUST_EXCEED_MIN_BUY_IN";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "max_buy_in {lhs} must be >= min_buy_in {rhs}";
    fn fields(&self) -> Vec<(&'static str, String)> {
        vec![("lhs", self.lhs.to_string()), ("rhs", self.rhs.to_string())]
    }
}
impl PreconditionError for MaxBuyInMustExceedMinBuyIn {}
impl RelationViolation for MaxBuyInMustExceedMinBuyIn {
    fn lhs(&self) -> i64 {
        self.lhs
    }
    fn rhs(&self) -> i64 {
        self.rhs
    }
}

pub struct MaxPlayersOutOfRange {
    pub got: i32,
}
impl CommandError for MaxPlayersOutOfRange {
    const CODE: &'static str = "MAX_PLAYERS_OUT_OF_RANGE";
    const STATUS: ErrorStatus = ErrorStatus::InvalidArgument;
    const TEMPLATE: &'static str = "max_players must be 2-10, got {got}";
    fn fields(&self) -> Vec<(&'static str, String)> {
        vec![("got", self.got.to_string())]
    }
}
impl ValidationError for MaxPlayersOutOfRange {}
impl ValueOutOfRange for MaxPlayersOutOfRange {
    fn got(&self) -> i64 {
        self.got as i64
    }
}

pub struct TableNotFound;
impl CommandError for TableNotFound {
    const CODE: &'static str = "TABLE_NOT_FOUND";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "Table does not exist";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl PreconditionError for TableNotFound {}
impl AggregateNotFound for TableNotFound {}

pub struct PlayerRootRequired;
impl CommandError for PlayerRootRequired {
    const CODE: &'static str = "PLAYER_ROOT_REQUIRED";
    const STATUS: ErrorStatus = ErrorStatus::InvalidArgument;
    const TEMPLATE: &'static str = "player_root is required";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl ValidationError for PlayerRootRequired {}
impl FieldRequired for PlayerRootRequired {}

pub struct PlayerAlreadySeated;
impl CommandError for PlayerAlreadySeated {
    const CODE: &'static str = "PLAYER_ALREADY_SEATED";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "Player already seated at table";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl PreconditionError for PlayerAlreadySeated {}
impl StateAlreadyEntered for PlayerAlreadySeated {}

pub struct TableIsFull;
impl CommandError for TableIsFull {
    const CODE: &'static str = "TABLE_IS_FULL";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "Table is full";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl PreconditionError for TableIsFull {}
impl ContainerFull for TableIsFull {}

pub struct BuyInBelowMin {
    pub got: i64,
    pub bound: i64,
}
impl CommandError for BuyInBelowMin {
    const CODE: &'static str = "BUY_IN_BELOW_MIN";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "Buy-in {got} is below table minimum {bound}";
    fn fields(&self) -> Vec<(&'static str, String)> {
        vec![
            ("got", self.got.to_string()),
            ("bound", self.bound.to_string()),
        ]
    }
}
impl PreconditionError for BuyInBelowMin {}
impl BoundViolation for BuyInBelowMin {
    fn got(&self) -> i64 {
        self.got
    }
    fn bound(&self) -> i64 {
        self.bound
    }
    fn kind(&self) -> BoundKind {
        BoundKind::BelowMin
    }
}

pub struct BuyInAboveMax {
    pub got: i64,
    pub bound: i64,
}
impl CommandError for BuyInAboveMax {
    const CODE: &'static str = "BUY_IN_ABOVE_MAX";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "Buy-in {got} is above table maximum {bound}";
    fn fields(&self) -> Vec<(&'static str, String)> {
        vec![
            ("got", self.got.to_string()),
            ("bound", self.bound.to_string()),
        ]
    }
}
impl PreconditionError for BuyInAboveMax {}
impl BoundViolation for BuyInAboveMax {
    fn got(&self) -> i64 {
        self.got
    }
    fn bound(&self) -> i64 {
        self.bound
    }
    fn kind(&self) -> BoundKind {
        BoundKind::AboveMax
    }
}

pub struct SeatOccupied {
    pub seat: i32,
}
impl CommandError for SeatOccupied {
    const CODE: &'static str = "SEAT_OCCUPIED";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "Seat {seat} is occupied";
    fn fields(&self) -> Vec<(&'static str, String)> {
        vec![("seat", self.seat.to_string())]
    }
}
impl PreconditionError for SeatOccupied {}
impl EntityKeyedConflict for SeatOccupied {}

pub struct PlayerNotSeated;
impl CommandError for PlayerNotSeated {
    const CODE: &'static str = "PLAYER_NOT_SEATED";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "Player is not seated at table";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl PreconditionError for PlayerNotSeated {}
impl EntityNotInContainer for PlayerNotSeated {}

pub struct CannotLeaveDuringHand;
impl CommandError for CannotLeaveDuringHand {
    const CODE: &'static str = "CANNOT_LEAVE_DURING_HAND";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "Cannot leave table during a hand";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl PreconditionError for CannotLeaveDuringHand {}
impl InvalidOperationInState for CannotLeaveDuringHand {}

pub struct HandAlreadyInProgress;
impl CommandError for HandAlreadyInProgress {
    const CODE: &'static str = "HAND_ALREADY_IN_PROGRESS";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "Hand already in progress";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl PreconditionError for HandAlreadyInProgress {}
impl StateAlreadyEntered for HandAlreadyInProgress {}

pub struct NotEnoughPlayersToStartHand {
    pub requested: i64,
    pub available: i64,
}
impl CommandError for NotEnoughPlayersToStartHand {
    const CODE: &'static str = "NOT_ENOUGH_PLAYERS_TO_START_HAND";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str =
        "Not enough players to start hand: requested {requested}, available {available}";
    fn fields(&self) -> Vec<(&'static str, String)> {
        vec![
            ("requested", self.requested.to_string()),
            ("available", self.available.to_string()),
        ]
    }
}
impl PreconditionError for NotEnoughPlayersToStartHand {}
impl InsufficientCapacity for NotEnoughPlayersToStartHand {
    fn requested(&self) -> i64 {
        self.requested
    }
    fn available(&self) -> i64 {
        self.available
    }
}

pub struct NoHandInProgress;
impl CommandError for NoHandInProgress {
    const CODE: &'static str = "NO_HAND_IN_PROGRESS";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "No hand in progress";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl PreconditionError for NoHandInProgress {}
impl StateMismatch for NoHandInProgress {}

/// One-off: `hand_root` is bytes, doesn't fit `IdentityMismatch` (i64).
pub struct HandRootMismatch;
impl CommandError for HandRootMismatch {
    const CODE: &'static str = "HAND_ROOT_MISMATCH";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "Hand root mismatch";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl PreconditionError for HandRootMismatch {}

pub struct AmountMustBePositive {
    pub value: i64,
}
impl CommandError for AmountMustBePositive {
    const CODE: &'static str = "AMOUNT_MUST_BE_POSITIVE";
    const STATUS: ErrorStatus = ErrorStatus::InvalidArgument;
    const TEMPLATE: &'static str = "Amount must be positive, got {value}";
    fn fields(&self) -> Vec<(&'static str, String)> {
        vec![("value", self.value.to_string())]
    }
}
impl ValidationError for AmountMustBePositive {}
impl MustBePositive for AmountMustBePositive {
    fn value(&self) -> i64 {
        self.value
    }
}

pub struct SeatPositionMismatch {
    pub expected: i64,
    pub got: i64,
}
impl CommandError for SeatPositionMismatch {
    const CODE: &'static str = "SEAT_POSITION_MISMATCH";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "Seat position mismatch: expected {expected}, got {got}";
    fn fields(&self) -> Vec<(&'static str, String)> {
        vec![
            ("expected", self.expected.to_string()),
            ("got", self.got.to_string()),
        ]
    }
}
impl PreconditionError for SeatPositionMismatch {}
impl IdentityMismatch for SeatPositionMismatch {
    fn expected(&self) -> i64 {
        self.expected
    }
    fn got(&self) -> i64 {
        self.got
    }
}

// --- Hand-for-hand (TDA Rule 12) ---------------------------------------

pub struct TableAlreadyInHandForHand;
impl CommandError for TableAlreadyInHandForHand {
    const CODE: &'static str = "TABLE_ALREADY_IN_HAND_FOR_HAND";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "Table is already in hand-for-hand mode";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl PreconditionError for TableAlreadyInHandForHand {}
impl StateAlreadyEntered for TableAlreadyInHandForHand {}

pub struct TableNotInHandForHand;
impl CommandError for TableNotInHandForHand {
    const CODE: &'static str = "TABLE_NOT_IN_HAND_FOR_HAND";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "Table is not in hand-for-hand mode";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl PreconditionError for TableNotInHandForHand {}
impl StateMismatch for TableNotInHandForHand {}

// --- ChangeSeats (PR #12 design decision 2) ----------------------------

/// PR #12 design decision 2 — `ChangeSeats` whose `current_seat ==
/// requested_seat` is a structured rejection. Coordinators / UIs should
/// surface bad operator input rather than silently no-op (which would
/// hide the wired-twice-same-seat bug). The error payload carries the
/// duplicated seat for log traceability. Stable code:
/// [`angzarr_client::error_codes::codes::SEATS_IDENTICAL`].
pub struct SeatsIdentical {
    pub seat: i32,
}
impl CommandError for SeatsIdentical {
    const CODE: &'static str = "SEATS_IDENTICAL";
    const STATUS: ErrorStatus = ErrorStatus::InvalidArgument;
    const TEMPLATE: &'static str = "current_seat and requested_seat are identical at {seat}";
    fn fields(&self) -> Vec<(&'static str, String)> {
        vec![("seat", self.seat.to_string())]
    }
}
impl ValidationError for SeatsIdentical {}
impl ValueOutOfRange for SeatsIdentical {
    fn got(&self) -> i64 {
        self.seat as i64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use examples_utils::reject;

    #[test]
    fn table_already_exists_publishes_code_status_and_template() {
        let r = reject(TableAlreadyExists);
        assert_eq!(r.code, "TABLE_ALREADY_EXISTS");
        assert_eq!(r.status_code, "FAILED_PRECONDITION");
        assert_eq!(TableAlreadyExists.render(), "Table already exists");
    }

    #[test]
    fn small_blind_must_be_positive_renders_with_value() {
        let err = SmallBlindMustBePositive { value: -5 };
        assert_eq!(err.render(), "small_blind must be positive, got -5");
        let r = reject(SmallBlindMustBePositive { value: -5 });
        assert_eq!(r.code, "SMALL_BLIND_MUST_BE_POSITIVE");
        assert_eq!(r.status_code, "INVALID_ARGUMENT");
        assert_eq!(r.details.get("value"), Some(&"-5".to_string()));
    }

    #[test]
    fn buy_in_below_min_carries_both_fields_in_details() {
        let err = BuyInBelowMin {
            got: 10,
            bound: 200,
        };
        assert_eq!(err.render(), "Buy-in 10 is below table minimum 200");
        let r = reject(BuyInBelowMin {
            got: 10,
            bound: 200,
        });
        assert_eq!(r.code, "BUY_IN_BELOW_MIN");
        assert_eq!(r.status_code, "FAILED_PRECONDITION");
        assert_eq!(r.details.get("got"), Some(&"10".to_string()));
        assert_eq!(r.details.get("bound"), Some(&"200".to_string()));
    }

    #[test]
    fn big_blind_must_exceed_small_blind_renders_both_fields() {
        let err = BigBlindMustExceedSmallBlind { lhs: 5, rhs: 10 };
        assert_eq!(err.render(), "big_blind 5 must be >= small_blind 10");
        let r = reject(BigBlindMustExceedSmallBlind { lhs: 5, rhs: 10 });
        assert_eq!(r.details.get("lhs"), Some(&"5".to_string()));
        assert_eq!(r.details.get("rhs"), Some(&"10".to_string()));
    }

    #[test]
    fn seat_occupied_carries_seat() {
        let err = SeatOccupied { seat: 3 };
        assert_eq!(err.render(), "Seat 3 is occupied");
        let r = reject(SeatOccupied { seat: 3 });
        assert_eq!(r.code, "SEAT_OCCUPIED");
        assert_eq!(r.details.get("seat"), Some(&"3".to_string()));
    }

    #[test]
    fn empty_field_errors_render_template_unchanged() {
        assert_eq!(TableNotFound.render(), "Table does not exist");
        assert_eq!(PlayerRootRequired.render(), "player_root is required");
        assert_eq!(
            PlayerAlreadySeated.render(),
            "Player already seated at table"
        );
        assert_eq!(TableIsFull.render(), "Table is full");
        assert_eq!(PlayerNotSeated.render(), "Player is not seated at table");
        assert_eq!(
            CannotLeaveDuringHand.render(),
            "Cannot leave table during a hand"
        );
        assert_eq!(HandAlreadyInProgress.render(), "Hand already in progress");
        let nepts = NotEnoughPlayersToStartHand {
            requested: 2,
            available: 1,
        };
        assert_eq!(
            nepts.render(),
            "Not enough players to start hand: requested 2, available 1"
        );
        assert_eq!(NoHandInProgress.render(), "No hand in progress");
        assert_eq!(HandRootMismatch.render(), "Hand root mismatch");
    }
}
