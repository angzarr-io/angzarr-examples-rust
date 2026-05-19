//! Hand-domain command error catalog.
//!
//! One [`CommandError`] impl per distinct rejection. Cucumber asserts on
//! [`CommandError::CODE`] and individual `details` fields; rendered text
//! is for human display only.

use examples_utils::error_shapes::{
    BoundKind, BoundViolation, ContainerFull, EntityNotInContainer, FieldRequired,
    IdentityMismatch, InsufficientCapacity, InvalidOperationInState, MustBePositive,
    PreconditionError, StateAlreadyEntered, StateMismatch, ValidationError, ValueOutOfRange,
    VariantMismatch,
};
use examples_utils::{CommandError, ErrorStatus};

// --- DealCards ---

pub struct HandAlreadyDealt;
impl CommandError for HandAlreadyDealt {
    const CODE: &'static str = "HAND_ALREADY_DEALT";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "Hand already dealt";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl PreconditionError for HandAlreadyDealt {}
impl StateAlreadyEntered for HandAlreadyDealt {}

/// One-off: bare state-precondition with no shape-fitting structure.
pub struct NoPlayersInHand;
impl CommandError for NoPlayersInHand {
    const CODE: &'static str = "NO_PLAYERS_IN_HAND";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "No players in hand";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl PreconditionError for NoPlayersInHand {}

pub struct NeedAtLeast2Players {
    pub got: i32,
}
impl CommandError for NeedAtLeast2Players {
    const CODE: &'static str = "NEED_AT_LEAST_2_PLAYERS";
    const STATUS: ErrorStatus = ErrorStatus::InvalidArgument;
    const TEMPLATE: &'static str = "Need at least 2 players, got {got}";
    fn fields(&self) -> Vec<(&'static str, String)> {
        vec![("got", self.got.to_string())]
    }
}
impl ValidationError for NeedAtLeast2Players {}
impl ValueOutOfRange for NeedAtLeast2Players {
    fn got(&self) -> i64 {
        self.got as i64
    }
}

// --- Shared lifecycle / player gates ---

pub struct HandNotDealt;
impl CommandError for HandNotDealt {
    const CODE: &'static str = "HAND_NOT_DEALT";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "Hand not dealt";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl PreconditionError for HandNotDealt {}
impl StateMismatch for HandNotDealt {}

pub struct HandAlreadyComplete;
impl CommandError for HandAlreadyComplete {
    const CODE: &'static str = "HAND_ALREADY_COMPLETE";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "Hand already complete";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl PreconditionError for HandAlreadyComplete {}
impl StateAlreadyEntered for HandAlreadyComplete {}

pub struct CannotPostAnteAfterBlinds;
impl CommandError for CannotPostAnteAfterBlinds {
    const CODE: &'static str = "CANNOT_POST_ANTE_AFTER_BLINDS";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "Antes must be posted before the small/big blind (TDA Rule 7)";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl PreconditionError for CannotPostAnteAfterBlinds {}

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

pub struct PlayerNotInHand;
impl CommandError for PlayerNotInHand {
    const CODE: &'static str = "PLAYER_NOT_IN_HAND";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "Player not in hand";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl PreconditionError for PlayerNotInHand {}
impl EntityNotInContainer for PlayerNotInHand {}

pub struct PlayerHasFolded;
impl CommandError for PlayerHasFolded {
    const CODE: &'static str = "PLAYER_HAS_FOLDED";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "Player has folded";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl PreconditionError for PlayerHasFolded {}
impl StateAlreadyEntered for PlayerHasFolded {}

pub struct PlayerIsAllIn;
impl CommandError for PlayerIsAllIn {
    const CODE: &'static str = "PLAYER_IS_ALL_IN";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "Player is all-in";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl PreconditionError for PlayerIsAllIn {}
impl StateAlreadyEntered for PlayerIsAllIn {}

// --- PostBlind ---

pub struct BlindAmountMustBePositive {
    pub value: i64,
}
impl CommandError for BlindAmountMustBePositive {
    const CODE: &'static str = "BLIND_AMOUNT_MUST_BE_POSITIVE";
    const STATUS: ErrorStatus = ErrorStatus::InvalidArgument;
    const TEMPLATE: &'static str = "Blind amount must be positive, got {value}";
    fn fields(&self) -> Vec<(&'static str, String)> {
        vec![("value", self.value.to_string())]
    }
}
impl ValidationError for BlindAmountMustBePositive {}
impl MustBePositive for BlindAmountMustBePositive {
    fn value(&self) -> i64 {
        self.value
    }
}

// --- PlayerAction ---

pub struct NotInBettingPhase;
impl CommandError for NotInBettingPhase {
    const CODE: &'static str = "NOT_IN_BETTING_PHASE";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "Not in betting phase";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl PreconditionError for NotInBettingPhase {}
impl StateMismatch for NotInBettingPhase {}

pub struct CannotCheckWithBet;
impl CommandError for CannotCheckWithBet {
    const CODE: &'static str = "CANNOT_CHECK_WITH_BET";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "Cannot check, there is a bet to call";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl PreconditionError for CannotCheckWithBet {}
impl InvalidOperationInState for CannotCheckWithBet {}

pub struct NothingToCall;
impl CommandError for NothingToCall {
    const CODE: &'static str = "NOTHING_TO_CALL";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "Nothing to call";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl PreconditionError for NothingToCall {}
impl InvalidOperationInState for NothingToCall {}

pub struct CannotBetOverExistingBet;
impl CommandError for CannotBetOverExistingBet {
    const CODE: &'static str = "CANNOT_BET_OVER_EXISTING_BET";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "Cannot bet, there is already a bet";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl PreconditionError for CannotBetOverExistingBet {}
impl InvalidOperationInState for CannotBetOverExistingBet {}

pub struct BetBelowMinRaise {
    pub got: i64,
    pub bound: i64,
}
impl CommandError for BetBelowMinRaise {
    const CODE: &'static str = "BET_BELOW_MIN_RAISE";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "Bet must be at least {bound}, got {got}";
    fn fields(&self) -> Vec<(&'static str, String)> {
        vec![
            ("got", self.got.to_string()),
            ("bound", self.bound.to_string()),
        ]
    }
}
impl PreconditionError for BetBelowMinRaise {}
impl BoundViolation for BetBelowMinRaise {
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

pub struct BetExceedsStack {
    pub got: i64,
    pub bound: i64,
}
impl CommandError for BetExceedsStack {
    const CODE: &'static str = "BET_EXCEEDS_STACK";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "Bet {got} exceeds stack {bound}";
    fn fields(&self) -> Vec<(&'static str, String)> {
        vec![
            ("got", self.got.to_string()),
            ("bound", self.bound.to_string()),
        ]
    }
}
impl PreconditionError for BetExceedsStack {}
impl BoundViolation for BetExceedsStack {
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

pub struct CannotRaiseWithoutBet;
impl CommandError for CannotRaiseWithoutBet {
    const CODE: &'static str = "CANNOT_RAISE_WITHOUT_BET";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "Cannot raise, there is no bet";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl PreconditionError for CannotRaiseWithoutBet {}
impl InvalidOperationInState for CannotRaiseWithoutBet {}

pub struct RaiseBelowMin {
    pub got: i64,
    pub bound: i64,
}
impl CommandError for RaiseBelowMin {
    const CODE: &'static str = "RAISE_BELOW_MIN";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "Raise must be at least {bound}, got {got}";
    fn fields(&self) -> Vec<(&'static str, String)> {
        vec![
            ("got", self.got.to_string()),
            ("bound", self.bound.to_string()),
        ]
    }
}
impl PreconditionError for RaiseBelowMin {}
impl BoundViolation for RaiseBelowMin {
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

pub struct RaiseExceedsStack {
    pub got: i64,
    pub bound: i64,
}
impl CommandError for RaiseExceedsStack {
    const CODE: &'static str = "RAISE_EXCEEDS_STACK";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "Raise {got} exceeds stack {bound}";
    fn fields(&self) -> Vec<(&'static str, String)> {
        vec![
            ("got", self.got.to_string()),
            ("bound", self.bound.to_string()),
        ]
    }
}
impl PreconditionError for RaiseExceedsStack {}
impl BoundViolation for RaiseExceedsStack {
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

pub struct InvalidAction {
    pub got: i32,
}
impl CommandError for InvalidAction {
    const CODE: &'static str = "INVALID_ACTION";
    const STATUS: ErrorStatus = ErrorStatus::InvalidArgument;
    const TEMPLATE: &'static str = "Invalid action: {got}";
    fn fields(&self) -> Vec<(&'static str, String)> {
        vec![("got", self.got.to_string())]
    }
}
impl ValidationError for InvalidAction {}
impl ValueOutOfRange for InvalidAction {
    fn got(&self) -> i64 {
        self.got as i64
    }
}

// --- DealCommunityCards ---

pub struct MustDealAtLeast1Card {
    pub got: i32,
    pub bound: i32,
}
impl CommandError for MustDealAtLeast1Card {
    const CODE: &'static str = "MUST_DEAL_AT_LEAST_1_CARD";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "Must deal at least {bound} card, got {got}";
    fn fields(&self) -> Vec<(&'static str, String)> {
        vec![
            ("got", self.got.to_string()),
            ("bound", self.bound.to_string()),
        ]
    }
}
impl PreconditionError for MustDealAtLeast1Card {}
impl BoundViolation for MustDealAtLeast1Card {
    fn got(&self) -> i64 {
        self.got as i64
    }
    fn bound(&self) -> i64 {
        self.bound as i64
    }
    fn kind(&self) -> BoundKind {
        BoundKind::BelowMin
    }
}

pub struct CommunityCardsNotUsedInVariant;
impl CommandError for CommunityCardsNotUsedInVariant {
    const CODE: &'static str = "COMMUNITY_CARDS_NOT_USED_IN_VARIANT";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "Community cards not used in this variant";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl PreconditionError for CommunityCardsNotUsedInVariant {}
impl VariantMismatch for CommunityCardsNotUsedInVariant {}

pub struct NoMorePhases;
impl CommandError for NoMorePhases {
    const CODE: &'static str = "NO_MORE_PHASES";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "No more phases";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl PreconditionError for NoMorePhases {}
impl ContainerFull for NoMorePhases {}

pub struct CannotDealMoreCommunityCards;
impl CommandError for CannotDealMoreCommunityCards {
    const CODE: &'static str = "CANNOT_DEAL_MORE_COMMUNITY_CARDS";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "Cannot deal more community cards";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl PreconditionError for CannotDealMoreCommunityCards {}
impl InvalidOperationInState for CannotDealMoreCommunityCards {}

pub struct WrongCardCountForPhase {
    pub expected: i32,
    pub got: i32,
    pub phase: String,
}
impl CommandError for WrongCardCountForPhase {
    const CODE: &'static str = "WRONG_CARD_COUNT_FOR_PHASE";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "Expected {expected} cards for phase {phase}, got {got}";
    fn fields(&self) -> Vec<(&'static str, String)> {
        vec![
            ("expected", self.expected.to_string()),
            ("got", self.got.to_string()),
            ("phase", self.phase.clone()),
        ]
    }
}
impl PreconditionError for WrongCardCountForPhase {}
impl IdentityMismatch for WrongCardCountForPhase {
    fn expected(&self) -> i64 {
        self.expected as i64
    }
    fn got(&self) -> i64 {
        self.got as i64
    }
}

pub struct NotEnoughCardsInDeck {
    pub requested: i32,
    pub available: i32,
}
impl CommandError for NotEnoughCardsInDeck {
    const CODE: &'static str = "NOT_ENOUGH_CARDS_IN_DECK";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str =
        "Not enough cards in deck: requested {requested}, available {available}";
    fn fields(&self) -> Vec<(&'static str, String)> {
        vec![
            ("requested", self.requested.to_string()),
            ("available", self.available.to_string()),
        ]
    }
}
impl PreconditionError for NotEnoughCardsInDeck {}
impl InsufficientCapacity for NotEnoughCardsInDeck {
    fn requested(&self) -> i64 {
        self.requested as i64
    }
    fn available(&self) -> i64 {
        self.available as i64
    }
}

// --- RequestDraw ---

pub struct DrawNotSupportedInVariant;
impl CommandError for DrawNotSupportedInVariant {
    const CODE: &'static str = "DRAW_NOT_SUPPORTED_IN_VARIANT";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "Draw not supported in this game variant";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl PreconditionError for DrawNotSupportedInVariant {}
impl VariantMismatch for DrawNotSupportedInVariant {}

pub struct NotInDrawPhase;
impl CommandError for NotInDrawPhase {
    const CODE: &'static str = "NOT_IN_DRAW_PHASE";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "Not in draw phase";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl PreconditionError for NotInDrawPhase {}
impl StateMismatch for NotInDrawPhase {}

pub struct TooManyDiscards {
    pub got: i32,
    pub bound: i32,
}
impl CommandError for TooManyDiscards {
    const CODE: &'static str = "TOO_MANY_DISCARDS";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "Cannot discard more than {bound} cards, got {got}";
    fn fields(&self) -> Vec<(&'static str, String)> {
        vec![
            ("got", self.got.to_string()),
            ("bound", self.bound.to_string()),
        ]
    }
}
impl PreconditionError for TooManyDiscards {}
impl BoundViolation for TooManyDiscards {
    fn got(&self) -> i64 {
        self.got as i64
    }
    fn bound(&self) -> i64 {
        self.bound as i64
    }
    fn kind(&self) -> BoundKind {
        BoundKind::AboveMax
    }
}

/// One-off: list-content uniqueness violation, no scalar fits a shape.
pub struct DuplicateCardIndices;
impl CommandError for DuplicateCardIndices {
    const CODE: &'static str = "DUPLICATE_CARD_INDICES";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "Duplicate card indices";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl PreconditionError for DuplicateCardIndices {}

pub struct InvalidCardIndex {
    pub got: i32,
}
impl CommandError for InvalidCardIndex {
    const CODE: &'static str = "INVALID_CARD_INDEX";
    const STATUS: ErrorStatus = ErrorStatus::InvalidArgument;
    const TEMPLATE: &'static str = "Invalid card index {got}, must be 0-4";
    fn fields(&self) -> Vec<(&'static str, String)> {
        vec![("got", self.got.to_string())]
    }
}
impl ValidationError for InvalidCardIndex {}
impl ValueOutOfRange for InvalidCardIndex {
    fn got(&self) -> i64 {
        self.got as i64
    }
}

// --- RevealCards ---

pub struct NotInShowdownPhase;
impl CommandError for NotInShowdownPhase {
    const CODE: &'static str = "NOT_IN_SHOWDOWN_PHASE";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "Not in showdown phase";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl PreconditionError for NotInShowdownPhase {}
impl StateMismatch for NotInShowdownPhase {}

// --- AwardPot ---

/// One-off: missing-content precondition, no scalar fits a shape.
pub struct NoAwardsSpecified;
impl CommandError for NoAwardsSpecified {
    const CODE: &'static str = "NO_AWARDS_SPECIFIED";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "No awards specified";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl PreconditionError for NoAwardsSpecified {}

pub struct AwardPlayerNotInHand;
impl CommandError for AwardPlayerNotInHand {
    const CODE: &'static str = "AWARD_PLAYER_NOT_IN_HAND";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "Award to player not in hand";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl PreconditionError for AwardPlayerNotInHand {}
impl EntityNotInContainer for AwardPlayerNotInHand {}

pub struct FoldedPlayerCannotWin;
impl CommandError for FoldedPlayerCannotWin {
    const CODE: &'static str = "FOLDED_PLAYER_CANNOT_WIN";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "Folded player cannot win";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl PreconditionError for FoldedPlayerCannotWin {}
impl InvalidOperationInState for FoldedPlayerCannotWin {}

pub struct AwardsExceedPot {
    pub got: i64,
    pub bound: i64,
}
impl CommandError for AwardsExceedPot {
    const CODE: &'static str = "AWARDS_EXCEED_POT";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "Awards {got} exceed pot total {bound}";
    fn fields(&self) -> Vec<(&'static str, String)> {
        vec![
            ("got", self.got.to_string()),
            ("bound", self.bound.to_string()),
        ]
    }
}
impl PreconditionError for AwardsExceedPot {}
impl BoundViolation for AwardsExceedPot {
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

// --- Action clock (TDA Rule 29) -----------------------------------------

pub struct NotActorsTurn {
    pub player_root_hex: String,
}
impl CommandError for NotActorsTurn {
    const CODE: &'static str = "NOT_ACTORS_TURN";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str =
        "Cannot start action clock on {player_root_hex}; that player is not the seat to act";
    fn fields(&self) -> Vec<(&'static str, String)> {
        vec![("player_root_hex", self.player_root_hex.clone())]
    }
}
impl PreconditionError for NotActorsTurn {}
impl StateMismatch for NotActorsTurn {}

pub struct ClockSecondsMustBePositive {
    pub value: i64,
}
impl CommandError for ClockSecondsMustBePositive {
    const CODE: &'static str = "CLOCK_SECONDS_MUST_BE_POSITIVE";
    const STATUS: ErrorStatus = ErrorStatus::InvalidArgument;
    const TEMPLATE: &'static str = "Clock seconds must be positive, got {value}";
    fn fields(&self) -> Vec<(&'static str, String)> {
        vec![("value", self.value.to_string())]
    }
}
impl ValidationError for ClockSecondsMustBePositive {}
impl MustBePositive for ClockSecondsMustBePositive {
    fn value(&self) -> i64 {
        self.value
    }
}

// --- Declare action (TDA Rule 49 / verbal) ------------------------------

pub struct VerbalActionUnknown {
    pub got: String,
}
impl CommandError for VerbalActionUnknown {
    const CODE: &'static str = "VERBAL_ACTION_UNKNOWN";
    const STATUS: ErrorStatus = ErrorStatus::InvalidArgument;
    const TEMPLATE: &'static str = "Verbal action {got} not recognised";
    fn fields(&self) -> Vec<(&'static str, String)> {
        vec![("got", self.got.clone())]
    }
}
impl ValidationError for VerbalActionUnknown {}

// --- Pull back prior chip (TDA Rule 46B) --------------------------------

pub struct ChipsPulledMustBePositive {
    pub value: i64,
}
impl CommandError for ChipsPulledMustBePositive {
    const CODE: &'static str = "CHIPS_PULLED_MUST_BE_POSITIVE";
    const STATUS: ErrorStatus = ErrorStatus::InvalidArgument;
    const TEMPLATE: &'static str = "chips_pulled must be positive, got {value}";
    fn fields(&self) -> Vec<(&'static str, String)> {
        vec![("value", self.value.to_string())]
    }
}
impl ValidationError for ChipsPulledMustBePositive {}
impl MustBePositive for ChipsPulledMustBePositive {
    fn value(&self) -> i64 {
        self.value
    }
}

// --- DeclareMisdeal (TDA Rule 35 family) --------------------------------

pub struct MisdealReasonRequired;
impl CommandError for MisdealReasonRequired {
    const CODE: &'static str = "MISDEAL_REASON_REQUIRED";
    const STATUS: ErrorStatus = ErrorStatus::InvalidArgument;
    const TEMPLATE: &'static str = "reason is required";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl ValidationError for MisdealReasonRequired {}
impl FieldRequired for MisdealReasonRequired {}

// --- ReportFouledDeck (TDA Rule 35E) ------------------------------------

pub struct DuplicateCardRequired;
impl CommandError for DuplicateCardRequired {
    const CODE: &'static str = "DUPLICATE_CARD_REQUIRED";
    const STATUS: ErrorStatus = ErrorStatus::InvalidArgument;
    const TEMPLATE: &'static str = "duplicate_card is required";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl ValidationError for DuplicateCardRequired {}
impl FieldRequired for DuplicateCardRequired {}

// --- RedealHand (TDA Rule 35C) -----------------------------------------

pub struct TableRootRequired;
impl CommandError for TableRootRequired {
    const CODE: &'static str = "TABLE_ROOT_REQUIRED";
    const STATUS: ErrorStatus = ErrorStatus::InvalidArgument;
    const TEMPLATE: &'static str = "table_root is required";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl ValidationError for TableRootRequired {}
impl FieldRequired for TableRootRequired {}

pub struct BlindLevelMustBeNonNegative {
    pub value: i64,
}
impl CommandError for BlindLevelMustBeNonNegative {
    const CODE: &'static str = "BLIND_LEVEL_MUST_BE_NON_NEGATIVE";
    const STATUS: ErrorStatus = ErrorStatus::InvalidArgument;
    const TEMPLATE: &'static str = "level must be non-negative, got {value}";
    fn fields(&self) -> Vec<(&'static str, String)> {
        vec![("value", self.value.to_string())]
    }
}
impl ValidationError for BlindLevelMustBeNonNegative {}
impl ValueOutOfRange for BlindLevelMustBeNonNegative {
    fn got(&self) -> i64 {
        self.value
    }
}

// --- ReplaceButtonCard / ReplaceSeventhStreetCard / ReportExposedStudDowncard ---

pub struct ReplacementCardRequired;
impl CommandError for ReplacementCardRequired {
    const CODE: &'static str = "REPLACEMENT_CARD_REQUIRED";
    const STATUS: ErrorStatus = ErrorStatus::InvalidArgument;
    const TEMPLATE: &'static str = "replacement_card is required";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl ValidationError for ReplacementCardRequired {}
impl FieldRequired for ReplacementCardRequired {}

pub struct ExposedCardRequired;
impl CommandError for ExposedCardRequired {
    const CODE: &'static str = "EXPOSED_CARD_REQUIRED";
    const STATUS: ErrorStatus = ErrorStatus::InvalidArgument;
    const TEMPLATE: &'static str = "exposed_card is required";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl ValidationError for ExposedCardRequired {}
impl FieldRequired for ExposedCardRequired {}

pub struct OriginalCardRequired;
impl CommandError for OriginalCardRequired {
    const CODE: &'static str = "ORIGINAL_CARD_REQUIRED";
    const STATUS: ErrorStatus = ErrorStatus::InvalidArgument;
    const TEMPLATE: &'static str = "original_card is required";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl ValidationError for OriginalCardRequired {}
impl FieldRequired for OriginalCardRequired {}

// --- DealStudStreet / DealStudCommunityCard / ScrambleAllDownCards ---

pub struct InvalidStudStreet {
    pub got: i32,
}
impl CommandError for InvalidStudStreet {
    const CODE: &'static str = "INVALID_STUD_STREET";
    const STATUS: ErrorStatus = ErrorStatus::InvalidArgument;
    const TEMPLATE: &'static str = "invalid stud street: {got}";
    fn fields(&self) -> Vec<(&'static str, String)> {
        vec![("got", self.got.to_string())]
    }
}
impl ValidationError for InvalidStudStreet {}
impl ValueOutOfRange for InvalidStudStreet {
    fn got(&self) -> i64 {
        self.got as i64
    }
}

pub struct NoUpCardsProvided;
impl CommandError for NoUpCardsProvided {
    const CODE: &'static str = "NO_UP_CARDS_PROVIDED";
    const STATUS: ErrorStatus = ErrorStatus::InvalidArgument;
    const TEMPLATE: &'static str = "up_cards must not be empty";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl ValidationError for NoUpCardsProvided {}

pub struct NoSharedWithProvided;
impl CommandError for NoSharedWithProvided {
    const CODE: &'static str = "NO_SHARED_WITH_PROVIDED";
    const STATUS: ErrorStatus = ErrorStatus::InvalidArgument;
    const TEMPLATE: &'static str = "shared_with must not be empty";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl ValidationError for NoSharedWithProvided {}

pub struct RngSeedRequired;
impl CommandError for RngSeedRequired {
    const CODE: &'static str = "RNG_SEED_REQUIRED";
    const STATUS: ErrorStatus = ErrorStatus::InvalidArgument;
    const TEMPLATE: &'static str = "rng_seed must not be empty";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl ValidationError for RngSeedRequired {}
impl FieldRequired for RngSeedRequired {}

// --- CorrectBringIn -----------------------------------------------------

pub struct IncorrectRootRequired;
impl CommandError for IncorrectRootRequired {
    const CODE: &'static str = "INCORRECT_ROOT_REQUIRED";
    const STATUS: ErrorStatus = ErrorStatus::InvalidArgument;
    const TEMPLATE: &'static str = "incorrect_root is required";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl ValidationError for IncorrectRootRequired {}
impl FieldRequired for IncorrectRootRequired {}

pub struct CorrectRootRequired;
impl CommandError for CorrectRootRequired {
    const CODE: &'static str = "CORRECT_ROOT_REQUIRED";
    const STATUS: ErrorStatus = ErrorStatus::InvalidArgument;
    const TEMPLATE: &'static str = "correct_root is required";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl ValidationError for CorrectRootRequired {}
impl FieldRequired for CorrectRootRequired {}

pub struct ReturnedAmountMustBeNonNegative {
    pub value: i64,
}
impl CommandError for ReturnedAmountMustBeNonNegative {
    const CODE: &'static str = "RETURNED_AMOUNT_MUST_BE_NON_NEGATIVE";
    const STATUS: ErrorStatus = ErrorStatus::InvalidArgument;
    const TEMPLATE: &'static str = "returned_amount must be non-negative, got {value}";
    fn fields(&self) -> Vec<(&'static str, String)> {
        vec![("value", self.value.to_string())]
    }
}
impl ValidationError for ReturnedAmountMustBeNonNegative {}
impl ValueOutOfRange for ReturnedAmountMustBeNonNegative {
    fn got(&self) -> i64 {
        self.value
    }
}

pub struct IncorrectAndCorrectRootIdentical;
impl CommandError for IncorrectAndCorrectRootIdentical {
    const CODE: &'static str = "INCORRECT_AND_CORRECT_ROOT_IDENTICAL";
    const STATUS: ErrorStatus = ErrorStatus::InvalidArgument;
    const TEMPLATE: &'static str = "incorrect_root and correct_root must differ";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl ValidationError for IncorrectAndCorrectRootIdentical {}

// --- DeclareMisdeal SA-gate (TDA Rule 35-D) -----------------------------

pub struct PostSubstantialActionMisdeal;
impl CommandError for PostSubstantialActionMisdeal {
    const CODE: &'static str = "POST_SUBSTANTIAL_ACTION_MISDEAL";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str =
        "Cannot declare a misdeal after substantial action (TDA Rule 35-D)";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl PreconditionError for PostSubstantialActionMisdeal {}
impl InvalidOperationInState for PostSubstantialActionMisdeal {}

// --- ReplaceButtonCard (TDA Rule 37) ------------------------------------

pub struct NotOnButton;
impl CommandError for NotOnButton {
    const CODE: &'static str = "NOT_ON_BUTTON";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "Named player is not on the dealer button";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl PreconditionError for NotOnButton {}
impl StateMismatch for NotOnButton {}

// --- ReplaceSeventhStreetCard (TDA RP-10B) ------------------------------

pub struct NotSeventhStreet;
impl CommandError for NotSeventhStreet {
    const CODE: &'static str = "NOT_SEVENTH_STREET";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "Replacement is only valid on 7th street";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl PreconditionError for NotSeventhStreet {}
impl StateMismatch for NotSeventhStreet {}

// --- CorrectBringIn correction window (WSOP §SC Stud #5) ----------------

pub struct OutsideCorrectionWindow;
impl CommandError for OutsideCorrectionWindow {
    const CODE: &'static str = "OUTSIDE_CORRECTION_WINDOW";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str =
        "Bring-in correction window has closed; next player already acted";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl PreconditionError for OutsideCorrectionWindow {}
impl StateMismatch for OutsideCorrectionWindow {}

// --- Correct illegal bet (TDA Rule 52B) ---------------------------------

pub struct CorrectionReasonRequired;
impl CommandError for CorrectionReasonRequired {
    const CODE: &'static str = "CORRECTION_REASON_REQUIRED";
    const STATUS: ErrorStatus = ErrorStatus::InvalidArgument;
    const TEMPLATE: &'static str = "reason is required";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl ValidationError for CorrectionReasonRequired {}
impl FieldRequired for CorrectionReasonRequired {}

pub struct CorrectedAmountMustBePositive {
    pub value: i64,
}
impl CommandError for CorrectedAmountMustBePositive {
    const CODE: &'static str = "CORRECTED_AMOUNT_MUST_BE_POSITIVE";
    const STATUS: ErrorStatus = ErrorStatus::InvalidArgument;
    const TEMPLATE: &'static str = "corrected_amount must be positive, got {value}";
    fn fields(&self) -> Vec<(&'static str, String)> {
        vec![("value", self.value.to_string())]
    }
}
impl ValidationError for CorrectedAmountMustBePositive {}
impl MustBePositive for CorrectedAmountMustBePositive {
    fn value(&self) -> i64 {
        self.value
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use examples_utils::reject;

    #[test]
    fn need_at_least_2_publishes_code_and_invalid_argument_status() {
        let r = reject(NeedAtLeast2Players { got: 1 });
        assert_eq!(r.code, "NEED_AT_LEAST_2_PLAYERS");
        assert_eq!(r.status_code, "INVALID_ARGUMENT");
        assert_eq!(r.details.get("got"), Some(&"1".to_string()));
    }

    #[test]
    fn bet_below_min_raise_renders_with_both_fields_and_carries_details() {
        let err = BetBelowMinRaise {
            got: 50,
            bound: 200,
        };
        assert_eq!(err.render(), "Bet must be at least 200, got 50");
        let r = reject(BetBelowMinRaise {
            got: 50,
            bound: 200,
        });
        assert_eq!(r.code, "BET_BELOW_MIN_RAISE");
        assert_eq!(r.status_code, "FAILED_PRECONDITION");
        assert_eq!(r.details.get("got"), Some(&"50".to_string()));
        assert_eq!(r.details.get("bound"), Some(&"200".to_string()));
    }

    #[test]
    fn wrong_card_count_for_phase_renders_three_field_template() {
        let err = WrongCardCountForPhase {
            expected: 3,
            got: 1,
            phase: "FLOP".to_string(),
        };
        assert_eq!(err.render(), "Expected 3 cards for phase FLOP, got 1");
        let r = reject(WrongCardCountForPhase {
            expected: 3,
            got: 1,
            phase: "FLOP".to_string(),
        });
        assert_eq!(r.code, "WRONG_CARD_COUNT_FOR_PHASE");
        assert_eq!(r.details.get("expected"), Some(&"3".to_string()));
        assert_eq!(r.details.get("got"), Some(&"1".to_string()));
        assert_eq!(r.details.get("phase"), Some(&"FLOP".to_string()));
    }

    #[test]
    fn no_field_errors_render_template_unchanged() {
        assert_eq!(HandAlreadyDealt.render(), "Hand already dealt");
        assert_eq!(HandNotDealt.render(), "Hand not dealt");
        assert_eq!(PlayerNotInHand.render(), "Player not in hand");
        assert_eq!(NothingToCall.render(), "Nothing to call");
        assert_eq!(NoAwardsSpecified.render(), "No awards specified");
    }

    #[test]
    fn awards_exceed_pot_renders_two_field_template() {
        let err = AwardsExceedPot { got: 50, bound: 30 };
        assert_eq!(err.render(), "Awards 50 exceed pot total 30");
    }

    #[test]
    fn invalid_card_index_carries_got_field() {
        let r = reject(InvalidCardIndex { got: 7 });
        assert_eq!(r.code, "INVALID_CARD_INDEX");
        assert_eq!(r.details.get("got"), Some(&"7".to_string()));
        assert_eq!(
            InvalidCardIndex { got: 7 }.render(),
            "Invalid card index 7, must be 0-4"
        );
    }

    #[test]
    fn template_is_static_for_log_greppability() {
        let a = BlindAmountMustBePositive { value: -1 };
        let b = BlindAmountMustBePositive { value: -100 };
        let r_a = reject(BlindAmountMustBePositive { value: a.value });
        let r_b = reject(BlindAmountMustBePositive { value: b.value });
        assert_eq!(r_a.message, r_b.message);
        assert_ne!(a.render(), b.render());
    }
}
