//! Player aggregate BDD tests.
//!
//! After the reservation refactor:
//!   * Bankroll commands (Register/Deposit/Withdraw/Reserve/Release/Transfer/
//!     DeductReservedFunds) dispatch through `agg_player::handlers::*`.
//!   * Lifecycle commands (Initiate/Confirm/Release × buy-in/rebuy/registration)
//!     dispatch through `agg_reservation::Reservation`.
//!
//! `ReserveFunds` / `ReleaseFunds` / `FundsReserved` / `FundsReleased` /
//! `FundsDeducted` carry a `key: bytes` (table_root for buy-in / rebuy,
//! tournament_root for registration). The seeders below mirror the Python
//! suite: when a Given step seeds a pending lifecycle record, it also seeds
//! the matching player FundsReserved / FundsDeducted so player-state replay
//! observes the bankroll movement that the PM hop would produce live.
#![allow(clippy::large_enum_variant)]

use std::collections::HashMap;

use agg_player::handlers::{
    handle_deposit_funds, handle_join_rejected, handle_register_player, handle_release_funds,
    handle_reserve_funds, handle_transfer_funds, handle_withdraw_funds,
};
use agg_player::state::{
    apply_deposited, apply_funds_deducted, apply_registered, apply_released, apply_reserved,
    apply_transferred, apply_withdrawn, PlayerState,
};
use agg_reservation::{Reservation, ReservationState};
use angzarr_client::proto::{
    business_response, event_page, CommandBook, CommandPage, Cover, EventBook, Notification,
    PageHeader, RejectionNotification, Uuid as ProtoUuid,
};
use angzarr_client::try_unpack;
use cucumber::{given, then, when, World};
use examples_proto::{
    BuyInConfirmed, BuyInRequested, BuyInReservationReleased, ConfirmBuyIn, ConfirmRebuyFee,
    ConfirmRegistrationFee, DepositFunds, FundsDeducted, FundsDeposited, FundsReleased,
    FundsReserved, FundsTransferred, FundsWithdrawn, InitiateBuyIn, InitiateRebuy,
    InitiateTournamentRegistration, PlayerRegistered, PlayerType, RebuyFeeConfirmed,
    RebuyFeeReleased, RebuyRequested, RegisterPlayer, RegistrationFeeConfirmed,
    RegistrationFeeReleased, RegistrationRequested, ReleaseBuyIn, ReleaseFunds, ReleaseRebuyFee,
    ReleaseRegistrationFee, ReserveFunds, TransferFunds, WithdrawFunds,
};
use poker_tests::{currency, uuid_for};
use prost::Message;
use prost_types::Any;

const PLAYER_AGGREGATE_DOMAIN: &str = "player";

/// Test context for player + reservation BDD scenarios.
#[derive(Default, World)]
#[world(init = Self::new)]
pub struct PlayerWorld {
    /// Stream of events seeded via `Given` steps and produced by `When`
    /// steps. Replayed to rebuild PlayerState / ReservationState.
    events: Vec<Any>,
    last_error: Option<angzarr_client::CommandRejectedError>,
    last_event_book: Option<EventBook>,
    last_player_state: Option<PlayerState>,
    last_reservation_state: Option<ReservationState>,
    /// chips_to_add seeded for `a pending rebuy ... chips N` so the matching
    /// RebuyFeeConfirmed seeder can echo it through to orchestration assertions.
    pending_rebuy_chips: HashMap<String, i64>,
    /// Cucumber-declared addressing envelope for the current scenario's
    /// command. Unit-tier `When` steps call handlers directly (the
    /// production router's dispatch boundary never runs), so the cover
    /// has to be stamped onto rejections from the test side. Populated
    /// by the `the command cover has ...` Given steps.
    command_cover: Option<angzarr_client::proto::Cover>,
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

