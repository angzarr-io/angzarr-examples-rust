//! The wallet's history as older versions wrote it, read as the current
//! shape (wallet.proto): FundsDepositedV1 is FundsDeposited, its
//! `amount_chips` the amount. Never emitted; it exists only in stored
//! history, and is rewritten before any handler sees it.

use angzarr_diagnostic::Report;

use crate::wire::{FundsDeposited, FundsDepositedV1};

/// FundsDepositedV1 as FundsDeposited.
pub fn funds_deposited_v1(old: FundsDepositedV1) -> Result<FundsDeposited, Report> {
    Ok(FundsDeposited {
        amount: old.amount_chips,
    })
}
