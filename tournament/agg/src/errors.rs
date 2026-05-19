//! Tournament-domain command error catalog.
//!
//! Note: enrollment- and rebuy-denial reasons are event-payload strings on
//! `TournamentEnrollmentRejected` / `RebuyDenied`, not structured command
//! rejections — they reach the saga as event-shape, not as gRPC errors.
//! Only *command* rejection sites are catalogued here.

use examples_utils::error_shapes::{
    AggregateAlreadyExists, AggregateNotFound, EntityNotInContainer, Exhausted, FieldRequired,
    InsufficientCapacity, InvalidOperationInState, MustBePositive, PreconditionError,
    RelationViolation, StateAlreadyEntered, StateMismatch, ValidationError, ValueOutOfRange,
};
use examples_utils::{CommandError, ErrorStatus};

pub struct TournamentAlreadyExists;
impl CommandError for TournamentAlreadyExists {
    const CODE: &'static str = "TOURNAMENT_ALREADY_EXISTS";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "Tournament already exists";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl PreconditionError for TournamentAlreadyExists {}
impl AggregateAlreadyExists for TournamentAlreadyExists {}

pub struct NameRequired;
impl CommandError for NameRequired {
    const CODE: &'static str = "NAME_REQUIRED";
    const STATUS: ErrorStatus = ErrorStatus::InvalidArgument;
    const TEMPLATE: &'static str = "name is required";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl ValidationError for NameRequired {}
impl FieldRequired for NameRequired {}

pub struct BuyInMustBePositive {
    pub value: i64,
}
impl CommandError for BuyInMustBePositive {
    const CODE: &'static str = "BUY_IN_MUST_BE_POSITIVE";
    const STATUS: ErrorStatus = ErrorStatus::InvalidArgument;
    const TEMPLATE: &'static str = "buy_in must be positive, got {value}";
    fn fields(&self) -> Vec<(&'static str, String)> {
        vec![("value", self.value.to_string())]
    }
}
impl ValidationError for BuyInMustBePositive {}
impl MustBePositive for BuyInMustBePositive {
    fn value(&self) -> i64 {
        self.value
    }
}

pub struct StartingStackMustBePositive {
    pub value: i64,
}
impl CommandError for StartingStackMustBePositive {
    const CODE: &'static str = "STARTING_STACK_MUST_BE_POSITIVE";
    const STATUS: ErrorStatus = ErrorStatus::InvalidArgument;
    const TEMPLATE: &'static str = "starting_stack must be positive, got {value}";
    fn fields(&self) -> Vec<(&'static str, String)> {
        vec![("value", self.value.to_string())]
    }
}
impl ValidationError for StartingStackMustBePositive {}
impl MustBePositive for StartingStackMustBePositive {
    fn value(&self) -> i64 {
        self.value
    }
}

pub struct MaxPlayersTooFew {
    pub got: i32,
}
impl CommandError for MaxPlayersTooFew {
    const CODE: &'static str = "MAX_PLAYERS_TOO_FEW";
    const STATUS: ErrorStatus = ErrorStatus::InvalidArgument;
    const TEMPLATE: &'static str = "max_players must be at least 2, got {got}";
    fn fields(&self) -> Vec<(&'static str, String)> {
        vec![("got", self.got.to_string())]
    }
}
impl ValidationError for MaxPlayersTooFew {}
impl ValueOutOfRange for MaxPlayersTooFew {
    fn got(&self) -> i64 {
        self.got as i64
    }
}

pub struct MinPlayersTooFew {
    pub got: i32,
}
impl CommandError for MinPlayersTooFew {
    const CODE: &'static str = "MIN_PLAYERS_TOO_FEW";
    const STATUS: ErrorStatus = ErrorStatus::InvalidArgument;
    const TEMPLATE: &'static str = "min_players must be at least 2, got {got}";
    fn fields(&self) -> Vec<(&'static str, String)> {
        vec![("got", self.got.to_string())]
    }
}
impl ValidationError for MinPlayersTooFew {}
impl ValueOutOfRange for MinPlayersTooFew {
    fn got(&self) -> i64 {
        self.got as i64
    }
}

pub struct MinPlayersExceedsMax {
    pub lhs: i32,
    pub rhs: i32,
}
impl CommandError for MinPlayersExceedsMax {
    const CODE: &'static str = "MIN_PLAYERS_EXCEEDS_MAX";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "min_players ({lhs}) cannot exceed max_players ({rhs})";
    fn fields(&self) -> Vec<(&'static str, String)> {
        vec![("lhs", self.lhs.to_string()), ("rhs", self.rhs.to_string())]
    }
}
impl PreconditionError for MinPlayersExceedsMax {}
impl RelationViolation for MinPlayersExceedsMax {
    fn lhs(&self) -> i64 {
        self.lhs as i64
    }
    fn rhs(&self) -> i64 {
        self.rhs as i64
    }
}

pub struct TournamentNotFound;
impl CommandError for TournamentNotFound {
    const CODE: &'static str = "TOURNAMENT_NOT_FOUND";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "Tournament does not exist";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl PreconditionError for TournamentNotFound {}
impl AggregateNotFound for TournamentNotFound {}

pub struct CannotOpenRegistrationRunning;
impl CommandError for CannotOpenRegistrationRunning {
    const CODE: &'static str = "CANNOT_OPEN_REGISTRATION_RUNNING";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "Cannot open registration on a running tournament";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl PreconditionError for CannotOpenRegistrationRunning {}
impl InvalidOperationInState for CannotOpenRegistrationRunning {}

pub struct RegistrationAlreadyOpen;
impl CommandError for RegistrationAlreadyOpen {
    const CODE: &'static str = "REGISTRATION_ALREADY_OPEN";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "Registration is already open";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl PreconditionError for RegistrationAlreadyOpen {}
impl StateAlreadyEntered for RegistrationAlreadyOpen {}

pub struct RegistrationNotOpen;
impl CommandError for RegistrationNotOpen {
    const CODE: &'static str = "REGISTRATION_NOT_OPEN";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "Registration is not open";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl PreconditionError for RegistrationNotOpen {}
impl StateMismatch for RegistrationNotOpen {}

pub struct TournamentNotRunning;
impl CommandError for TournamentNotRunning {
    const CODE: &'static str = "TOURNAMENT_NOT_RUNNING";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "Tournament is not running";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl PreconditionError for TournamentNotRunning {}
impl StateMismatch for TournamentNotRunning {}

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

pub struct PlayerNotRegistered {
    pub player_root_hex: String,
}
impl CommandError for PlayerNotRegistered {
    const CODE: &'static str = "PLAYER_NOT_REGISTERED";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "Player {player_root_hex} is not registered in this tournament";
    fn fields(&self) -> Vec<(&'static str, String)> {
        vec![("player_root_hex", self.player_root_hex.clone())]
    }
}
impl PreconditionError for PlayerNotRegistered {}
impl EntityNotInContainer for PlayerNotRegistered {}

pub struct TournamentAlreadyPaused;
impl CommandError for TournamentAlreadyPaused {
    const CODE: &'static str = "TOURNAMENT_ALREADY_PAUSED";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "Tournament is already paused";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl PreconditionError for TournamentAlreadyPaused {}
impl StateAlreadyEntered for TournamentAlreadyPaused {}

pub struct TournamentNotPaused;
impl CommandError for TournamentNotPaused {
    const CODE: &'static str = "TOURNAMENT_NOT_PAUSED";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "Tournament is not paused";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl PreconditionError for TournamentNotPaused {}
impl StateMismatch for TournamentNotPaused {}

pub struct NotEnoughPlayersToStart {
    pub requested: i64,
    pub available: i64,
}
impl CommandError for NotEnoughPlayersToStart {
    const CODE: &'static str = "NOT_ENOUGH_PLAYERS_TO_START";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str =
        "Not enough players to start: requested {requested}, available {available}";
    fn fields(&self) -> Vec<(&'static str, String)> {
        vec![
            ("requested", self.requested.to_string()),
            ("available", self.available.to_string()),
        ]
    }
}
impl PreconditionError for NotEnoughPlayersToStart {}
impl InsufficientCapacity for NotEnoughPlayersToStart {
    fn requested(&self) -> i64 {
        self.requested
    }
    fn available(&self) -> i64 {
        self.available
    }
}

pub struct TournamentAlreadyCompleted;
impl CommandError for TournamentAlreadyCompleted {
    const CODE: &'static str = "TOURNAMENT_ALREADY_COMPLETED";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "Tournament is already completed";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl PreconditionError for TournamentAlreadyCompleted {}
impl StateAlreadyEntered for TournamentAlreadyCompleted {}

pub struct TournamentNotRunningOrPaused;
impl CommandError for TournamentNotRunningOrPaused {
    const CODE: &'static str = "TOURNAMENT_NOT_RUNNING_OR_PAUSED";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "Tournament must be running or paused to complete";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl PreconditionError for TournamentNotRunningOrPaused {}
impl StateMismatch for TournamentNotRunningOrPaused {}

pub struct AlreadyInHandForHand;
impl CommandError for AlreadyInHandForHand {
    const CODE: &'static str = "ALREADY_IN_HAND_FOR_HAND";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "Tournament is already in hand-for-hand mode";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl PreconditionError for AlreadyInHandForHand {}
impl StateAlreadyEntered for AlreadyInHandForHand {}

pub struct NotInHandForHand;
impl CommandError for NotInHandForHand {
    const CODE: &'static str = "NOT_IN_HAND_FOR_HAND";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "Tournament is not in hand-for-hand mode";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl PreconditionError for NotInHandForHand {}
impl StateMismatch for NotInHandForHand {}

pub struct TableNotActiveInHandForHand {
    pub table_root_hex: String,
}
impl CommandError for TableNotActiveInHandForHand {
    const CODE: &'static str = "TABLE_NOT_ACTIVE_IN_HAND_FOR_HAND";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str =
        "Table {table_root_hex} is not active in the current hand-for-hand round";
    fn fields(&self) -> Vec<(&'static str, String)> {
        vec![("table_root_hex", self.table_root_hex.clone())]
    }
}
impl PreconditionError for TableNotActiveInHandForHand {}
impl EntityNotInContainer for TableNotActiveInHandForHand {}

pub struct PlayerRootsRequired;
impl CommandError for PlayerRootsRequired {
    const CODE: &'static str = "PLAYER_ROOTS_REQUIRED";
    const STATUS: ErrorStatus = ErrorStatus::InvalidArgument;
    const TEMPLATE: &'static str = "at least one player_root is required";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl ValidationError for PlayerRootsRequired {}
impl FieldRequired for PlayerRootsRequired {}

pub struct AmountMustBePositive {
    pub value: i64,
}
impl CommandError for AmountMustBePositive {
    const CODE: &'static str = "AMOUNT_MUST_BE_POSITIVE";
    const STATUS: ErrorStatus = ErrorStatus::InvalidArgument;
    const TEMPLATE: &'static str = "amount must be positive, got {value}";
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

pub struct InvalidPenaltyType {
    pub value: String,
}
impl CommandError for InvalidPenaltyType {
    const CODE: &'static str = "INVALID_PENALTY_TYPE";
    const STATUS: ErrorStatus = ErrorStatus::InvalidArgument;
    const TEMPLATE: &'static str =
        "Penalty type {value} is not one of VERBAL_WARNING/MISSED_HAND/MISSED_ROUND/DISQUALIFIED";
    fn fields(&self) -> Vec<(&'static str, String)> {
        vec![("value", self.value.clone())]
    }
}
impl ValidationError for InvalidPenaltyType {}

pub struct PlayerNotOnPenalty {
    pub player_root_hex: String,
}
impl CommandError for PlayerNotOnPenalty {
    const CODE: &'static str = "PLAYER_NOT_ON_PENALTY";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "Player {player_root_hex} is not currently on penalty";
    fn fields(&self) -> Vec<(&'static str, String)> {
        vec![("player_root_hex", self.player_root_hex.clone())]
    }
}
impl PreconditionError for PlayerNotOnPenalty {}
impl StateMismatch for PlayerNotOnPenalty {}

pub struct NewHandsAlreadyHalted;
impl CommandError for NewHandsAlreadyHalted {
    const CODE: &'static str = "NEW_HANDS_ALREADY_HALTED";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "New hands are already halted";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl PreconditionError for NewHandsAlreadyHalted {}
impl StateAlreadyEntered for NewHandsAlreadyHalted {}

pub struct NewHandsNotHalted;
impl CommandError for NewHandsNotHalted {
    const CODE: &'static str = "NEW_HANDS_NOT_HALTED";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "Bag-and-tag requires new hands to be halted first";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl PreconditionError for NewHandsNotHalted {}
impl StateMismatch for NewHandsNotHalted {}

pub struct BlindStructureExhausted {
    pub current: i32,
    pub max_value: i32,
}
impl CommandError for BlindStructureExhausted {
    const CODE: &'static str = "BLIND_STRUCTURE_EXHAUSTED";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str =
        "Blind structure exhausted at level {current}; max defined level is {max_value}";
    fn fields(&self) -> Vec<(&'static str, String)> {
        vec![
            ("current", self.current.to_string()),
            ("max_value", self.max_value.to_string()),
        ]
    }
}
impl PreconditionError for BlindStructureExhausted {}
impl Exhausted for BlindStructureExhausted {
    fn current(&self) -> i64 {
        self.current as i64
    }
    fn max_value(&self) -> i64 {
        self.max_value as i64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use examples_utils::reject;

    #[test]
    fn buy_in_must_be_positive_carries_value() {
        let err = BuyInMustBePositive { value: -100 };
        assert_eq!(err.render(), "buy_in must be positive, got -100");
        let r = reject(BuyInMustBePositive { value: -100 });
        assert_eq!(r.code, "BUY_IN_MUST_BE_POSITIVE");
        assert_eq!(r.details.get("value"), Some(&"-100".to_string()));
    }

    #[test]
    fn min_exceeds_max_renders_both_fields() {
        let err = MinPlayersExceedsMax { lhs: 5, rhs: 4 };
        assert_eq!(
            err.render(),
            "min_players (5) cannot exceed max_players (4)"
        );
    }

    #[test]
    fn blind_structure_exhausted_carries_levels() {
        let err = BlindStructureExhausted {
            current: 3,
            max_value: 3,
        };
        assert_eq!(
            err.render(),
            "Blind structure exhausted at level 3; max defined level is 3"
        );
        let r = reject(BlindStructureExhausted {
            current: 3,
            max_value: 3,
        });
        assert_eq!(r.code, "BLIND_STRUCTURE_EXHAUSTED");
        assert_eq!(r.details.get("current"), Some(&"3".to_string()));
        assert_eq!(r.details.get("max_value"), Some(&"3".to_string()));
    }

    #[test]
    fn empty_field_errors_render_template_unchanged() {
        assert_eq!(
            TournamentAlreadyExists.render(),
            "Tournament already exists"
        );
        assert_eq!(TournamentNotFound.render(), "Tournament does not exist");
        let nepts = NotEnoughPlayersToStart {
            requested: 2,
            available: 1,
        };
        assert_eq!(
            nepts.render(),
            "Not enough players to start: requested 2, available 1"
        );
    }

    #[test]
    fn player_not_registered_carries_hex() {
        let err = PlayerNotRegistered {
            player_root_hex: "deadbeef".to_string(),
        };
        assert_eq!(
            err.render(),
            "Player deadbeef is not registered in this tournament"
        );
    }
}
