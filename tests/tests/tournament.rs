//! Tournament aggregate BDD tests using cucumber-rs.
//!
//! State is rebuilt by iterating event pages and dispatching to the
//! `apply_*` functions in `agg_tournament::state`. Handlers are free
//! functions in `agg_tournament::handlers`.

use agg_tournament::handlers::{
    handle_advance_blind_level, handle_close_registration, handle_create_tournament,
    handle_eliminate_player, handle_enroll_player, handle_open_registration,
    handle_pause_tournament, handle_process_rebuy, handle_resume_tournament,
    handle_start_tournament,
};
use agg_tournament::state::{
    apply_blind_advanced, apply_completed, apply_created, apply_enrollment_rejected, apply_paused,
    apply_player_eliminated, apply_player_enrolled, apply_rebuy_denied, apply_rebuy_processed,
    apply_registration_closed, apply_registration_opened, apply_resumed, TournamentState,
};
use angzarr_client::proto::{event_page, EventBook};
use angzarr_client::{try_unpack, CommandRejectedError};
use cucumber::{given, then, when, World};
use examples_proto::{
    AdvanceBlindLevel, BlindLevel, BlindLevelAdvanced, CloseRegistration, CreateTournament,
    EliminatePlayer, EnrollPlayer, GameVariant, OpenRegistration, PauseTournament,
    PlayerEliminated, ProcessRebuy, RebuyConfig, RebuyDenied, RebuyProcessed, RegistrationClosed,
    RegistrationOpened, ResumeTournament, StartTournament, TournamentCompleted, TournamentCreated,
    TournamentEnrollmentRejected, TournamentPaused, TournamentPlayerEnrolled, TournamentResumed,
    TournamentStarted,
};
use prost_types::Any;

fn pack<T: prost::Message + prost::Name>(event: &T) -> Any {
    angzarr_client::pack_event(event, &T::full_name())
}

fn player_root_bytes(label: &str) -> Vec<u8> {
    if label.is_empty() {
        Vec::new()
    } else {
        label.as_bytes().to_vec()
    }
}

fn reservation_bytes(label: &str) -> Vec<u8> {
    if label.is_empty() {
        Vec::new()
    } else {
        label.as_bytes().to_vec()
    }
}

#[derive(Default, World)]
#[world(init = Self::new)]
pub struct TournamentWorld {
    events: Vec<Any>,
    last_error: Option<CommandRejectedError>,
    last_event_book: Option<EventBook>,
    last_state: Option<TournamentState>,
}

impl std::fmt::Debug for TournamentWorld {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TournamentWorld")
            .field("events", &self.events.len())
            .field("last_error", &self.last_error)
            .finish()
    }
}

impl TournamentWorld {
    fn new() -> Self {
        Self::default()
    }

    fn next_seq(&self) -> u32 {
        self.events.len() as u32
    }

    fn add_event(&mut self, event_any: Any) {
        self.events.push(event_any);
    }

    fn rebuild_state(&self) -> TournamentState {
        let mut state = TournamentState::default();
        for event_any in &self.events {
            if let Some(ev) = try_unpack::<TournamentCreated>(event_any) {
                apply_created(&mut state, ev);
            } else if let Some(ev) = try_unpack::<RegistrationOpened>(event_any) {
                apply_registration_opened(&mut state, ev);
            } else if let Some(ev) = try_unpack::<RegistrationClosed>(event_any) {
                apply_registration_closed(&mut state, ev);
            } else if let Some(ev) = try_unpack::<TournamentPlayerEnrolled>(event_any) {
                apply_player_enrolled(&mut state, ev);
            } else if let Some(ev) = try_unpack::<TournamentEnrollmentRejected>(event_any) {
                apply_enrollment_rejected(&mut state, ev);
            } else if let Some(ev) = try_unpack::<RebuyProcessed>(event_any) {
                apply_rebuy_processed(&mut state, ev);
            } else if let Some(ev) = try_unpack::<RebuyDenied>(event_any) {
                apply_rebuy_denied(&mut state, ev);
            } else if let Some(ev) = try_unpack::<BlindLevelAdvanced>(event_any) {
                apply_blind_advanced(&mut state, ev);
            } else if let Some(ev) = try_unpack::<PlayerEliminated>(event_any) {
                apply_player_eliminated(&mut state, ev);
            } else if let Some(ev) = try_unpack::<TournamentPaused>(event_any) {
                apply_paused(&mut state, ev);
            } else if let Some(ev) = try_unpack::<TournamentResumed>(event_any) {
                apply_resumed(&mut state, ev);
            } else if let Some(ev) = try_unpack::<TournamentCompleted>(event_any) {
                apply_completed(&mut state, ev);
            } else if try_unpack::<TournamentStarted>(event_any).is_some() {
                // TournamentStarted currently has no state applier; status is
                // captured indirectly via the next command's guards using the
                // `started` helper in this test by seeding the status manually.
            }
        }
        state
    }

