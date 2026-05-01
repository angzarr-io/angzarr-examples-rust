//! Reservation aggregate library.
//!
//! Owns the pending records for buy-in / rebuy / tournament-registration flows.
//! Three command shapes per flavor:
//!   * `Initiate*` opens a pending record and emits a `*Requested` event
//!   * `Confirm*` closes the pending record on success and emits `*Confirmed`
//!   * `Release*` closes on failure and emits `*Released`
//!
//! The downstream PM (`pmg-reservation`) translates each event into the
//! matching player primitive (ReserveFunds / DeductReservedFunds / ReleaseFunds).

pub mod state;

pub use state::{PendingBuyIn, PendingRebuy, PendingRegistration, ReservationState};

use std::sync::Arc;

use angzarr_client::proto::EventBook;
use angzarr_client::{command_handler, CommandResult, QueryClient};
use examples_proto::{
    BuyInConfirmed, BuyInRequested, BuyInReservationReleased, ConfirmBuyIn, ConfirmRebuyFee,
    ConfirmRegistrationFee, Currency, InitiateBuyIn, InitiateRebuy, InitiateTournamentRegistration,
    RebuyFeeConfirmed, RebuyFeeReleased, RebuyRequested, RegistrationFeeConfirmed,
    RegistrationFeeReleased, RegistrationRequested, ReleaseBuyIn, ReleaseRebuyFee,
    ReleaseRegistrationFee,
};
use examples_utils::{event_page, invalid_arg, pack_event, rejected};
use uuid::Uuid;

fn new_reservation_id() -> Vec<u8> {
    Uuid::new_v4().as_bytes().to_vec()
}

fn currency(amount: i64) -> Currency {
    Currency {
        amount,
        currency_code: "CHIPS".to_string(),
    }
}

fn one_event_book(seq: u32, event_any: prost_types::Any) -> EventBook {
    EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    }
}

/// Reservation aggregate.
///
/// Holds an optional `QueryClient` for sync DECISION queries against the
/// player aggregate. When the client is `None` (test paths or unwired
/// servers) the cross-aggregate check is skipped — handlers still
/// validate inputs and pending state.
#[derive(Default)]
pub struct Reservation {
    query_client: Option<Arc<QueryClient>>,
}

impl Reservation {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_query_client(query_client: Arc<QueryClient>) -> Self {
        Self {
            query_client: Some(query_client),
        }
    }

    /// Read the configured query client. Currently unused — the sync DECISION
    /// path through the async client requires more plumbing than fits into the
    /// sync `#[handles]` signature, so we treat all checks as soft-skipped.
    /// Keeping the field around so the call site can be lit up later without
    /// reshuffling state.
    pub fn query_client(&self) -> Option<&Arc<QueryClient>> {
        self.query_client.as_ref()
    }
}

#[command_handler(domain = "reservation", state = ReservationState)]
impl Reservation {
    // =========================================================================
    // Buy-in
    // =========================================================================

    #[handles(InitiateBuyIn)]
    pub fn on_initiate_buy_in(
        &self,
        cmd: InitiateBuyIn,
        _state: &ReservationState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        if cmd.player_root.is_empty() {
            return Err(rejected("player_root is required"));
        }
        if cmd.table_root.is_empty() {
            return Err(rejected("table_root is required"));
        }
        let amount = cmd.amount.as_ref().map(|c| c.amount).unwrap_or(0);
        if amount <= 0 {
            return Err(invalid_arg("amount must be positive"));
        }

        let event = BuyInRequested {
            reservation_id: new_reservation_id(),
            player_root: cmd.player_root,
            table_root: cmd.table_root,
            seat: cmd.seat,
            amount: cmd.amount,
            requested_at: Some(angzarr_client::now()),
        };
        Ok(one_event_book(
            seq,
            pack_event(&event, "examples.BuyInRequested"),
        ))
    }

