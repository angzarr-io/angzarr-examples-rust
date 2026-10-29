//! agg-table's [`Rejection`] as the table domain's message-only rejection:
//! a localizable diagnostic, one code per `ErrorInfo.reason`.

use std::fmt;

use agg_table::{Rejection, RejectionCode};
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

impl fmt::Display for TableRefused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0.message)
    }
}

impl Diagnostic for TableRefused {
    fn code(&self) -> &'static str {
        match self.0.code {
            RejectionCode::TableAlreadyExists => "examples-table-table-already-exists",
            RejectionCode::InvalidTableConfig => "examples-table-invalid-table-config",
        }
    }

    fn args(&self) -> Vec<(&'static str, Arg)> {
        vec![("message", self.0.message.as_str().into())]
    }
}
