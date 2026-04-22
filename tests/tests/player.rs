//! Player aggregate BDD tests using cucumber-rs against Tier 5 Router API.
//!
//! Handlers now take decoded command structs directly (Tier 5 convention).
//! State is rebuilt by iterating event pages and dispatching to the free
//! `apply_*` functions in `agg_player::state`.

use agg_player::handlers::{
    handle_confirm_buy_in, handle_confirm_rebuy_fee, handle_confirm_registration_fee,
    handle_deposit_funds, handle_initiate_buy_in, handle_initiate_rebuy,
    handle_initiate_tournament_registration, handle_join_rejected, handle_register_player,
    handle_release_buy_in, handle_release_funds, handle_release_rebuy_fee,
    handle_release_registration_fee, handle_reserve_funds, handle_transfer_funds,
    handle_withdraw_funds,
};
use agg_player::state::{
    apply_buy_in_confirmed, apply_buy_in_released, apply_buy_in_requested, apply_deposited,
    apply_rebuy_confirmed, apply_rebuy_released, apply_rebuy_requested, apply_registered,
    apply_registration_confirmed, apply_registration_released, apply_registration_requested,
    apply_released, apply_reserved, apply_transferred, apply_withdrawn, PlayerState,
};
use angzarr_client::proto::{
    business_response, event_page, CommandBook, CommandPage, Cover, EventBook, Notification,
    PageHeader, RejectionNotification, Uuid as ProtoUuid,
};
use angzarr_client::{try_unpack, type_matches};
use prost::Message;
use cucumber::{given, then, when, World};
use examples_proto::{
    BuyInConfirmed, BuyInRequested, BuyInReservationReleased, ConfirmBuyIn, ConfirmRebuyFee,
    ConfirmRegistrationFee, DepositFunds, FundsDeposited, FundsReleased, FundsReserved,
    FundsTransferred, FundsWithdrawn, InitiateBuyIn, InitiateRebuy,
    InitiateTournamentRegistration, PlayerRegistered, PlayerType, RebuyFeeConfirmed,
    RebuyFeeReleased, RebuyRequested, RegisterPlayer, RegistrationFeeConfirmed,
    RegistrationFeeReleased, RegistrationRequested, ReleaseBuyIn, ReleaseFunds,
    ReleaseRebuyFee, ReleaseRegistrationFee, ReserveFunds, TransferFunds, WithdrawFunds,
};
use poker_tests::{currency, uuid_for};
use prost_types::Any;

/// Test context for player scenarios.
#[derive(Default, World)]
#[world(init = Self::new)]
pub struct PlayerWorld {
    events: Vec<Any>,
    last_error: Option<angzarr_client::CommandRejectedError>,
    last_event_book: Option<EventBook>,
    last_state: Option<PlayerState>,
    /// Chips-to-add seeded for `a pending rebuy ... chips N` so the matching
    /// RebuyFeeConfirmed seeder can echo it (apply_rebuy_requested doesn't
    /// preserve the value).
    pending_rebuy_chips: std::collections::HashMap<String, i64>,
    /// State mutators applied after replay (e.g. to set PendingRebuy.chips_to_add
    /// since RebuyRequested doesn't carry it).
    state_seeders: Vec<Box<dyn Fn(&mut PlayerState) + Send>>,
}

impl std::fmt::Debug for PlayerWorld {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlayerWorld")
            .field("events", &self.events.len())
            .field("last_error", &self.last_error)
            .finish()
    }
}

impl PlayerWorld {
    fn new() -> Self {
        Self::default()
    }

    fn next_sequence(&self) -> u32 {
        self.events.len() as u32
    }

    fn add_event(&mut self, event_any: Any) {
        self.events.push(event_any);
    }

    /// Rebuild player state by dispatching each event to the right applier,
    /// then applying any registered post-replay seeders.
    fn rebuild_state(&self) -> PlayerState {
        let mut state = self.rebuild_state_no_seed();
        for seeder in &self.state_seeders {
            seeder(&mut state);
        }
        state
    }