    fn get_last_event(&self) -> Option<&Any> {
        self.last_event_book
            .as_ref()
            .and_then(|eb| eb.pages.first())
            .and_then(|p| match &p.payload {
                Some(event_page::Payload::Event(e)) => Some(e),
                _ => None,
            })
    }
}

// --- Given steps ---

#[given("no prior events for the tournament aggregate")]
fn given_no_prior(world: &mut TournamentWorld) {
    world.events.clear();
}

#[given(
    expr = "a TournamentCreated event with name {string} buy_in {int} starting_stack {int} max_players {int} min_players {int}"
)]
fn given_created_event(
    world: &mut TournamentWorld,
    name: String,
    buy_in: i64,
    starting_stack: i64,
    max_players: i32,
    min_players: i32,
) {
    let event = TournamentCreated {
        name,
        game_variant: GameVariant::TexasHoldem as i32,
        buy_in,
        starting_stack,
        max_players,
        min_players,
        scheduled_start: None,
        rebuy_config: None,
        addon_config: None,
        blind_structure: vec![],
        created_at: Some(angzarr_client::now()),
    };
    world.add_event(pack(&event));
}

fn append_created(world: &mut TournamentWorld, buy_in: i64, max_players: i32, min_players: i32) {
    let event = TournamentCreated {
        name: "Test Tournament".into(),
        game_variant: GameVariant::TexasHoldem as i32,
        buy_in,
        starting_stack: 1000,
        max_players,
        min_players,
        scheduled_start: None,
        rebuy_config: None,
        addon_config: None,
        blind_structure: vec![],
        created_at: Some(angzarr_client::now()),
    };
    world.add_event(pack(&event));
}

fn append_registration_opened(world: &mut TournamentWorld) {
    let event = RegistrationOpened {
        opened_at: Some(angzarr_client::now()),
    };
    world.add_event(pack(&event));
}

fn append_player_enrolled(world: &mut TournamentWorld, label: &str) {
    let state = world.rebuild_state();
    let event = TournamentPlayerEnrolled {
        player_root: player_root_bytes(label),
        reservation_id: Vec::new(),
        fee_paid: state.buy_in,
        starting_stack: state.starting_stack,
        registration_number: (state.registered_players.len() as i32) + 1,
        enrolled_at: Some(angzarr_client::now()),
    };
    world.add_event(pack(&event));
}

#[given("a tournament with registration open")]
fn given_tournament_registration_open(world: &mut TournamentWorld) {
    append_created(world, 100, 100, 10);
    append_registration_opened(world);
}

#[given(expr = "a tournament with max_players {int} and min_players {int} and registration open")]
fn given_tournament_max_min_open(world: &mut TournamentWorld, max_players: i32, min_players: i32) {
    append_created(world, 100, max_players, min_players);
    append_registration_opened(world);
}

#[given(expr = "a tournament with min_players {int} and max_players {int} and registration open")]
fn given_tournament_min_max_open(world: &mut TournamentWorld, min_players: i32, max_players: i32) {
    append_created(world, 100, max_players, min_players);
    append_registration_opened(world);
}

#[given(expr = "a player {string} enrolled")]
fn given_player_enrolled(world: &mut TournamentWorld, label: String) {
    append_player_enrolled(world, &label);
}

fn append_created_with_blinds_and_rebuy(
    world: &mut TournamentWorld,
    blind_structure: Vec<BlindLevel>,
    rebuy_config: Option<RebuyConfig>,
) {
    let event = TournamentCreated {
        name: "Test Tournament".into(),
        game_variant: GameVariant::TexasHoldem as i32,
        buy_in: 100,
        starting_stack: 1000,
        max_players: 10,
        min_players: 2,
        scheduled_start: None,
        rebuy_config,
        addon_config: None,
        blind_structure,
        created_at: Some(angzarr_client::now()),
    };
    world.add_event(pack(&event));
}

#[given("a running tournament with a two-level blind structure")]
fn given_running_with_blinds(world: &mut TournamentWorld) {
    append_created_with_blinds_and_rebuy(
        world,
        vec![
            BlindLevel {
                level: 1,
                small_blind: 25,
                big_blind: 50,
                ante: 0,
                duration_minutes: 20,
            },
            BlindLevel {
                level: 2,
                small_blind: 50,
                big_blind: 100,
                ante: 10,
                duration_minutes: 20,
            },
        ],
        None,
    );
    // TournamentResumed applier sets status to Running.
    world.add_event(pack(&TournamentResumed {
        resumed_at: Some(angzarr_client::now()),
    }));
}

