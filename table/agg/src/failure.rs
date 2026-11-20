//! How the table fails a decision with a bug: a refusal is the spec's typed
//! message of its own (table.proto), returned through `Failed::rejected`.

use angzarr_diagnostic::Diagnostic;
use examples_proto::domain::Failed;

/// A message the table decided over its wire state, as the domain type
/// its event carries. Every card the table holds came from a shuffle or a
/// recorded event, so one the domain type cannot hold is a bug, reported
/// by the conversion's own code.
pub(crate) fn domain<W, D, R>(wire: W) -> Result<D, Failed<R>>
where
    D: TryFrom<W>,
    D::Error: Diagnostic,
{
    D::try_from(wire).map_err(|error| Failed::bug(&error))
}
