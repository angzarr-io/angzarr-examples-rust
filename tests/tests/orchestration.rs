//! Process Manager orchestration BDD tests.
//!
//! Tests the PM handlers that coordinate cross-aggregate flows.

use angzarr_client::proto::{
    command_page, event_page, page_header, Cover, EventBook, EventPage, PageHeader,
    Uuid as ProtoUuid,
};
use angzarr_client::{pack_event, type_name_from_url, Destinations, ProcessManagerDomainHandler, UnpackAny};
use cucumber::{given, then, when, World, WriterExt};
use examples_proto::{
    BuyInFailed, BuyInRequested, Currency, PlayerJoined, PlayerSeated, RebuyChipsAdded,
    RebuyDenied, RebuyFailed, RebuyProcessed, RebuyRequested, RegistrationFailed,
    RegistrationRequested, SeatingRejected, TableCreated, TournamentCreated,
    TournamentEnrollmentRejected, TournamentPlayerEnrolled, TournamentStatus,
};
use hex;
use pmg_buy_in::{BuyInPmHandler, BuyInState};
use pmg_rebuy::{RebuyPmHandler, RebuyState};
use pmg_registration::{RegistrationPmHandler, RegistrationState};
use poker_tests::uuid_for;
use prost_types::Any;
use std::collections::HashMap;

/// Create a currency value (USD for orchestration tests).
fn currency(amount: i64) -> Currency {
    Currency {
        amount,
        currency_code: "USD".to_string(),
    }
}

/// Create an EventBook from a list of events.
fn make_event_book(domain: &str, root: &[u8], events: &[Any]) -> EventBook {
    EventBook {
        cover: Some(Cover {
            domain: domain.to_string(),
            root: Some(ProtoUuid {
                value: root.to_vec(),
            }),
            correlation_id: String::new(),
            edition: None,
        }),
        pages: events
            .iter()
            .enumerate()
            .map(|(i, e)| EventPage {
                header: Some(PageHeader {
                    sequence_type: Some(page_header::SequenceType::Sequence(i as u32)),
                }),
                payload: Some(event_page::Payload::Event(e.clone())),
                created_at: Some(angzarr_client::now()),
                committed: true,
                cascade_id: None,
            })
            .collect(),
        snapshot: None,
        next_sequence: events.len() as u32,
    }
}

// =============================================================================
// Test World
// =============================================================================

#[derive(Default, World)]
#[world(init = Self::new)]
pub struct OrchestrationWorld {
    // Roots for test entities
    player_root: Vec<u8>,
    table_root: Vec<u8>,
    tournament_root: Vec<u8>,
    reservation_id: Vec<u8>,

    // Table state
    table_events: Vec<Any>,
    table_min_buy_in: i64,
    table_max_buy_in: i64,
    table_max_players: i32,

    // Tournament state
    tournament_events: Vec<Any>,
    tournament_status: TournamentStatus,
    tournament_max_players: i32,
    tournament_current_players: i32,
    tournament_rebuy_allowed: bool,

    // Trigger event
    trigger_domain: String,
    trigger_event: Option<Any>,

    // PM results
    pm_result: Option<angzarr_client::ProcessManagerResponse>,

    // Track occupied seats
    occupied_seats: HashMap<i32, Vec<u8>>,
}

impl std::fmt::Debug for OrchestrationWorld {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OrchestrationWorld")
            .field("player_root", &hex::encode(&self.player_root))
            .field("table_root", &hex::encode(&self.table_root))
            .field("tournament_root", &hex::encode(&self.tournament_root))
            .field("trigger_domain", &self.trigger_domain)
            .field("has_pm_result", &self.pm_result.is_some())
            .finish()
    }
}

impl OrchestrationWorld {
    fn new() -> Self {
        Self::default()
    }

    fn player_event_book(&self) -> EventBook {
        make_event_book("player", &self.player_root, &[])
    }

    fn table_event_book(&self) -> EventBook {
        make_event_book("table", &self.table_root, &self.table_events)
    }

    fn tournament_event_book(&self) -> EventBook {
        make_event_book("tournament", &self.tournament_root, &self.tournament_events)
    }

    fn trigger_event_book(&self) -> EventBook {
        let events = match &self.trigger_event {
            Some(e) => vec![e.clone()],
            None => vec![],
        };
        make_event_book(&self.trigger_domain, &self.player_root, &events)
    }

