//! How the wallet refuses a client command: with the spec's typed
//! `WalletRefused` (wallet.proto; every command of a domain a policy
//! targets names one, angzarr decision 14.76), its reason the code and the
//! values it names its other fields. HoldFunds' one typed refusal is the
//! spec's `InsufficientFunds`.

use examples_proto::domain::{WalletRefused, WalletRefusedReason};

/// A refusal for `reason`, naming no value yet: set the fields it names
/// with struct update syntax, `WalletRefused { amount: Some(n),
/// ..refused(r) }`.
pub(crate) fn refused(reason: WalletRefusedReason) -> WalletRefused {
    WalletRefused {
        reason,
        amount: None,
        available: None,
        requested: None,
    }
}
