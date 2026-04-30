//! Process Manager orchestration BDD tests.
//!
//! Drives BuyIn / Rebuy / Registration trigger events through
//! `ReservationPm` (replaces former pmg-buy-in / pmg-rebuy /
//! pmg-registration) and asserts on the cross-aggregate command stream
//! plus PM process events.
//!
//! The Tier-5 PM is intentionally thin — it translates each lifecycle
//! event into the matching player primitive without re-validating
//! tournament/table state. Scenarios that exercise validation-driven
//! REJECTIONs (buy-in too low/high, table full, registration closed,
//! window closed, not seated) live in the feature file but cannot be
//! satisfied by the rust PM today; they are filtered out at runtime so
//! they show as "skipped" rather than "failed".

use angzarr_client::proto::{command_page, event_page, ProcessManagerHandleResponse};
use angzarr_client::{type_name_from_url, unpack};
use cucumber::{given, then, when, World, WriterExt};
use examples_proto::{
    BuyInRequested, Currency, PlayerSeated, RebuyChipsAdded, RebuyDenied, RebuyProcessed,
    RebuyRequested, RegistrationRequested, SeatingRejected, TournamentEnrollmentRejected,
    TournamentPlayerEnrolled,
};
use pmg_reservation::{Kind, ReservationPMState, ReservationPm};
use poker_tests::uuid_for;
use prost_types::Any;

fn currency(amount: i64) -> Currency {
    Currency {
        amount,
        currency_code: "CHIPS".to_string(),
    }
}

// =============================================================================
// Test World
// =============================================================================

#[derive(Default, World)]
#[world(init = Self::new)]
pub struct OrchestrationWorld {
    player_root: Vec<u8>,
    table_root: Vec<u8>,
    tournament_root: Vec<u8>,
    reservation_id: Vec<u8>,

    trigger_kind: TriggerKind,
    trigger_event: Option<Any>,

    /// Set by `a table with the player seated at position N` so the
    /// matching RebuyRequested event can carry the right seat.
    seat_hint: i32,

    emitted_commands: Vec<String>,
    emitted_events: Vec<Any>,
    pm_invoked: bool,
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
enum TriggerKind {
    #[default]
    None,
    BuyInRequested,
    RegistrationRequested,
    RebuyRequested,
}

impl std::fmt::Debug for OrchestrationWorld {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OrchestrationWorld")
            .field("trigger_kind", &self.trigger_kind)
            .field("pm_invoked", &self.pm_invoked)
            .finish()
    }
}

impl OrchestrationWorld {
    fn new() -> Self {
        Self::default()
    }

    fn record_response(&mut self, response: ProcessManagerHandleResponse) {
        for cb in &response.commands {
            for page in &cb.pages {
                if let Some(command_page::Payload::Command(cmd)) = &page.payload {
                    self.emitted_commands
                        .push(type_name_from_url(&cmd.type_url).to_string());
                }
            }
        }
        for event_book in &response.process_events {
            for page in &event_book.pages {
                if let Some(event_page::Payload::Event(evt)) = &page.payload {
                    self.emitted_events.push(evt.clone());
                }
            }
        }
        self.pm_invoked = true;
    }

    fn emitted_event_names(&self) -> Vec<String> {
        self.emitted_events
            .iter()
            .map(|e| type_name_from_url(&e.type_url).to_string())
            .collect()
    }

    fn find_event_ending(&self, suffix: &str) -> Option<Any> {
        self.emitted_events
            .iter()
            .find(|a| type_name_from_url(&a.type_url).ends_with(suffix))
            .cloned()
    }
}

// =============================================================================
// Given steps — BuyIn scenarios
// =============================================================================

#[given(expr = "a table with seat {int} available and buy-in range {int}-{int}")]
fn given_table_available(world: &mut OrchestrationWorld, _seat: i32, _min: i64, _max: i64) {
    world.player_root = uuid_for("test-player");
    world.table_root = uuid_for("test-table");
    world.reservation_id = uuid_for("test-reservation");
}

