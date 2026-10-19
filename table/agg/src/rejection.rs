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
}

impl RejectionCode {
    /// The `ErrorInfo.reason` string, e.g. `"TABLE_ALREADY_EXISTS"`.
    pub fn reason(self) -> &'static str {
        match self {
            Self::TableAlreadyExists => "TABLE_ALREADY_EXISTS",
            Self::InvalidTableConfig => "INVALID_TABLE_CONFIG",
        }
    }

    /// The gRPC status class.
    pub fn status(self) -> StatusCode {
        match self {
            Self::TableAlreadyExists => StatusCode::FailedPrecondition,
            Self::InvalidTableConfig => StatusCode::InvalidArgument,
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
