//! Reservation process manager — single PM for all three lifecycle flavors.
//!
//! Replaces the former `pmg-buy-in`, `pmg-rebuy`, and `pmg-registration` PMs.
//! Subscribes to `reservation`, `table`, and `tournament`. Translates each
//! reservation lifecycle event into the matching player primitive
//! (`ReserveFunds` on request, `DeductReservedFunds` on confirm, `ReleaseFunds`
//! on release) and coordinates the non-funds side of the flow with table /
//! tournament.
//!
//! Event → action map:
//!
//!   Buy-in:
//!     reservation.BuyInRequested        → player.ReserveFunds + table.SeatPlayer
//!     table.PlayerSeated                → reservation.ConfirmBuyIn
//!     reservation.BuyInConfirmed        → player.DeductReservedFunds
//!     table.SeatingRejected             → reservation.ReleaseBuyIn
//!     reservation.BuyInReservationReleased → player.ReleaseFunds
//!
//!   Rebuy:
//!     reservation.RebuyRequested        → player.ReserveFunds + tournament.ProcessRebuy
//!     tournament.RebuyProcessed         → table.AddRebuyChips
//!     table.RebuyChipsAdded             → reservation.ConfirmRebuyFee
//!     reservation.RebuyFeeConfirmed     → player.DeductReservedFunds
//!     tournament.RebuyDenied            → reservation.ReleaseRebuyFee
//!     reservation.RebuyFeeReleased      → player.ReleaseFunds
//!
//!   Registration:
//!     reservation.RegistrationRequested → player.ReserveFunds + tournament.EnrollPlayer
//!     tournament.TournamentPlayerEnrolled → reservation.ConfirmRegistrationFee
//!     reservation.RegistrationFeeConfirmed → player.DeductReservedFunds
//!     tournament.TournamentEnrollmentRejected → reservation.ReleaseRegistrationFee
//!     reservation.RegistrationFeeReleased → player.ReleaseFunds

pub mod state;

pub use state::{Kind, ReservationPMState};

use std::sync::Arc;

use angzarr_client::proto::command_page::Payload as CommandPayload;
use angzarr_client::proto::{
    event_page::Payload as EventPayload, page_header::SequenceType, CommandBook, CommandPage,
    Cover, EventBook, EventPage, MergeStrategy, PageHeader, ProcessManagerHandleResponse,
    Uuid as ProtoUuid,
};
use angzarr_client::{applies, handles, process_manager, type_url, CommandResult, QueryClient};
use examples_proto::{
    AddRebuyChips, BuyInCompleted, BuyInConfirmed, BuyInFailed, BuyInInitiated, BuyInPhase,
    BuyInRequested, BuyInReservationReleased, ConfirmBuyIn, ConfirmRebuyFee,
    ConfirmRegistrationFee, Currency, DeductReservedFunds, EnrollPlayer, OrchestrationFailure,
    PlayerSeated, ProcessRebuy, RebuyChipsAdded, RebuyCompleted, RebuyDenied, RebuyFailed,
    RebuyFeeConfirmed, RebuyFeeReleased, RebuyInitiated, RebuyPhase, RebuyProcessed,
    RebuyRequested, RegistrationCompleted, RegistrationFailed, RegistrationFeeConfirmed,
    RegistrationFeeReleased, RegistrationInitiated, RegistrationPhase, RegistrationRequested,
    ReleaseBuyIn, ReleaseFunds, ReleaseRebuyFee, ReleaseRegistrationFee, ReserveFunds, SeatPlayer,
    SeatingRejected, TournamentEnrollmentRejected, TournamentPlayerEnrolled,
};
use examples_utils::pack_event;
use prost::Message;
use prost_types::Any;

const PM_DOMAIN: &str = "pmg-reservation";

fn currency(amount: i64) -> Currency {
    Currency {
        amount,
        currency_code: "CHIPS".to_string(),
    }
}

fn make_command<M: Message>(
    domain: &str,
    root: &[u8],
    proto_type_name: &str,
    message: &M,
) -> CommandBook {
    CommandBook {
        cover: Some(Cover {
            domain: domain.to_string(),
            root: Some(ProtoUuid {
                value: root.to_vec(),
            }),
            correlation_id: uuid::Uuid::new_v4().to_string(),
            edition: None,
        }),
        pages: vec![CommandPage {
            header: Some(PageHeader {
                sequence_type: Some(SequenceType::Sequence(0)),
            }),
            merge_strategy: MergeStrategy::MergeCommutative as i32,
            payload: Some(CommandPayload::Command(Any {
                type_url: type_url(proto_type_name),
                value: message.encode_to_vec(),
            })),
        }],
    }
}

