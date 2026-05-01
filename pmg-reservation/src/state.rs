//! Unified PM state for the three lifecycle flows.

use examples_proto::{BuyInPhase, RebuyPhase, RegistrationPhase};

/// Discriminator for the flavor of reservation flow this PM instance is driving.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Kind {
    #[default]
    Unspecified,
    BuyIn,
    Rebuy,
    Registration,
}

#[derive(Debug, Clone, Default)]
pub struct ReservationPMState {
    pub reservation_id: Vec<u8>,
    pub kind: Kind,
    pub player_root: Vec<u8>,
    pub table_root: Vec<u8>,
    pub tournament_root: Vec<u8>,
    pub seat: i32,
    pub amount: i64,
    pub fee: i64,
    /// Loosely-typed phase: stash whichever of the three enum values is
    /// relevant for this flow's `kind`.
    pub phase: i32,
}

impl ReservationPMState {
    pub fn is_initialized(&self) -> bool {
        !self.reservation_id.is_empty()
    }

    /// Bytes passed as `ReserveFunds.key` / `DeductReservedFunds.key`.
    /// Buy-in / rebuy reserve against `table_root`; registration reserves
    /// against `tournament_root` (no table yet).
    pub fn reservation_key(&self) -> &[u8] {
        match self.kind {
            Kind::Registration => &self.tournament_root,
            _ => &self.table_root,
        }
    }
}

// Phase constants exposed for handler convenience.
pub const BUY_IN_SEATING: i32 = BuyInPhase::BuyInSeating as i32;
pub const BUY_IN_COMPLETED: i32 = BuyInPhase::BuyInCompleted as i32;
pub const BUY_IN_FAILED: i32 = BuyInPhase::BuyInFailed as i32;
pub const REBUY_APPROVING: i32 = RebuyPhase::RebuyApproving as i32;
pub const REBUY_COMPLETED: i32 = RebuyPhase::RebuyCompleted as i32;
pub const REBUY_FAILED: i32 = RebuyPhase::RebuyFailed as i32;
pub const REGISTRATION_ENROLLING: i32 = RegistrationPhase::RegistrationEnrolling as i32;
pub const REGISTRATION_COMPLETED: i32 = RegistrationPhase::RegistrationCompleted as i32;
pub const REGISTRATION_FAILED: i32 = RegistrationPhase::RegistrationFailed as i32;