#[given("a paused tournament")]
fn given_paused_tournament(world: &mut TournamentWorld) {
    append_created(world, 100, 10, 2);
    append_registration_opened(world);
    append_player_enrolled(world, "p0");
    append_player_enrolled(world, "p1");
    // Start → Resumed → Paused: status flips to Paused via apply_paused.
    let state = world.rebuild_state();
    let book = handle_start_tournament(StartTournament {}, &state, world.next_seq())
        .expect("start tournament");
    if let Some(page) = book.pages.first() {
        if let Some(event_page::Payload::Event(e)) = &page.payload {
            world.add_event(e.clone());
        }
    }
    world.add_event(pack(&TournamentResumed {
        resumed_at: Some(angzarr_client::now()),
    }));
    world.add_event(pack(&TournamentPaused {
        reason: "initial".into(),
        paused_at: Some(angzarr_client::now()),
    }));
}

#[given(expr = "a running tournament with rebuys enabled and {int} enrolled player")]
fn given_running_with_rebuys(world: &mut TournamentWorld, n: i32) {
    append_created_with_blinds_and_rebuy(
        world,
        vec![],
        Some(RebuyConfig {
            enabled: true,
            max_rebuys: 3,
            rebuy_level_cutoff: 10,
            stack_threshold: 5000,
            rebuy_cost: 100,
            rebuy_chips: 1000,
        }),
    );
    world.add_event(pack(&RegistrationOpened {
        opened_at: Some(angzarr_client::now()),
    }));
    for i in 0..n {
        append_player_enrolled(world, &format!("p{}", i));
    }
    world.add_event(pack(&TournamentResumed {
        resumed_at: Some(angzarr_client::now()),
    }));
}

#[given(
    expr = "a running tournament with min_players {int} and max_players {int} and {int} enrolled players"
)]
fn given_running_with_n_players(
    world: &mut TournamentWorld,
    min_players: i32,
    max_players: i32,
    n: i32,
) {
    append_created(world, 100, max_players, min_players);
    append_registration_opened(world);
    for i in 0..n {
        append_player_enrolled(world, &format!("p{}", i));
    }
    // Fire StartTournament to emit TournamentStarted then additionally flip
    // status to Running via direct state mutation. Since there's no
    // apply_started in the state module, tests seed status via a dispatcher
    // trick: we invoke the handler to produce the event then also inject a
    // synthetic state-transition event.
    let state = world.rebuild_state();
    match handle_start_tournament(StartTournament {}, &state, world.next_seq()) {
        Ok(book) => {
            if let Some(page) = book.pages.first() {
                if let Some(event_page::Payload::Event(e)) = &page.payload {
                    world.add_event(e.clone());
                }
            }
        }
        Err(e) => panic!("start failed: {}", e.reason),
    }
    // Inject a post-start status flip via a "started_at" paused/resumed noop:
    // use apply_resumed which sets status to Running. Seed a TournamentResumed
    // event — the replay applier will transition status correctly on replay.
    let resumed = TournamentResumed {
        resumed_at: Some(angzarr_client::now()),
    };
    world.add_event(pack(&resumed));
}

// --- When steps ---

macro_rules! dispatch {
    ($world:ident, $handler:expr, $cmd:expr) => {{
        let state = $world.rebuild_state();
        match $handler($cmd, &state, $world.next_seq()) {
            Ok(book) => {
                $world.last_event_book = Some(book);
                $world.last_error = None;
            }
            Err(e) => {
                $world.last_error = Some(e);
                $world.last_event_book = None;
            }
        }
    }};
}

#[when(
    expr = "I handle a CreateTournament command with name {string} buy_in {int} starting_stack {int} max_players {int} min_players {int}"
)]
fn when_create_tournament(
    world: &mut TournamentWorld,
    name: String,
    buy_in: i64,
    starting_stack: i64,
    max_players: i32,
    min_players: i32,
) {
    let cmd = CreateTournament {
        name,
        game_variant: GameVariant::TexasHoldem as i32,
        buy_in,
        starting_stack,
        max_players,
        min_players,
        scheduled_start: None,
        rebuy_config: None,
        addon_config: None,
        blind_structure: vec![],
    };
    dispatch!(world, handle_create_tournament, cmd);
}

#[when("I handle an OpenRegistration command")]
fn when_open_registration(world: &mut TournamentWorld) {
    dispatch!(world, handle_open_registration, OpenRegistration {});
}

#[when("I handle a CloseRegistration command")]
fn when_close_registration(world: &mut TournamentWorld) {
    dispatch!(world, handle_close_registration, CloseRegistration {});
}