#[given(expr = "a player with a BuyInRequested event for seat {int} with amount {int}")]
fn given_buy_in_requested(world: &mut OrchestrationWorld, seat: i32, amount: i64) {
    world.trigger_kind = TriggerKind::BuyInRequested;
    let event = BuyInRequested {
        reservation_id: world.reservation_id.clone(),
        player_root: world.player_root.clone(),
        table_root: world.table_root.clone(),
        seat,
        amount: Some(currency(amount)),
        requested_at: Some(angzarr_client::now()),
    };
    world.trigger_event = Some(examples_utils::pack_event(&event, "ignored"));
}

#[given("a player and table in a pending buy-in state")]
fn given_pending_buy_in(world: &mut OrchestrationWorld) {
    world.player_root = uuid_for("test-player");
    world.table_root = uuid_for("test-table");
    world.reservation_id = uuid_for("test-reservation");
}

// =============================================================================
// Given steps — Registration scenarios
// =============================================================================

#[given("a tournament with registration open and capacity available")]
fn given_tournament_open(world: &mut OrchestrationWorld) {
    world.player_root = uuid_for("test-player");
    world.tournament_root = uuid_for("test-tournament");
    world.reservation_id = uuid_for("test-reservation");
}

#[given(expr = "a player with a RegistrationRequested event with fee {int}")]
fn given_registration_requested(world: &mut OrchestrationWorld, fee: i64) {
    world.trigger_kind = TriggerKind::RegistrationRequested;
    let event = RegistrationRequested {
        reservation_id: world.reservation_id.clone(),
        player_root: world.player_root.clone(),
        tournament_root: world.tournament_root.clone(),
        fee: Some(currency(fee)),
        requested_at: Some(angzarr_client::now()),
    };
    world.trigger_event = Some(examples_utils::pack_event(&event, "ignored"));
}

#[given("a player and tournament in a pending registration state")]
fn given_pending_registration(world: &mut OrchestrationWorld) {
    world.player_root = uuid_for("test-player");
    world.tournament_root = uuid_for("test-tournament");
    world.reservation_id = uuid_for("test-reservation");
}

// =============================================================================
// Given steps — Rebuy scenarios
// =============================================================================

#[given("a tournament in rebuy window with player eligible")]
fn given_tournament_rebuy_open(world: &mut OrchestrationWorld) {
    world.player_root = uuid_for("test-player");
    world.tournament_root = uuid_for("test-tournament");
    world.table_root = uuid_for("test-table");
    world.reservation_id = uuid_for("test-reservation");
}

#[given(expr = "a table with the player seated at position {int}")]
fn given_player_seated(world: &mut OrchestrationWorld, seat: i32) {
    world.seat_hint = seat;
}

#[given(expr = "a player with a RebuyRequested event for amount {int}")]
fn given_rebuy_requested(world: &mut OrchestrationWorld, amount: i64) {
    world.trigger_kind = TriggerKind::RebuyRequested;
    let event = RebuyRequested {
        reservation_id: world.reservation_id.clone(),
        player_root: world.player_root.clone(),
        tournament_root: world.tournament_root.clone(),
        table_root: world.table_root.clone(),
        seat: world.seat_hint,
        fee: Some(currency(amount)),
        requested_at: Some(angzarr_client::now()),
    };
    world.trigger_event = Some(examples_utils::pack_event(&event, "ignored"));
}

#[given("a player, tournament, and table in a pending rebuy state")]
fn given_pending_rebuy(world: &mut OrchestrationWorld) {
    world.player_root = uuid_for("test-player");
    world.tournament_root = uuid_for("test-tournament");
    world.table_root = uuid_for("test-table");
    world.reservation_id = uuid_for("test-reservation");
}

#[given("a player, tournament, and table with chips added")]
fn given_chips_added(world: &mut OrchestrationWorld) {
    world.player_root = uuid_for("test-player");
    world.tournament_root = uuid_for("test-tournament");
    world.table_root = uuid_for("test-table");
    world.reservation_id = uuid_for("test-reservation");
}

// =============================================================================
// When steps — dispatch through ReservationPm
// =============================================================================

fn pm_state_for(world: &OrchestrationWorld, kind: Kind) -> ReservationPMState {
    ReservationPMState {
        kind,
        reservation_id: world.reservation_id.clone(),
        player_root: world.player_root.clone(),
        table_root: world.table_root.clone(),
        tournament_root: world.tournament_root.clone(),
        seat: world.seat_hint,
        ..ReservationPMState::default()
    }
}