    #[handles(ConfirmBuyIn)]
    pub fn on_confirm_buy_in(
        &self,
        cmd: ConfirmBuyIn,
        state: &ReservationState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        if cmd.reservation_id.is_empty() {
            return Err(rejected("reservation_id is required"));
        }
        let key = hex::encode(&cmd.reservation_id);
        let pending = state
            .pending_buy_ins
            .get(&key)
            .ok_or_else(|| rejected("No pending buy-in with this reservation_id"))?;
        let event = BuyInConfirmed {
            reservation_id: cmd.reservation_id,
            player_root: pending.player_root.clone(),
            table_root: pending.table_root.clone(),
            seat: pending.seat,
            amount: Some(currency(pending.amount)),
            confirmed_at: Some(angzarr_client::now()),
        };
        Ok(one_event_book(
            seq,
            pack_event(&event, "examples.BuyInConfirmed"),
        ))
    }

    #[handles(ReleaseBuyIn)]
    pub fn on_release_buy_in(
        &self,
        cmd: ReleaseBuyIn,
        state: &ReservationState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        if cmd.reservation_id.is_empty() {
            return Err(rejected("reservation_id is required"));
        }
        let key = hex::encode(&cmd.reservation_id);
        let pending = state
            .pending_buy_ins
            .get(&key)
            .ok_or_else(|| rejected("No pending buy-in with this reservation_id"))?;
        let event = BuyInReservationReleased {
            reservation_id: cmd.reservation_id,
            reason: cmd.reason,
            released_at: Some(angzarr_client::now()),
            player_root: pending.player_root.clone(),
            table_root: pending.table_root.clone(),
            amount: Some(currency(pending.amount)),
        };
        Ok(one_event_book(
            seq,
            pack_event(&event, "examples.BuyInReservationReleased"),
        ))
    }

    // =========================================================================
    // Tournament registration
    // =========================================================================

    #[handles(InitiateTournamentRegistration)]
    pub fn on_initiate_registration(
        &self,
        cmd: InitiateTournamentRegistration,
        _state: &ReservationState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        if cmd.player_root.is_empty() {
            return Err(rejected("player_root is required"));
        }
        if cmd.tournament_root.is_empty() {
            return Err(rejected("tournament_root is required"));
        }
        let event = RegistrationRequested {
            reservation_id: new_reservation_id(),
            player_root: cmd.player_root,
            tournament_root: cmd.tournament_root,
            fee: None, // PM fills in from Tournament state
            requested_at: Some(angzarr_client::now()),
        };
        Ok(one_event_book(
            seq,
            pack_event(&event, "examples.RegistrationRequested"),
        ))
    }

    #[handles(ConfirmRegistrationFee)]
    pub fn on_confirm_registration(
        &self,
        cmd: ConfirmRegistrationFee,
        state: &ReservationState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        if cmd.reservation_id.is_empty() {
            return Err(rejected("reservation_id is required"));
        }
        let key = hex::encode(&cmd.reservation_id);
        let pending = state
            .pending_registrations
            .get(&key)
            .ok_or_else(|| rejected("No pending registration with this reservation_id"))?;
        let event = RegistrationFeeConfirmed {
            reservation_id: cmd.reservation_id,
            player_root: pending.player_root.clone(),
            tournament_root: pending.tournament_root.clone(),
            fee: Some(currency(pending.fee)),
            confirmed_at: Some(angzarr_client::now()),
        };
        Ok(one_event_book(
            seq,
            pack_event(&event, "examples.RegistrationFeeConfirmed"),
        ))
    }

    #[handles(ReleaseRegistrationFee)]
    pub fn on_release_registration(
        &self,
        cmd: ReleaseRegistrationFee,
        state: &ReservationState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        if cmd.reservation_id.is_empty() {
            return Err(rejected("reservation_id is required"));
        }
        let key = hex::encode(&cmd.reservation_id);
        let pending = state
            .pending_registrations
            .get(&key)
            .ok_or_else(|| rejected("No pending registration with this reservation_id"))?;
        let event = RegistrationFeeReleased {
            reservation_id: cmd.reservation_id,
            reason: cmd.reason,
            released_at: Some(angzarr_client::now()),
            player_root: pending.player_root.clone(),
            tournament_root: pending.tournament_root.clone(),
            fee: Some(currency(pending.fee)),
        };
        Ok(one_event_book(
            seq,
            pack_event(&event, "examples.RegistrationFeeReleased"),
        ))
    }

    // =========================================================================
    // Rebuy
    // =========================================================================