#[when(expr = "I handle an EnrollPlayer command for player {string} reservation {string}")]
fn when_enroll_player(world: &mut TournamentWorld, player_label: String, res_label: String) {
    let cmd = EnrollPlayer {
        player_root: player_root_bytes(&player_label),
        reservation_id: reservation_bytes(&res_label),
    };
    dispatch!(world, handle_enroll_player, cmd);
}

#[when(expr = "I handle a ProcessRebuy command for player {string}")]
fn when_process_rebuy(world: &mut TournamentWorld, label: String) {
    let cmd = ProcessRebuy {
        player_root: player_root_bytes(&label),
        reservation_id: Vec::new(),
    };
    dispatch!(world, handle_process_rebuy, cmd);
}

#[when(expr = "I handle an EliminatePlayer command for player {string}")]
fn when_eliminate_player(world: &mut TournamentWorld, label: String) {
    let cmd = EliminatePlayer {
        player_root: player_root_bytes(&label),
        hand_root: Vec::new(),
    };
    dispatch!(world, handle_eliminate_player, cmd);
}

#[when(expr = "I handle a PauseTournament command with reason {string}")]
fn when_pause_tournament(world: &mut TournamentWorld, reason: String) {
    let cmd = PauseTournament { reason };
    dispatch!(world, handle_pause_tournament, cmd);
}

#[when("I handle a ResumeTournament command")]
fn when_resume_tournament(world: &mut TournamentWorld) {
    dispatch!(world, handle_resume_tournament, ResumeTournament {});
}

#[when("I handle a StartTournament command")]
fn when_start_tournament(world: &mut TournamentWorld) {
    dispatch!(world, handle_start_tournament, StartTournament {});
}

#[when("I handle an AdvanceBlindLevel command")]
fn when_advance_blind_level(world: &mut TournamentWorld) {
    dispatch!(world, handle_advance_blind_level, AdvanceBlindLevel {});
}

#[when(expr = "I handle an EliminatePlayer command for player {string} with hand_root {string}")]
fn when_eliminate_player_with_hand(world: &mut TournamentWorld, label: String, hand_label: String) {
    let cmd = EliminatePlayer {
        player_root: player_root_bytes(&label),
        hand_root: hand_label.into_bytes(),
    };
    dispatch!(world, handle_eliminate_player, cmd);
}

// --- Then steps ---

macro_rules! result_is {
    ($name:ident, $ty:ty, $event_name:expr) => {
        #[then($event_name)]
        fn $name(world: &mut TournamentWorld) {
            let event = world.get_last_event().expect("No event found");
            assert!(
                angzarr_client::type_matches::<$ty>(event),
                "Expected {} but got {}",
                stringify!($ty),
                event.type_url
            );
        }
    };
}

result_is!(
    result_is_tournament_created,
    TournamentCreated,
    "the result is a examples.TournamentCreated event"
);
result_is!(
    result_is_registration_opened,
    RegistrationOpened,
    "the result is a examples.RegistrationOpened event"
);
result_is!(
    result_is_registration_closed,
    RegistrationClosed,
    "the result is a examples.RegistrationClosed event"
);
result_is!(
    result_is_player_enrolled,
    TournamentPlayerEnrolled,
    "the result is a examples.TournamentPlayerEnrolled event"
);
result_is!(
    result_is_enrollment_rejected,
    TournamentEnrollmentRejected,
    "the result is a examples.TournamentEnrollmentRejected event"
);
result_is!(
    result_is_rebuy_denied,
    RebuyDenied,
    "the result is a examples.RebuyDenied event"
);
result_is!(
    result_is_tournament_started,
    TournamentStarted,
    "the result is a examples.TournamentStarted event"
);
result_is!(
    result_is_blind_level_advanced,
    BlindLevelAdvanced,
    "the result is a examples.BlindLevelAdvanced event"
);
result_is!(
    result_is_player_eliminated,
    PlayerEliminated,
    "the result is a examples.PlayerEliminated event"
);
result_is!(
    result_is_tournament_paused,
    TournamentPaused,
    "the result is a examples.TournamentPaused event"
);
result_is!(
    result_is_tournament_resumed,
    TournamentResumed,
    "the result is a examples.TournamentResumed event"
);
result_is!(
    result_is_rebuy_processed,
    RebuyProcessed,
    "the result is a examples.RebuyProcessed event"
);

#[then(expr = "the command fails with status {string}")]
fn then_command_fails_with_status(world: &mut TournamentWorld, status: String) {
    let err = world
        .last_error
        .as_ref()
        .expect("Expected command to fail but it succeeded");
    assert_eq!(
        err.status_code, status,
        "Expected status {}, got {} (reason={:?})",
        status, err.status_code, err.reason
    );
}