    fn get_pm_commands(&self) -> Vec<String> {
        self.pm_result
            .as_ref()
            .map(|r| {
                r.commands
                    .iter()
                    .flat_map(|cb| {
                        cb.pages.iter().filter_map(|p| {
                            if let Some(command_page::Payload::Command(cmd)) = &p.payload {
                                Some(type_name_from_url(&cmd.type_url).to_string())
                            } else {
                                None
                            }
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn get_pm_events(&self) -> Vec<String> {
        self.pm_result
            .as_ref()
            .and_then(|r| r.process_events.as_ref())
            .map(|eb| {
                eb.pages
                    .iter()
                    .filter_map(|p| {
                        if let Some(event_page::Payload::Event(evt)) = &p.payload {
                            Some(type_name_from_url(&evt.type_url).to_string())
                        } else {
                            None
                        }
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    #[allow(dead_code)]
    fn get_pm_event_any(&self, type_name: &str) -> Option<Any> {
        self.pm_result
            .as_ref()
            .and_then(|r| r.process_events.as_ref())
            .and_then(|eb| {
                eb.pages.iter().find_map(|p| {
                    if let Some(event_page::Payload::Event(evt)) = &p.payload {
                        if type_name_from_url(&evt.type_url) == type_name {
                            return Some(evt.clone());
                        }
                    }
                    None
                })
            })
    }

    fn get_pm_event_any_suffix(&self, suffix: &str) -> Option<Any> {
        self.pm_result
            .as_ref()
            .and_then(|r| r.process_events.as_ref())
            .and_then(|eb| {
                eb.pages.iter().find_map(|p| {
                    if let Some(event_page::Payload::Event(evt)) = &p.payload {
                        if type_name_from_url(&evt.type_url).ends_with(suffix) {
                            return Some(evt.clone());
                        }
                    }
                    None
                })
            })
    }
}

// =============================================================================
// Given steps - BuyIn scenarios
// =============================================================================

#[given(expr = "a table with seat {int} available and buy-in range {int}-{int}")]
fn given_table_available(world: &mut OrchestrationWorld, _seat: i32, min: i64, max: i64) {
    world.player_root = uuid_for("test-player");
    world.table_root = uuid_for("test-table");
    world.reservation_id = uuid_for("test-reservation");

    world.table_min_buy_in = min;
    world.table_max_buy_in = max;
    world.table_max_players = 9;

    // Add TableCreated event
    let created = TableCreated {
        table_name: "Test Table".to_string(),
        game_variant: 0,
        small_blind: 5,
        big_blind: 10,
        min_buy_in: min,
        max_buy_in: max,
        max_players: 9,
        action_timeout_seconds: 30,
        created_at: None,
    };
    world
        .table_events
        .push(pack_event(&created, "examples.TableCreated"));
}

#[given(expr = "a player with a BuyInRequested event for seat {int} with amount {int}")]
fn given_buy_in_requested(world: &mut OrchestrationWorld, seat: i32, amount: i64) {
    world.trigger_domain = "player".to_string();

    let event = BuyInRequested {
        reservation_id: world.reservation_id.clone(),
        table_root: world.table_root.clone(),
        seat,
        amount: Some(currency(amount)),
        requested_at: Some(angzarr_client::now()),
    };
    world.trigger_event = Some(pack_event(&event, "examples.BuyInRequested"));
}

#[given(expr = "a table with seat {int} occupied by another player")]
fn given_seat_occupied(world: &mut OrchestrationWorld, seat: i32) {
    world.player_root = uuid_for("test-player");
    world.table_root = uuid_for("test-table");
    world.reservation_id = uuid_for("test-reservation");

    world.table_min_buy_in = 200;
    world.table_max_buy_in = 2000;
    world.table_max_players = 9;

    // Add TableCreated event
    let created = TableCreated {
        table_name: "Test Table".to_string(),
        game_variant: 0,
        small_blind: 5,
        big_blind: 10,
        min_buy_in: 200,
        max_buy_in: 2000,
        max_players: 9,
        action_timeout_seconds: 30,
        created_at: None,
    };
    world
        .table_events
        .push(pack_event(&created, "examples.TableCreated"));

    // Add another player at the seat
    let other_player = uuid_for("other-player");
    let joined = PlayerJoined {
        player_root: other_player.clone(),
        seat_position: seat,
        buy_in_amount: 500,
        stack: 500,
        joined_at: None,
    };
    world
        .table_events
        .push(pack_event(&joined, "examples.PlayerJoined"));
    world.occupied_seats.insert(seat, other_player);
}

#[given("a table that is full with 9 players")]
fn given_table_full(world: &mut OrchestrationWorld) {
    world.player_root = uuid_for("test-player");
    world.table_root = uuid_for("test-table");
    world.reservation_id = uuid_for("test-reservation");

    world.table_min_buy_in = 200;
    world.table_max_buy_in = 2000;
    world.table_max_players = 9;

    // Add TableCreated event
    let created = TableCreated {
        table_name: "Test Table".to_string(),
        game_variant: 0,
        small_blind: 5,
        big_blind: 10,
        min_buy_in: 200,
        max_buy_in: 2000,
        max_players: 9,
        action_timeout_seconds: 30,
        created_at: None,
    };
    world
        .table_events
        .push(pack_event(&created, "examples.TableCreated"));

    // Fill all seats
    for seat in 0..9 {
        let player = uuid_for(&format!("player-{}", seat));
        let joined = PlayerJoined {
            player_root: player.clone(),
            seat_position: seat,
            buy_in_amount: 500,
            stack: 500,
            joined_at: None,
        };
        world
            .table_events
            .push(pack_event(&joined, "examples.PlayerJoined"));
        world.occupied_seats.insert(seat, player);
    }
}

#[given(expr = "a player with a BuyInRequested event for any seat with amount {int}")]
fn given_buy_in_any_seat(world: &mut OrchestrationWorld, amount: i64) {
    world.trigger_domain = "player".to_string();

    let event = BuyInRequested {
        reservation_id: world.reservation_id.clone(),
        table_root: world.table_root.clone(),
        seat: -1, // Any seat
        amount: Some(currency(amount)),
        requested_at: Some(angzarr_client::now()),
    };
    world.trigger_event = Some(pack_event(&event, "examples.BuyInRequested"));
}

#[given("a player and table in a pending buy-in state")]
fn given_pending_buy_in(world: &mut OrchestrationWorld) {
    world.player_root = uuid_for("test-player");
    world.table_root = uuid_for("test-table");
    world.reservation_id = uuid_for("test-reservation");
}

// =============================================================================
// Given steps - Registration scenarios
// =============================================================================

#[given("a tournament with registration open and capacity available")]
fn given_tournament_open(world: &mut OrchestrationWorld) {
    world.player_root = uuid_for("test-player");
    world.tournament_root = uuid_for("test-tournament");
    world.reservation_id = uuid_for("test-reservation");

    world.tournament_status = TournamentStatus::TournamentRegistrationOpen;
    world.tournament_max_players = 100;
    world.tournament_current_players = 50;

    let created = TournamentCreated {
        name: "Test Tournament".to_string(),
        game_variant: 0, // TexasHoldem
        buy_in: 1000,
        starting_stack: 5000,
        max_players: 100,
        min_players: 10,
        scheduled_start: None,
        rebuy_config: None, // No rebuy
        addon_config: None,
        blind_structure: vec![],
        created_at: Some(angzarr_client::now()),
    };
    world
        .tournament_events
        .push(pack_event(&created, "examples.TournamentCreated"));

    // Add RegistrationOpened event to set status
    let opened = examples_proto::RegistrationOpened {
        opened_at: Some(angzarr_client::now()),
    };
    world
        .tournament_events
        .push(pack_event(&opened, "examples.RegistrationOpened"));
}

#[given(expr = "a player with a RegistrationRequested event with fee {int}")]
fn given_registration_requested(world: &mut OrchestrationWorld, fee: i64) {
    world.trigger_domain = "player".to_string();

    let event = RegistrationRequested {
        reservation_id: world.reservation_id.clone(),
        tournament_root: world.tournament_root.clone(),
        fee: Some(currency(fee)),
        requested_at: Some(angzarr_client::now()),
    };
    world.trigger_event = Some(pack_event(&event, "examples.RegistrationRequested"));
}

#[given("a tournament that is full")]
fn given_tournament_full(world: &mut OrchestrationWorld) {
    world.player_root = uuid_for("test-player");
    world.tournament_root = uuid_for("test-tournament");
    world.reservation_id = uuid_for("test-reservation");

    world.tournament_status = TournamentStatus::TournamentRegistrationOpen;
    world.tournament_max_players = 100;
    world.tournament_current_players = 100; // Full

    let created = TournamentCreated {
        name: "Test Tournament".to_string(),
        game_variant: 0,
        buy_in: 1000,
        starting_stack: 5000,
        max_players: 100,
        min_players: 10,
        scheduled_start: None,
        rebuy_config: None,
        addon_config: None,
        blind_structure: vec![],
        created_at: Some(angzarr_client::now()),
    };
    world
        .tournament_events
        .push(pack_event(&created, "examples.TournamentCreated"));

    // Add enrolled players to fill tournament
    for i in 0..100 {
        let enrolled = TournamentPlayerEnrolled {
            player_root: uuid_for(&format!("player-{}", i)),
            reservation_id: uuid_for(&format!("reservation-{}", i)),
            fee_paid: 1000,
            starting_stack: 5000,
            registration_number: i,
            enrolled_at: Some(angzarr_client::now()),
        };
        world
            .tournament_events
            .push(pack_event(&enrolled, "examples.TournamentPlayerEnrolled"));
    }
}

#[given("a tournament with registration closed")]
fn given_tournament_closed(world: &mut OrchestrationWorld) {
    world.player_root = uuid_for("test-player");
    world.tournament_root = uuid_for("test-tournament");
    world.reservation_id = uuid_for("test-reservation");

    world.tournament_status = TournamentStatus::TournamentRunning;
    world.tournament_max_players = 100;
    world.tournament_current_players = 50;

    let created = TournamentCreated {
        name: "Test Tournament".to_string(),
        game_variant: 0,
        buy_in: 1000,
        starting_stack: 5000,
        max_players: 100,
        min_players: 10,
        scheduled_start: None,
        rebuy_config: None,
        addon_config: None,
        blind_structure: vec![],
        created_at: Some(angzarr_client::now()),
    };
    world
        .tournament_events
        .push(pack_event(&created, "examples.TournamentCreated"));
}

#[given("a player and tournament in a pending registration state")]
fn given_pending_registration(world: &mut OrchestrationWorld) {
    world.player_root = uuid_for("test-player");
    world.tournament_root = uuid_for("test-tournament");
    world.reservation_id = uuid_for("test-reservation");
}

// =============================================================================
// Given steps - Rebuy scenarios
// =============================================================================

#[given("a tournament in rebuy window with player eligible")]
fn given_tournament_rebuy_open(world: &mut OrchestrationWorld) {
    world.player_root = uuid_for("test-player");
    world.tournament_root = uuid_for("test-tournament");
    world.table_root = uuid_for("test-table");
    world.reservation_id = uuid_for("test-reservation");

    world.tournament_status = TournamentStatus::TournamentRunning;
    world.tournament_rebuy_allowed = true;

    let created = TournamentCreated {
        name: "Test Tournament".to_string(),
        game_variant: 0,
        buy_in: 1000,
        starting_stack: 5000,
        max_players: 100,
        min_players: 10,
        scheduled_start: None,
        rebuy_config: Some(examples_proto::RebuyConfig {
            enabled: true,
            max_rebuys: 3,
            rebuy_level_cutoff: 3,
            stack_threshold: 2500,
            rebuy_cost: 1000,
            rebuy_chips: 5000,
        }),
        addon_config: None,
        blind_structure: vec![],
        created_at: Some(angzarr_client::now()),
    };
    world
        .tournament_events
        .push(pack_event(&created, "examples.TournamentCreated"));

    // Add TournamentStarted event to set status to Running
    let started = examples_proto::TournamentStarted {
        total_players: 1,
        tables_created: 1,
        total_prize_pool: 5000,
        started_at: Some(angzarr_client::now()),
    };
    world
        .tournament_events
        .push(pack_event(&started, "examples.TournamentStarted"));

    // Register the test player in the tournament
    let enrolled = TournamentPlayerEnrolled {
        player_root: world.player_root.clone(),
        reservation_id: world.reservation_id.clone(),
        fee_paid: 1000,
        starting_stack: 5000,
        registration_number: 1,
        enrolled_at: Some(angzarr_client::now()),
    };
    world
        .tournament_events
        .push(pack_event(&enrolled, "examples.TournamentPlayerEnrolled"));
}

#[given(expr = "a table with the player seated at position {int}")]
fn given_player_seated(world: &mut OrchestrationWorld, seat: i32) {
    world.table_min_buy_in = 200;
    world.table_max_buy_in = 2000;
    world.table_max_players = 9;

    let created = TableCreated {
        table_name: "Test Table".to_string(),
        game_variant: 0,
        small_blind: 5,
        big_blind: 10,
        min_buy_in: 200,
        max_buy_in: 2000,
        max_players: 9,
        action_timeout_seconds: 30,
        created_at: None,
    };
    world
        .table_events
        .push(pack_event(&created, "examples.TableCreated"));

    let joined = PlayerJoined {
        player_root: world.player_root.clone(),
        seat_position: seat,
        buy_in_amount: 500,
        stack: 500,
        joined_at: None,
    };
    world
        .table_events
        .push(pack_event(&joined, "examples.PlayerJoined"));
    world.occupied_seats.insert(seat, world.player_root.clone());
}

#[given(expr = "a player with a RebuyRequested event for amount {int}")]
fn given_rebuy_requested(world: &mut OrchestrationWorld, amount: i64) {
    world.trigger_domain = "player".to_string();

    let event = RebuyRequested {
        reservation_id: world.reservation_id.clone(),
        tournament_root: world.tournament_root.clone(),
        table_root: world.table_root.clone(),
        seat: 2, // Seat from earlier given step
        fee: Some(currency(amount)),
        requested_at: Some(angzarr_client::now()),
    };
    world.trigger_event = Some(pack_event(&event, "examples.RebuyRequested"));
}

#[given("a tournament with rebuy window closed")]
fn given_rebuy_closed(world: &mut OrchestrationWorld) {
    world.player_root = uuid_for("test-player");
    world.tournament_root = uuid_for("test-tournament");
    world.table_root = uuid_for("test-table");
    world.reservation_id = uuid_for("test-reservation");

    world.tournament_status = TournamentStatus::TournamentRunning;
    world.tournament_rebuy_allowed = false; // Rebuy closed

    let created = TournamentCreated {
        name: "Test Tournament".to_string(),
        game_variant: 0,
        buy_in: 1000,
        starting_stack: 5000,
        max_players: 100,
        min_players: 10,
        scheduled_start: None,
        rebuy_config: None, // No rebuy allowed
        addon_config: None,
        blind_structure: vec![],
        created_at: Some(angzarr_client::now()),
    };
    world
        .tournament_events
        .push(pack_event(&created, "examples.TournamentCreated"));
}

#[given("a table without the player seated")]
fn given_player_not_seated(world: &mut OrchestrationWorld) {
    world.table_min_buy_in = 200;
    world.table_max_buy_in = 2000;
    world.table_max_players = 9;

    let created = TableCreated {
        table_name: "Test Table".to_string(),
        game_variant: 0,
        small_blind: 5,
        big_blind: 10,
        min_buy_in: 200,
        max_buy_in: 2000,
        max_players: 9,
        action_timeout_seconds: 30,
        created_at: None,
    };
    world
        .table_events
        .push(pack_event(&created, "examples.TableCreated"));
    // No player seated
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
// When steps
// =============================================================================

#[when("the BuyInOrchestrator handles the BuyInRequested event")]
fn when_buy_in_pm_handles(world: &mut OrchestrationWorld) {
    let handler = BuyInPmHandler;
    let state = BuyInState::default();

    let trigger = world.trigger_event_book();
    let event = world.trigger_event.as_ref().expect("No trigger event");
    let destinations = Destinations::from_sequences(
        [("table".to_string(), 0u32)].into_iter().collect()
    );

    match handler.handle(&trigger, &state, event, &destinations) {
        Ok(response) => world.pm_result = Some(response),
        Err(e) => panic!("PM handler failed: {}", e),
    }
}

#[when("the BuyInOrchestrator handles a PlayerSeated event")]
fn when_buy_in_pm_handles_seated(world: &mut OrchestrationWorld) {
    let handler = BuyInPmHandler;
    let state = BuyInState::default();

    let event = PlayerSeated {
        player_root: world.player_root.clone(),
        reservation_id: world.reservation_id.clone(),
        seat_position: 0,
        stack: 500,
        seated_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.PlayerSeated");

    world.trigger_domain = "table".to_string();
    let trigger = make_event_book("table", &world.table_root, &[event_any.clone()]);
    let destinations = Destinations::from_sequences(
        [("player".to_string(), 0u32)].into_iter().collect()
    );

    match handler.handle(&trigger, &state, &event_any, &destinations) {
        Ok(response) => world.pm_result = Some(response),
        Err(e) => panic!("PM handler failed: {}", e),
    }
}

#[when("the BuyInOrchestrator handles a SeatingRejected event")]
fn when_buy_in_pm_handles_rejected(world: &mut OrchestrationWorld) {
    let handler = BuyInPmHandler;
    let state = BuyInState::default();

    let event = SeatingRejected {
        player_root: world.player_root.clone(),
        reservation_id: world.reservation_id.clone(),
        requested_seat: 0,
        reason: "Seat taken by another player".to_string(),
        rejected_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.SeatingRejected");

    world.trigger_domain = "table".to_string();
    let trigger = make_event_book("table", &world.table_root, &[event_any.clone()]);
    let destinations = Destinations::from_sequences(
        [("player".to_string(), 0u32)].into_iter().collect()
    );

    match handler.handle(&trigger, &state, &event_any, &destinations) {
        Ok(response) => world.pm_result = Some(response),
        Err(e) => panic!("PM handler failed: {}", e),
    }
}

#[when("the RegistrationOrchestrator handles the RegistrationRequested event")]
fn when_registration_pm_handles(world: &mut OrchestrationWorld) {
    let handler = RegistrationPmHandler;
    let state = RegistrationState::default();

    let trigger = world.trigger_event_book();
    let event = world.trigger_event.as_ref().expect("No trigger event");
    let destinations = Destinations::from_sequences(
        [("tournament".to_string(), 0u32)].into_iter().collect()
    );

    match handler.handle(&trigger, &state, event, &destinations) {
        Ok(response) => world.pm_result = Some(response),
        Err(e) => panic!("PM handler failed: {}", e),
    }
}

#[when("the RegistrationOrchestrator handles a TournamentPlayerEnrolled event")]
fn when_registration_pm_handles_enrolled(world: &mut OrchestrationWorld) {
    let handler = RegistrationPmHandler;
    let state = RegistrationState::default();

    let event = TournamentPlayerEnrolled {
        player_root: world.player_root.clone(),
        reservation_id: world.reservation_id.clone(),
        fee_paid: 1000,
        starting_stack: 5000,
        registration_number: 1,
        enrolled_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.TournamentPlayerEnrolled");

    world.trigger_domain = "tournament".to_string();
    let trigger = make_event_book("tournament", &world.tournament_root, &[event_any.clone()]);
    let destinations = Destinations::from_sequences(
        [("player".to_string(), 0u32)].into_iter().collect()
    );

    match handler.handle(&trigger, &state, &event_any, &destinations) {
        Ok(response) => world.pm_result = Some(response),
        Err(e) => panic!("PM handler failed: {}", e),
    }
}

#[when("the RegistrationOrchestrator handles a TournamentEnrollmentRejected event")]
fn when_registration_pm_handles_rejected(world: &mut OrchestrationWorld) {
    let handler = RegistrationPmHandler;
    let state = RegistrationState::default();

    let event = TournamentEnrollmentRejected {
        player_root: world.player_root.clone(),
        reservation_id: world.reservation_id.clone(),
        reason: "Tournament full".to_string(),
        rejected_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.TournamentEnrollmentRejected");

    world.trigger_domain = "tournament".to_string();
    let trigger = make_event_book("tournament", &world.tournament_root, &[event_any.clone()]);
    let destinations = Destinations::from_sequences(
        [("player".to_string(), 0u32)].into_iter().collect()
    );

    match handler.handle(&trigger, &state, &event_any, &destinations) {
        Ok(response) => world.pm_result = Some(response),
        Err(e) => panic!("PM handler failed: {}", e),
    }
}

#[when("the RebuyOrchestrator handles the RebuyRequested event")]
fn when_rebuy_pm_handles(world: &mut OrchestrationWorld) {
    let handler = RebuyPmHandler;
    let state = RebuyState::default();

    let trigger = world.trigger_event_book();
    let event = world.trigger_event.as_ref().expect("No trigger event");
    let destinations = Destinations::from_sequences(
        [("tournament".to_string(), 0u32), ("table".to_string(), 0u32)].into_iter().collect()
    );

    match handler.handle(&trigger, &state, event, &destinations) {
        Ok(response) => world.pm_result = Some(response),
        Err(e) => panic!("PM handler failed: {}", e),
    }
}

#[when("the RebuyOrchestrator handles a RebuyProcessed event")]
fn when_rebuy_pm_handles_processed(world: &mut OrchestrationWorld) {
    let handler = RebuyPmHandler;
    let state = RebuyState::default();

    let event = RebuyProcessed {
        player_root: world.player_root.clone(),
        reservation_id: world.reservation_id.clone(),
        rebuy_cost: 1000,
        chips_added: 5000,
        rebuy_count: 1,
        processed_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.RebuyProcessed");

    world.trigger_domain = "tournament".to_string();
    let trigger = make_event_book("tournament", &world.tournament_root, &[event_any.clone()]);
    let destinations = Destinations::from_sequences(
        [("table".to_string(), 0u32), ("player".to_string(), 0u32)].into_iter().collect()
    );

    match handler.handle(&trigger, &state, &event_any, &destinations) {
        Ok(response) => world.pm_result = Some(response),
        Err(e) => panic!("PM handler failed: {}", e),
    }
}

#[when("the RebuyOrchestrator handles a RebuyChipsAdded event")]
fn when_rebuy_pm_handles_chips_added(world: &mut OrchestrationWorld) {
    let handler = RebuyPmHandler;
    let state = RebuyState::default();

    let event = RebuyChipsAdded {
        player_root: world.player_root.clone(),
        reservation_id: world.reservation_id.clone(),
        seat: 2,
        amount: 5000,
        new_stack: 5500,
        added_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.RebuyChipsAdded");

    world.trigger_domain = "table".to_string();
    let trigger = make_event_book("table", &world.table_root, &[event_any.clone()]);
    let destinations = Destinations::from_sequences(
        [("player".to_string(), 0u32)].into_iter().collect()
    );

    match handler.handle(&trigger, &state, &event_any, &destinations) {
        Ok(response) => world.pm_result = Some(response),
        Err(e) => panic!("PM handler failed: {}", e),
    }
}

#[when("the RebuyOrchestrator handles a RebuyDenied event")]
fn when_rebuy_pm_handles_denied(world: &mut OrchestrationWorld) {
    let handler = RebuyPmHandler;
    let state = RebuyState::default();

    let event = RebuyDenied {
        player_root: world.player_root.clone(),
        reservation_id: world.reservation_id.clone(),
        reason: "Rebuy limit reached".to_string(),
        denied_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.RebuyDenied");

    world.trigger_domain = "tournament".to_string();
    let trigger = make_event_book("tournament", &world.tournament_root, &[event_any.clone()]);
    let destinations = Destinations::from_sequences(
        [("player".to_string(), 0u32)].into_iter().collect()
    );

    match handler.handle(&trigger, &state, &event_any, &destinations) {
        Ok(response) => world.pm_result = Some(response),
        Err(e) => panic!("PM handler failed: {}", e),
    }
}

// =============================================================================
// Then steps
// =============================================================================

#[then("the PM emits a SeatPlayer command to the table")]
fn then_emits_seat_player(world: &mut OrchestrationWorld) {
    let commands = world.get_pm_commands();
    assert!(
        commands.iter().any(|c| c.ends_with("SeatPlayer")),
        "Expected SeatPlayer command, got: {:?}",
        commands
    );
}

#[then("the PM emits a BuyInInitiated process event")]
fn then_emits_buy_in_initiated(world: &mut OrchestrationWorld) {
    let events = world.get_pm_events();
    assert!(
        events.iter().any(|e| e.ends_with("BuyInInitiated")),
        "Expected BuyInInitiated event, got: {:?}",
        events
    );
}

#[then("the PM emits no commands")]
fn then_emits_no_commands(world: &mut OrchestrationWorld) {
    let commands = world.get_pm_commands();
    assert!(
        commands.is_empty(),
        "Expected no commands, got: {:?}",
        commands
    );
}

#[then(expr = "the PM emits a BuyInFailed process event with code {string}")]
fn then_emits_buy_in_failed(world: &mut OrchestrationWorld, code: String) {
    let events = world.get_pm_events();
    assert!(
        events.iter().any(|e| e.ends_with("BuyInFailed")),
        "Expected BuyInFailed event, got: {:?}",
        events
    );

    let event_any = world
        .get_pm_event_any_suffix("BuyInFailed")
        .expect("BuyInFailed event not found");
    let event: BuyInFailed = event_any.unpack().expect("Failed to decode BuyInFailed");
    let failure = event.failure.expect("No failure in BuyInFailed");
    assert_eq!(
        failure.code, code,
        "Expected failure code '{}', got '{}'",
        code, failure.code
    );
}

#[then("the PM emits a ConfirmBuyIn command to the player")]
fn then_emits_confirm_buy_in(world: &mut OrchestrationWorld) {
    let commands = world.get_pm_commands();
    assert!(
        commands.iter().any(|c| c.ends_with("ConfirmBuyIn")),
        "Expected ConfirmBuyIn command, got: {:?}",
        commands
    );
}

#[then("the PM emits a BuyInCompleted process event")]
fn then_emits_buy_in_completed(world: &mut OrchestrationWorld) {
    let events = world.get_pm_events();
    assert!(
        events.iter().any(|e| e.ends_with("BuyInCompleted")),
        "Expected BuyInCompleted event, got: {:?}",
        events
    );
}

#[then("the PM emits a ReleaseBuyIn command to the player")]
fn then_emits_release_buy_in(world: &mut OrchestrationWorld) {
    let commands = world.get_pm_commands();
    assert!(
        commands.iter().any(|c| c.ends_with("ReleaseBuyIn")),
        "Expected ReleaseBuyIn command, got: {:?}",
        commands
    );
}

#[then("the PM emits an EnrollPlayer command to the tournament")]
fn then_emits_enroll_player(world: &mut OrchestrationWorld) {
    let commands = world.get_pm_commands();
    assert!(
        commands.iter().any(|c| c.ends_with("EnrollPlayer")),
        "Expected EnrollPlayer command, got: {:?}",
        commands
    );
}

#[then("the PM emits a RegistrationInitiated process event")]
fn then_emits_registration_initiated(world: &mut OrchestrationWorld) {
    let events = world.get_pm_events();
    assert!(
        events.iter().any(|e| e.ends_with("RegistrationInitiated")),
        "Expected RegistrationInitiated event, got: {:?}",
        events
    );
}

#[then(expr = "the PM emits a RegistrationFailed process event with code {string}")]
fn then_emits_registration_failed(world: &mut OrchestrationWorld, code: String) {
    let events = world.get_pm_events();
    assert!(
        events.iter().any(|e| e.ends_with("RegistrationFailed")),
        "Expected RegistrationFailed event, got: {:?}",
        events
    );

    let event_any = world
        .get_pm_event_any_suffix("RegistrationFailed")
        .expect("RegistrationFailed event not found");
    let event: RegistrationFailed = event_any
        .unpack()
        .expect("Failed to decode RegistrationFailed");
    let failure = event.failure.expect("No failure in RegistrationFailed");
    assert_eq!(
        failure.code, code,
        "Expected failure code '{}', got '{}'",
        code, failure.code
    );
}

#[then("the PM emits a ConfirmRegistrationFee command to the player")]
fn then_emits_confirm_registration_fee(world: &mut OrchestrationWorld) {
    let commands = world.get_pm_commands();
    assert!(
        commands
            .iter()
            .any(|c| c.ends_with("ConfirmRegistrationFee")),
        "Expected ConfirmRegistrationFee command, got: {:?}",
        commands
    );
}

#[then("the PM emits a RegistrationCompleted process event")]
fn then_emits_registration_completed(world: &mut OrchestrationWorld) {
    let events = world.get_pm_events();
    assert!(
        events.iter().any(|e| e.ends_with("RegistrationCompleted")),
        "Expected RegistrationCompleted event, got: {:?}",
        events
    );
}

#[then("the PM emits a ReleaseRegistrationFee command to the player")]
fn then_emits_release_registration_fee(world: &mut OrchestrationWorld) {
    let commands = world.get_pm_commands();
    assert!(
        commands
            .iter()
            .any(|c| c.ends_with("ReleaseRegistrationFee")),
        "Expected ReleaseRegistrationFee command, got: {:?}",
        commands
    );
}

#[then("the PM emits a ProcessRebuy command to the tournament")]
fn then_emits_process_rebuy(world: &mut OrchestrationWorld) {
    let commands = world.get_pm_commands();
    assert!(
        commands.iter().any(|c| c.ends_with("ProcessRebuy")),
        "Expected ProcessRebuy command, got: {:?}",
        commands
    );
}

#[then("the PM emits a RebuyInitiated process event")]
fn then_emits_rebuy_initiated(world: &mut OrchestrationWorld) {
    let events = world.get_pm_events();
    assert!(
        events.iter().any(|e| e.ends_with("RebuyInitiated")),
        "Expected RebuyInitiated event, got: {:?}",
        events
    );
}

#[then(expr = "the PM emits a RebuyFailed process event with code {string}")]
fn then_emits_rebuy_failed(world: &mut OrchestrationWorld, code: String) {
    let events = world.get_pm_events();
    assert!(
        events.iter().any(|e| e.ends_with("RebuyFailed")),
        "Expected RebuyFailed event, got: {:?}",
        events
    );

    let event_any = world
        .get_pm_event_any_suffix("RebuyFailed")
        .expect("RebuyFailed event not found");
    let event: RebuyFailed = event_any.unpack().expect("Failed to decode RebuyFailed");
    let failure = event.failure.expect("No failure in RebuyFailed");
    assert_eq!(
        failure.code, code,
        "Expected failure code '{}', got '{}'",
        code, failure.code
    );
}

#[then("the PM emits an AddRebuyChips command to the table")]
fn then_emits_add_rebuy_chips(world: &mut OrchestrationWorld) {
    let commands = world.get_pm_commands();
    assert!(
        commands.iter().any(|c| c.ends_with("AddRebuyChips")),
        "Expected AddRebuyChips command, got: {:?}",
        commands
    );
}

#[then("the PM emits a RebuyChipsAdded process event")]
fn then_emits_rebuy_chips_added(world: &mut OrchestrationWorld) {
    let events = world.get_pm_events();
    assert!(
        events.iter().any(|e| e.ends_with("RebuyChipsAdded")),
        "Expected RebuyChipsAdded event, got: {:?}",
        events
    );
}

#[then("the PM emits a ConfirmRebuyFee command to the player")]
fn then_emits_confirm_rebuy_fee(world: &mut OrchestrationWorld) {
    let commands = world.get_pm_commands();
    assert!(
        commands.iter().any(|c| c.ends_with("ConfirmRebuyFee")),
        "Expected ConfirmRebuyFee command, got: {:?}",
        commands
    );
}

#[then("the PM emits a RebuyCompleted process event")]
fn then_emits_rebuy_completed(world: &mut OrchestrationWorld) {
    let events = world.get_pm_events();
    assert!(
        events.iter().any(|e| e.ends_with("RebuyCompleted")),
        "Expected RebuyCompleted event, got: {:?}",
        events
    );
}

#[then("the PM emits a ReleaseRebuyFee command to the player")]
fn then_emits_release_rebuy_fee(world: &mut OrchestrationWorld) {
    let commands = world.get_pm_commands();
    assert!(
        commands.iter().any(|c| c.ends_with("ReleaseRebuyFee")),
        "Expected ReleaseRebuyFee command, got: {:?}",
        commands
    );
}

// =============================================================================
// Main
// =============================================================================

#[tokio::main]
async fn main() {
    OrchestrationWorld::cucumber()
        .with_writer(
            cucumber::writer::Basic::stdout()
                .summarized()
                .assert_normalized(),
        )
        .run("features/unit/orchestration.feature")
        .await;
}