    #[handles(InitiateRebuy)]
    pub fn on_initiate_rebuy(
        &self,
        cmd: InitiateRebuy,
        _state: &ReservationState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        if cmd.player_root.is_empty() {
            return Err(rejected("player_root is required"));
        }
        if cmd.tournament_root.is_empty() {
            return Err(rejected("tournament_root is required"));
        }
        if cmd.table_root.is_empty() {
            return Err(rejected("table_root is required"));
        }
        let event = RebuyRequested {
            reservation_id: new_reservation_id(),
            player_root: cmd.player_root,
            tournament_root: cmd.tournament_root,
            table_root: cmd.table_root,
            seat: cmd.seat,
            fee: None, // PM fills in from Tournament state
            requested_at: Some(angzarr_client::now()),
        };
        Ok(one_event_book(
            seq,
            pack_event(&event, "examples.RebuyRequested"),
        ))
    }

    #[handles(ConfirmRebuyFee)]
    pub fn on_confirm_rebuy(
        &self,
        cmd: ConfirmRebuyFee,
        state: &ReservationState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        if cmd.reservation_id.is_empty() {
            return Err(rejected("reservation_id is required"));
        }
        let key = hex::encode(&cmd.reservation_id);
        let pending = state
            .pending_rebuys
            .get(&key)
            .ok_or_else(|| rejected("No pending rebuy with this reservation_id"))?;
        let event = RebuyFeeConfirmed {
            reservation_id: cmd.reservation_id,
            player_root: pending.player_root.clone(),
            tournament_root: pending.tournament_root.clone(),
            table_root: pending.table_root.clone(),
            fee: Some(currency(pending.fee)),
            chips_added: 0, // Set by PM after Tournament processes the rebuy
            confirmed_at: Some(angzarr_client::now()),
        };
        Ok(one_event_book(
            seq,
            pack_event(&event, "examples.RebuyFeeConfirmed"),
        ))
    }

    #[handles(ReleaseRebuyFee)]
    pub fn on_release_rebuy(
        &self,
        cmd: ReleaseRebuyFee,
        state: &ReservationState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        if cmd.reservation_id.is_empty() {
            return Err(rejected("reservation_id is required"));
        }
        let key = hex::encode(&cmd.reservation_id);
        let pending = state
            .pending_rebuys
            .get(&key)
            .ok_or_else(|| rejected("No pending rebuy with this reservation_id"))?;
        let event = RebuyFeeReleased {
            reservation_id: cmd.reservation_id,
            reason: cmd.reason,
            released_at: Some(angzarr_client::now()),
            player_root: pending.player_root.clone(),
            tournament_root: pending.tournament_root.clone(),
            table_root: pending.table_root.clone(),
            fee: Some(currency(pending.fee)),
        };
        Ok(one_event_book(
            seq,
            pack_event(&event, "examples.RebuyFeeReleased"),
        ))
    }

    // =========================================================================
    // Event appliers
    // =========================================================================

    #[applies(BuyInRequested)]
    pub fn apply_buy_in_requested(state: &mut ReservationState, event: BuyInRequested) {
        let amount = event.amount.as_ref().map(|c| c.amount).unwrap_or(0);
        state.pending_buy_ins.insert(
            hex::encode(&event.reservation_id),
            PendingBuyIn {
                player_root: event.player_root,
                table_root: event.table_root,
                seat: event.seat,
                amount,
            },
        );
    }

    #[applies(BuyInConfirmed)]
    pub fn apply_buy_in_confirmed(state: &mut ReservationState, event: BuyInConfirmed) {
        state
            .pending_buy_ins
            .remove(&hex::encode(&event.reservation_id));
    }

    #[applies(BuyInReservationReleased)]
    pub fn apply_buy_in_released(state: &mut ReservationState, event: BuyInReservationReleased) {
        state
            .pending_buy_ins
            .remove(&hex::encode(&event.reservation_id));
    }

    #[applies(RegistrationRequested)]
    pub fn apply_registration_requested(
        state: &mut ReservationState,
        event: RegistrationRequested,
    ) {
        let fee = event.fee.as_ref().map(|c| c.amount).unwrap_or(0);
        state.pending_registrations.insert(
            hex::encode(&event.reservation_id),
            PendingRegistration {
                player_root: event.player_root,
                tournament_root: event.tournament_root,
                fee,
            },
        );
    }

