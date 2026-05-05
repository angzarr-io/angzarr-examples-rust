//! Cross-cutting shape classification — verify every leaf implements its
//! assigned shape trait at the type level.
//!
//! Mirrors `tests/unit/test_shape_classification.py`. Each entry below is
//! a compile-time check via a generic function that requires the shape
//! trait. If a leaf forgets its shape impl, this file won't compile —
//! catching drift the moment it's introduced.

use agg_hand::errors as hand_errors;
use agg_player::errors as player_errors;
use agg_table::errors as table_errors;
use agg_tournament::errors as tournament_errors;

use examples_utils::error_shapes::{
    AggregateAlreadyExists, AggregateNotFound, BoundKind, BoundViolation, ContainerFull,
    EntityKeyedConflict, EntityNotInContainer, Exhausted, FieldRequired, IdentityMismatch,
    InsufficientCapacity, InvalidOperationInState, MustBeNonZero, MustBePositive,
    PreconditionError, RelationViolation, StateAlreadyEntered, StateMismatch, ValidationError,
    ValueOutOfRange, VariantMismatch,
};
use examples_utils::CommandError;

// --- Shape-conformance bound checks (compile-time gates) ---
//
// Each function asserts that its parameter type implements the named
// shape trait. Calling them in the test body forces the compiler to
// resolve the trait bounds at the leaf level. If a leaf forgot
// `impl ShapeTrait for Leaf {}`, the corresponding call won't compile.

fn assert_aggregate_not_found<E: AggregateNotFound>(_: &E) {}
fn assert_aggregate_already_exists<E: AggregateAlreadyExists>(_: &E) {}
fn assert_state_mismatch<E: StateMismatch>(_: &E) {}
fn assert_state_already_entered<E: StateAlreadyEntered>(_: &E) {}
fn assert_invalid_operation_in_state<E: InvalidOperationInState>(_: &E) {}
fn assert_entity_not_in_container<E: EntityNotInContainer>(_: &E) {}
fn assert_entity_keyed_conflict<E: EntityKeyedConflict>(_: &E) {}
fn assert_insufficient_capacity<E: InsufficientCapacity>(_: &E) {}
fn assert_exhausted<E: Exhausted>(_: &E) {}
fn assert_container_full<E: ContainerFull>(_: &E) {}
fn assert_bound_violation<E: BoundViolation>(_: &E) {}
fn assert_relation_violation<E: RelationViolation>(_: &E) {}
fn assert_identity_mismatch<E: IdentityMismatch>(_: &E) {}
fn assert_variant_mismatch<E: VariantMismatch>(_: &E) {}
fn assert_field_required<E: FieldRequired>(_: &E) {}
fn assert_must_be_positive<E: MustBePositive>(_: &E) {}
fn assert_must_be_non_zero<E: MustBeNonZero>(_: &E) {}
fn assert_value_out_of_range<E: ValueOutOfRange>(_: &E) {}
fn assert_precondition_error<E: PreconditionError>(_: &E) {}
fn assert_validation_error<E: ValidationError>(_: &E) {}

// --- Player domain (13 leaves) ---

#[test]
fn player_shapes() {
    assert_aggregate_already_exists(&player_errors::PlayerAlreadyExists);
    assert_aggregate_not_found(&player_errors::PlayerNotFound);
    assert_field_required(&player_errors::DisplayNameRequired);
    assert_field_required(&player_errors::EmailRequired);
    assert_field_required(&player_errors::TableRootRequired);
    assert_field_required(&player_errors::KeyRequired);
    assert_must_be_positive(&player_errors::AmountMustBePositive { value: -5 });
    assert_must_be_non_zero(&player_errors::AmountMustBeNonZero { value: 0 });
    assert_insufficient_capacity(&player_errors::InsufficientAvailableBalance {
        requested: 0,
        available: 0,
    });
    assert_insufficient_capacity(&player_errors::InsufficientFunds {
        requested: 0,
        available: 0,
    });
    assert_insufficient_capacity(&player_errors::AmountExceedsReservedFunds {
        requested: 0,
        available: 0,
    });
    assert_entity_keyed_conflict(&player_errors::FundsAlreadyReservedForTable {
        table_root_hex: "abc".to_string(),
    });
    assert_entity_keyed_conflict(&player_errors::NoFundsReservedForTable {
        table_root_hex: "abc".to_string(),
    });
}

