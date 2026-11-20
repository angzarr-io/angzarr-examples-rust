//! A refusal of a command the policy never sends: its one refusal callback
//! (angzarr decision 14.89) is handed every command of its target, and
//! answering one it never sends is a bug, never a fact.

use angzarr_diagnostic::code;

use crate::domain::Bug;

/// The key of a refusal of a command the policy never sends.
pub const NEVER_SENT: &str = code!("never-sent");

/// The bug of a refusal of `command`, which this policy never sends.
pub(crate) fn never_sent<C>(_command: &C) -> Bug {
    Bug::new(NEVER_SENT).arg("command", std::any::type_name::<C>())
}