fn single_event_book(domain: &str, root: &[u8], event: Any) -> EventBook {
    EventBook {
        cover: Some(Cover {
            domain: domain.to_string(),
            root: Some(ProtoUuid {
                value: root.to_vec(),
            }),
            correlation_id: String::new(),
            edition: None,
        }),
        pages: vec![EventPage {
            header: Some(PageHeader {
                sequence_type: Some(SequenceType::Sequence(0)),
            }),
            created_at: Some(angzarr_client::now()),
            no_commit: false,
            cascade_id: None,
            payload: Some(EventPayload::Event(event)),
        }],
        snapshot: None,
        next_sequence: 0,
    }
}

fn failure(code: &str, message: &str, phase: &str) -> OrchestrationFailure {
    OrchestrationFailure {
        code: code.to_string(),
        message: message.to_string(),
        failed_at_phase: phase.to_string(),
        failed_at: Some(angzarr_client::now()),
    }
}

/// Consolidated reservation process manager.
#[derive(Default)]
pub struct ReservationPm {
    query_client: Option<Arc<QueryClient>>,
}

impl ReservationPm {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_query_client(query_client: Arc<QueryClient>) -> Self {
        Self {
            query_client: Some(query_client),
        }
    }

    /// Reads the configured query client (used in cross-aggregate sync DECISION
    /// pre-validation). Currently soft-skipped: the async client cannot be
    /// driven from the sync `#[handles]` signature without nesting runtimes,
    /// so live cluster runs lean on the destination aggregate to re-validate.
    pub fn query_client(&self) -> Option<&Arc<QueryClient>> {
        self.query_client.as_ref()
    }
}

#[process_manager(
    name = "pmg-reservation",
    pm_domain = "pmg-reservation",
    sources = ["reservation", "table", "tournament"],
    targets = ["player", "reservation", "table", "tournament"],
    state = ReservationPMState
)]
impl ReservationPm {
    // =========================================================================
    // Buy-in handlers
    // =========================================================================

    #[handles(BuyInRequested)]
    pub fn on_buy_in_requested(
        &self,
        event: BuyInRequested,
        _state: &ReservationPMState,
    ) -> CommandResult<ProcessManagerHandleResponse> {
        let amount = event.amount.as_ref().map(|c| c.amount).unwrap_or(0);

        let reserve = ReserveFunds {
            amount: Some(currency(amount)),
            key: event.table_root.clone(),
        };
        let seat = SeatPlayer {
            player_root: event.player_root.clone(),
            reservation_id: event.reservation_id.clone(),
            seat: event.seat,
            amount,
        };

        let initiated = BuyInInitiated {
            player_root: event.player_root.clone(),
            table_root: event.table_root.clone(),
            reservation_id: event.reservation_id.clone(),
            seat: event.seat,
            amount: Some(currency(amount)),
            phase: BuyInPhase::BuyInSeating as i32,
            initiated_at: Some(angzarr_client::now()),
        };

        Ok(ProcessManagerHandleResponse {
            commands: vec![
                make_command(
                    "player",
                    &event.player_root,
                    "examples.ReserveFunds",
                    &reserve,
                ),
                make_command("table", &event.table_root, "examples.SeatPlayer", &seat),
            ],
            process_events: vec![single_event_book(
                PM_DOMAIN,
                &event.reservation_id,
                pack_event(&initiated, "examples.BuyInInitiated"),
            )],
            facts: vec![],
        })
    }