#[then(expr = "the error message contains {string}")]
fn then_error_contains(world: &mut TournamentWorld, expected: String) {
    let err = world.last_error.as_ref().expect("No error found");
    assert!(
        err.reason.to_lowercase().contains(&expected.to_lowercase()),
        "Expected error to contain '{}' but got '{}'",
        expected,
        err.reason
    );
}

#[then(expr = "the tournament event has name {string}")]
fn then_event_name(world: &mut TournamentWorld, name: String) {
    let event_any = world.get_last_event().expect("No event found");
    let ev: TournamentCreated = try_unpack(event_any).expect("TournamentCreated expected");
    assert_eq!(ev.name, name);
}

#[then(expr = "the tournament event has buy_in {int}")]
fn then_event_buy_in(world: &mut TournamentWorld, amt: i64) {
    let event_any = world.get_last_event().expect("No event found");
    let ev: TournamentCreated = try_unpack(event_any).expect("TournamentCreated expected");
    assert_eq!(ev.buy_in, amt);
}

#[then(expr = "the tournament event has starting_stack {int}")]
fn then_event_starting_stack(world: &mut TournamentWorld, amt: i64) {
    let event_any = world.get_last_event().expect("No event found");
    let ev: TournamentCreated = try_unpack(event_any).expect("TournamentCreated expected");
    assert_eq!(ev.starting_stack, amt);
}

#[then(expr = "the tournament event has player_root {string}")]
fn then_event_player_root(world: &mut TournamentWorld, label: String) {
    let event_any = world.get_last_event().expect("No event found");
    let expected = player_root_bytes(&label);
    let actual = if let Some(e) = try_unpack::<TournamentPlayerEnrolled>(event_any) {
        e.player_root
    } else if let Some(e) = try_unpack::<TournamentEnrollmentRejected>(event_any) {
        e.player_root
    } else if let Some(e) = try_unpack::<RebuyProcessed>(event_any) {
        e.player_root
    } else if let Some(e) = try_unpack::<RebuyDenied>(event_any) {
        e.player_root
    } else if let Some(e) = try_unpack::<PlayerEliminated>(event_any) {
        e.player_root
    } else {
        panic!("no player_root field on event: {}", event_any.type_url);
    };
    assert_eq!(actual, expected);
}

#[then(expr = "the tournament event has fee_paid {int}")]
fn then_event_fee_paid(world: &mut TournamentWorld, amt: i64) {
    let event_any = world.get_last_event().expect("No event found");
    let ev: TournamentPlayerEnrolled =
        try_unpack(event_any).expect("TournamentPlayerEnrolled expected");
    assert_eq!(ev.fee_paid, amt);
}

#[then(expr = "the tournament event has reason containing {string}")]
fn then_event_reason_contains(world: &mut TournamentWorld, text: String) {
    let event_any = world.get_last_event().expect("No event found");
    let reason = if let Some(e) = try_unpack::<TournamentEnrollmentRejected>(event_any) {
        e.reason
    } else if let Some(e) = try_unpack::<RebuyDenied>(event_any) {
        e.reason
    } else {
        panic!("no reason field on event: {}", event_any.type_url);
    };
    assert!(
        reason.to_lowercase().contains(&text.to_lowercase()),
        "Expected reason to contain {}, got {}",
        text,
        reason
    );
}

#[then(expr = "the tournament event has total_players {int}")]
fn then_event_total_players(world: &mut TournamentWorld, n: i32) {
    let event_any = world.get_last_event().expect("No event found");
    let ev: TournamentStarted = try_unpack(event_any).expect("TournamentStarted expected");
    assert_eq!(ev.total_players, n);
}

#[then(expr = "the tournament event has blind level {int}")]
fn then_event_blind_level(world: &mut TournamentWorld, level: i32) {
    let event_any = world.get_last_event().expect("No event found");
    let ev: BlindLevelAdvanced = try_unpack(event_any).expect("BlindLevelAdvanced expected");
    assert_eq!(ev.level, level);
}

#[then(expr = "the tournament event has small_blind {int}")]
fn then_event_small_blind(world: &mut TournamentWorld, amt: i64) {
    let event_any = world.get_last_event().expect("No event found");
    let ev: BlindLevelAdvanced = try_unpack(event_any).expect("BlindLevelAdvanced expected");
    assert_eq!(ev.small_blind, amt);
}

#[then(expr = "the tournament event has ante {int}")]
fn then_event_ante(world: &mut TournamentWorld, amt: i64) {
    let event_any = world.get_last_event().expect("No event found");
    let ev: BlindLevelAdvanced = try_unpack(event_any).expect("BlindLevelAdvanced expected");
    assert_eq!(ev.ante, amt);
}

#[then(expr = "the tournament event has hand_root {string}")]
fn then_event_hand_root(world: &mut TournamentWorld, hand_label: String) {
    let event_any = world.get_last_event().expect("No event found");
    let ev: PlayerEliminated = try_unpack(event_any).expect("PlayerEliminated expected");
    assert_eq!(ev.hand_root, hand_label.into_bytes());
}