    fn rebuild_state_no_seed(&self) -> PlayerState {
        let mut state = PlayerState::default();
        for event_any in &self.events {
            if let Some(ev) = try_unpack::<PlayerRegistered>(event_any) {
                apply_registered(&mut state, ev);
            } else if let Some(ev) = try_unpack::<FundsDeposited>(event_any) {
                apply_deposited(&mut state, ev);
            } else if let Some(ev) = try_unpack::<FundsWithdrawn>(event_any) {
                apply_withdrawn(&mut state, ev);
            } else if let Some(ev) = try_unpack::<FundsReserved>(event_any) {
                apply_reserved(&mut state, ev);
            } else if let Some(ev) = try_unpack::<FundsReleased>(event_any) {
                apply_released(&mut state, ev);
            } else if let Some(ev) = try_unpack::<FundsTransferred>(event_any) {
                apply_transferred(&mut state, ev);
            } else if let Some(ev) = try_unpack::<BuyInRequested>(event_any) {
                apply_buy_in_requested(&mut state, ev);
            } else if let Some(ev) = try_unpack::<BuyInConfirmed>(event_any) {
                apply_buy_in_confirmed(&mut state, ev);
            } else if let Some(ev) = try_unpack::<BuyInReservationReleased>(event_any) {
                apply_buy_in_released(&mut state, ev);
            } else if let Some(ev) = try_unpack::<RegistrationRequested>(event_any) {
                apply_registration_requested(&mut state, ev);
            } else if let Some(ev) = try_unpack::<RegistrationFeeConfirmed>(event_any) {
                apply_registration_confirmed(&mut state, ev);
            } else if let Some(ev) = try_unpack::<RegistrationFeeReleased>(event_any) {
                apply_registration_released(&mut state, ev);
            } else if let Some(ev) = try_unpack::<RebuyRequested>(event_any) {
                apply_rebuy_requested(&mut state, ev);
            } else if let Some(ev) = try_unpack::<RebuyFeeConfirmed>(event_any) {
                apply_rebuy_confirmed(&mut state, ev);
            } else if let Some(ev) = try_unpack::<RebuyFeeReleased>(event_any) {
                apply_rebuy_released(&mut state, ev);
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

fn pack_event_any<T: prost::Message + prost::Name>(event: &T) -> Any {
    angzarr_client::pack_event(event, &T::full_name())
}

// --- Given Step Definitions ---

#[given("no prior events for the player aggregate")]
fn no_prior_events(world: &mut PlayerWorld) {
    world.events.clear();
}

#[given(expr = "a PlayerRegistered event for {string}")]
fn player_registered_event(world: &mut PlayerWorld, name: String) {
    let event = PlayerRegistered {
        display_name: name.clone(),
        email: format!("{}@example.com", name.to_lowercase()),
        player_type: PlayerType::Human as i32,
        ai_model_id: String::new(),
        registered_at: Some(angzarr_client::now()),
    };
    world.add_event(pack_event_any(&event));
}

#[given(expr = "a FundsDeposited event with amount {int}")]
fn funds_deposited_event(world: &mut PlayerWorld, amount: i64) {
    let state = world.rebuild_state();
    let new_balance = state.bankroll + amount;

    let event = FundsDeposited {
        amount: Some(currency(amount)),
        new_balance: Some(currency(new_balance)),
        deposited_at: Some(angzarr_client::now()),
    };
    world.add_event(pack_event_any(&event));
}

#[given(expr = "a FundsReserved event with amount {int} for table {string}")]
fn funds_reserved_event(world: &mut PlayerWorld, amount: i64, table_name: String) {
    let state = world.rebuild_state();
    let new_reserved = state.reserved_funds + amount;
    let new_available = state.bankroll - new_reserved;

    let event = FundsReserved {
        table_root: uuid_for(&table_name),
        amount: Some(currency(amount)),
        new_available_balance: Some(currency(new_available)),
        new_reserved_balance: Some(currency(new_reserved)),
        reserved_at: Some(angzarr_client::now()),
    };
    world.add_event(pack_event_any(&event));
}

// --- When Step Definitions ---

#[when(expr = "I handle a RegisterPlayer command with name {string} and email {string}")]
fn handle_register_player_cmd(world: &mut PlayerWorld, name: String, email: String) {
    let cmd = RegisterPlayer {
        display_name: name,
        email,
        player_type: PlayerType::Human as i32,
        ai_model_id: String::new(),
    };
    let state = world.rebuild_state();
    match handle_register_player(cmd, &state, world.next_sequence()) {
        Ok(event_book) => {
            world.last_event_book = Some(event_book);
            world.last_error = None;
        }
        Err(e) => {
            world.last_error = Some(e);
            world.last_event_book = None;
        }
    }
}

#[when(expr = "I handle a RegisterPlayer command with name {string} and email {string} as AI")]
fn handle_register_player_ai_cmd(world: &mut PlayerWorld, name: String, email: String) {
    let cmd = RegisterPlayer {
        display_name: name,
        email,
        player_type: PlayerType::Ai as i32,
        ai_model_id: "gpt-4".to_string(),
    };
    let state = world.rebuild_state();
    match handle_register_player(cmd, &state, world.next_sequence()) {
        Ok(event_book) => {
            world.last_event_book = Some(event_book);
            world.last_error = None;
        }
        Err(e) => {
            world.last_error = Some(e);
            world.last_event_book = None;
        }
    }
}

#[when(expr = "I handle a DepositFunds command with amount {int}")]
fn handle_deposit_funds_cmd(world: &mut PlayerWorld, amount: i64) {
    let cmd = DepositFunds {
        amount: Some(currency(amount)),
    };
    let state = world.rebuild_state();
    match handle_deposit_funds(cmd, &state, world.next_sequence()) {
        Ok(event_book) => {
            world.last_event_book = Some(event_book);
            world.last_error = None;
        }
        Err(e) => {
            world.last_error = Some(e);
            world.last_event_book = None;
        }
    }
}

#[when(expr = "I handle a WithdrawFunds command with amount {int}")]
fn handle_withdraw_funds_cmd(world: &mut PlayerWorld, amount: i64) {
    let cmd = WithdrawFunds {
        amount: Some(currency(amount)),
    };
    let state = world.rebuild_state();
    match handle_withdraw_funds(cmd, &state, world.next_sequence()) {
        Ok(event_book) => {
            world.last_event_book = Some(event_book);
            world.last_error = None;
        }
        Err(e) => {
            world.last_error = Some(e);
            world.last_event_book = None;
        }
    }
}

#[when(expr = "I handle a ReserveFunds command with amount {int} for table {string}")]
fn handle_reserve_funds_cmd(world: &mut PlayerWorld, amount: i64, table_name: String) {
    let cmd = ReserveFunds {
        table_root: uuid_for_or_empty(&table_name),
        amount: Some(currency(amount)),
    };
    let state = world.rebuild_state();
    match handle_reserve_funds(cmd, &state, world.next_sequence()) {
        Ok(event_book) => {
            world.last_event_book = Some(event_book);
            world.last_error = None;
        }
        Err(e) => {
            world.last_error = Some(e);
            world.last_event_book = None;
        }
    }
}

#[when(expr = "I handle a ReleaseFunds command for table {string}")]
fn handle_release_funds_cmd(world: &mut PlayerWorld, table_name: String) {
    let cmd = ReleaseFunds {
        table_root: uuid_for_or_empty(&table_name),
    };
    let state = world.rebuild_state();
    match handle_release_funds(cmd, &state, world.next_sequence()) {
        Ok(event_book) => {
            world.last_event_book = Some(event_book);
            world.last_error = None;
        }
        Err(e) => {
            world.last_error = Some(e);
            world.last_event_book = None;
        }
    }
}

#[when("I rebuild the player state")]
fn rebuild_player_state(world: &mut PlayerWorld) {
    world.last_state = Some(world.rebuild_state());
}

fn build_join_rejection_notification(table_name: &str) -> Notification {
    let table_root = uuid_for(table_name);
    let rejected_command = CommandBook {
        cover: Some(Cover {
            domain: "table".into(),
            root: Some(ProtoUuid { value: table_root }),
            correlation_id: String::new(),
            edition: None,
        }),
        pages: vec![CommandPage {
            header: Some(PageHeader {
                sequence_type: None,
            }),
            merge_strategy: 0,
            payload: None,
        }],
    };
    let rejection = RejectionNotification {
        rejected_command: Some(rejected_command),
        rejection_reason: "seat_occupied".into(),
    };
    let payload_any = Any {
        type_url: format!(
            "{}{}",
            angzarr_client::TYPE_URL_PREFIX,
            "angzarr.RejectionNotification"
        ),
        value: rejection.encode_to_vec(),
    };
    Notification {
        cover: Some(Cover {
            domain: "player".into(),
            root: Some(ProtoUuid {
                value: vec![0xff; 16],
            }),
            correlation_id: String::new(),
            edition: None,
        }),
        payload: Some(payload_any),
        sent_at: None,
    }
}

#[when(expr = "I handle a JoinTable rejection notification for table {string}")]
fn when_handle_join_rejection(world: &mut PlayerWorld, table_name: String) {
    let notification = build_join_rejection_notification(&table_name);
    let state = world.rebuild_state();
    match handle_join_rejected(&notification, &state) {
        Ok(response) => match response.result {
            Some(business_response::Result::Events(book)) => {
                world.last_event_book = Some(book);
                world.last_error = None;
            }
            other => panic!("expected Events variant, got {:?}", other.is_some()),
        },
        Err(e) => {
            world.last_error = Some(e);
            world.last_event_book = None;
        }
    }
}

// --- Then Step Definitions ---

#[then("the result is a examples.PlayerRegistered event")]
fn result_is_player_registered(world: &mut PlayerWorld) {
    let event = world.get_last_event().expect("No event found");
    assert!(
        type_matches::<PlayerRegistered>(event),
        "Expected PlayerRegistered event but got {}",
        event.type_url
    );
}

#[then("the result is a examples.FundsDeposited event")]
fn result_is_funds_deposited(world: &mut PlayerWorld) {
    let event = world.get_last_event().expect("No event found");
    assert!(
        type_matches::<FundsDeposited>(event),
        "Expected FundsDeposited event but got {}",
        event.type_url
    );
}

#[then("the result is a examples.FundsWithdrawn event")]
fn result_is_funds_withdrawn(world: &mut PlayerWorld) {
    let event = world.get_last_event().expect("No event found");
    assert!(
        type_matches::<FundsWithdrawn>(event),
        "Expected FundsWithdrawn event but got {}",
        event.type_url
    );
}

#[then("the result is a examples.FundsReserved event")]
fn result_is_funds_reserved(world: &mut PlayerWorld) {
    let event = world.get_last_event().expect("No event found");
    assert!(
        type_matches::<FundsReserved>(event),
        "Expected FundsReserved event but got {}",
        event.type_url
    );
}

#[then("the result is a examples.FundsReleased event")]
fn result_is_funds_released(world: &mut PlayerWorld) {
    let event = world.get_last_event().expect("No event found");
    assert!(
        type_matches::<FundsReleased>(event),
        "Expected FundsReleased event but got {}",
        event.type_url
    );
}

#[then(expr = "the command fails with status {string}")]
fn command_fails_with_status(world: &mut PlayerWorld, status: String) {
    let err = world
        .last_error
        .as_ref()
        .expect("Expected command to fail but it succeeded");
    assert_eq!(
        err.status_code, status,
        "Expected status {}, got {}",
        status, err.status_code
    );
}

#[then(expr = "the error message contains {string}")]
fn error_message_contains(world: &mut PlayerWorld, expected: String) {
    let error = world.last_error.as_ref().expect("No error found");
    assert!(
        error
            .reason
            .to_lowercase()
            .contains(&expected.to_lowercase()),
        "Expected error to contain '{}' but got '{}'",
        expected,
        error.reason
    );
}

#[then(expr = "the player event has display_name {string}")]
fn player_event_has_display_name(world: &mut PlayerWorld, expected: String) {
    let event_any = world.get_last_event().expect("No event found");
    let event: PlayerRegistered =
        try_unpack::<PlayerRegistered>(event_any).expect("Failed to unpack event");
    assert_eq!(
        event.display_name, expected,
        "Expected display_name '{}' but got '{}'",
        expected, event.display_name
    );
}

#[then(expr = "the player event has email {string}")]
fn player_event_has_email(world: &mut PlayerWorld, expected: String) {
    let event_any = world.get_last_event().expect("No event found");
    let event: PlayerRegistered =
        try_unpack::<PlayerRegistered>(event_any).expect("Failed to unpack event");
    assert_eq!(event.email, expected);
}

#[then(expr = "the player event has ai_model_id {string}")]
fn player_event_has_ai_model_id(world: &mut PlayerWorld, expected: String) {
    let event_any = world.get_last_event().expect("No event found");
    let event: PlayerRegistered =
        try_unpack::<PlayerRegistered>(event_any).expect("Failed to unpack event");
    assert_eq!(event.ai_model_id, expected);
}

#[then(expr = "the player event has player_type {string}")]
fn player_event_has_player_type(world: &mut PlayerWorld, expected: String) {
    let event_any = world.get_last_event().expect("No event found");
    let event: PlayerRegistered =
        try_unpack::<PlayerRegistered>(event_any).expect("Failed to unpack event");
    let player_type = PlayerType::try_from(event.player_type).unwrap_or_default();
    let type_str = match player_type {
        PlayerType::Human => "HUMAN",
        PlayerType::Ai => "AI",
        _ => "UNKNOWN",
    };
    assert_eq!(
        type_str, expected,
        "Expected player_type '{}' but got '{}'",
        expected, type_str
    );
}

#[then(expr = "the player event has amount {int}")]
fn player_event_has_amount(world: &mut PlayerWorld, expected: i64) {
    let event_any = world.get_last_event().expect("No event found");

    let amount = if let Some(event) = try_unpack::<FundsDeposited>(event_any) {
        event.amount.map(|c| c.amount).unwrap_or(0)
    } else if let Some(event) = try_unpack::<FundsWithdrawn>(event_any) {
        event.amount.map(|c| c.amount).unwrap_or(0)
    } else if let Some(event) = try_unpack::<FundsReserved>(event_any) {
        event.amount.map(|c| c.amount).unwrap_or(0)
    } else if let Some(event) = try_unpack::<FundsReleased>(event_any) {
        event.amount.map(|c| c.amount).unwrap_or(0)
    } else if let Some(event) = try_unpack::<FundsTransferred>(event_any) {
        event.amount.map(|c| c.amount).unwrap_or(0)
    } else {
        panic!("Unknown event type: {}", event_any.type_url);
    };

    assert_eq!(
        amount, expected,
        "Expected amount {} but got {}",
        expected, amount
    );
}

#[then(expr = "the player event has new_balance {int}")]
fn player_event_has_new_balance(world: &mut PlayerWorld, expected: i64) {
    let event_any = world.get_last_event().expect("No event found");

    let balance = if let Some(event) = try_unpack::<FundsDeposited>(event_any) {
        event.new_balance.map(|c| c.amount).unwrap_or(0)
    } else if let Some(event) = try_unpack::<FundsWithdrawn>(event_any) {
        event.new_balance.map(|c| c.amount).unwrap_or(0)
    } else if let Some(event) = try_unpack::<FundsTransferred>(event_any) {
        event.new_balance.map(|c| c.amount).unwrap_or(0)
    } else {
        panic!("Unknown event type for new_balance: {}", event_any.type_url);
    };

    assert_eq!(
        balance, expected,
        "Expected new_balance {} but got {}",
        expected, balance
    );
}

#[then(expr = "the player event has new_available_balance {int}")]
fn player_event_has_new_available_balance(world: &mut PlayerWorld, expected: i64) {
    let event_any = world.get_last_event().expect("No event found");

    let available = if let Some(event) = try_unpack::<FundsReserved>(event_any) {
        event.new_available_balance.map(|c| c.amount).unwrap_or(0)
    } else if let Some(event) = try_unpack::<FundsReleased>(event_any) {
        event.new_available_balance.map(|c| c.amount).unwrap_or(0)
    } else {
        panic!(
            "Unknown event type for new_available_balance: {}",
            event_any.type_url
        );
    };

    assert_eq!(
        available, expected,
        "Expected new_available_balance {} but got {}",
        expected, available
    );
}

#[then(expr = "the player state has bankroll {int}")]
fn player_state_has_bankroll(world: &mut PlayerWorld, expected: i64) {
    let state = world.last_state.as_ref().expect("No state found");
    assert_eq!(
        state.bankroll, expected,
        "Expected bankroll {} but got {}",
        expected, state.bankroll
    );
}

#[then(expr = "the player state has reserved_funds {int}")]
fn player_state_has_reserved_funds(world: &mut PlayerWorld, expected: i64) {
    let state = world.last_state.as_ref().expect("No state found");
    assert_eq!(
        state.reserved_funds, expected,
        "Expected reserved_funds {} but got {}",
        expected, state.reserved_funds
    );
}

#[then(expr = "the player state has available_balance {int}")]
fn player_state_has_available_balance(world: &mut PlayerWorld, expected: i64) {
    let state = world.last_state.as_ref().expect("No state found");
    let available = state.available_balance();
    assert_eq!(
        available, expected,
        "Expected available_balance {} but got {}",
        expected, available
    );
}

// --- Additional Given Steps (event seeders) ---

#[given(expr = "a FundsWithdrawn event with amount {int}")]
fn funds_withdrawn_event(world: &mut PlayerWorld, amount: i64) {
    let state = world.rebuild_state();
    let new_balance = state.bankroll - amount;
    let event = FundsWithdrawn {
        amount: Some(currency(amount)),
        new_balance: Some(currency(new_balance)),
        withdrawn_at: Some(angzarr_client::now()),
    };
    world.add_event(pack_event_any(&event));
}

#[given(expr = "a FundsReleased event for table {string} with amount {int}")]
fn funds_released_event(world: &mut PlayerWorld, table_name: String, amount: i64) {
    let state = world.rebuild_state();
    let new_reserved = state.reserved_funds - amount;
    let new_available = state.bankroll - new_reserved;
    let event = FundsReleased {
        amount: Some(currency(amount)),
        table_root: uuid_for(&table_name),
        new_available_balance: Some(currency(new_available)),
        new_reserved_balance: Some(currency(new_reserved)),
        released_at: Some(angzarr_client::now()),
    };
    world.add_event(pack_event_any(&event));
}

#[given(expr = "a pending buy-in {string} for table {string} seat {int} amount {int}")]
fn pending_buy_in(
    world: &mut PlayerWorld,
    reservation: String,
    table_name: String,
    seat: i32,
    amount: i64,
) {
    let event = BuyInRequested {
        reservation_id: uuid_for(&reservation),
        table_root: uuid_for(&table_name),
        seat,
        amount: Some(currency(amount)),
        requested_at: Some(angzarr_client::now()),
    };
    world.add_event(pack_event_any(&event));
}

#[given(expr = "a BuyInConfirmed event for reservation {string} table {string}")]
fn buy_in_confirmed_event(
    world: &mut PlayerWorld,
    reservation: String,
    table_name: String,
) {
    // Look up amount/seat from prior pending buy-in if present
    let state = world.rebuild_state();
    let res_hex = hex::encode(uuid_for(&reservation));
    let (seat, amount) = state
        .pending_buy_ins
        .get(&res_hex)
        .map(|p| (p.seat, p.amount))
        .unwrap_or((0, 0));
    let event = BuyInConfirmed {
        reservation_id: uuid_for(&reservation),
        table_root: uuid_for(&table_name),
        seat,
        amount: Some(currency(amount)),
        confirmed_at: Some(angzarr_client::now()),
    };
    world.add_event(pack_event_any(&event));
}

#[given(expr = "a pending registration {string} for tournament {string} fee {int}")]
fn pending_registration(
    world: &mut PlayerWorld,
    reservation: String,
    tournament: String,
    fee: i64,
) {
    let event = RegistrationRequested {
        reservation_id: uuid_for(&reservation),
        tournament_root: uuid_for(&tournament),
        fee: Some(currency(fee)),
        requested_at: Some(angzarr_client::now()),
    };
    world.add_event(pack_event_any(&event));
}

#[given(expr = "a RegistrationFeeConfirmed event for reservation {string} tournament {string}")]
fn registration_fee_confirmed_event(
    world: &mut PlayerWorld,
    reservation: String,
    tournament: String,
) {
    let state = world.rebuild_state();
    let res_hex = hex::encode(uuid_for(&reservation));
    let fee = state
        .pending_registrations
        .get(&res_hex)
        .map(|p| p.fee)
        .unwrap_or(0);
    let event = RegistrationFeeConfirmed {
        reservation_id: uuid_for(&reservation),
        tournament_root: uuid_for(&tournament),
        fee: Some(currency(fee)),
        confirmed_at: Some(angzarr_client::now()),
    };
    world.add_event(pack_event_any(&event));
}

#[given(
    expr = "a pending rebuy {string} for tournament {string} table {string} seat {int} fee {int} chips {int}"
)]
fn pending_rebuy(
    world: &mut PlayerWorld,
    reservation: String,
    tournament: String,
    table_name: String,
    seat: i32,
    fee: i64,
    chips: i64,
) {
    let event = RebuyRequested {
        reservation_id: uuid_for(&reservation),
        tournament_root: uuid_for(&tournament),
        table_root: uuid_for(&table_name),
        seat,
        fee: Some(currency(fee)),
        requested_at: Some(angzarr_client::now()),
    };
    world.add_event(pack_event_any(&event));
    // Stash for the matching RebuyFeeConfirmed seeder
    world
        .pending_rebuy_chips
        .insert(reservation.clone(), chips);
    // RebuyRequested doesn't carry chips_to_add — patch it into the
    // materialized state after replay.
    let res_hex = hex::encode(uuid_for(&reservation));
    world.state_seeders.push(Box::new(move |state| {
        if let Some(pending) = state.pending_rebuys.get_mut(&res_hex) {
            pending.chips_to_add = chips;
        }
    }));
}

#[given(expr = "a RebuyFeeConfirmed event for reservation {string}")]
fn rebuy_fee_confirmed_event(world: &mut PlayerWorld, reservation: String) {
    let state = world.rebuild_state();
    let res_hex = hex::encode(uuid_for(&reservation));
    let pending = state.pending_rebuys.get(&res_hex);
    let (tournament_root, fee) = pending
        .map(|p| (p.tournament_root.clone(), p.fee))
        .unwrap_or((Vec::new(), 0));
    let chips_added = world
        .pending_rebuy_chips
        .get(&reservation)
        .copied()
        .unwrap_or(0);
    let event = RebuyFeeConfirmed {
        reservation_id: uuid_for(&reservation),
        tournament_root,
        fee: Some(currency(fee)),
        chips_added,
        confirmed_at: Some(angzarr_client::now()),
    };
    world.add_event(pack_event_any(&event));
}

// --- Additional When Steps ---

macro_rules! handle_cmd {
    ($world:ident, $handler:expr, $cmd:expr) => {{
        let state = $world.rebuild_state();
        match $handler($cmd, &state, $world.next_sequence()) {
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

#[when(expr = "I handle a TransferFunds command from {string} with amount {int} for hand {string} reason {string}")]
fn handle_transfer_funds_cmd(
    world: &mut PlayerWorld,
    from: String,
    amount: i64,
    hand: String,
    reason: String,
) {
    let cmd = TransferFunds {
        from_player_root: uuid_for(&from),
        amount: Some(currency(amount)),
        hand_root: uuid_for(&hand),
        reason,
    };
    handle_cmd!(world, handle_transfer_funds, cmd);
}

#[when(expr = "I handle an InitiateBuyIn command for table {string} seat {int} amount {int}")]
fn handle_initiate_buy_in_cmd(
    world: &mut PlayerWorld,
    table_name: String,
    seat: i32,
    amount: i64,
) {
    let cmd = InitiateBuyIn {
        table_root: uuid_for_or_empty(&table_name),
        seat,
        amount: Some(currency(amount)),
    };
    handle_cmd!(world, handle_initiate_buy_in, cmd);
}

#[when(expr = "I handle a ConfirmBuyIn command for reservation {string}")]
fn handle_confirm_buy_in_cmd(world: &mut PlayerWorld, reservation: String) {
    let cmd = ConfirmBuyIn {
        reservation_id: uuid_for_or_empty(&reservation),
    };
    handle_cmd!(world, handle_confirm_buy_in, cmd);
}

#[when(expr = "I handle a ReleaseBuyIn command for reservation {string} reason {string}")]
fn handle_release_buy_in_cmd(world: &mut PlayerWorld, reservation: String, reason: String) {
    let cmd = ReleaseBuyIn {
        reservation_id: uuid_for_or_empty(&reservation),
        reason,
    };
    handle_cmd!(world, handle_release_buy_in, cmd);
}

#[when(expr = "I handle an InitiateTournamentRegistration command for tournament {string}")]
fn handle_initiate_registration_cmd(world: &mut PlayerWorld, tournament: String) {
    let cmd = InitiateTournamentRegistration {
        tournament_root: uuid_for_or_empty(&tournament),
    };
    handle_cmd!(world, handle_initiate_tournament_registration, cmd);
}

#[when(expr = "I handle a ConfirmRegistrationFee command for reservation {string}")]
fn handle_confirm_registration_fee_cmd(world: &mut PlayerWorld, reservation: String) {
    let cmd = ConfirmRegistrationFee {
        reservation_id: uuid_for_or_empty(&reservation),
    };
    handle_cmd!(world, handle_confirm_registration_fee, cmd);
}

#[when(expr = "I handle a ReleaseRegistrationFee command for reservation {string} reason {string}")]
fn handle_release_registration_fee_cmd(
    world: &mut PlayerWorld,
    reservation: String,
    reason: String,
) {
    let cmd = ReleaseRegistrationFee {
        reservation_id: uuid_for_or_empty(&reservation),
        reason,
    };
    handle_cmd!(world, handle_release_registration_fee, cmd);
}

#[when(expr = "I handle an InitiateRebuy command for tournament {string} table {string} seat {int}")]
fn handle_initiate_rebuy_cmd(
    world: &mut PlayerWorld,
    tournament: String,
    table_name: String,
    seat: i32,
) {
    let cmd = InitiateRebuy {
        tournament_root: uuid_for_or_empty(&tournament),
        table_root: uuid_for_or_empty(&table_name),
        seat,
    };
    handle_cmd!(world, handle_initiate_rebuy, cmd);
}

#[when(expr = "I handle a ConfirmRebuyFee command for reservation {string}")]
fn handle_confirm_rebuy_fee_cmd(world: &mut PlayerWorld, reservation: String) {
    let cmd = ConfirmRebuyFee {
        reservation_id: uuid_for_or_empty(&reservation),
    };
    handle_cmd!(world, handle_confirm_rebuy_fee, cmd);
}

#[when(expr = "I handle a ReleaseRebuyFee command for reservation {string} reason {string}")]
fn handle_release_rebuy_fee_cmd(world: &mut PlayerWorld, reservation: String, reason: String) {
    let cmd = ReleaseRebuyFee {
        reservation_id: uuid_for_or_empty(&reservation),
        reason,
    };
    handle_cmd!(world, handle_release_rebuy_fee, cmd);
}

fn uuid_for_or_empty(s: &str) -> Vec<u8> {
    if s.is_empty() {
        Vec::new()
    } else {
        uuid_for(s)
    }
}

// --- Additional Then Steps ---

macro_rules! result_is {
    ($name:ident, $ty:ty, $event_name:expr) => {
        #[then($event_name)]
        fn $name(world: &mut PlayerWorld) {
            let event = world.get_last_event().expect("No event found");
            assert!(
                type_matches::<$ty>(event),
                "Expected {} but got {}",
                stringify!($ty),
                event.type_url
            );
        }
    };
}

result_is!(
    result_is_funds_transferred,
    FundsTransferred,
    "the result is a examples.FundsTransferred event"
);
result_is!(
    result_is_buy_in_requested,
    BuyInRequested,
    "the result is a examples.BuyInRequested event"
);
result_is!(
    result_is_buy_in_confirmed,
    BuyInConfirmed,
    "the result is a examples.BuyInConfirmed event"
);
result_is!(
    result_is_buy_in_released,
    BuyInReservationReleased,
    "the result is a examples.BuyInReservationReleased event"
);
result_is!(
    result_is_registration_requested,
    RegistrationRequested,
    "the result is a examples.RegistrationRequested event"
);
result_is!(
    result_is_registration_fee_confirmed,
    RegistrationFeeConfirmed,
    "the result is a examples.RegistrationFeeConfirmed event"
);
result_is!(
    result_is_registration_fee_released,
    RegistrationFeeReleased,
    "the result is a examples.RegistrationFeeReleased event"
);
result_is!(
    result_is_rebuy_requested,
    RebuyRequested,
    "the result is a examples.RebuyRequested event"
);
result_is!(
    result_is_rebuy_fee_confirmed,
    RebuyFeeConfirmed,
    "the result is a examples.RebuyFeeConfirmed event"
);
result_is!(
    result_is_rebuy_fee_released,
    RebuyFeeReleased,
    "the result is a examples.RebuyFeeReleased event"
);

#[then(expr = "the event has a timestamp {word}")]
fn event_has_timestamp(world: &mut PlayerWorld, _field: String) {
    // Every emitted event includes a timestamp; presence of an event satisfies this.
    world.get_last_event().expect("No event found");
}

#[then(expr = "the error message equals {string}")]
fn error_message_equals(world: &mut PlayerWorld, expected: String) {
    let err = world.last_error.as_ref().expect("No error found");
    assert_eq!(
        err.reason, expected,
        "Expected error '{}' but got '{}'",
        expected, err.reason
    );
}

#[then(expr = "the player event has reason {string}")]
fn player_event_has_reason(world: &mut PlayerWorld, expected: String) {
    let event_any = world.get_last_event().expect("No event found");
    let reason = if let Some(e) = try_unpack::<FundsTransferred>(event_any) {
        e.reason
    } else {
        panic!("Unknown event type for reason: {}", event_any.type_url);
    };
    assert_eq!(reason, expected);
}

#[then(expr = "the player event has from_player_root {string}")]
fn player_event_has_from_player_root(world: &mut PlayerWorld, expected: String) {
    let event_any = world.get_last_event().expect("No event found");
    let event: FundsTransferred = try_unpack(event_any).expect("FundsTransferred expected");
    assert_eq!(event.from_player_root, uuid_for(&expected));
}

#[then(expr = "the player event has hand_root {string}")]
fn player_event_has_hand_root(world: &mut PlayerWorld, expected: String) {
    let event_any = world.get_last_event().expect("No event found");
    let event: FundsTransferred = try_unpack(event_any).expect("FundsTransferred expected");
    assert_eq!(event.hand_root, uuid_for(&expected));
}

#[then(expr = "the player event has to_player_root for player {string}")]
fn player_event_has_to_player_root(world: &mut PlayerWorld, email: String) {
    let event_any = world.get_last_event().expect("No event found");
    let event: FundsTransferred = try_unpack(event_any).expect("FundsTransferred expected");
    let expected = format!("player_{}", email).into_bytes();
    assert_eq!(event.to_player_root, expected);
}

#[then(expr = "the player event has new_reserved_balance {int}")]
fn player_event_has_new_reserved_balance(world: &mut PlayerWorld, expected: i64) {
    let event_any = world.get_last_event().expect("No event found");
    let reserved = if let Some(e) = try_unpack::<FundsReserved>(event_any) {
        e.new_reserved_balance.map(|c| c.amount).unwrap_or(0)
    } else if let Some(e) = try_unpack::<FundsReleased>(event_any) {
        e.new_reserved_balance.map(|c| c.amount).unwrap_or(0)
    } else {
        panic!("Unknown event type for new_reserved_balance: {}", event_any.type_url);
    };
    assert_eq!(reserved, expected);
}

// --- Orchestration event field assertions ---

fn orch_amount(event_any: &Any) -> Option<i64> {
    if let Some(e) = try_unpack::<BuyInRequested>(event_any) {
        return e.amount.map(|c| c.amount);
    }
    if let Some(e) = try_unpack::<BuyInConfirmed>(event_any) {
        return e.amount.map(|c| c.amount);
    }
    None
}

fn orch_reservation_id(event_any: &Any) -> Option<Vec<u8>> {
    if let Some(e) = try_unpack::<BuyInRequested>(event_any) {
        return Some(e.reservation_id);
    }
    if let Some(e) = try_unpack::<BuyInConfirmed>(event_any) {
        return Some(e.reservation_id);
    }
    if let Some(e) = try_unpack::<BuyInReservationReleased>(event_any) {
        return Some(e.reservation_id);
    }
    if let Some(e) = try_unpack::<RegistrationRequested>(event_any) {
        return Some(e.reservation_id);
    }
    if let Some(e) = try_unpack::<RegistrationFeeConfirmed>(event_any) {
        return Some(e.reservation_id);
    }
    if let Some(e) = try_unpack::<RegistrationFeeReleased>(event_any) {
        return Some(e.reservation_id);
    }
    if let Some(e) = try_unpack::<RebuyRequested>(event_any) {
        return Some(e.reservation_id);
    }
    if let Some(e) = try_unpack::<RebuyFeeConfirmed>(event_any) {
        return Some(e.reservation_id);
    }
    if let Some(e) = try_unpack::<RebuyFeeReleased>(event_any) {
        return Some(e.reservation_id);
    }
    None
}

fn orch_tournament_root(event_any: &Any) -> Option<Vec<u8>> {
    if let Some(e) = try_unpack::<RegistrationRequested>(event_any) {
        return Some(e.tournament_root);
    }
    if let Some(e) = try_unpack::<RegistrationFeeConfirmed>(event_any) {
        return Some(e.tournament_root);
    }
    if let Some(e) = try_unpack::<RebuyRequested>(event_any) {
        return Some(e.tournament_root);
    }
    if let Some(e) = try_unpack::<RebuyFeeConfirmed>(event_any) {
        return Some(e.tournament_root);
    }
    None
}

fn orch_table_root(event_any: &Any) -> Option<Vec<u8>> {
    if let Some(e) = try_unpack::<BuyInRequested>(event_any) {
        return Some(e.table_root);
    }
    if let Some(e) = try_unpack::<BuyInConfirmed>(event_any) {
        return Some(e.table_root);
    }
    if let Some(e) = try_unpack::<RebuyRequested>(event_any) {
        return Some(e.table_root);
    }
    None
}

fn orch_seat(event_any: &Any) -> Option<i32> {
    if let Some(e) = try_unpack::<BuyInRequested>(event_any) {
        return Some(e.seat);
    }
    if let Some(e) = try_unpack::<BuyInConfirmed>(event_any) {
        return Some(e.seat);
    }
    if let Some(e) = try_unpack::<RebuyRequested>(event_any) {
        return Some(e.seat);
    }
    None
}

fn orch_fee(event_any: &Any) -> Option<i64> {
    if let Some(e) = try_unpack::<RegistrationFeeConfirmed>(event_any) {
        return e.fee.map(|c| c.amount);
    }
    if let Some(e) = try_unpack::<RebuyFeeConfirmed>(event_any) {
        return e.fee.map(|c| c.amount);
    }
    None
}

fn orch_reason(event_any: &Any) -> Option<String> {
    if let Some(e) = try_unpack::<BuyInReservationReleased>(event_any) {
        return Some(e.reason);
    }
    if let Some(e) = try_unpack::<RegistrationFeeReleased>(event_any) {
        return Some(e.reason);
    }
    if let Some(e) = try_unpack::<RebuyFeeReleased>(event_any) {
        return Some(e.reason);
    }
    None
}

fn orch_chips_added(event_any: &Any) -> Option<i64> {
    try_unpack::<RebuyFeeConfirmed>(event_any).map(|e| e.chips_added)
}

#[then(expr = "the orchestration event has amount {int}")]
fn orch_event_has_amount(world: &mut PlayerWorld, expected: i64) {
    let event = world.get_last_event().expect("No event found");
    assert_eq!(orch_amount(event).expect("no amount"), expected);
}

#[then("the orchestration event has a reservation_id")]
fn orch_event_has_reservation_id_present(world: &mut PlayerWorld) {
    let event = world.get_last_event().expect("No event found");
    let rid = orch_reservation_id(event).expect("no reservation_id");
    assert!(!rid.is_empty(), "reservation_id should be non-empty");
}

#[then(expr = "the orchestration event has reservation_id {string}")]
fn orch_event_has_reservation_id(world: &mut PlayerWorld, expected: String) {
    let event = world.get_last_event().expect("No event found");
    let rid = orch_reservation_id(event).expect("no reservation_id");
    assert_eq!(rid, uuid_for(&expected));
}

#[then(expr = "the orchestration event has tournament_root {string}")]
fn orch_event_has_tournament_root(world: &mut PlayerWorld, expected: String) {
    let event = world.get_last_event().expect("No event found");
    let root = orch_tournament_root(event).expect("no tournament_root");
    assert_eq!(root, uuid_for(&expected));
}

#[then(expr = "the orchestration event has table_root {string}")]
fn orch_event_has_table_root(world: &mut PlayerWorld, expected: String) {
    let event = world.get_last_event().expect("No event found");
    let root = orch_table_root(event).expect("no table_root");
    assert_eq!(root, uuid_for(&expected));
}

#[then(expr = "the orchestration event has seat {int}")]
fn orch_event_has_seat(world: &mut PlayerWorld, expected: i32) {
    let event = world.get_last_event().expect("No event found");
    assert_eq!(orch_seat(event).expect("no seat"), expected);
}

#[then(expr = "the orchestration event has fee {int}")]
fn orch_event_has_fee(world: &mut PlayerWorld, expected: i64) {
    let event = world.get_last_event().expect("No event found");
    assert_eq!(orch_fee(event).expect("no fee"), expected);
}

#[then(expr = "the orchestration event has reason {string}")]
fn orch_event_has_reason(world: &mut PlayerWorld, expected: String) {
    let event = world.get_last_event().expect("No event found");
    assert_eq!(orch_reason(event).expect("no reason"), expected);
}

#[then(expr = "the orchestration event has chips_added {int}")]
fn orch_event_has_chips_added(world: &mut PlayerWorld, expected: i64) {
    let event = world.get_last_event().expect("No event found");
    assert_eq!(orch_chips_added(event).expect("no chips_added"), expected);
}

#[then(expr = "the player event has table_root {string}")]
fn player_event_has_table_root(world: &mut PlayerWorld, expected: String) {
    let event_any = world.get_last_event().expect("No event found");
    let table_root = if let Some(e) = try_unpack::<FundsReserved>(event_any) {
        e.table_root
    } else if let Some(e) = try_unpack::<FundsReleased>(event_any) {
        e.table_root
    } else {
        panic!("Unknown event type for table_root: {}", event_any.type_url);
    };
    assert_eq!(table_root, uuid_for(&expected));
}

#[then(expr = "the player state has no pending buy-in {string}")]
fn player_state_has_no_pending_buy_in(world: &mut PlayerWorld, reservation: String) {
    let state = world.last_state.as_ref().expect("No state found");
    let res_hex = hex::encode(uuid_for(&reservation));
    assert!(
        !state.pending_buy_ins.contains_key(&res_hex),
        "pending buy-in {} should be cleared",
        reservation
    );
}

#[then(expr = "the player state has no pending registration {string}")]
fn player_state_has_no_pending_registration(world: &mut PlayerWorld, reservation: String) {
    let state = world.last_state.as_ref().expect("No state found");
    let res_hex = hex::encode(uuid_for(&reservation));
    assert!(
        !state.pending_registrations.contains_key(&res_hex),
        "pending registration {} should be cleared",
        reservation
    );
}

#[then(expr = "the player state has no pending rebuy {string}")]
fn player_state_has_no_pending_rebuy(world: &mut PlayerWorld, reservation: String) {
    let state = world.last_state.as_ref().expect("No state found");
    let res_hex = hex::encode(uuid_for(&reservation));
    assert!(
        !state.pending_rebuys.contains_key(&res_hex),
        "pending rebuy {} should be cleared",
        reservation
    );
}

#[tokio::main]
async fn main() {
    PlayerWorld::run("features/example/unit/player.feature").await;
}