    #[handles(PlayerSeated)]
    pub fn on_player_seated(
        &self,
        event: PlayerSeated,
        state: &ReservationPMState,
    ) -> CommandResult<ProcessManagerHandleResponse> {
        let confirm = ConfirmBuyIn {
            reservation_id: event.reservation_id.clone(),
        };
        let completed = BuyInCompleted {
            player_root: event.player_root.clone(),
            table_root: state.table_root.clone(),
            reservation_id: event.reservation_id.clone(),
            seat: event.seat_position,
            amount: Some(currency(event.stack)),
            completed_at: Some(angzarr_client::now()),
        };
        Ok(ProcessManagerHandleResponse {
            commands: vec![make_command(
                "reservation",
                &event.reservation_id,
                "examples.ConfirmBuyIn",
                &confirm,
            )],
            process_events: vec![single_event_book(
                PM_DOMAIN,
                &event.reservation_id,
                pack_event(&completed, "examples.BuyInCompleted"),
            )],
            facts: vec![],
        })
    }

    #[handles(SeatingRejected)]
    pub fn on_seating_rejected(
        &self,
        event: SeatingRejected,
        state: &ReservationPMState,
    ) -> CommandResult<ProcessManagerHandleResponse> {
        let release = ReleaseBuyIn {
            reservation_id: event.reservation_id.clone(),
            reason: event.reason.clone(),
        };
        let failed = BuyInFailed {
            player_root: event.player_root.clone(),
            table_root: state.table_root.clone(),
            reservation_id: event.reservation_id.clone(),
            failure: Some(failure("SEATING_REJECTED", &event.reason, "SEATING")),
        };
        Ok(ProcessManagerHandleResponse {
            commands: vec![make_command(
                "reservation",
                &event.reservation_id,
                "examples.ReleaseBuyIn",
                &release,
            )],
            process_events: vec![single_event_book(
                PM_DOMAIN,
                &event.reservation_id,
                pack_event(&failed, "examples.BuyInFailed"),
            )],
            facts: vec![],
        })
    }

    #[handles(BuyInConfirmed)]
    pub fn on_buy_in_confirmed(
        &self,
        event: BuyInConfirmed,
        state: &ReservationPMState,
    ) -> CommandResult<ProcessManagerHandleResponse> {
        let amount = event
            .amount
            .as_ref()
            .map(|c| c.amount)
            .unwrap_or(state.amount);
        let player_root = if event.player_root.is_empty() {
            state.player_root.clone()
        } else {
            event.player_root.clone()
        };
        let key = if event.table_root.is_empty() {
            state.table_root.clone()
        } else {
            event.table_root.clone()
        };
        let deduct = DeductReservedFunds {
            amount: Some(currency(amount)),
            key,
            reservation_id: event.reservation_id.clone(),
        };
        Ok(ProcessManagerHandleResponse {
            commands: vec![make_command(
                "player",
                &player_root,
                "examples.DeductReservedFunds",
                &deduct,
            )],
            process_events: vec![],
            facts: vec![],
        })
    }

    #[handles(BuyInReservationReleased)]
    pub fn on_buy_in_released(
        &self,
        event: BuyInReservationReleased,
        state: &ReservationPMState,
    ) -> CommandResult<ProcessManagerHandleResponse> {
        let player_root = if event.player_root.is_empty() {
            state.player_root.clone()
        } else {
            event.player_root.clone()
        };
        let key = if event.table_root.is_empty() {
            state.table_root.clone()
        } else {
            event.table_root.clone()
        };
        let release = ReleaseFunds { key };
        Ok(ProcessManagerHandleResponse {
            commands: vec![make_command(
                "player",
                &player_root,
                "examples.ReleaseFunds",
                &release,
            )],
            process_events: vec![],
            facts: vec![],
        })
    }

    // =========================================================================
    // Rebuy handlers
    // =========================================================================

    #[handles(RebuyRequested)]
    pub fn on_rebuy_requested(
        &self,
        event: RebuyRequested,
        _state: &ReservationPMState,
    ) -> CommandResult<ProcessManagerHandleResponse> {
        let fee = event.fee.as_ref().map(|c| c.amount).unwrap_or(0);

        let reserve = ReserveFunds {
            amount: Some(currency(fee)),
            key: event.table_root.clone(),
        };
        let process = ProcessRebuy {
            player_root: event.player_root.clone(),
            reservation_id: event.reservation_id.clone(),
        };

        let initiated = RebuyInitiated {
            player_root: event.player_root.clone(),
            tournament_root: event.tournament_root.clone(),
            table_root: event.table_root.clone(),
            reservation_id: event.reservation_id.clone(),
            seat: event.seat,
            fee: Some(currency(fee)),
            chips_to_add: 0,
            phase: RebuyPhase::RebuyApproving as i32,
            initiated_at: Some(angzarr_client::now()),
        };

        Ok(ProcessManagerHandleResponse {
            commands: vec![
                make_command(
                    "player",
                    &event.player_root,
                    "examples.ReserveFunds",
                    &reserve,
                ),
                make_command(
                    "tournament",
                    &event.tournament_root,
                    "examples.ProcessRebuy",
                    &process,
                ),
            ],
            process_events: vec![single_event_book(
                PM_DOMAIN,
                &event.reservation_id,
                pack_event(&initiated, "examples.RebuyInitiated"),
            )],
            facts: vec![],
        })
    }

