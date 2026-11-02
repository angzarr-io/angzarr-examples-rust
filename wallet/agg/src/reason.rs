//! The wallet's rejection keys (wallet.proto, Rejections): each a code of
//! this crate's, its args the values it names. wallet.proto lists them as
//! reason codes but declares no refusal message for them, so each is the
//! generated layer's `Rejection` (a key and its args); HoldFunds' one typed
//! refusal is the spec's `InsufficientFunds`.

use angzarr_diagnostic::code;

/// RegisterWallet or ImportWallet for a wallet that exists.
pub const WALLET_ALREADY_EXISTS: &str = code!("wallet-already-exists");
/// Any command but RegisterWallet and ImportWallet before either.
pub const WALLET_NOT_FOUND: &str = code!("wallet-not-found");
/// A withdrawal beyond what is available (the bankroll minus open holds);
/// args `available`, `requested`.
pub const INSUFFICIENT_AVAILABLE_FUNDS: &str = code!("insufficient-available-funds");
/// A deposit or withdrawal of nothing or less; arg `amount`.
pub const AMOUNT_NOT_POSITIVE: &str = code!("amount-not-positive");
/// A registration, import or profile with no display name.
pub const DISPLAY_NAME_REQUIRED: &str = code!("display-name-required");
/// A registration or import with no email.
pub const EMAIL_REQUIRED: &str = code!("email-required");