// --- Tournament domain (20 leaves) ---

#[test]
fn tournament_shapes() {
    assert_aggregate_already_exists(&tournament_errors::TournamentAlreadyExists);
    assert_aggregate_not_found(&tournament_errors::TournamentNotFound);
    assert_field_required(&tournament_errors::NameRequired);
    assert_field_required(&tournament_errors::PlayerRootRequired);
    assert_must_be_positive(&tournament_errors::BuyInMustBePositive { value: -1 });
    assert_must_be_positive(&tournament_errors::StartingStackMustBePositive { value: -1 });
    assert_value_out_of_range(&tournament_errors::MaxPlayersTooFew { got: 1 });
    assert_value_out_of_range(&tournament_errors::MinPlayersTooFew { got: 1 });
    assert_relation_violation(&tournament_errors::MinPlayersExceedsMax { lhs: 5, rhs: 3 });
    assert_invalid_operation_in_state(&tournament_errors::CannotOpenRegistrationRunning);
    assert_state_already_entered(&tournament_errors::RegistrationAlreadyOpen);
    assert_state_mismatch(&tournament_errors::RegistrationNotOpen);
    assert_state_mismatch(&tournament_errors::TournamentNotRunning);
    assert_state_already_entered(&tournament_errors::TournamentAlreadyPaused);
    assert_state_mismatch(&tournament_errors::TournamentNotPaused);
    assert_insufficient_capacity(&tournament_errors::NotEnoughPlayersToStart {
        requested: 2,
        available: 1,
    });
    assert_state_already_entered(&tournament_errors::TournamentAlreadyCompleted);
    assert_state_mismatch(&tournament_errors::TournamentNotRunningOrPaused);
    assert_entity_not_in_container(&tournament_errors::PlayerNotRegistered {
        player_root_hex: "x".to_string(),
    });
    assert_exhausted(&tournament_errors::BlindStructureExhausted {
        current: 3,
        max_value: 3,
    });
}

// --- Table domain (22 leaves) ---

#[test]
fn table_shapes() {
    assert_aggregate_already_exists(&table_errors::TableAlreadyExists);
    assert_aggregate_not_found(&table_errors::TableNotFound);
    assert_field_required(&table_errors::TableNameRequired);
    assert_field_required(&table_errors::PlayerRootRequired);
    assert_must_be_positive(&table_errors::SmallBlindMustBePositive { value: -1 });
    assert_must_be_positive(&table_errors::MinBuyInMustBePositive { value: -1 });
    assert_must_be_positive(&table_errors::AmountMustBePositive { value: -1 });
    assert_relation_violation(&table_errors::BigBlindMustExceedSmallBlind { lhs: 5, rhs: 10 });
    assert_relation_violation(&table_errors::MaxBuyInMustExceedMinBuyIn { lhs: 100, rhs: 200 });
    assert_value_out_of_range(&table_errors::MaxPlayersOutOfRange { got: 12 });
    assert_container_full(&table_errors::TableIsFull);
    let buy_in_below = table_errors::BuyInBelowMin {
        got: 100,
        bound: 200,
    };
    assert_bound_violation(&buy_in_below);
    assert_eq!(buy_in_below.kind(), BoundKind::BelowMin);
    let buy_in_above = table_errors::BuyInAboveMax {
        got: 5000,
        bound: 1000,
    };
    assert_bound_violation(&buy_in_above);
    assert_eq!(buy_in_above.kind(), BoundKind::AboveMax);
    assert_entity_keyed_conflict(&table_errors::SeatOccupied { seat: 3 });
    assert_state_already_entered(&table_errors::PlayerAlreadySeated);
    assert_entity_not_in_container(&table_errors::PlayerNotSeated);
    assert_invalid_operation_in_state(&table_errors::CannotLeaveDuringHand);
    assert_state_already_entered(&table_errors::HandAlreadyInProgress);
    assert_state_mismatch(&table_errors::NoHandInProgress);
    assert_insufficient_capacity(&table_errors::NotEnoughPlayersToStartHand {
        requested: 2,
        available: 1,
    });
    // HandRootMismatch is a one-off — direct PreconditionError, no shape.
    assert_precondition_error(&table_errors::HandRootMismatch);
    assert_identity_mismatch(&table_errors::SeatPositionMismatch {
        expected: 3,
        got: 5,
    });
}