    #[handles(RebuyProcessed)]
    pub fn on_rebuy_processed(
        &self,
        event: RebuyProcessed,
        state: &ReservationPMState,
    ) -> CommandResult<ProcessManagerHandleResponse> {
        let add_chips = AddRebuyChips {
            player_root: event.player_root.clone(),
            reservation_id: event.reservation_id.clone(),
            seat: state.seat,
            amount: event.chips_added,
        };
        Ok(ProcessManagerHandleResponse {
            commands: vec![make_command(
                "table",
                &state.table_root,
                "examples.AddRebuyChips",
                &add_chips,
            )],
            process_events: vec![],
            facts: vec![],
        })
    }

    #[handles(RebuyChipsAdded)]
    pub fn on_rebuy_chips_added(
        &self,
        event: RebuyChipsAdded,
        state: &ReservationPMState,
    ) -> CommandResult<ProcessManagerHandleResponse> {
        let confirm = ConfirmRebuyFee {
            reservation_id: event.reservation_id.clone(),
        };
        let completed = RebuyCompleted {
            player_root: event.player_root.clone(),
            tournament_root: state.tournament_root.clone(),
            table_root: state.table_root.clone(),
            reservation_id: event.reservation_id.clone(),
            fee: Some(currency(state.fee)),
            chips_added: event.amount,
            completed_at: Some(angzarr_client::now()),
        };
        Ok(ProcessManagerHandleResponse {
            commands: vec![make_command(
                "reservation",
                &event.reservation_id,
                "examples.ConfirmRebuyFee",
                &confirm,
            )],
            process_events: vec![single_event_book(
                PM_DOMAIN,
                &event.reservation_id,
                pack_event(&completed, "examples.RebuyCompleted"),
            )],
            facts: vec![],
        })
    }

    #[handles(RebuyDenied)]
    pub fn on_rebuy_denied(
        &self,
        event: RebuyDenied,
        state: &ReservationPMState,
    ) -> CommandResult<ProcessManagerHandleResponse> {
        let release = ReleaseRebuyFee {
            reservation_id: event.reservation_id.clone(),
            reason: event.reason.clone(),
        };
        let failed = RebuyFailed {
            player_root: event.player_root.clone(),
            tournament_root: state.tournament_root.clone(),
            reservation_id: event.reservation_id.clone(),
            failure: Some(failure("REBUY_DENIED", &event.reason, "APPROVING")),
        };
        Ok(ProcessManagerHandleResponse {
            commands: vec![make_command(
                "reservation",
                &event.reservation_id,
                "examples.ReleaseRebuyFee",
                &release,
            )],
            process_events: vec![single_event_book(
                PM_DOMAIN,
                &event.reservation_id,
                pack_event(&failed, "examples.RebuyFailed"),
            )],
            facts: vec![],
        })
    }

    #[handles(RebuyFeeConfirmed)]
    pub fn on_rebuy_fee_confirmed(
        &self,
        event: RebuyFeeConfirmed,
        state: &ReservationPMState,
    ) -> CommandResult<ProcessManagerHandleResponse> {
        let fee = event.fee.as_ref().map(|c| c.amount).unwrap_or(state.fee);
        let player_root = if event.player_root.is_empty() {
            state.player_root.clone()
        } else {
            event.player_root.clone()
        };
        let key = if event.table_root.is_empty() {
            state.table_root.clone()
        } else {
            event.table_root.clone()
        };
        let deduct = DeductReservedFunds {
            amount: Some(currency(fee)),
            key,
            reservation_id: event.reservation_id.clone(),
        };
        Ok(ProcessManagerHandleResponse {
            commands: vec![make_command(
                "player",
                &player_root,
                "examples.DeductReservedFunds",
                &deduct,
            )],
            process_events: vec![],
            facts: vec![],
        })
    }

