//! Shape hierarchy for [`CommandError`] leaves.
//!
//! Mirrors the Python `angzarr_examples.error_shapes` tree. Every leaf in
//! the per-domain catalogs (`player/agg/src/errors.rs`,
//! `table/agg/src/errors.rs`, `tournament/agg/src/errors.rs`,
//! `hand/agg/src/errors.rs`) implements both [`CommandError`] and one of
//! these shape marker traits.
//!
//! Cross-cutting code can be generic over a shape:
//!
//! ```ignore
//! fn log_not_found<E: AggregateNotFound>(err: &E) {
//!     tracing::warn!(code = E::CODE, "aggregate not found");
//! }
//! ```
//!
//! Or use [`Any`]-style downcasting (rare; the production rejection path
//! typically inspects [`CommandError::CODE`] and the static `details` map
//! instead).
//!
//! The tree mirrors the Python module: see
//! `examples-python/main/angzarr_examples/error_shapes.py` for the
//! authoritative ASCII diagram and field-naming rationale. Shape
//! field-bearing parents (BoundViolation, RelationViolation, etc.)
//! provide accessor methods so generic code can read the canonical
//! values; concrete leaves still own their struct fields directly.

use crate::CommandError;

// ============================================================================
// Tier 2 — status-default markers
// ============================================================================

/// Implemented by every leaf with `STATUS == ErrorStatus::PreconditionFailed`.
pub trait PreconditionError: CommandError {}

/// Implemented by every leaf with `STATUS == ErrorStatus::InvalidArgument`.
pub trait ValidationError: CommandError {}

// ============================================================================
// Tier 3 — Aggregate lifecycle (markers)
// ============================================================================

pub trait AggregateNotFound: PreconditionError {}
pub trait AggregateAlreadyExists: PreconditionError {}

// ============================================================================
// Tier 3 — State / status (markers)
// ============================================================================

pub trait StateMismatch: PreconditionError {}
pub trait StateAlreadyEntered: PreconditionError {}
pub trait InvalidOperationInState: PreconditionError {}

// ============================================================================
// Tier 3 — Entity-in-container (markers)
// ============================================================================

pub trait EntityNotInContainer: PreconditionError {}
pub trait EntityKeyedConflict: PreconditionError {}

// ============================================================================
// Tier 3 — Capacity
// ============================================================================

/// Operation requested more than the aggregate can provide. Accessors
/// return canonical scalar field values regardless of the leaf's wording.
pub trait InsufficientCapacity: PreconditionError {
    fn requested(&self) -> i64;
    fn available(&self) -> i64;
}

/// A bounded resource has been fully consumed.
pub trait Exhausted: PreconditionError {
    fn current(&self) -> i64;
    fn max_value(&self) -> i64;
}

/// A capped container has reached capacity (no scalar payload).
pub trait ContainerFull: PreconditionError {}

// ============================================================================
// Tier 3 — Comparison violations
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoundKind {
    BelowMin,
    AboveMax,
}

/// A scalar input violates a min/max bound.
pub trait BoundViolation: PreconditionError {
    fn got(&self) -> i64;
    fn bound(&self) -> i64;
    fn kind(&self) -> BoundKind;
}

/// Two related scalars violate the required relation between them.
/// `lhs`/`rhs` are intentionally generic — leaves give them semantic
/// names via their TEMPLATE.
pub trait RelationViolation: PreconditionError {
    fn lhs(&self) -> i64;
    fn rhs(&self) -> i64;
}

/// A submitted identifier or count does not match the expected one.
pub trait IdentityMismatch: PreconditionError {
    fn expected(&self) -> i64;
    fn got(&self) -> i64;
}

// ============================================================================
// Tier 3 — Variant
// ============================================================================

/// Operation is not supported for the aggregate's game variant.
pub trait VariantMismatch: PreconditionError {}

// ============================================================================
// Tier 3 — Validation (input shape)
// ============================================================================

/// A required input field was missing or empty. The field name is
/// encoded in the leaf's CODE; no runtime data needed.
pub trait FieldRequired: ValidationError {}