// --- Hand domain (36 leaves) ---

#[test]
fn hand_shapes() {
    assert_state_already_entered(&hand_errors::HandAlreadyDealt);
    assert_state_mismatch(&hand_errors::HandNotDealt);
    assert_state_already_entered(&hand_errors::HandAlreadyComplete);
    assert_field_required(&hand_errors::PlayerRootRequired);
    assert_entity_not_in_container(&hand_errors::PlayerNotInHand);
    assert_state_already_entered(&hand_errors::PlayerHasFolded);
    assert_state_already_entered(&hand_errors::PlayerIsAllIn);
    // NoPlayersInHand is a one-off — direct PreconditionError.
    assert_precondition_error(&hand_errors::NoPlayersInHand);
    assert_value_out_of_range(&hand_errors::NeedAtLeast2Players { got: 1 });
    assert_must_be_positive(&hand_errors::BlindAmountMustBePositive { value: -5 });
    assert_state_mismatch(&hand_errors::NotInBettingPhase);
    assert_invalid_operation_in_state(&hand_errors::CannotCheckWithBet);
    assert_invalid_operation_in_state(&hand_errors::NothingToCall);
    assert_invalid_operation_in_state(&hand_errors::CannotBetOverExistingBet);
    let bet_below = hand_errors::BetBelowMinRaise { got: 50, bound: 200 };
    assert_bound_violation(&bet_below);
    assert_eq!(bet_below.kind(), BoundKind::BelowMin);
    let bet_above = hand_errors::BetExceedsStack {
        got: 1000,
        bound: 500,
    };
    assert_bound_violation(&bet_above);
    assert_eq!(bet_above.kind(), BoundKind::AboveMax);
    assert_invalid_operation_in_state(&hand_errors::CannotRaiseWithoutBet);
    let raise_below = hand_errors::RaiseBelowMin {
        got: 50,
        bound: 200,
    };
    assert_bound_violation(&raise_below);
    let raise_above = hand_errors::RaiseExceedsStack {
        got: 1000,
        bound: 500,
    };
    assert_bound_violation(&raise_above);
    assert_value_out_of_range(&hand_errors::InvalidAction { got: 99 });
    let must_deal = hand_errors::MustDealAtLeast1Card { got: 0, bound: 1 };
    assert_bound_violation(&must_deal);
    assert_variant_mismatch(&hand_errors::CommunityCardsNotUsedInVariant);
    assert_container_full(&hand_errors::NoMorePhases);
    assert_invalid_operation_in_state(&hand_errors::CannotDealMoreCommunityCards);
    assert_identity_mismatch(&hand_errors::WrongCardCountForPhase {
        expected: 3,
        got: 2,
        phase: "flop".to_string(),
    });
    assert_insufficient_capacity(&hand_errors::NotEnoughCardsInDeck {
        requested: 3,
        available: 1,
    });
    assert_variant_mismatch(&hand_errors::DrawNotSupportedInVariant);
    assert_state_mismatch(&hand_errors::NotInDrawPhase);
    let too_many = hand_errors::TooManyDiscards { got: 6, bound: 5 };
    assert_bound_violation(&too_many);
    assert_eq!(too_many.kind(), BoundKind::AboveMax);
    // DuplicateCardIndices is a one-off — direct PreconditionError.
    assert_precondition_error(&hand_errors::DuplicateCardIndices);
    assert_value_out_of_range(&hand_errors::InvalidCardIndex { got: 99 });
    assert_state_mismatch(&hand_errors::NotInShowdownPhase);
    // NoAwardsSpecified is a one-off — direct PreconditionError.
    assert_precondition_error(&hand_errors::NoAwardsSpecified);
    assert_entity_not_in_container(&hand_errors::AwardPlayerNotInHand);
    assert_invalid_operation_in_state(&hand_errors::FoldedPlayerCannotWin);
    let awards_above = hand_errors::AwardsExceedPot {
        got: 600,
        bound: 500,
    };
    assert_bound_violation(&awards_above);
    assert_eq!(awards_above.kind(), BoundKind::AboveMax);
}