#[then(expr = "the tournament event has reason {string}")]
fn then_event_reason(world: &mut TournamentWorld, reason: String) {
    let event_any = world.get_last_event().expect("No event found");
    let ev: TournamentPaused = try_unpack(event_any).expect("TournamentPaused expected");
    assert_eq!(ev.reason, reason);
}

#[then(expr = "the tournament event has total_registrations {int}")]
fn then_event_total_registrations(world: &mut TournamentWorld, n: i32) {
    let event_any = world.get_last_event().expect("No event found");
    let ev: RegistrationClosed = try_unpack(event_any).expect("RegistrationClosed expected");
    assert_eq!(ev.total_registrations, n);
}

#[then(expr = "the tournament event has rebuy_cost {int}")]
fn then_event_rebuy_cost(world: &mut TournamentWorld, amt: i64) {
    let event_any = world.get_last_event().expect("No event found");
    let ev: RebuyProcessed = try_unpack(event_any).expect("RebuyProcessed expected");
    assert_eq!(ev.rebuy_cost, amt);
}

#[then(expr = "the tournament event has chips_added {int}")]
fn then_event_chips_added(world: &mut TournamentWorld, amt: i64) {
    let event_any = world.get_last_event().expect("No event found");
    let ev: RebuyProcessed = try_unpack(event_any).expect("RebuyProcessed expected");
    assert_eq!(ev.chips_added, amt);
}

#[then(expr = "the tournament event has rebuy_count {int}")]
fn then_event_rebuy_count(world: &mut TournamentWorld, count: i32) {
    let event_any = world.get_last_event().expect("No event found");
    let ev: RebuyProcessed = try_unpack(event_any).expect("RebuyProcessed expected");
    assert_eq!(ev.rebuy_count, count);
}

// --- Given steps for rebuy threshold variations ---

fn append_running_with_rebuy_config(
    world: &mut TournamentWorld,
    rebuy_config: RebuyConfig,
    n_players: i32,
) {
    append_created_with_blinds_and_rebuy(world, vec![], Some(rebuy_config));
    world.add_event(pack(&RegistrationOpened {
        opened_at: Some(angzarr_client::now()),
    }));
    for i in 0..n_players {
        append_player_enrolled(world, &format!("p{}", i));
    }
    world.add_event(pack(&TournamentResumed {
        resumed_at: Some(angzarr_client::now()),
    }));
}

#[given(expr = "a running tournament with rebuys disabled and {int} enrolled player")]
fn given_running_with_rebuys_disabled(world: &mut TournamentWorld, n: i32) {
    append_running_with_rebuy_config(
        world,
        RebuyConfig {
            enabled: false,
            max_rebuys: 3,
            rebuy_level_cutoff: 10,
            stack_threshold: 5000,
            rebuy_cost: 100,
            rebuy_chips: 1000,
        },
        n,
    );
}

#[given(
    expr = "a running tournament with rebuy cutoff {int} and {int} enrolled player at level {int}"
)]
fn given_running_with_rebuy_cutoff(world: &mut TournamentWorld, cutoff: i32, n: i32, level: i32) {
    append_running_with_rebuy_config(
        world,
        RebuyConfig {
            enabled: true,
            max_rebuys: 3,
            rebuy_level_cutoff: cutoff,
            stack_threshold: 5000,
            rebuy_cost: 100,
            rebuy_chips: 1000,
        },
        n,
    );
    if level > 1 {
        world.add_event(pack(&BlindLevelAdvanced {
            level,
            small_blind: 0,
            big_blind: 0,
            ante: 0,
            advanced_at: Some(angzarr_client::now()),
        }));
    }
}

#[given(
    expr = "a running tournament with max_rebuys {int} and player {string} who has used {int} rebuys"
)]
fn given_running_with_max_rebuys_and_prior_use(
    world: &mut TournamentWorld,
    max_rebuys: i32,
    label: String,
    prior: i32,
) {
    append_created_with_blinds_and_rebuy(
        world,
        vec![],
        Some(RebuyConfig {
            enabled: true,
            max_rebuys,
            rebuy_level_cutoff: 10,
            stack_threshold: 5000,
            rebuy_cost: 100,
            rebuy_chips: 1000,
        }),
    );
    world.add_event(pack(&RegistrationOpened {
        opened_at: Some(angzarr_client::now()),
    }));
    append_player_enrolled(world, &label);
    for i in 1..=prior {
        world.add_event(pack(&RebuyProcessed {
            player_root: player_root_bytes(&label),
            reservation_id: Vec::new(),
            rebuy_cost: 100,
            chips_added: 1000,
            rebuy_count: i,
            processed_at: Some(angzarr_client::now()),
        }));
    }
    world.add_event(pack(&TournamentResumed {
        resumed_at: Some(angzarr_client::now()),
    }));
}