/// A scalar input must be > 0.
pub trait MustBePositive: ValidationError {
    fn value(&self) -> i64;
}

/// A scalar input must be != 0.
pub trait MustBeNonZero: ValidationError {
    fn value(&self) -> i64;
}

/// A scalar input is outside an allowed range.
pub trait ValueOutOfRange: ValidationError {
    fn got(&self) -> i64;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ErrorStatus;

    // Sample leaves used to exercise the trait mechanics.

    struct SampleNotFound;
    impl CommandError for SampleNotFound {
        const CODE: &'static str = "X_NOT_FOUND";
        const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
        const TEMPLATE: &'static str = "X does not exist";
        fn fields(&self) -> Vec<(&'static str, String)> {
            Vec::new()
        }
    }
    impl PreconditionError for SampleNotFound {}
    impl AggregateNotFound for SampleNotFound {}

    struct SampleBoundViolation {
        pub got: i64,
        pub bound: i64,
    }
    impl CommandError for SampleBoundViolation {
        const CODE: &'static str = "SAMPLE_BELOW_MIN";
        const STATUS: ErrorStatus = ErrorStatus::InvalidArgument;
        const TEMPLATE: &'static str = "Sample {got} below limit {bound}";
        fn fields(&self) -> Vec<(&'static str, String)> {
            vec![
                ("got", self.got.to_string()),
                ("bound", self.bound.to_string()),
            ]
        }
    }
    impl ValidationError for SampleBoundViolation {}
    impl PreconditionError for SampleBoundViolation {}
    impl BoundViolation for SampleBoundViolation {
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

    struct SampleInsufficient {
        pub requested: i64,
        pub available: i64,
    }
    impl CommandError for SampleInsufficient {
        const CODE: &'static str = "SAMPLE_INSUFFICIENT";
        const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
        const TEMPLATE: &'static str = "requested {requested}, available {available}";
        fn fields(&self) -> Vec<(&'static str, String)> {
            vec![
                ("requested", self.requested.to_string()),
                ("available", self.available.to_string()),
            ]
        }
    }
    impl PreconditionError for SampleInsufficient {}
    impl InsufficientCapacity for SampleInsufficient {
        fn requested(&self) -> i64 {
            self.requested
        }
        fn available(&self) -> i64 {
            self.available
        }
    }

    fn read_bound<E: BoundViolation>(err: &E) -> (i64, i64, BoundKind) {
        (err.got(), err.bound(), err.kind())
    }

    fn read_capacity<E: InsufficientCapacity>(err: &E) -> (i64, i64) {
        (err.requested(), err.available())
    }

    #[test]
    fn aggregate_not_found_is_a_precondition_error() {
        // Compile-time bound — if PreconditionError isn't a supertrait,
        // this won't type-check.
        fn assert_precondition<E: PreconditionError>(_: &E) {}
        assert_precondition(&SampleNotFound);
    }

    #[test]
    fn bound_violation_accessors_return_struct_fields() {
        let err = SampleBoundViolation { got: 50, bound: 200 };
        let (got, bound, kind) = read_bound(&err);
        assert_eq!(got, 50);
        assert_eq!(bound, 200);
        assert_eq!(kind, BoundKind::BelowMin);
    }

    #[test]
    fn insufficient_capacity_accessors_return_struct_fields() {
        let err = SampleInsufficient {
            requested: 500,
            available: 100,
        };
        let (req, av) = read_capacity(&err);
        assert_eq!(req, 500);
        assert_eq!(av, 100);
    }

    #[test]
    fn shape_traits_are_distinct_super_traits_of_command_error() {
        // Sanity: a leaf that does NOT impl AggregateNotFound shouldn't
        // be acceptable where AggregateNotFound is required. Compile-time
        // — the absence of an `impl AggregateNotFound for SampleBoundViolation`
        // means `fn(&dyn AggregateNotFound)` cannot accept it.
        fn _take_not_found<E: AggregateNotFound>(_: &E) {}
        // _take_not_found(&SampleBoundViolation { got: 0, bound: 0 });  // would fail to compile
        // Positive case still works:
        _take_not_found(&SampleNotFound);
    }
}