    #[handles(RebuyFeeReleased)]
    pub fn on_rebuy_fee_released(
        &self,
        event: RebuyFeeReleased,
        state: &ReservationPMState,
    ) -> CommandResult<ProcessManagerHandleResponse> {
        let player_root = if event.player_root.is_empty() {
            state.player_root.clone()
        } else {
            event.player_root.clone()
        };
        let key = if event.table_root.is_empty() {
            state.table_root.clone()
        } else {
            event.table_root.clone()
        };
        Ok(ProcessManagerHandleResponse {
            commands: vec![make_command(
                "player",
                &player_root,
                "examples.ReleaseFunds",
                &ReleaseFunds { key },
            )],
            process_events: vec![],
            facts: vec![],
        })
    }

    // =========================================================================
    // Registration handlers
    // =========================================================================

    #[handles(RegistrationRequested)]
    pub fn on_registration_requested(
        &self,
        event: RegistrationRequested,
        _state: &ReservationPMState,
    ) -> CommandResult<ProcessManagerHandleResponse> {
        let fee = event.fee.as_ref().map(|c| c.amount).unwrap_or(0);

        let reserve = ReserveFunds {
            amount: Some(currency(fee)),
            key: event.tournament_root.clone(),
        };
        let enroll = EnrollPlayer {
            player_root: event.player_root.clone(),
            reservation_id: event.reservation_id.clone(),
        };

        let initiated = RegistrationInitiated {
            player_root: event.player_root.clone(),
            tournament_root: event.tournament_root.clone(),
            reservation_id: event.reservation_id.clone(),
            fee: Some(currency(fee)),
            phase: RegistrationPhase::RegistrationEnrolling as i32,
            initiated_at: Some(angzarr_client::now()),
        };

        Ok(ProcessManagerHandleResponse {
            commands: vec![
                make_command(
                    "player",
                    &event.player_root,
                    "examples.ReserveFunds",
                    &reserve,
                ),
                make_command(
                    "tournament",
                    &event.tournament_root,
                    "examples.EnrollPlayer",
                    &enroll,
                ),
            ],
            process_events: vec![single_event_book(
                PM_DOMAIN,
                &event.reservation_id,
                pack_event(&initiated, "examples.RegistrationInitiated"),
            )],
            facts: vec![],
        })
    }

    #[handles(TournamentPlayerEnrolled)]
    pub fn on_player_enrolled(
        &self,
        event: TournamentPlayerEnrolled,
        state: &ReservationPMState,
    ) -> CommandResult<ProcessManagerHandleResponse> {
        let confirm = ConfirmRegistrationFee {
            reservation_id: event.reservation_id.clone(),
        };
        let completed = RegistrationCompleted {
            player_root: event.player_root.clone(),
            tournament_root: state.tournament_root.clone(),
            reservation_id: event.reservation_id.clone(),
            fee: Some(currency(state.fee)),
            starting_stack: event.starting_stack,
            completed_at: Some(angzarr_client::now()),
        };
        Ok(ProcessManagerHandleResponse {
            commands: vec![make_command(
                "reservation",
                &event.reservation_id,
                "examples.ConfirmRegistrationFee",
                &confirm,
            )],
            process_events: vec![single_event_book(
                PM_DOMAIN,
                &event.reservation_id,
                pack_event(&completed, "examples.RegistrationCompleted"),
            )],
            facts: vec![],
        })
    }

    #[handles(TournamentEnrollmentRejected)]
    pub fn on_enrollment_rejected(
        &self,
        event: TournamentEnrollmentRejected,
        state: &ReservationPMState,
    ) -> CommandResult<ProcessManagerHandleResponse> {
        let release = ReleaseRegistrationFee {
            reservation_id: event.reservation_id.clone(),
            reason: event.reason.clone(),
        };
        let failed = RegistrationFailed {
            player_root: event.player_root.clone(),
            tournament_root: state.tournament_root.clone(),
            reservation_id: event.reservation_id.clone(),
            failure: Some(failure("ENROLLMENT_REJECTED", &event.reason, "ENROLLING")),
        };
        Ok(ProcessManagerHandleResponse {
            commands: vec![make_command(
                "reservation",
                &event.reservation_id,
                "examples.ReleaseRegistrationFee",
                &release,
            )],
            process_events: vec![single_event_book(
                PM_DOMAIN,
                &event.reservation_id,
                pack_event(&failed, "examples.RegistrationFailed"),
            )],
            facts: vec![],
        })
    }