// --- Given steps for rebuild scenarios ---

#[given(
    expr = "a TournamentCreated event for {string} with buy_in {int} starting_stack {int} max_players {int} min_players {int}"
)]
fn given_created_named(
    world: &mut TournamentWorld,
    name: String,
    buy_in: i64,
    starting_stack: i64,
    max_players: i32,
    min_players: i32,
) {
    let event = TournamentCreated {
        name,
        game_variant: GameVariant::TexasHoldem as i32,
        buy_in,
        starting_stack,
        max_players,
        min_players,
        scheduled_start: None,
        rebuy_config: None,
        addon_config: None,
        blind_structure: vec![BlindLevel {
            level: 1,
            small_blind: 25,
            big_blind: 50,
            ante: 0,
            duration_minutes: 20,
        }],
        created_at: Some(angzarr_client::now()),
    };
    world.add_event(pack(&event));
}

#[given("a RegistrationOpened event")]
fn given_registration_opened_event(world: &mut TournamentWorld) {
    world.add_event(pack(&RegistrationOpened {
        opened_at: Some(angzarr_client::now()),
    }));
}

#[given("a RegistrationClosed event")]
fn given_registration_closed_event(world: &mut TournamentWorld) {
    world.add_event(pack(&RegistrationClosed {
        total_registrations: 0,
        closed_at: Some(angzarr_client::now()),
    }));
}

#[given(expr = "a TournamentPlayerEnrolled event for player {string} with fee_paid {int}")]
fn given_player_enrolled_event(world: &mut TournamentWorld, label: String, fee_paid: i64) {
    let state = world.rebuild_state();
    world.add_event(pack(&TournamentPlayerEnrolled {
        player_root: player_root_bytes(&label),
        reservation_id: Vec::new(),
        fee_paid,
        starting_stack: state.starting_stack,
        registration_number: (state.registered_players.len() as i32) + 1,
        enrolled_at: Some(angzarr_client::now()),
    }));
}

#[given(expr = "a TournamentEnrollmentRejected event for player {string} with reason {string}")]
fn given_enrollment_rejected_event(world: &mut TournamentWorld, label: String, reason: String) {
    world.add_event(pack(&TournamentEnrollmentRejected {
        player_root: player_root_bytes(&label),
        reservation_id: Vec::new(),
        reason,
        rejected_at: Some(angzarr_client::now()),
    }));
}

#[given(
    expr = "a RebuyProcessed event for player {string} with rebuy_cost {int} rebuy_count {int}"
)]
fn given_rebuy_processed_event(
    world: &mut TournamentWorld,
    label: String,
    rebuy_cost: i64,
    rebuy_count: i32,
) {
    world.add_event(pack(&RebuyProcessed {
        player_root: player_root_bytes(&label),
        reservation_id: Vec::new(),
        rebuy_cost,
        chips_added: 1000,
        rebuy_count,
        processed_at: Some(angzarr_client::now()),
    }));
}

#[given(expr = "a RebuyDenied event for player {string} with reason {string}")]
fn given_rebuy_denied_event(world: &mut TournamentWorld, label: String, reason: String) {
    world.add_event(pack(&RebuyDenied {
        player_root: player_root_bytes(&label),
        reservation_id: Vec::new(),
        reason,
        denied_at: Some(angzarr_client::now()),
    }));
}

#[given(expr = "a BlindLevelAdvanced event to level {int}")]
fn given_blind_advanced_event(world: &mut TournamentWorld, level: i32) {
    world.add_event(pack(&BlindLevelAdvanced {
        level,
        small_blind: 100,
        big_blind: 200,
        ante: 25,
        advanced_at: Some(angzarr_client::now()),
    }));
}

#[given(expr = "a PlayerEliminated event for player {string}")]
fn given_player_eliminated_event(world: &mut TournamentWorld, label: String) {
    world.add_event(pack(&PlayerEliminated {
        player_root: player_root_bytes(&label),
        finish_position: 0,
        hand_root: Vec::new(),
        payout: 0,
        eliminated_at: Some(angzarr_client::now()),
    }));
}

#[given("a TournamentPaused event")]
fn given_paused_event(world: &mut TournamentWorld) {
    world.add_event(pack(&TournamentPaused {
        reason: "break".into(),
        paused_at: Some(angzarr_client::now()),
    }));
}

#[given("a TournamentResumed event")]
fn given_resumed_event(world: &mut TournamentWorld) {
    world.add_event(pack(&TournamentResumed {
        resumed_at: Some(angzarr_client::now()),
    }));
}