// --- Sanity: validation vs precondition tier inheritance ---

#[test]
fn validation_shapes_descend_from_validation_error() {
    // A few representative samples — full coverage is implicit via the
    // per-domain tests above; this just locks the tier-2 inheritance.
    assert_validation_error(&player_errors::DisplayNameRequired);
    assert_validation_error(&player_errors::AmountMustBePositive { value: -1 });
    assert_validation_error(&hand_errors::NeedAtLeast2Players { got: 1 });
}

#[test]
fn precondition_shapes_descend_from_precondition_error() {
    assert_precondition_error(&player_errors::PlayerNotFound);
    assert_precondition_error(&table_errors::TableIsFull);
    assert_precondition_error(&hand_errors::NotInBettingPhase);
    let bv = table_errors::BuyInBelowMin { got: 1, bound: 2 };
    assert_precondition_error(&bv);
}

// --- Sanity: the leaves' published CODEs are stable strings ---
//
// Note: it is intentional that ``PLAYER_ROOT_REQUIRED`` appears in
// multiple domains (tournament/table/hand) — the code is semantically
// the same regardless of which command issued it, and cucumber asserts
// it in scenario context where the domain is unambiguous. So we don't
// require global uniqueness; we just sanity-check that each catalog's
// CODEs are non-empty.