    #[handles(RegistrationFeeConfirmed)]
    pub fn on_registration_confirmed(
        &self,
        event: RegistrationFeeConfirmed,
        state: &ReservationPMState,
    ) -> CommandResult<ProcessManagerHandleResponse> {
        let fee = event.fee.as_ref().map(|c| c.amount).unwrap_or(state.fee);
        let player_root = if event.player_root.is_empty() {
            state.player_root.clone()
        } else {
            event.player_root.clone()
        };
        let key = if event.tournament_root.is_empty() {
            state.tournament_root.clone()
        } else {
            event.tournament_root.clone()
        };
        let deduct = DeductReservedFunds {
            amount: Some(currency(fee)),
            key,
            reservation_id: event.reservation_id.clone(),
        };
        Ok(ProcessManagerHandleResponse {
            commands: vec![make_command(
                "player",
                &player_root,
                "examples.DeductReservedFunds",
                &deduct,
            )],
            process_events: vec![],
            facts: vec![],
        })
    }

    #[handles(RegistrationFeeReleased)]
    pub fn on_registration_released(
        &self,
        event: RegistrationFeeReleased,
        state: &ReservationPMState,
    ) -> CommandResult<ProcessManagerHandleResponse> {
        let player_root = if event.player_root.is_empty() {
            state.player_root.clone()
        } else {
            event.player_root.clone()
        };
        let key = if event.tournament_root.is_empty() {
            state.tournament_root.clone()
        } else {
            event.tournament_root.clone()
        };
        Ok(ProcessManagerHandleResponse {
            commands: vec![make_command(
                "player",
                &player_root,
                "examples.ReleaseFunds",
                &ReleaseFunds { key },
            )],
            process_events: vec![],
            facts: vec![],
        })
    }

    // =========================================================================
    // State appliers
    // =========================================================================

    #[applies(BuyInInitiated)]
    pub fn apply_buy_in_initiated(state: &mut ReservationPMState, event: BuyInInitiated) {
        state.kind = Kind::BuyIn;
        state.reservation_id = event.reservation_id;
        state.player_root = event.player_root;
        state.table_root = event.table_root;
        state.seat = event.seat;
        state.amount = event.amount.as_ref().map(|c| c.amount).unwrap_or(0);
        state.phase = event.phase;
    }

    #[applies(BuyInCompleted)]
    pub fn apply_buy_in_completed(state: &mut ReservationPMState, _event: BuyInCompleted) {
        state.phase = BuyInPhase::BuyInCompleted as i32;
    }

    #[applies(BuyInFailed)]
    pub fn apply_buy_in_failed(state: &mut ReservationPMState, _event: BuyInFailed) {
        state.phase = BuyInPhase::BuyInFailed as i32;
    }

    #[applies(RebuyInitiated)]
    pub fn apply_rebuy_initiated(state: &mut ReservationPMState, event: RebuyInitiated) {
        state.kind = Kind::Rebuy;
        state.reservation_id = event.reservation_id;
        state.player_root = event.player_root;
        state.tournament_root = event.tournament_root;
        state.table_root = event.table_root;
        state.seat = event.seat;
        state.fee = event.fee.as_ref().map(|c| c.amount).unwrap_or(0);
        state.phase = event.phase;
    }

    #[applies(RebuyCompleted)]
    pub fn apply_rebuy_completed(state: &mut ReservationPMState, _event: RebuyCompleted) {
        state.phase = RebuyPhase::RebuyCompleted as i32;
    }

    #[applies(RebuyFailed)]
    pub fn apply_rebuy_failed(state: &mut ReservationPMState, _event: RebuyFailed) {
        state.phase = RebuyPhase::RebuyFailed as i32;
    }

    #[applies(RegistrationInitiated)]
    pub fn apply_registration_initiated(
        state: &mut ReservationPMState,
        event: RegistrationInitiated,
    ) {
        state.kind = Kind::Registration;
        state.reservation_id = event.reservation_id;
        state.player_root = event.player_root;
        state.tournament_root = event.tournament_root;
        state.fee = event.fee.as_ref().map(|c| c.amount).unwrap_or(0);
        state.phase = event.phase;
    }