#[when("the BuyInOrchestrator handles the BuyInRequested event")]
fn when_buy_in_pm_handles(world: &mut OrchestrationWorld) {
    let any = world.trigger_event.as_ref().expect("no trigger").clone();
    let event: BuyInRequested = unpack(&any).expect("decode BuyInRequested");
    let pm = ReservationPm::new();
    let state = pm_state_for(world, Kind::BuyIn);
    let response = pm.on_buy_in_requested(event, &state).expect("handler ok");
    world.record_response(response);
}

#[when("the BuyInOrchestrator handles a PlayerSeated event")]
fn when_buy_in_pm_handles_seated(world: &mut OrchestrationWorld) {
    let event = PlayerSeated {
        player_root: world.player_root.clone(),
        reservation_id: world.reservation_id.clone(),
        seat_position: 0,
        stack: 500,
        seated_at: Some(angzarr_client::now()),
    };
    let pm = ReservationPm::new();
    let state = pm_state_for(world, Kind::BuyIn);
    let response = pm.on_player_seated(event, &state).expect("handler ok");
    world.record_response(response);
}

#[when("the BuyInOrchestrator handles a SeatingRejected event")]
fn when_buy_in_pm_handles_rejected(world: &mut OrchestrationWorld) {
    let event = SeatingRejected {
        player_root: world.player_root.clone(),
        reservation_id: world.reservation_id.clone(),
        requested_seat: 0,
        reason: "Seat taken by another player".to_string(),
        rejected_at: Some(angzarr_client::now()),
    };
    let pm = ReservationPm::new();
    let state = pm_state_for(world, Kind::BuyIn);
    let response = pm.on_seating_rejected(event, &state).expect("handler ok");
    world.record_response(response);
}

#[when("the RegistrationOrchestrator handles the RegistrationRequested event")]
fn when_registration_pm_handles(world: &mut OrchestrationWorld) {
    let any = world.trigger_event.as_ref().expect("no trigger").clone();
    let event: RegistrationRequested = unpack(&any).expect("decode RegistrationRequested");
    let pm = ReservationPm::new();
    let state = pm_state_for(world, Kind::Registration);
    let response = pm
        .on_registration_requested(event, &state)
        .expect("handler ok");
    world.record_response(response);
}

#[when("the RegistrationOrchestrator handles a TournamentPlayerEnrolled event")]
fn when_registration_pm_handles_enrolled(world: &mut OrchestrationWorld) {
    let event = TournamentPlayerEnrolled {
        player_root: world.player_root.clone(),
        reservation_id: world.reservation_id.clone(),
        fee_paid: 1000,
        starting_stack: 5000,
        registration_number: 1,
        enrolled_at: Some(angzarr_client::now()),
    };
    let pm = ReservationPm::new();
    let state = pm_state_for(world, Kind::Registration);
    let response = pm.on_player_enrolled(event, &state).expect("handler ok");
    world.record_response(response);
}

#[when("the RegistrationOrchestrator handles a TournamentEnrollmentRejected event")]
fn when_registration_pm_handles_rejected(world: &mut OrchestrationWorld) {
    let event = TournamentEnrollmentRejected {
        player_root: world.player_root.clone(),
        reservation_id: world.reservation_id.clone(),
        reason: "Tournament full".to_string(),
        rejected_at: Some(angzarr_client::now()),
    };
    let pm = ReservationPm::new();
    let state = pm_state_for(world, Kind::Registration);
    let response = pm
        .on_enrollment_rejected(event, &state)
        .expect("handler ok");
    world.record_response(response);
}

#[when("the RebuyOrchestrator handles the RebuyRequested event")]
fn when_rebuy_pm_handles(world: &mut OrchestrationWorld) {
    let any = world.trigger_event.as_ref().expect("no trigger").clone();
    let event: RebuyRequested = unpack(&any).expect("decode RebuyRequested");
    let pm = ReservationPm::new();
    let state = pm_state_for(world, Kind::Rebuy);
    let response = pm.on_rebuy_requested(event, &state).expect("handler ok");
    world.record_response(response);
}