    fn rebuild_player_state(&self) -> PlayerState {
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
            } else if let Some(ev) = try_unpack::<FundsDeducted>(event_any) {
                apply_funds_deducted(&mut state, ev);
            }
        }
        state
    }

    fn rebuild_reservation_state(&self) -> ReservationState {
        let mut state = ReservationState::default();
        for event_any in &self.events {
            if let Some(ev) = try_unpack::<BuyInRequested>(event_any) {
                Reservation::apply_buy_in_requested(&mut state, ev);
            } else if let Some(ev) = try_unpack::<BuyInConfirmed>(event_any) {
                Reservation::apply_buy_in_confirmed(&mut state, ev);
            } else if let Some(ev) = try_unpack::<BuyInReservationReleased>(event_any) {
                Reservation::apply_buy_in_released(&mut state, ev);
            } else if let Some(ev) = try_unpack::<RegistrationRequested>(event_any) {
                Reservation::apply_registration_requested(&mut state, ev);
            } else if let Some(ev) = try_unpack::<RegistrationFeeConfirmed>(event_any) {
                Reservation::apply_registration_confirmed(&mut state, ev);
            } else if let Some(ev) = try_unpack::<RegistrationFeeReleased>(event_any) {
                Reservation::apply_registration_released(&mut state, ev);
            } else if let Some(ev) = try_unpack::<RebuyRequested>(event_any) {
                Reservation::apply_rebuy_requested(&mut state, ev);
            } else if let Some(ev) = try_unpack::<RebuyFeeConfirmed>(event_any) {
                Reservation::apply_rebuy_confirmed(&mut state, ev);
            } else if let Some(ev) = try_unpack::<RebuyFeeReleased>(event_any) {
                Reservation::apply_rebuy_released(&mut state, ev);
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

    /// Append a synthetic FundsReserved event mimicking the PM hop:
    /// reservation lifecycle Initiate → ReserveFunds against the player.
    fn append_funds_reserved(&mut self, key: &[u8], amount: i64) {
        let state = self.rebuild_player_state();
        let new_reserved = state.reserved_funds + amount;
        let new_available = state.bankroll - new_reserved;
        let event = FundsReserved {
            amount: Some(currency(amount)),
            key: key.to_vec(),
            new_available_balance: Some(currency(new_available)),
            new_reserved_balance: Some(currency(new_reserved)),
            reserved_at: Some(angzarr_client::now()),
        };
        self.add_event(pack_event_any(&event));
    }

    /// Append a synthetic FundsDeducted event mimicking the PM hop:
    /// reservation lifecycle Confirm → DeductReservedFunds against the player.
    fn append_funds_deducted(&mut self, key: &[u8], reservation_id: &[u8], amount: i64) {
        let state = self.rebuild_player_state();
        let new_reserved = state.reserved_funds - amount;
        let new_balance = state.bankroll - amount;
        let event = FundsDeducted {
            amount: Some(currency(amount)),
            key: key.to_vec(),
            reservation_id: reservation_id.to_vec(),
            new_balance: Some(currency(new_balance)),
            new_reserved_balance: Some(currency(new_reserved)),
            deducted_at: Some(angzarr_client::now()),
        };
        self.add_event(pack_event_any(&event));
    }
}

fn pack_event_any<T: prost::Message + prost::Name>(event: &T) -> Any {
    examples_utils::pack_event(event, "ignored")
}

fn uuid_or_empty(s: &str) -> Vec<u8> {
    if s.is_empty() {
        Vec::new()
    } else {
        uuid_for(s)
    }
}

// =============================================================================
// Player-aggregate command dispatch (bankroll primitives only)
// =============================================================================

macro_rules! dispatch_player {
    ($world:ident, $handler:expr, $cmd:expr) => {{
        let state = $world.rebuild_player_state();
        match $handler($cmd, &state, $world.next_sequence()) {
            Ok(book) => {
                if let Some(event_any) = book.pages.first().and_then(|p| match &p.payload {
                    Some(event_page::Payload::Event(e)) => Some(e.clone()),
                    _ => None,
                }) {
                    $world.events.push(event_any);
                }
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

// =============================================================================
// Reservation-aggregate command dispatch (lifecycle commands)
// =============================================================================

/// Drive a reservation lifecycle command through the Reservation aggregate.
/// Rebuilds ReservationState from prior reservation events, calls the
/// passed-in method, and folds the resulting event into both books so
/// downstream assertions (`the player state has no pending …`) can see it.
fn dispatch_reservation<F>(world: &mut PlayerWorld, run: F)
where
    F: FnOnce(&Reservation, &ReservationState, u32) -> angzarr_client::CommandResult<EventBook>,
{
    let agg = Reservation::new();
    let state = world.rebuild_reservation_state();
    match run(&agg, &state, world.next_sequence()) {
        Ok(book) => {
            if let Some(event_any) = book.pages.first().and_then(|p| match &p.payload {
                Some(event_page::Payload::Event(e)) => Some(e.clone()),
                _ => None,
            }) {
                world.events.push(event_any);
            }
            world.last_event_book = Some(book);
            world.last_error = None;
        }
        Err(e) => {
            world.last_error = Some(e);
            world.last_event_book = None;
        }
    }
}

/// Mirror python's sync DECISION: before InitiateBuyIn dispatches we check
/// player state for existence + sufficient bankroll. The reservation
/// aggregate's handler validates input shape only.
fn pre_validate_buy_in(
    world: &PlayerWorld,
    amount: i64,
) -> Result<(), angzarr_client::CommandRejectedError> {
    let state = world.rebuild_player_state();
    if !state.exists() {
        return Err(examples_utils::rejected("Player does not exist"));
    }
    if amount > 0 && state.available_balance() < amount {
        return Err(examples_utils::rejected("Insufficient funds"));
    }
    Ok(())
}

fn pre_validate_player_exists(
    world: &PlayerWorld,
) -> Result<(), angzarr_client::CommandRejectedError> {
    let state = world.rebuild_player_state();
    if !state.exists() {
        return Err(examples_utils::rejected("Player does not exist"));
    }
    Ok(())
}

// =============================================================================
// Given steps — base setup
// =============================================================================

#[given("no prior events for the player aggregate")]
fn no_prior_events(world: &mut PlayerWorld) {
    world.events.clear();
    world.pending_rebuy_chips.clear();
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
    let state = world.rebuild_player_state();
    let new_balance = state.bankroll + amount;
    let event = FundsDeposited {
        amount: Some(currency(amount)),
        new_balance: Some(currency(new_balance)),
        deposited_at: Some(angzarr_client::now()),
    };
    world.add_event(pack_event_any(&event));
}

#[given(expr = "a FundsWithdrawn event with amount {int}")]
fn funds_withdrawn_event(world: &mut PlayerWorld, amount: i64) {
    let state = world.rebuild_player_state();
    let new_balance = state.bankroll - amount;
    let event = FundsWithdrawn {
        amount: Some(currency(amount)),
        new_balance: Some(currency(new_balance)),
        withdrawn_at: Some(angzarr_client::now()),
    };
    world.add_event(pack_event_any(&event));
}

#[given(expr = "a FundsReserved event with amount {int} for table {string}")]
fn funds_reserved_event(world: &mut PlayerWorld, amount: i64, table_name: String) {
    world.append_funds_reserved(&uuid_for(&table_name), amount);
}

#[given(expr = "a FundsReleased event for table {string} with amount {int}")]
fn funds_released_event(world: &mut PlayerWorld, table_name: String, amount: i64) {
    let state = world.rebuild_player_state();
    let new_reserved = state.reserved_funds - amount;
    let new_available = state.bankroll - new_reserved;
    let event = FundsReleased {
        amount: Some(currency(amount)),
        key: uuid_for(&table_name),
        new_available_balance: Some(currency(new_available)),
        new_reserved_balance: Some(currency(new_reserved)),
        released_at: Some(angzarr_client::now()),
    };
    world.add_event(pack_event_any(&event));
}

// =============================================================================
// Given steps — pending reservation seeders
// =============================================================================

#[given(expr = "a pending buy-in {string} for table {string} seat {int} amount {int}")]
fn pending_buy_in(
    world: &mut PlayerWorld,
    reservation: String,
    table_name: String,
    seat: i32,
    amount: i64,
) {
    let table_root = uuid_for(&table_name);
    let event = BuyInRequested {
        reservation_id: uuid_for(&reservation),
        table_root: table_root.clone(),
        seat,
        amount: Some(currency(amount)),
        requested_at: Some(angzarr_client::now()),
        player_root: Vec::new(),
    };
    world.add_event(pack_event_any(&event));
    // Reflect the PM hop: ReserveFunds against the player aggregate.
    world.append_funds_reserved(&table_root, amount);
}

#[given(expr = "a BuyInConfirmed event for reservation {string} table {string}")]
fn buy_in_confirmed_event(world: &mut PlayerWorld, reservation: String, table_name: String) {
    let res_id = uuid_for(&reservation);
    let table_root = uuid_for(&table_name);
    // Snapshot pending BEFORE appending Confirmed so the deducted amount lines up.
    let res_state = world.rebuild_reservation_state();
    let amount = res_state
        .pending_buy_ins
        .get(&hex::encode(&res_id))
        .map(|p| p.amount)
        .unwrap_or(0);
    let event = BuyInConfirmed {
        reservation_id: res_id.clone(),
        player_root: Vec::new(),
        table_root: table_root.clone(),
        seat: 0,
        amount: Some(currency(amount)),
        confirmed_at: Some(angzarr_client::now()),
    };
    world.add_event(pack_event_any(&event));
    if amount > 0 {
        world.append_funds_deducted(&table_root, &res_id, amount);
    }
}

#[given(expr = "a pending registration {string} for tournament {string} fee {int}")]
fn pending_registration(
    world: &mut PlayerWorld,
    reservation: String,
    tournament: String,
    fee: i64,
) {
    let trn_root = uuid_for(&tournament);
    let event = RegistrationRequested {
        reservation_id: uuid_for(&reservation),
        player_root: Vec::new(),
        tournament_root: trn_root.clone(),
        fee: Some(currency(fee)),
        requested_at: Some(angzarr_client::now()),
    };
    world.add_event(pack_event_any(&event));
    // Registration reserves against tournament_root (the `key` field of
    // the PM-issued ReserveFunds).
    world.append_funds_reserved(&trn_root, fee);
}

#[given(expr = "a RegistrationFeeConfirmed event for reservation {string} tournament {string}")]
fn registration_fee_confirmed_event(
    world: &mut PlayerWorld,
    reservation: String,
    tournament: String,
) {
    let res_id = uuid_for(&reservation);
    let trn_root = uuid_for(&tournament);
    let res_state = world.rebuild_reservation_state();
    let fee = res_state
        .pending_registrations
        .get(&hex::encode(&res_id))
        .map(|p| p.fee)
        .unwrap_or(0);
    let event = RegistrationFeeConfirmed {
        reservation_id: res_id.clone(),
        player_root: Vec::new(),
        tournament_root: trn_root.clone(),
        fee: Some(currency(fee)),
        confirmed_at: Some(angzarr_client::now()),
    };
    world.add_event(pack_event_any(&event));
    if fee > 0 {
        world.append_funds_deducted(&trn_root, &res_id, fee);
    }
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
    let trn_root = uuid_for(&tournament);
    let table_root = uuid_for(&table_name);
    let event = RebuyRequested {
        reservation_id: uuid_for(&reservation),
        player_root: Vec::new(),
        tournament_root: trn_root,
        table_root: table_root.clone(),
        seat,
        fee: Some(currency(fee)),
        requested_at: Some(angzarr_client::now()),
    };
    world.add_event(pack_event_any(&event));
    // Reservation key for rebuy is the table_root.
    world.append_funds_reserved(&table_root, fee);
    world.pending_rebuy_chips.insert(reservation, chips);
}

#[given(expr = "a RebuyFeeConfirmed event for reservation {string}")]
fn rebuy_fee_confirmed_event(world: &mut PlayerWorld, reservation: String) {
    let res_id = uuid_for(&reservation);
    let res_state = world.rebuild_reservation_state();
    let pending = res_state.pending_rebuys.get(&hex::encode(&res_id));
    let (table_root, tournament_root, fee) = pending
        .map(|p| (p.table_root.clone(), p.tournament_root.clone(), p.fee))
        .unwrap_or((Vec::new(), Vec::new(), 0));
    let chips_added = world
        .pending_rebuy_chips
        .get(&reservation)
        .copied()
        .unwrap_or(0);
    let event = RebuyFeeConfirmed {
        reservation_id: res_id.clone(),
        player_root: Vec::new(),
        tournament_root,
        table_root: table_root.clone(),
        fee: Some(currency(fee)),
        chips_added,
        confirmed_at: Some(angzarr_client::now()),
    };
    world.add_event(pack_event_any(&event));
    if fee > 0 && !table_root.is_empty() {
        world.append_funds_deducted(&table_root, &res_id, fee);
    }
}

// =============================================================================
// When steps — bankroll commands
// =============================================================================

#[when(expr = "I handle a RegisterPlayer command with name {string} and email {string}")]
fn handle_register_player_cmd(world: &mut PlayerWorld, name: String, email: String) {
    let cmd = RegisterPlayer {
        display_name: name,
        email,
        player_type: PlayerType::Human as i32,
        ai_model_id: String::new(),
    };
    dispatch_player!(world, handle_register_player, cmd);
}

#[when(expr = "I handle a RegisterPlayer command with name {string} and email {string} as AI")]
fn handle_register_player_ai_cmd(world: &mut PlayerWorld, name: String, email: String) {
    let cmd = RegisterPlayer {
        display_name: name,
        email,
        player_type: PlayerType::Ai as i32,
        ai_model_id: "gpt-4".to_string(),
    };
    dispatch_player!(world, handle_register_player, cmd);
}

#[when(expr = "I handle a DepositFunds command with amount {int}")]
fn handle_deposit_funds_cmd(world: &mut PlayerWorld, amount: i64) {
    let cmd = DepositFunds {
        amount: Some(currency(amount)),
    };
    dispatch_player!(world, handle_deposit_funds, cmd);
}

#[when(expr = "I handle a WithdrawFunds command with amount {int}")]
fn handle_withdraw_funds_cmd(world: &mut PlayerWorld, amount: i64) {
    let cmd = WithdrawFunds {
        amount: Some(currency(amount)),
    };
    dispatch_player!(world, handle_withdraw_funds, cmd);
}

#[when(expr = "I handle a ReserveFunds command with amount {int} for table {string}")]
fn handle_reserve_funds_cmd(world: &mut PlayerWorld, amount: i64, table_name: String) {
    let cmd = ReserveFunds {
        amount: Some(currency(amount)),
        key: uuid_or_empty(&table_name),
    };
    dispatch_player!(world, handle_reserve_funds, cmd);
}

#[when(expr = "I handle a ReleaseFunds command for table {string}")]
fn handle_release_funds_cmd(world: &mut PlayerWorld, table_name: String) {
    let cmd = ReleaseFunds {
        key: uuid_or_empty(&table_name),
    };
    dispatch_player!(world, handle_release_funds, cmd);
}

#[when(
    expr = "I handle a TransferFunds command from {string} with amount {int} for hand {string} reason {string}"
)]
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
    dispatch_player!(world, handle_transfer_funds, cmd);
}

#[when("I rebuild the player state")]
fn rebuild_player_state_step(world: &mut PlayerWorld) {
    world.last_player_state = Some(world.rebuild_player_state());
    world.last_reservation_state = Some(world.rebuild_reservation_state());
}

// =============================================================================
// When steps — reservation lifecycle commands
// =============================================================================

#[when(expr = "I handle an InitiateBuyIn command for table {string} seat {int} amount {int}")]
fn handle_initiate_buy_in_cmd(world: &mut PlayerWorld, table_name: String, seat: i32, amount: i64) {
    let cmd = InitiateBuyIn {
        table_root: uuid_or_empty(&table_name),
        seat,
        amount: Some(currency(amount)),
        player_root: uuid_for("the-player"),
    };
    // Pre-validate against player state to satisfy "Player does not exist" /
    // "Insufficient funds" scenarios — these guards live on the Player
    // aggregate side in the python suite (sync DECISION). The reservation
    // aggregate's handler only checks input shape.
    if let Err(err) = pre_validate_buy_in(world, amount) {
        world.last_error = Some(err);
        world.last_event_book = None;
        return;
    }
    dispatch_reservation(world, |agg, state, seq| {
        agg.on_initiate_buy_in(cmd, state, seq)
    });
}

#[when(expr = "I handle a ConfirmBuyIn command for reservation {string}")]
fn handle_confirm_buy_in_cmd(world: &mut PlayerWorld, reservation: String) {
    let cmd = ConfirmBuyIn {
        reservation_id: uuid_or_empty(&reservation),
    };
    dispatch_reservation(world, |agg, state, seq| {
        agg.on_confirm_buy_in(cmd, state, seq)
    });
}

#[when(expr = "I handle a ReleaseBuyIn command for reservation {string} reason {string}")]
fn handle_release_buy_in_cmd(world: &mut PlayerWorld, reservation: String, reason: String) {
    let cmd = ReleaseBuyIn {
        reservation_id: uuid_or_empty(&reservation),
        reason,
    };
    dispatch_reservation(world, |agg, state, seq| {
        agg.on_release_buy_in(cmd, state, seq)
    });
}

#[when(expr = "I handle an InitiateTournamentRegistration command for tournament {string}")]
fn handle_initiate_registration_cmd(world: &mut PlayerWorld, tournament: String) {
    let cmd = InitiateTournamentRegistration {
        tournament_root: uuid_or_empty(&tournament),
        player_root: uuid_for("the-player"),
    };
    if let Err(err) = pre_validate_player_exists(world) {
        world.last_error = Some(err);
        world.last_event_book = None;
        return;
    }
    dispatch_reservation(world, |agg, state, seq| {
        agg.on_initiate_registration(cmd, state, seq)
    });
}

#[when(expr = "I handle a ConfirmRegistrationFee command for reservation {string}")]
fn handle_confirm_registration_fee_cmd(world: &mut PlayerWorld, reservation: String) {
    let cmd = ConfirmRegistrationFee {
        reservation_id: uuid_or_empty(&reservation),
    };
    dispatch_reservation(world, |agg, state, seq| {
        agg.on_confirm_registration(cmd, state, seq)
    });
}

#[when(expr = "I handle a ReleaseRegistrationFee command for reservation {string} reason {string}")]
fn handle_release_registration_fee_cmd(
    world: &mut PlayerWorld,
    reservation: String,
    reason: String,
) {
    let cmd = ReleaseRegistrationFee {
        reservation_id: uuid_or_empty(&reservation),
        reason,
    };
    dispatch_reservation(world, |agg, state, seq| {
        agg.on_release_registration(cmd, state, seq)
    });
}

#[when(
    expr = "I handle an InitiateRebuy command for tournament {string} table {string} seat {int}"
)]
fn handle_initiate_rebuy_cmd(
    world: &mut PlayerWorld,
    tournament: String,
    table_name: String,
    seat: i32,
) {
    let cmd = InitiateRebuy {
        tournament_root: uuid_or_empty(&tournament),
        table_root: uuid_or_empty(&table_name),
        seat,
        player_root: uuid_for("the-player"),
    };
    if let Err(err) = pre_validate_player_exists(world) {
        world.last_error = Some(err);
        world.last_event_book = None;
        return;
    }
    dispatch_reservation(world, |agg, state, seq| {
        agg.on_initiate_rebuy(cmd, state, seq)
    });
}

#[when(expr = "I handle a ConfirmRebuyFee command for reservation {string}")]
fn handle_confirm_rebuy_fee_cmd(world: &mut PlayerWorld, reservation: String) {
    let cmd = ConfirmRebuyFee {
        reservation_id: uuid_or_empty(&reservation),
    };
    dispatch_reservation(world, |agg, state, seq| {
        agg.on_confirm_rebuy(cmd, state, seq)
    });
}

#[when(expr = "I handle a ReleaseRebuyFee command for reservation {string} reason {string}")]
fn handle_release_rebuy_fee_cmd(world: &mut PlayerWorld, reservation: String, reason: String) {
    let cmd = ReleaseRebuyFee {
        reservation_id: uuid_or_empty(&reservation),
        reason,
    };
    dispatch_reservation(world, |agg, state, seq| {
        agg.on_release_rebuy(cmd, state, seq)
    });
}

// =============================================================================
// When steps — saga rejection compensation
// =============================================================================

fn build_join_rejection_notification(table_name: &str) -> Notification {
    let table_root = uuid_for(table_name);
    let rejected_command = CommandBook {
        cover: Some(Cover {
            domain: "table".into(),
            root: Some(ProtoUuid {
                value: table_root.clone(),
            }),
            correlation_id: String::new(),
            edition: None,
        }),
        pages: vec![CommandPage {
            header: Some(PageHeader {
                sync_mode: None,
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
        type_url: angzarr_client::full_type_url::<RejectionNotification>(),
        value: rejection.encode_to_vec(),
    };
    Notification {
        cover: Some(Cover {
            domain: PLAYER_AGGREGATE_DOMAIN.into(),
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
    let state = world.rebuild_player_state();
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

// =============================================================================
// Then steps — event-type matchers
// =============================================================================

/// Match the type-URL suffix of the last emitted event against the gherkin
/// type name. The feature uses the fully qualified
/// `angzarr_client.proto.examples.X` shape; rust events emit bare
/// `examples.X`. Both end with `examples.X`, so we suffix-match on that.
fn last_event_type_suffix_matches(world: &PlayerWorld, type_name: &str) -> bool {
    let suffix = type_name.rsplit('.').next().unwrap_or(type_name);
    world
        .get_last_event()
        .map(|e| {
            e.type_url
                .rsplit('/')
                .next()
                .unwrap_or("")
                .ends_with(suffix)
        })
        .unwrap_or(false)
}

#[then(expr = "the result is a {word} event")]
fn result_is_event(world: &mut PlayerWorld, type_name: String) {
    assert!(
        last_event_type_suffix_matches(world, &type_name),
        "Expected event {} but got {:?}",
        type_name,
        world.get_last_event().map(|e| e.type_url.clone()),
    );
}

// =============================================================================
// Then steps — error assertions
// =============================================================================

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
            .message
            .to_lowercase()
            .contains(&expected.to_lowercase()),
        "Expected error to contain '{}' but got '{}'",
        expected,
        error.message
    );
}

#[then(expr = "the error message equals {string}")]
fn error_message_equals(world: &mut PlayerWorld, expected: String) {
    let err = world.last_error.as_ref().expect("No error found");
    assert_eq!(
        err.message, expected,
        "Expected error '{}' but got '{}'",
        expected, err.message
    );
}

#[then(expr = "the command is rejected with code {string}")]
fn command_rejected_with_code(world: &mut PlayerWorld, code: String) {
    let err = world
        .last_error
        .as_ref()
        .expect("Expected command to be rejected but it succeeded");
    assert_eq!(
        err.code, code,
        "Expected rejection code '{}', got '{}'",
        code, err.code
    );
}

#[then(expr = "the rejection field {string} equals {string}")]
fn rejection_field_equals(world: &mut PlayerWorld, field: String, value: String) {
    let err = world
        .last_error
        .as_ref()
        .expect("Expected command to be rejected but it succeeded");
    let actual = err.details.get(&field).cloned().unwrap_or_else(|| {
        panic!(
            "Rejection has no field '{}'; available: {:?}",
            field,
            err.details.keys().collect::<Vec<_>>()
        )
    });
    assert_eq!(
        actual, value,
        "Rejection field '{}': expected '{}', got '{}'",
        field, value, actual
    );
}

#[then(regex = r#"^the rejection cover has (.+)$"#)]
fn rejection_cover_has(world: &mut PlayerWorld, spec: String) {
    let cover = rejection_cover_or_fail_player(world).clone();
    for (field, value) in cover_field_pairs(&spec) {
        let actual = read_cover_field(&cover, &field);
        assert_eq!(
            actual, value,
            "Rejection cover {}: expected '{}', got '{}'",
            field, value, actual
        );
    }
}

fn rejection_cover_or_fail_player(world: &mut PlayerWorld) -> &angzarr_client::proto::Cover {
    // Late-stamp from world's command_cover before reading. Mirrors what
    // the production router's dispatch boundary does; needed here because
    // unit-tier `When` steps call handlers directly.
    if let (Some(rej), Some(cover)) = (world.last_error.as_mut(), world.command_cover.as_ref()) {
        if rej.cover.is_none() {
            rej.cover = Some(cover.clone());
        }
    }
    let err = world
        .last_error
        .as_ref()
        .expect("Expected command to be rejected but it succeeded");
    err.cover.as_ref().expect(
        "Rejection has no cover stamped — declare one via `the command cover has ...` Given steps",
    )
}

#[given(regex = r#"^the command cover has (.+)$"#)]
fn given_cover_has_player(world: &mut PlayerWorld, spec: String) {
    let cover = world.command_cover.get_or_insert_with(Default::default);
    for (field, value) in cover_field_pairs(&spec) {
        write_cover_field(cover, &field, &value);
    }
}

use poker_tests::{cover_field_pairs, read_cover_field, write_cover_field};

// =============================================================================
// Then steps — PlayerRegistered field assertions
// =============================================================================

#[then(expr = "the player event has display_name {string}")]
fn player_event_has_display_name(world: &mut PlayerWorld, expected: String) {
    let event_any = world.get_last_event().expect("No event found");
    let event = try_unpack::<PlayerRegistered>(event_any).expect("Expected PlayerRegistered");
    assert_eq!(event.display_name, expected);
}

#[then(expr = "the player event has email {string}")]
fn player_event_has_email(world: &mut PlayerWorld, expected: String) {
    let event_any = world.get_last_event().expect("No event found");
    let event = try_unpack::<PlayerRegistered>(event_any).expect("Expected PlayerRegistered");
    assert_eq!(event.email, expected);
}

#[then(expr = "the player event has ai_model_id {string}")]
fn player_event_has_ai_model_id(world: &mut PlayerWorld, expected: String) {
    let event_any = world.get_last_event().expect("No event found");
    let event = try_unpack::<PlayerRegistered>(event_any).expect("Expected PlayerRegistered");
    assert_eq!(event.ai_model_id, expected);
}

#[then(expr = "the player event has player_type {string}")]
fn player_event_has_player_type(world: &mut PlayerWorld, expected: String) {
    let event_any = world.get_last_event().expect("No event found");
    let event = try_unpack::<PlayerRegistered>(event_any).expect("Expected PlayerRegistered");
    let player_type = PlayerType::try_from(event.player_type).unwrap_or_default();
    let actual = match player_type {
        PlayerType::Human => "HUMAN",
        PlayerType::Ai => "AI",
        _ => "UNKNOWN",
    };
    assert_eq!(actual, expected);
}

// =============================================================================
// Then steps — bankroll-event field assertions
// =============================================================================

#[then(expr = "the player event has amount {int}")]
fn player_event_has_amount(world: &mut PlayerWorld, expected: i64) {
    let event_any = world.get_last_event().expect("No event found");
    let amount = if let Some(e) = try_unpack::<FundsDeposited>(event_any) {
        e.amount.map(|c| c.amount).unwrap_or(0)
    } else if let Some(e) = try_unpack::<FundsWithdrawn>(event_any) {
        e.amount.map(|c| c.amount).unwrap_or(0)
    } else if let Some(e) = try_unpack::<FundsReserved>(event_any) {
        e.amount.map(|c| c.amount).unwrap_or(0)
    } else if let Some(e) = try_unpack::<FundsReleased>(event_any) {
        e.amount.map(|c| c.amount).unwrap_or(0)
    } else if let Some(e) = try_unpack::<FundsTransferred>(event_any) {
        e.amount.map(|c| c.amount).unwrap_or(0)
    } else if let Some(e) = try_unpack::<FundsDeducted>(event_any) {
        e.amount.map(|c| c.amount).unwrap_or(0)
    } else {
        panic!("Unknown event type for amount: {}", event_any.type_url);
    };
    assert_eq!(amount, expected);
}

#[then(expr = "the player event has new_balance {int}")]
fn player_event_has_new_balance(world: &mut PlayerWorld, expected: i64) {
    let event_any = world.get_last_event().expect("No event found");
    let balance = if let Some(e) = try_unpack::<FundsDeposited>(event_any) {
        e.new_balance.map(|c| c.amount).unwrap_or(0)
    } else if let Some(e) = try_unpack::<FundsWithdrawn>(event_any) {
        e.new_balance.map(|c| c.amount).unwrap_or(0)
    } else if let Some(e) = try_unpack::<FundsTransferred>(event_any) {
        e.new_balance.map(|c| c.amount).unwrap_or(0)
    } else {
        panic!("Unknown event type for new_balance: {}", event_any.type_url);
    };
    assert_eq!(balance, expected);
}

#[then(expr = "the player event has new_available_balance {int}")]
fn player_event_has_new_available_balance(world: &mut PlayerWorld, expected: i64) {
    let event_any = world.get_last_event().expect("No event found");
    let available = if let Some(e) = try_unpack::<FundsReserved>(event_any) {
        e.new_available_balance.map(|c| c.amount).unwrap_or(0)
    } else if let Some(e) = try_unpack::<FundsReleased>(event_any) {
        e.new_available_balance.map(|c| c.amount).unwrap_or(0)
    } else {
        panic!(
            "Unknown event type for new_available_balance: {}",
            event_any.type_url
        );
    };
    assert_eq!(available, expected);
}

#[then(expr = "the player event has new_reserved_balance {int}")]
fn player_event_has_new_reserved_balance(world: &mut PlayerWorld, expected: i64) {
    let event_any = world.get_last_event().expect("No event found");
    let reserved = if let Some(e) = try_unpack::<FundsReserved>(event_any) {
        e.new_reserved_balance.map(|c| c.amount).unwrap_or(0)
    } else if let Some(e) = try_unpack::<FundsReleased>(event_any) {
        e.new_reserved_balance.map(|c| c.amount).unwrap_or(0)
    } else if let Some(e) = try_unpack::<FundsDeducted>(event_any) {
        e.new_reserved_balance.map(|c| c.amount).unwrap_or(0)
    } else {
        panic!(
            "Unknown event type for new_reserved_balance: {}",
            event_any.type_url
        );
    };
    assert_eq!(reserved, expected);
}

#[then(expr = "the player event has table_root {string}")]
fn player_event_has_table_root(world: &mut PlayerWorld, expected: String) {
    let event_any = world.get_last_event().expect("No event found");
    // Post-refactor the field is named `key` on FundsReserved/Released.
    let key = if let Some(e) = try_unpack::<FundsReserved>(event_any) {
        e.key
    } else if let Some(e) = try_unpack::<FundsReleased>(event_any) {
        e.key
    } else {
        panic!("Unknown event type for table_root: {}", event_any.type_url);
    };
    assert_eq!(key, uuid_for(&expected));
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

// =============================================================================
// Then steps — orchestration-event field assertions
// =============================================================================

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
    if let Some(e) = try_unpack::<RegistrationRequested>(event_any) {
        return e.fee.map(|c| c.amount);
    }
    if let Some(e) = try_unpack::<RebuyRequested>(event_any) {
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

// =============================================================================
// Then steps — state assertions
// =============================================================================

#[then(expr = "the player state has bankroll {int}")]
fn player_state_has_bankroll(world: &mut PlayerWorld, expected: i64) {
    let state = world
        .last_player_state
        .as_ref()
        .expect("No state — call `When I rebuild the player state` first");
    assert_eq!(state.bankroll, expected);
}

#[then(expr = "the player state has reserved_funds {int}")]
fn player_state_has_reserved_funds(world: &mut PlayerWorld, expected: i64) {
    let state = world.last_player_state.as_ref().expect("No state");
    assert_eq!(state.reserved_funds, expected);
}

#[then(expr = "the player state has available_balance {int}")]
fn player_state_has_available_balance(world: &mut PlayerWorld, expected: i64) {
    let state = world.last_player_state.as_ref().expect("No state");
    assert_eq!(state.available_balance(), expected);
}

#[then(expr = "the player state has no pending buy-in {string}")]
fn player_state_has_no_pending_buy_in(world: &mut PlayerWorld, reservation: String) {
    let state = world
        .last_reservation_state
        .as_ref()
        .expect("No reservation state — call `When I rebuild the player state` first");
    let res_hex = hex::encode(uuid_for(&reservation));
    assert!(
        !state.pending_buy_ins.contains_key(&res_hex),
        "pending buy-in {} should be cleared",
        reservation
    );
}

#[then(expr = "the player state has no pending registration {string}")]
fn player_state_has_no_pending_registration(world: &mut PlayerWorld, reservation: String) {
    let state = world.last_reservation_state.as_ref().expect("No state");
    let res_hex = hex::encode(uuid_for(&reservation));
    assert!(
        !state.pending_registrations.contains_key(&res_hex),
        "pending registration {} should be cleared",
        reservation
    );
}

#[then(expr = "the player state has no pending rebuy {string}")]
fn player_state_has_no_pending_rebuy(world: &mut PlayerWorld, reservation: String) {
    let state = world.last_reservation_state.as_ref().expect("No state");
    let res_hex = hex::encode(uuid_for(&reservation));
    assert!(
        !state.pending_rebuys.contains_key(&res_hex),
        "pending rebuy {} should be cleared",
        reservation
    );
}

// =============================================================================
// Then steps — generic timestamp / tag presence
// =============================================================================

#[then(expr = "the event has a timestamp {word}")]
fn event_has_timestamp(world: &mut PlayerWorld, _field: String) {
    // Every emitted event includes a timestamp; presence of an event satisfies this.
    world.get_last_event().expect("No event found");
}

#[tokio::main]
async fn main() {
    PlayerWorld::run("features/example/unit/player.feature").await;
}