#[given("a TournamentCompleted event")]
fn given_completed_event(world: &mut TournamentWorld) {
    world.add_event(pack(&TournamentCompleted {
        winner_root: Vec::new(),
        total_prize_pool: 0,
        results: vec![],
        completed_at: Some(angzarr_client::now()),
    }));
}

// --- When / Then steps for rebuild ---

#[when("I rebuild the tournament state")]
fn when_rebuild_tournament_state(world: &mut TournamentWorld) {
    world.last_state = Some(world.rebuild_state());
}

fn status_name(status: examples_proto::TournamentStatus) -> &'static str {
    use examples_proto::TournamentStatus::*;
    match status {
        TournamentCreated => "Created",
        TournamentRegistrationOpen => "RegistrationOpen",
        TournamentRunning => "Running",
        TournamentPaused => "Paused",
        TournamentCompleted => "Completed",
        _ => "Unknown",
    }
}

#[then(expr = "the tournament state has status {string}")]
fn then_state_status(world: &mut TournamentWorld, expected: String) {
    let state = world.last_state.as_ref().expect("No state");
    assert_eq!(status_name(state.status), expected);
}

#[then(expr = "the tournament state has tournament_id {string}")]
fn then_state_tournament_id(world: &mut TournamentWorld, expected: String) {
    let state = world.last_state.as_ref().expect("No state");
    assert_eq!(state.tournament_id, expected);
}

#[then(expr = "the tournament state has name {string}")]
fn then_state_name(world: &mut TournamentWorld, expected: String) {
    let state = world.last_state.as_ref().expect("No state");
    assert_eq!(state.name, expected);
}

#[then(expr = "the tournament state has buy_in {int}")]
fn then_state_buy_in(world: &mut TournamentWorld, expected: i64) {
    let state = world.last_state.as_ref().expect("No state");
    assert_eq!(state.buy_in, expected);
}

#[then(expr = "the tournament state has starting_stack {int}")]
fn then_state_starting_stack(world: &mut TournamentWorld, expected: i64) {
    let state = world.last_state.as_ref().expect("No state");
    assert_eq!(state.starting_stack, expected);
}

#[then(expr = "the tournament state has max_players {int}")]
fn then_state_max_players(world: &mut TournamentWorld, expected: i32) {
    let state = world.last_state.as_ref().expect("No state");
    assert_eq!(state.max_players, expected);
}

#[then(expr = "the tournament state has min_players {int}")]
fn then_state_min_players(world: &mut TournamentWorld, expected: i32) {
    let state = world.last_state.as_ref().expect("No state");
    assert_eq!(state.min_players, expected);
}

#[then(expr = "the tournament state has current_level {int}")]
fn then_state_current_level(world: &mut TournamentWorld, expected: i32) {
    let state = world.last_state.as_ref().expect("No state");
    assert_eq!(state.current_level, expected);
}

#[then(expr = "the tournament state has blind_structure count {int}")]
fn then_state_blind_structure_count(world: &mut TournamentWorld, expected: usize) {
    let state = world.last_state.as_ref().expect("No state");
    assert_eq!(state.blind_structure.len(), expected);
}

#[then(expr = "the tournament state has total_prize_pool {int}")]
fn then_state_total_prize_pool(world: &mut TournamentWorld, expected: i64) {
    let state = world.last_state.as_ref().expect("No state");
    assert_eq!(state.total_prize_pool, expected);
}

#[then(expr = "the tournament state has registered_players count {int}")]
fn then_state_registered_count(world: &mut TournamentWorld, expected: usize) {
    let state = world.last_state.as_ref().expect("No state");
    assert_eq!(state.registered_players.len(), expected);
}

#[then(expr = "the tournament state has players_remaining {int}")]
fn then_state_players_remaining(world: &mut TournamentWorld, expected: i32) {
    let state = world.last_state.as_ref().expect("No state");
    assert_eq!(state.players_remaining, expected);
}

#[then(expr = "the tournament state has rebuys_used {int} for player {string}")]
fn then_state_rebuys_used(world: &mut TournamentWorld, expected: i32, label: String) {
    let state = world.last_state.as_ref().expect("No state");
    let key = hex::encode(player_root_bytes(&label));
    let reg = state
        .registered_players
        .get(&key)
        .expect("player not registered");
    assert_eq!(reg.rebuys_used, expected);
}

#[then(expr = "the tournament state has no registered player {string}")]
fn then_state_no_registered(world: &mut TournamentWorld, label: String) {
    let state = world.last_state.as_ref().expect("No state");
    let key = hex::encode(player_root_bytes(&label));
    assert!(!state.registered_players.contains_key(&key));
}

#[tokio::main]
async fn main() {
    TournamentWorld::run("features/example/unit/tournament.feature").await;
}