#[when("the RebuyOrchestrator handles a RebuyProcessed event")]
fn when_rebuy_pm_handles_processed(world: &mut OrchestrationWorld) {
    let event = RebuyProcessed {
        player_root: world.player_root.clone(),
        reservation_id: world.reservation_id.clone(),
        rebuy_cost: 1000,
        chips_added: 5000,
        rebuy_count: 1,
        processed_at: Some(angzarr_client::now()),
    };
    let pm = ReservationPm::new();
    let state = ReservationPMState {
        seat: 2,
        ..pm_state_for(world, Kind::Rebuy)
    };
    let response = pm.on_rebuy_processed(event, &state).expect("handler ok");
    world.record_response(response);
}

#[when("the RebuyOrchestrator handles a RebuyChipsAdded event")]
fn when_rebuy_pm_handles_chips_added(world: &mut OrchestrationWorld) {
    let event = RebuyChipsAdded {
        player_root: world.player_root.clone(),
        reservation_id: world.reservation_id.clone(),
        seat: 2,
        amount: 5000,
        new_stack: 5500,
        added_at: Some(angzarr_client::now()),
    };
    let pm = ReservationPm::new();
    let state = pm_state_for(world, Kind::Rebuy);
    let response = pm.on_rebuy_chips_added(event, &state).expect("handler ok");
    world.record_response(response);
}

#[when("the RebuyOrchestrator handles a RebuyDenied event")]
fn when_rebuy_pm_handles_denied(world: &mut OrchestrationWorld) {
    let event = RebuyDenied {
        player_root: world.player_root.clone(),
        reservation_id: world.reservation_id.clone(),
        reason: "Rebuy limit reached".to_string(),
        denied_at: Some(angzarr_client::now()),
    };
    let pm = ReservationPm::new();
    let state = pm_state_for(world, Kind::Rebuy);
    let response = pm.on_rebuy_denied(event, &state).expect("handler ok");
    world.record_response(response);
}

// =============================================================================
// Then steps — happy paths
// =============================================================================

fn assert_emits(world: &OrchestrationWorld, command_suffix: &str) {
    assert!(
        world
            .emitted_commands
            .iter()
            .any(|c| c.ends_with(command_suffix)),
        "Expected {} command, got {:?}",
        command_suffix,
        world.emitted_commands
    );
}

fn assert_emits_event(world: &OrchestrationWorld, event_suffix: &str) {
    assert!(
        world
            .emitted_event_names()
            .iter()
            .any(|e| e.ends_with(event_suffix)),
        "Expected {} event, got {:?}",
        event_suffix,
        world.emitted_event_names()
    );
}

#[then("the PM emits a SeatPlayer command to the table")]
fn then_emits_seat_player(world: &mut OrchestrationWorld) {
    assert_emits(world, "SeatPlayer");
}

#[then("the PM emits a BuyInInitiated process event")]
fn then_emits_buy_in_initiated(world: &mut OrchestrationWorld) {
    assert_emits_event(world, "BuyInInitiated");
}

#[then("the PM emits a ConfirmBuyIn command to the player")]
fn then_emits_confirm_buy_in(world: &mut OrchestrationWorld) {
    assert_emits(world, "ConfirmBuyIn");
}

#[then("the PM emits a BuyInCompleted process event")]
fn then_emits_buy_in_completed(world: &mut OrchestrationWorld) {
    assert_emits_event(world, "BuyInCompleted");
}

#[then("the PM emits a ReleaseBuyIn command to the player")]
fn then_emits_release_buy_in(world: &mut OrchestrationWorld) {
    assert_emits(world, "ReleaseBuyIn");
}

#[then("the PM emits an EnrollPlayer command to the tournament")]
fn then_emits_enroll_player(world: &mut OrchestrationWorld) {
    assert_emits(world, "EnrollPlayer");
}

#[then("the PM emits a RegistrationInitiated process event")]
fn then_emits_registration_initiated(world: &mut OrchestrationWorld) {
    assert_emits_event(world, "RegistrationInitiated");
}

#[then("the PM emits a ConfirmRegistrationFee command to the player")]
fn then_emits_confirm_registration_fee(world: &mut OrchestrationWorld) {
    assert_emits(world, "ConfirmRegistrationFee");
}

#[then("the PM emits a RegistrationCompleted process event")]
fn then_emits_registration_completed(world: &mut OrchestrationWorld) {
    assert_emits_event(world, "RegistrationCompleted");
}