    #[applies(RegistrationFeeConfirmed)]
    pub fn apply_registration_confirmed(
        state: &mut ReservationState,
        event: RegistrationFeeConfirmed,
    ) {
        state
            .pending_registrations
            .remove(&hex::encode(&event.reservation_id));
    }

    #[applies(RegistrationFeeReleased)]
    pub fn apply_registration_released(
        state: &mut ReservationState,
        event: RegistrationFeeReleased,
    ) {
        state
            .pending_registrations
            .remove(&hex::encode(&event.reservation_id));
    }

    #[applies(RebuyRequested)]
    pub fn apply_rebuy_requested(state: &mut ReservationState, event: RebuyRequested) {
        let fee = event.fee.as_ref().map(|c| c.amount).unwrap_or(0);
        state.pending_rebuys.insert(
            hex::encode(&event.reservation_id),
            PendingRebuy {
                player_root: event.player_root,
                tournament_root: event.tournament_root,
                table_root: event.table_root,
                seat: event.seat,
                fee,
            },
        );
    }

    #[applies(RebuyFeeConfirmed)]
    pub fn apply_rebuy_confirmed(state: &mut ReservationState, event: RebuyFeeConfirmed) {
        state
            .pending_rebuys
            .remove(&hex::encode(&event.reservation_id));
    }

    #[applies(RebuyFeeReleased)]
    pub fn apply_rebuy_released(state: &mut ReservationState, event: RebuyFeeReleased) {
        state
            .pending_rebuys
            .remove(&hex::encode(&event.reservation_id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buy_in_requested_then_confirmed_clears_pending() {
        let mut state = ReservationState::default();
        let agg = Reservation::new();

        let book = agg
            .on_initiate_buy_in(
                InitiateBuyIn {
                    table_root: vec![0xaa],
                    seat: 1,
                    amount: Some(currency(100)),
                    player_root: vec![0xbb],
                },
                &state,
                0,
            )
            .expect("initiate succeeds");
        assert_eq!(book.pages.len(), 1);

        // Replay the produced event onto state
        let payload = book.pages[0]
            .payload
            .as_ref()
            .and_then(|p| match p {
                angzarr_client::proto::event_page::Payload::Event(any) => Some(any),
                _ => None,
            })
            .expect("event payload");
        use prost::Message;
        let req = BuyInRequested::decode(payload.value.as_slice()).expect("decode");
        let rid = req.reservation_id.clone();
        Reservation::apply_buy_in_requested(&mut state, req);
        assert!(state.pending_buy_ins.contains_key(&hex::encode(&rid)));

        let book = agg
            .on_confirm_buy_in(
                ConfirmBuyIn {
                    reservation_id: rid.clone(),
                },
                &state,
                1,
            )
            .expect("confirm succeeds");
        assert_eq!(book.pages.len(), 1);

        Reservation::apply_buy_in_confirmed(
            &mut state,
            BuyInConfirmed {
                reservation_id: rid.clone(),
                player_root: vec![],
                table_root: vec![],
                seat: 0,
                amount: None,
                confirmed_at: None,
            },
        );
        assert!(!state.pending_buy_ins.contains_key(&hex::encode(&rid)));
    }

    #[test]
    fn release_unknown_reservation_rejects() {
        let state = ReservationState::default();
        let agg = Reservation::new();
        let err = agg
            .on_release_buy_in(
                ReleaseBuyIn {
                    reservation_id: vec![0xff],
                    reason: "test".into(),
                },
                &state,
                0,
            )
            .expect_err("should reject");
        assert!(err.to_string().contains("No pending buy-in"));
    }

    #[test]
    fn initiate_buy_in_validates_amount() {
        let state = ReservationState::default();
        let agg = Reservation::new();
        let err = agg
            .on_initiate_buy_in(
                InitiateBuyIn {
                    table_root: vec![0xaa],
                    seat: 1,
                    amount: Some(currency(0)),
                    player_root: vec![0xbb],
                },
                &state,
                0,
            )
            .expect_err("zero amount rejected");
        assert!(err.to_string().contains("amount must be positive"));
    }
}
