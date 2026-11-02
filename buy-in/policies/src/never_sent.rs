//! A refusal of a command the policy never sends: its generated interface
//! has a callback for every command of its target (angzarr decision 14.76),
//! and answering one it never sends is a bug, never a fact.

use angzarr_diagnostic::code;

use crate::domain::Bug;

/// The key of a refusal of a command the policy never sends.
pub const NEVER_SENT: &str = code!("never-sent");

/// The bug of a refusal of `command`, which this policy never sends.
pub(crate) fn never_sent<C>(_command: &C) -> Bug {
    Bug::new(NEVER_SENT).arg("command", std::any::type_name::<C>())
}