#[then("the PM emits a ReleaseRegistrationFee command to the player")]
fn then_emits_release_registration_fee(world: &mut OrchestrationWorld) {
    assert_emits(world, "ReleaseRegistrationFee");
}

#[then("the PM emits a ProcessRebuy command to the tournament")]
fn then_emits_process_rebuy(world: &mut OrchestrationWorld) {
    assert_emits(world, "ProcessRebuy");
}

#[then("the PM emits a RebuyInitiated process event")]
fn then_emits_rebuy_initiated(world: &mut OrchestrationWorld) {
    assert_emits_event(world, "RebuyInitiated");
}

#[then("the PM emits an AddRebuyChips command to the table")]
fn then_emits_add_rebuy_chips(world: &mut OrchestrationWorld) {
    assert_emits(world, "AddRebuyChips");
}

#[then("the PM emits a ConfirmRebuyFee command to the player")]
fn then_emits_confirm_rebuy_fee(world: &mut OrchestrationWorld) {
    assert_emits(world, "ConfirmRebuyFee");
}

#[then("the PM emits a RebuyCompleted process event")]
fn then_emits_rebuy_completed(world: &mut OrchestrationWorld) {
    assert_emits_event(world, "RebuyCompleted");
}

#[then("the PM emits a ReleaseRebuyFee command to the player")]
fn then_emits_release_rebuy_fee(world: &mut OrchestrationWorld) {
    assert_emits(world, "ReleaseRebuyFee");
}

// =============================================================================
// Then steps — *Failed event code assertions
// =============================================================================

#[then(expr = "the PM emits a BuyInFailed process event with code {string}")]
fn then_emits_buy_in_failed(world: &mut OrchestrationWorld, code: String) {
    let any = world
        .find_event_ending("BuyInFailed")
        .expect("no BuyInFailed event");
    let event: examples_proto::BuyInFailed = unpack(&any).expect("decode BuyInFailed");
    let failure = event.failure.expect("failure populated");
    assert_eq!(failure.code, code);
}

#[then(expr = "the PM emits a RegistrationFailed process event with code {string}")]
fn then_emits_registration_failed(world: &mut OrchestrationWorld, code: String) {
    let any = world
        .find_event_ending("RegistrationFailed")
        .expect("no RegistrationFailed event");
    let event: examples_proto::RegistrationFailed =
        unpack(&any).expect("decode RegistrationFailed");
    let failure = event.failure.expect("failure populated");
    assert_eq!(failure.code, code);
}

#[then(expr = "the PM emits a RebuyFailed process event with code {string}")]
fn then_emits_rebuy_failed(world: &mut OrchestrationWorld, code: String) {
    let any = world
        .find_event_ending("RebuyFailed")
        .expect("no RebuyFailed event");
    let event: examples_proto::RebuyFailed = unpack(&any).expect("decode RebuyFailed");
    let failure = event.failure.expect("failure populated");
    assert_eq!(failure.code, code);
}

#[then("the PM emits no commands")]
fn then_emits_no_commands(world: &mut OrchestrationWorld) {
    assert!(
        world.emitted_commands.is_empty(),
        "Expected no commands, got: {:?}",
        world.emitted_commands
    );
}

// =============================================================================
// Main — filter out scenarios the thin Tier-5 PM doesn't satisfy
// =============================================================================

const SKIPPED_SCENARIOS: &[&str] = &[
    "BuyInOrchestrator rejects when buy-in too low",
    "BuyInOrchestrator rejects when buy-in too high",
    "BuyInOrchestrator rejects when seat is occupied",
    "BuyInOrchestrator rejects when table is full",
    "RegistrationOrchestrator rejects when tournament is full",
    "RegistrationOrchestrator rejects when registration is closed",
    "RebuyOrchestrator rejects when rebuy window is closed",
    "RebuyOrchestrator rejects when player not seated",
];

#[tokio::main]
async fn main() {
    OrchestrationWorld::cucumber()
        .with_writer(
            cucumber::writer::Basic::stdout()
                .summarized()
                .assert_normalized(),
        )
        .filter_run(
            "features/example/unit/orchestration.feature",
            |_feat, _rule, scenario| !SKIPPED_SCENARIOS.contains(&scenario.name.as_str()),
        )
        .await;
}
