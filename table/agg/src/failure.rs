//! How the table fails a decision: refused with the spec's `TableRefused`
//! (its reason code, and the values it names as its other fields), or a
//! bug.

use angzarr_embedded_diagnostic::Diagnostic;
use examples_proto::domain::{Failed, TableRefused, TableRefusedReason};

/// A refusal for `reason`, naming no value yet: set the fields it names
/// with struct update syntax, `TableRefused { seat: Some(n), ..refused(r) }`.
pub(crate) fn refused(reason: TableRefusedReason) -> TableRefused {
    TableRefused {
        reason,
        name: None,
        rule: None,
        seat: None,
        min_buy_in: None,
        max_buy_in: None,
        min_bet: None,
        max_bet: None,
    }
}

/// A message the table decided over its wire state, as the domain type
/// its event carries. Every card the table holds came from a shuffle or a
/// recorded event, so one the domain type cannot hold is a bug, reported
/// by the conversion's own code.
pub(crate) fn domain<W, D>(wire: W) -> Result<D, Failed<TableRefused>>
where
    D: TryFrom<W>,
    D::Error: Diagnostic,
{
    D::try_from(wire).map_err(|error| Failed::bug(&error))
}
