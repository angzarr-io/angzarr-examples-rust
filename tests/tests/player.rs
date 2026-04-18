//! Player aggregate BDD tests using cucumber-rs against Tier 5 Router API.
//!
//! Handlers now take decoded command structs directly (Tier 5 convention).
//! State is rebuilt by iterating event pages and dispatching to the free
//! `apply_*` functions in `agg_player::state`.

use agg_player::handlers::{
    handle_deposit_funds, handle_register_player, handle_release_funds, handle_reserve_funds,
    handle_withdraw_funds,
};
use agg_player::state::{
    apply_buy_in_confirmed, apply_buy_in_released, apply_buy_in_requested, apply_deposited,
    apply_rebuy_confirmed, apply_rebuy_released, apply_rebuy_requested, apply_registered,
    apply_registration_confirmed, apply_registration_released, apply_registration_requested,
    apply_released, apply_reserved, apply_transferred, apply_withdrawn, PlayerState,
};
use angzarr_client::proto::{event_page, EventBook};
use angzarr_client::{try_unpack, type_matches};
use cucumber::{given, then, when, World};
use examples_proto::{
    BuyInConfirmed, BuyInRequested, BuyInReservationReleased, DepositFunds, FundsDeposited,
    FundsReleased, FundsReserved, FundsTransferred, FundsWithdrawn, PlayerRegistered, PlayerType,
    RebuyFeeConfirmed, RebuyFeeReleased, RebuyRequested, RegisterPlayer, RegistrationFeeConfirmed,
    RegistrationFeeReleased, RegistrationRequested, ReleaseFunds, ReserveFunds, WithdrawFunds,
};
use poker_tests::{currency, uuid_for};
use prost_types::Any;

/// Test context for player scenarios.
#[derive(Debug, Default, World)]
#[world(init = Self::new)]
pub struct PlayerWorld {
    events: Vec<Any>,
    last_error: Option<angzarr_client::CommandRejectedError>,
    last_event_book: Option<EventBook>,
    last_state: Option<PlayerState>,
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

    /// Rebuild player state by dispatching each event to the right applier.
    fn rebuild_state(&self) -> PlayerState {
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
        table_root: uuid_for(&table_name),
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
        table_root: uuid_for(&table_name),
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

#[tokio::main]
async fn main() {
    PlayerWorld::run("features/unit/player.feature").await;
}
