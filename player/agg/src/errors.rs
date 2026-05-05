//! Player-domain command error catalog.
//!
//! One [`CommandError`] impl per distinct rejection. Cucumber asserts on
//! [`CommandError::CODE`] and individual `details` fields; rendered text
//! is for human display only.
//!
//! Each leaf also implements one of the shape marker traits in
//! [`examples_utils::error_shapes`] so cross-cutting code can dispatch on
//! shape rather than on individual leaves.

use examples_utils::error_shapes::{
    AggregateAlreadyExists, AggregateNotFound, EntityKeyedConflict, FieldRequired,
    InsufficientCapacity, MustBeNonZero, MustBePositive, PreconditionError, ValidationError,
};
use examples_utils::{CommandError, ErrorStatus};

pub struct PlayerAlreadyExists;
impl CommandError for PlayerAlreadyExists {
    const CODE: &'static str = "PLAYER_ALREADY_EXISTS";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "Player already exists";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl PreconditionError for PlayerAlreadyExists {}
impl AggregateAlreadyExists for PlayerAlreadyExists {}

pub struct DisplayNameRequired;
impl CommandError for DisplayNameRequired {
    const CODE: &'static str = "DISPLAY_NAME_REQUIRED";
    const STATUS: ErrorStatus = ErrorStatus::InvalidArgument;
    const TEMPLATE: &'static str = "display_name is required";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl ValidationError for DisplayNameRequired {}
impl FieldRequired for DisplayNameRequired {}

pub struct EmailRequired;
impl CommandError for EmailRequired {
    const CODE: &'static str = "EMAIL_REQUIRED";
    const STATUS: ErrorStatus = ErrorStatus::InvalidArgument;
    const TEMPLATE: &'static str = "email is required";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl ValidationError for EmailRequired {}
impl FieldRequired for EmailRequired {}

pub struct PlayerNotFound;
impl CommandError for PlayerNotFound {
    const CODE: &'static str = "PLAYER_NOT_FOUND";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "Player does not exist";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl PreconditionError for PlayerNotFound {}
impl AggregateNotFound for PlayerNotFound {}

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

pub struct AmountMustBeNonZero {
    pub value: i64,
}
impl CommandError for AmountMustBeNonZero {
    const CODE: &'static str = "AMOUNT_MUST_BE_NON_ZERO";
    const STATUS: ErrorStatus = ErrorStatus::InvalidArgument;
    const TEMPLATE: &'static str = "Amount must be non-zero";
    fn fields(&self) -> Vec<(&'static str, String)> {
        vec![("value", self.value.to_string())]
    }
}
impl ValidationError for AmountMustBeNonZero {}
impl MustBeNonZero for AmountMustBeNonZero {
    fn value(&self) -> i64 {
        self.value
    }
}

pub struct InsufficientAvailableBalance {
    pub requested: i64,
    pub available: i64,
}
impl CommandError for InsufficientAvailableBalance {
    const CODE: &'static str = "INSUFFICIENT_AVAILABLE_BALANCE";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str =
        "Insufficient available balance: requested {requested}, available {available}";
    fn fields(&self) -> Vec<(&'static str, String)> {
        vec![
            ("requested", self.requested.to_string()),
            ("available", self.available.to_string()),
        ]
    }
}
impl PreconditionError for InsufficientAvailableBalance {}
impl InsufficientCapacity for InsufficientAvailableBalance {
    fn requested(&self) -> i64 {
        self.requested
    }
    fn available(&self) -> i64 {
        self.available
    }
}

pub struct InsufficientFunds {
    pub requested: i64,
    pub available: i64,
}
impl CommandError for InsufficientFunds {
    const CODE: &'static str = "INSUFFICIENT_FUNDS";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str =
        "Insufficient funds: requested {requested}, available {available}";
    fn fields(&self) -> Vec<(&'static str, String)> {
        vec![
            ("requested", self.requested.to_string()),
            ("available", self.available.to_string()),
        ]
    }
}
impl PreconditionError for InsufficientFunds {}
impl InsufficientCapacity for InsufficientFunds {
    fn requested(&self) -> i64 {
        self.requested
    }
    fn available(&self) -> i64 {
        self.available
    }
}

pub struct FundsAlreadyReservedForTable {
    pub table_root_hex: String,
}
impl CommandError for FundsAlreadyReservedForTable {
    const CODE: &'static str = "FUNDS_ALREADY_RESERVED_FOR_TABLE";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "Funds already reserved for table {table_root_hex}";
    fn fields(&self) -> Vec<(&'static str, String)> {
        vec![("table_root_hex", self.table_root_hex.clone())]
    }
}
impl PreconditionError for FundsAlreadyReservedForTable {}
impl EntityKeyedConflict for FundsAlreadyReservedForTable {}

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

pub struct NoFundsReservedForTable {
    pub table_root_hex: String,
}
impl CommandError for NoFundsReservedForTable {
    const CODE: &'static str = "NO_FUNDS_RESERVED_FOR_TABLE";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str = "No funds reserved for table {table_root_hex}";
    fn fields(&self) -> Vec<(&'static str, String)> {
        vec![("table_root_hex", self.table_root_hex.clone())]
    }
}
impl PreconditionError for NoFundsReservedForTable {}
impl EntityKeyedConflict for NoFundsReservedForTable {}

pub struct KeyRequired;
impl CommandError for KeyRequired {
    const CODE: &'static str = "KEY_REQUIRED";
    const STATUS: ErrorStatus = ErrorStatus::InvalidArgument;
    const TEMPLATE: &'static str = "key is required";
    fn fields(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
impl ValidationError for KeyRequired {}
impl FieldRequired for KeyRequired {}

pub struct AmountExceedsReservedFunds {
    pub requested: i64,
    pub available: i64,
}
impl CommandError for AmountExceedsReservedFunds {
    const CODE: &'static str = "AMOUNT_EXCEEDS_RESERVED_FUNDS";
    const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
    const TEMPLATE: &'static str =
        "Amount exceeds reserved funds: requested {requested}, available {available}";
    fn fields(&self) -> Vec<(&'static str, String)> {
        vec![
            ("requested", self.requested.to_string()),
            ("available", self.available.to_string()),
        ]
    }
}
impl PreconditionError for AmountExceedsReservedFunds {}
impl InsufficientCapacity for AmountExceedsReservedFunds {
    fn requested(&self) -> i64 {
        self.requested
    }
    fn available(&self) -> i64 {
        self.available
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use examples_utils::reject;

    #[test]
    fn each_no_field_error_publishes_code_status_and_renders() {
        let cases: &[(&dyn Fn() -> (String, &'static str, &'static str, String), &str, &str, &str)] = &[];
        let _ = cases; // prevent unused-binding warning if list is empty

        for (code, status, render) in [
            (
                "PLAYER_ALREADY_EXISTS",
                "FAILED_PRECONDITION",
                "Player already exists",
            ),
        ] {
            let r = reject(PlayerAlreadyExists);
            assert_eq!(r.code, code);
            assert_eq!(r.status_code, status);
            assert_eq!(PlayerAlreadyExists.render(), render);
        }
    }

    #[test]
    fn amount_must_be_positive_renders_with_value() {
        let err = AmountMustBePositive { value: -7 };
        assert_eq!(err.render(), "Amount must be positive, got -7");
        let r = reject(AmountMustBePositive { value: -7 });
        assert_eq!(r.code, "AMOUNT_MUST_BE_POSITIVE");
        assert_eq!(r.status_code, "INVALID_ARGUMENT");
        assert_eq!(r.details.get("value"), Some(&"-7".to_string()));
    }

    #[test]
    fn insufficient_available_balance_renders_with_both_fields() {
        let err = InsufficientAvailableBalance {
            requested: 500,
            available: 100,
        };
        assert_eq!(
            err.render(),
            "Insufficient available balance: requested 500, available 100"
        );
        let r = reject(InsufficientAvailableBalance {
            requested: 500,
            available: 100,
        });
        assert_eq!(r.code, "INSUFFICIENT_AVAILABLE_BALANCE");
        assert_eq!(r.status_code, "FAILED_PRECONDITION");
        assert_eq!(r.details.get("requested"), Some(&"500".to_string()));
        assert_eq!(r.details.get("available"), Some(&"100".to_string()));
    }

    #[test]
    fn insufficient_funds_renders_with_both_fields() {
        let err = InsufficientFunds {
            requested: 200,
            available: 50,
        };
        assert_eq!(err.render(), "Insufficient funds: requested 200, available 50");
        assert_eq!(InsufficientFunds::CODE, "INSUFFICIENT_FUNDS");
    }

    #[test]
    fn funds_already_reserved_carries_table_root_hex() {
        let err = FundsAlreadyReservedForTable {
            table_root_hex: "abc123".to_string(),
        };
        assert_eq!(err.render(), "Funds already reserved for table abc123");
        let r = reject(FundsAlreadyReservedForTable {
            table_root_hex: "abc123".to_string(),
        });
        assert_eq!(r.details.get("table_root_hex"), Some(&"abc123".to_string()));
    }

    #[test]
    fn no_funds_reserved_carries_table_root_hex() {
        let err = NoFundsReservedForTable {
            table_root_hex: "deadbeef".to_string(),
        };
        assert_eq!(err.render(), "No funds reserved for table deadbeef");
    }

    #[test]
    fn amount_exceeds_reserved_renders_with_both_fields() {
        let err = AmountExceedsReservedFunds {
            requested: 300,
            available: 100,
        };
        assert_eq!(
            err.render(),
            "Amount exceeds reserved funds: requested 300, available 100"
        );
    }

    #[test]
    fn template_is_static_for_log_greppability() {
        let a = AmountMustBePositive { value: 1 };
        let b = AmountMustBePositive { value: 99 };
        let r_a = reject(AmountMustBePositive { value: a.value });
        let r_b = reject(AmountMustBePositive { value: b.value });
        assert_eq!(r_a.message, r_b.message);
        assert_ne!(a.render(), b.render());
    }

    #[test]
    fn no_field_errors_render_template_unchanged() {
        assert_eq!(PlayerAlreadyExists.render(), "Player already exists");
        assert_eq!(DisplayNameRequired.render(), "display_name is required");
        assert_eq!(EmailRequired.render(), "email is required");
        assert_eq!(PlayerNotFound.render(), "Player does not exist");
        assert_eq!(AmountMustBeNonZero { value: 0 }.render(), "Amount must be non-zero");
        assert_eq!(TableRootRequired.render(), "table_root is required");
        assert_eq!(KeyRequired.render(), "key is required");
    }
}