    #[applies(RegistrationCompleted)]
    pub fn apply_registration_completed(
        state: &mut ReservationPMState,
        _event: RegistrationCompleted,
    ) {
        state.phase = RegistrationPhase::RegistrationCompleted as i32;
    }

    #[applies(RegistrationFailed)]
    pub fn apply_registration_failed(state: &mut ReservationPMState, _event: RegistrationFailed) {
        state.phase = RegistrationPhase::RegistrationFailed as i32;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn first_command_domain(resp: &ProcessManagerHandleResponse, idx: usize) -> &str {
        resp.commands[idx]
            .cover
            .as_ref()
            .map(|c| c.domain.as_str())
            .unwrap_or("")
    }

    #[test]
    fn buy_in_requested_emits_reserve_and_seat() {
        let pm = ReservationPm::new();
        let resp = pm
            .on_buy_in_requested(
                BuyInRequested {
                    reservation_id: vec![0x1],
                    table_root: vec![0x2],
                    seat: 3,
                    amount: Some(currency(500)),
                    requested_at: None,
                    player_root: vec![0x4],
                },
                &ReservationPMState::default(),
            )
            .expect("ok");
        assert_eq!(resp.commands.len(), 2);
        assert_eq!(first_command_domain(&resp, 0), "player");
        assert_eq!(first_command_domain(&resp, 1), "table");
        assert_eq!(resp.process_events.len(), 1);
    }

    #[test]
    fn rebuy_requested_emits_reserve_and_process_rebuy() {
        let pm = ReservationPm::new();
        let resp = pm
            .on_rebuy_requested(
                RebuyRequested {
                    reservation_id: vec![0x1],
                    tournament_root: vec![0x9],
                    table_root: vec![0x2],
                    seat: 3,
                    fee: Some(currency(100)),
                    requested_at: None,
                    player_root: vec![0x4],
                },
                &ReservationPMState::default(),
            )
            .expect("ok");
        assert_eq!(resp.commands.len(), 2);
        assert_eq!(first_command_domain(&resp, 0), "player");
        assert_eq!(first_command_domain(&resp, 1), "tournament");
    }

    #[test]
    fn registration_requested_uses_tournament_root_as_reservation_key() {
        let pm = ReservationPm::new();
        let resp = pm
            .on_registration_requested(
                RegistrationRequested {
                    reservation_id: vec![0x1],
                    tournament_root: vec![0xab, 0xcd],
                    fee: Some(currency(300)),
                    requested_at: None,
                    player_root: vec![0x4],
                },
                &ReservationPMState::default(),
            )
            .expect("ok");
        assert_eq!(resp.commands.len(), 2);
        // First command is ReserveFunds against the tournament_root key
        let cmd0 = &resp.commands[0];
        let payload = match cmd0.pages[0].payload.as_ref().unwrap() {
            CommandPayload::Command(any) => any,
            _ => panic!("inline command"),
        };
        let reserve = ReserveFunds::decode(payload.value.as_slice()).unwrap();
        assert_eq!(reserve.key, vec![0xab, 0xcd]);
    }

    #[test]
    fn buy_in_confirmed_emits_deduct() {
        let pm = ReservationPm::new();
        let state = ReservationPMState {
            kind: Kind::BuyIn,
            player_root: vec![0xaa],
            table_root: vec![0xbb],
            amount: 500,
            ..Default::default()
        };
        let resp = pm
            .on_buy_in_confirmed(
                BuyInConfirmed {
                    reservation_id: vec![0x1],
                    table_root: vec![],
                    seat: 0,
                    amount: None,
                    confirmed_at: None,
                    player_root: vec![],
                },
                &state,
            )
            .expect("ok");
        assert_eq!(resp.commands.len(), 1);
        let cmd0 = &resp.commands[0];
        let payload = match cmd0.pages[0].payload.as_ref().unwrap() {
            CommandPayload::Command(any) => any,
            _ => panic!("inline command"),
        };
        let deduct = DeductReservedFunds::decode(payload.value.as_slice()).unwrap();
        assert_eq!(deduct.key, vec![0xbb]);
        assert_eq!(deduct.amount.unwrap().amount, 500);
    }
}
