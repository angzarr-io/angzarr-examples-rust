//! Structured command-error trait for examples.
//!
//! Implementors declare a stable `CODE` (SCREAMING_SNAKE), a `TEMPLATE`
//! containing `{field}` placeholders, a `STATUS`, and a `fields()` method
//! returning runtime context.
//!
//! Wire shape (matches Python's `StructuredCommandError`):
//! - `code`    = `CODE` — testable identifier, language-portable.
//! - `message` = `TEMPLATE` (placeholders intact, `&'static str`) — same
//!   exact string for every instance of the same predicate failure,
//!   suitable for log greppability and cross-language equality. Required
//!   by Audit #59 (`CommandRejectedError.message: &'static str`).
//! - `details` = the `fields()` map — runtime context, structured.
//!
//! `render()` substitutes `fields()` into `TEMPLATE` and returns a
//! human-readable string suitable for display or logging. Cucumber
//! scenarios assert on `code` and individual `details` fields — never on
//! rendered text.
//!
//! Use `reject(err)` to convert a `CommandError` impl into the
//! framework's `CommandRejectedError` for return from a handler.

use angzarr_client::CommandRejectedError;

/// gRPC-mapped status class for the rejection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorStatus {
    /// State-based rejection. Retryable after refreshing state.
    PreconditionFailed,
    /// Bad input. Not retryable.
    InvalidArgument,
    /// Aggregate does not exist. Not retryable.
    NotFound,
}

/// Trait implemented by every typed command-error in the examples.
pub trait CommandError {
    const CODE: &'static str;
    const STATUS: ErrorStatus;
    const TEMPLATE: &'static str;

    /// Runtime context as `(field_name, stringified_value)` pairs. Field
    /// names match `{placeholder}` tokens in `TEMPLATE`.
    fn fields(&self) -> Vec<(&'static str, String)>;

    /// Substitute `fields()` into `TEMPLATE` for human-readable display.
    fn render(&self) -> String {
        substitute(Self::TEMPLATE, &self.fields())
    }
}

/// Convert a typed `CommandError` into the framework's
/// `CommandRejectedError`. Use as `return Err(reject(MyError { ... }));`
/// from handler functions.
pub fn reject<E: CommandError>(err: E) -> CommandRejectedError {
    let fields = err.fields();
    match E::STATUS {
        ErrorStatus::PreconditionFailed => {
            CommandRejectedError::precondition_failed(E::CODE, E::TEMPLATE, fields)
        }
        ErrorStatus::InvalidArgument => {
            CommandRejectedError::invalid_argument(E::CODE, E::TEMPLATE, fields)
        }
        ErrorStatus::NotFound => CommandRejectedError::not_found(E::CODE, E::TEMPLATE, fields),
    }
}

fn substitute(template: &str, fields: &[(&'static str, String)]) -> String {
    let mut out = template.to_string();
    for (key, value) in fields {
        let placeholder = format!("{{{}}}", key);
        out = out.replace(&placeholder, value);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NeedAtLeast2Players {
        got: usize,
    }

    impl CommandError for NeedAtLeast2Players {
        const CODE: &'static str = "NEED_AT_LEAST_2_PLAYERS";
        const STATUS: ErrorStatus = ErrorStatus::InvalidArgument;
        const TEMPLATE: &'static str = "Need at least 2 players, got {got}";

        fn fields(&self) -> Vec<(&'static str, String)> {
            vec![("got", self.got.to_string())]
        }
    }

    struct BetBelowMinRaise {
        min_raise: i64,
        amount: i64,
    }

    impl CommandError for BetBelowMinRaise {
        const CODE: &'static str = "BET_BELOW_MIN_RAISE";
        const STATUS: ErrorStatus = ErrorStatus::InvalidArgument;
        const TEMPLATE: &'static str = "Bet must be at least {min_raise}, got {amount}";

        fn fields(&self) -> Vec<(&'static str, String)> {
            vec![
                ("min_raise", self.min_raise.to_string()),
                ("amount", self.amount.to_string()),
            ]
        }
    }

    struct NoFields;

    impl CommandError for NoFields {
        const CODE: &'static str = "GENERIC_FAILURE";
        const STATUS: ErrorStatus = ErrorStatus::PreconditionFailed;
        const TEMPLATE: &'static str = "Generic failure with no runtime context";

        fn fields(&self) -> Vec<(&'static str, String)> {
            Vec::new()
        }
    }

    #[test]
    fn reject_publishes_code_to_framework() {
        let r = reject(NeedAtLeast2Players { got: 1 });
        assert_eq!(r.code, "NEED_AT_LEAST_2_PLAYERS");
    }

    #[test]
    fn reject_uses_template_as_static_wire_message() {
        let r = reject(NeedAtLeast2Players { got: 1 });
        assert_eq!(r.message, "Need at least 2 players, got {got}");
    }

    #[test]
    fn reject_routes_invalid_argument_status() {
        let r = reject(NeedAtLeast2Players { got: 1 });
        assert_eq!(r.status_code, "INVALID_ARGUMENT");
        assert!(r.is_invalid_argument());
        assert!(!r.is_precondition_failed());
    }

    #[test]
    fn reject_routes_precondition_failed_default() {
        let r = reject(NoFields);
        assert_eq!(r.status_code, "FAILED_PRECONDITION");
        assert!(r.is_precondition_failed());
    }

    #[test]
    fn details_carry_structured_runtime_context() {
        let r = reject(BetBelowMinRaise {
            min_raise: 200,
            amount: 50,
        });
        assert_eq!(r.details.get("min_raise"), Some(&"200".to_string()));
        assert_eq!(r.details.get("amount"), Some(&"50".to_string()));
    }

    #[test]
    fn render_substitutes_template_placeholders() {
        let err = BetBelowMinRaise {
            min_raise: 200,
            amount: 50,
        };
        assert_eq!(err.render(), "Bet must be at least 200, got 50");
    }

    #[test]
    fn render_returns_template_unchanged_when_no_fields() {
        assert_eq!(NoFields.render(), "Generic failure with no runtime context");
    }

    #[test]
    fn two_instances_share_static_message_but_differ_in_render() {
        let a = NeedAtLeast2Players { got: 1 };
        let b = NeedAtLeast2Players { got: 99 };
        // Static wire messages match — log greppability holds.
        let r_a = reject(NeedAtLeast2Players { got: a.got });
        let r_b = reject(NeedAtLeast2Players { got: b.got });
        assert_eq!(r_a.message, r_b.message);
        // But rendered output differs.
        assert_ne!(a.render(), b.render());
    }
}