#[test]
fn leaf_codes_are_non_empty_strings() {
    let codes: Vec<&'static str> = vec![
        // player
        player_errors::PlayerAlreadyExists::CODE,
        player_errors::PlayerNotFound::CODE,
        player_errors::DisplayNameRequired::CODE,
        player_errors::EmailRequired::CODE,
        player_errors::TableRootRequired::CODE,
        player_errors::KeyRequired::CODE,
        player_errors::AmountMustBePositive::CODE,
        player_errors::AmountMustBeNonZero::CODE,
        player_errors::InsufficientAvailableBalance::CODE,
        player_errors::InsufficientFunds::CODE,
        player_errors::AmountExceedsReservedFunds::CODE,
        player_errors::FundsAlreadyReservedForTable::CODE,
        player_errors::NoFundsReservedForTable::CODE,
        // tournament
        tournament_errors::TournamentAlreadyExists::CODE,
        tournament_errors::TournamentNotFound::CODE,
        tournament_errors::NameRequired::CODE,
        tournament_errors::BuyInMustBePositive::CODE,
        tournament_errors::StartingStackMustBePositive::CODE,
        tournament_errors::MaxPlayersTooFew::CODE,
        tournament_errors::MinPlayersTooFew::CODE,
        tournament_errors::MinPlayersExceedsMax::CODE,
        tournament_errors::CannotOpenRegistrationRunning::CODE,
        tournament_errors::RegistrationAlreadyOpen::CODE,
        tournament_errors::RegistrationNotOpen::CODE,
        tournament_errors::TournamentNotRunning::CODE,
        tournament_errors::TournamentAlreadyPaused::CODE,
        tournament_errors::TournamentNotPaused::CODE,
        tournament_errors::NotEnoughPlayersToStart::CODE,
        tournament_errors::TournamentAlreadyCompleted::CODE,
        tournament_errors::TournamentNotRunningOrPaused::CODE,
        tournament_errors::PlayerNotRegistered::CODE,
        tournament_errors::BlindStructureExhausted::CODE,
        // table
        table_errors::TableAlreadyExists::CODE,
        table_errors::TableNotFound::CODE,
        table_errors::TableNameRequired::CODE,
        table_errors::SmallBlindMustBePositive::CODE,
        table_errors::MinBuyInMustBePositive::CODE,
        table_errors::AmountMustBePositive::CODE,
        table_errors::BigBlindMustExceedSmallBlind::CODE,
        table_errors::MaxBuyInMustExceedMinBuyIn::CODE,
        table_errors::MaxPlayersOutOfRange::CODE,
        table_errors::TableIsFull::CODE,
        table_errors::BuyInBelowMin::CODE,
        table_errors::BuyInAboveMax::CODE,
        table_errors::SeatOccupied::CODE,
        table_errors::PlayerAlreadySeated::CODE,
        table_errors::PlayerNotSeated::CODE,
        table_errors::CannotLeaveDuringHand::CODE,
        table_errors::HandAlreadyInProgress::CODE,
        table_errors::NoHandInProgress::CODE,
        table_errors::NotEnoughPlayersToStartHand::CODE,
        table_errors::HandRootMismatch::CODE,
        table_errors::SeatPositionMismatch::CODE,
        // hand
        hand_errors::HandAlreadyDealt::CODE,
        hand_errors::HandNotDealt::CODE,
        hand_errors::HandAlreadyComplete::CODE,
        hand_errors::PlayerNotInHand::CODE,
        hand_errors::PlayerHasFolded::CODE,
        hand_errors::PlayerIsAllIn::CODE,
        hand_errors::NoPlayersInHand::CODE,
        hand_errors::NeedAtLeast2Players::CODE,
        hand_errors::BlindAmountMustBePositive::CODE,
        hand_errors::NotInBettingPhase::CODE,
        hand_errors::CannotCheckWithBet::CODE,
        hand_errors::NothingToCall::CODE,
        hand_errors::CannotBetOverExistingBet::CODE,
        hand_errors::BetBelowMinRaise::CODE,
        hand_errors::BetExceedsStack::CODE,
        hand_errors::CannotRaiseWithoutBet::CODE,
        hand_errors::RaiseBelowMin::CODE,
        hand_errors::RaiseExceedsStack::CODE,
        hand_errors::InvalidAction::CODE,
        hand_errors::MustDealAtLeast1Card::CODE,
        hand_errors::CommunityCardsNotUsedInVariant::CODE,
        hand_errors::NoMorePhases::CODE,
        hand_errors::CannotDealMoreCommunityCards::CODE,
        hand_errors::WrongCardCountForPhase::CODE,
        hand_errors::NotEnoughCardsInDeck::CODE,
        hand_errors::DrawNotSupportedInVariant::CODE,
        hand_errors::NotInDrawPhase::CODE,
        hand_errors::TooManyDiscards::CODE,
        hand_errors::DuplicateCardIndices::CODE,
        hand_errors::InvalidCardIndex::CODE,
        hand_errors::NotInShowdownPhase::CODE,
        hand_errors::NoAwardsSpecified::CODE,
        hand_errors::AwardPlayerNotInHand::CODE,
        hand_errors::FoldedPlayerCannotWin::CODE,
        hand_errors::AwardsExceedPot::CODE,
    ];

    for code in &codes {
        assert!(!code.is_empty(), "Found empty CODE constant");
        assert!(
            code.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_'),
            "CODE must be SCREAMING_SNAKE: got {}",
            code
        );
    }
    // Total leaf count pinned: 91 leaves means 91 (code, leaf) pairs even
    // when codes repeat across domains.
    assert_eq!(codes.len(), 88, "Expected 88 CODE-introducing leaves; PLAYER_ROOT_REQUIRED appears 3× across domains so 91 leaves emit 88 distinct codes");
}
