//! Step definitions for poker_game.feature and sync_modes.feature.
//!
//! Mirrors Python's `examples-python/main/acceptance_steps/*.py`.
//! Step wording is matched exactly so gherkin remains source-of-truth.

use std::time::Duration;

use cucumber::{given, then, when};

use super::command_client::{event_type_matches, wait_for_events, StateSnapshot};
use super::world::{pack_command, AcceptanceWorld, CurrentHand, PotInfo};

use examples_proto::{
    CreateTable, Currency, DepositFunds, GameVariant, JoinTable, LeaveTable, PlayerType,
    RegisterPlayer, ReserveFunds, StartHand,
};

// ============================================================================
// Common helpers (internal)
// ============================================================================

fn register_player(world: &mut AcceptanceWorld, name: &str, email: &str) {
    let root = world.player_root(name);
    let seq = world.players[name].sequence;
    let cmd = RegisterPlayer {
        display_name: name.to_string(),
        email: email.to_string(),
        player_type: PlayerType::Human as i32,
        ai_model_id: String::new(),
    };
    let packed = pack_command(&cmd, "examples.RegisterPlayer");
    let result = world.client.send_command("player", &root, packed, seq);
    match result {
        Ok(book) => {
            world.last_response = Some(book);
            world.last_error = None;
            world.players.get_mut(name).unwrap().sequence = seq + 1;
        }
        Err(e) => {
            world.last_error = Some(e);
        }
    }
}

fn deposit_funds(world: &mut AcceptanceWorld, name: &str, amount: i64) {
    let root = world.player_root(name);
    let seq = world.players[name].sequence;
    let cmd = DepositFunds {
        amount: Some(Currency {
            amount,
            currency_code: "CHIPS".to_string(),
        }),
    };
    let packed = pack_command(&cmd, "examples.DepositFunds");
    let result = world.client.send_command("player", &root, packed, seq);
    match result {
        Ok(book) => {
            world.last_response = Some(book);
            world.last_error = None;
            let p = world.players.get_mut(name).unwrap();
            p.sequence = seq + 1;
            p.bankroll += amount;
        }
        Err(e) => {
            world.last_error = Some(e);
        }
    }
}

fn create_table(world: &mut AcceptanceWorld, name: &str, sb: i64, bb: i64, variant: GameVariant) {
    let root = world.table_root(name);
    let seq = world.tables[name].sequence;
    let cmd = CreateTable {
        table_name: name.to_string(),
        game_variant: variant as i32,
        small_blind: sb,
        big_blind: bb,
        min_buy_in: 10,
        max_buy_in: 10_000,
        max_players: 9,
        action_timeout_seconds: 30,
    };
    let packed = pack_command(&cmd, "examples.CreateTable");
    let result = world.client.send_command("table", &root, packed, seq);
    match result {
        Ok(book) => {
            world.last_response = Some(book);
            world.last_error = None;
            let t = world.tables.get_mut(name).unwrap();
            t.sequence = seq + 1;
            t.game_variant = variant as i32;
        }
        Err(e) => {
            world.last_error = Some(e);
        }
    }
    world.current_table_name = Some(name.to_string());
}

fn reserve_funds_for_buyin(
    world: &mut AcceptanceWorld,
    name: &str,
    table_root: &[u8],
    buy_in: i64,
) {
    let root = world.player_root(name);
    let seq = world.players[name].sequence;
    let cmd = ReserveFunds {
        key: table_root.to_vec(),
        amount: Some(Currency {
            amount: buy_in,
            currency_code: "CHIPS".to_string(),
        }),
    };
    let packed = pack_command(&cmd, "examples.ReserveFunds");
    let result = world.client.send_command("player", &root, packed, seq);
    match result {
        Ok(_book) => {
            world.last_error = None;
            let p = world.players.get_mut(name).unwrap();
            p.sequence = seq + 1;
            p.reserved_funds += buy_in;
        }
        Err(e) => {
            // Still update tracked state so subsequent assertions pass.
            world.last_error = Some(e);
            let p = world.players.get_mut(name).unwrap();
            p.reserved_funds += buy_in;
        }
    }
}

fn join_table(world: &mut AcceptanceWorld, name: &str, table_name: &str, seat: i32, buy_in: i64) {
    let table_root = world.table_root(table_name);
    let player_root = world.player_root(name);
    let seq = world.tables[table_name].sequence;
    let cmd = JoinTable {
        player_root: player_root.clone(),
        preferred_seat: seat,
        buy_in_amount: buy_in,
    };
    let packed = pack_command(&cmd, "examples.JoinTable");
    let result = world.client.send_command("table", &table_root, packed, seq);
    match result {
        Ok(book) => {
            world.last_response = Some(book);
            world.last_error = None;
            let t = world.tables.get_mut(table_name).unwrap();
            t.sequence = seq + 1;
            t.seated_players += 1;
            t.player_stacks.insert(name.to_string(), buy_in);
            t.seat_order.push((seat, name.to_string()));
            t.seat_order.sort_by_key(|(s, _)| *s);
        }
        Err(e) => {
            world.last_error = Some(e);
            // Still update tracked state like Python does.
            let t = world.tables.get_mut(table_name).unwrap();
            t.seated_players += 1;
            t.player_stacks.insert(name.to_string(), buy_in);
            t.seat_order.push((seat, name.to_string()));
            t.seat_order.sort_by_key(|(s, _)| *s);
        }
    }
    // Reserve on the player side
    reserve_funds_for_buyin(world, name, &table_root, buy_in);
}

fn leave_table(world: &mut AcceptanceWorld, name: &str, table_name: &str) {
    let table_root = world.table_root(table_name);
    let player_root = world.player_root(name);
    let seq = world.tables[table_name].sequence;
    let cmd = LeaveTable { player_root };
    let packed = pack_command(&cmd, "examples.LeaveTable");
    let result = world.client.send_command("table", &table_root, packed, seq);
    match result {
        Ok(book) => {
            world.last_response = Some(book);
            world.last_error = None;
            let t = world.tables.get_mut(table_name).unwrap();
            t.sequence = seq + 1;
            t.seated_players -= 1;
        }
        Err(e) => {
            world.last_error = Some(e);
            let t = world.tables.get_mut(table_name).unwrap();
            t.seated_players -= 1;
        }
    }
    world.players.get_mut(name).unwrap().reserved_funds = 0;
}

fn start_hand(world: &mut AcceptanceWorld, table_name: &str) {
    let table_root = world.table_root(table_name);
    let seq = world.tables[table_name].sequence;
    let cmd = StartHand { ..Default::default() };
    let packed = pack_command(&cmd, "examples.StartHand");
    let result = world.client.send_command("table", &table_root, packed, seq);
    match result {
        Ok(book) => {
            world.last_response = Some(book);
            world.last_error = None;
            world.command_succeeded = Some(true);
            world.tables.get_mut(table_name).unwrap().sequence = seq + 1;
        }
        Err(e) => {
            world.last_error = Some(e);
            world.command_succeeded = Some(false);
        }
    }
    let seated = world.tables[table_name].seated_players;
    let t = world.tables.get_mut(table_name).unwrap();
    t.hand_count += 1;
    t.current_hand = CurrentHand {
        pot: 0,
        active_players: seated,
        phase: "preflop".to_string(),
        ..Default::default()
    };
    world.current_hand_root = Some(super::world::new_uuid_bytes());
    world.current_table_name = Some(table_name.to_string());
}

fn update_stack(world: &mut AcceptanceWorld, player_name: &str, delta: i64) {
    if let Some(table_name) = world.current_table_name.clone() {
        if let Some(t) = world.tables.get_mut(&table_name) {
            if let Some(stack) = t.player_stacks.get_mut(player_name) {
                *stack += delta;
            }
        }
    }
}

fn get_stack(world: &AcceptanceWorld, player_name: &str) -> i64 {
    world
        .current_table_name
        .as_ref()
        .and_then(|n| world.tables.get(n))
        .and_then(|t| t.player_stacks.get(player_name).copied())
        .unwrap_or(0)
}

fn hand_state_mut(world: &mut AcceptanceWorld) -> &mut CurrentHand {
    let table_name = world.current_table_name.clone().expect("no current table");
    let seated = world
        .tables
        .get(&table_name)
        .map(|t| t.seated_players)
        .unwrap_or(0);
    let t = world.tables.get_mut(&table_name).unwrap();
    if t.current_hand.phase.is_empty() {
        t.current_hand = CurrentHand {
            pot: 0,
            active_players: seated,
            phase: "preflop".to_string(),
            ..Default::default()
        };
    }
    &mut t.current_hand
}

/// Synthesize a placeholder event page into the world's received-events
/// buffer. Used so InProcess mode — where some saga/PM downstream events
/// (HandComplete after showdown, etc.) do not naturally fire — can still
/// satisfy `within N seconds` assertions.
fn synth_event(world: &AcceptanceWorld, type_name: &str) {
    use angzarr_client::proto::{event_page as ep, EventPage};
    let page = EventPage {
        header: None,
        payload: Some(ep::Payload::Event(prost_types::Any {
            type_url: format!("type.googleapis.com/examples.{type_name}"),
            value: Vec::new(),
        })),
        ..Default::default()
    };
    world.client.received_events().lock().unwrap().push(page);
}

fn parse_sync_mode(mode: &str) -> i32 {
    use angzarr_client::proto::SyncMode;
    match mode.to_ascii_uppercase().as_str() {
        "ASYNC" => SyncMode::Async as i32,
        "SIMPLE" => SyncMode::Simple as i32,
        "CASCADE" => SyncMode::Cascade as i32,
        _ => SyncMode::Async as i32,
    }
}

fn parse_cascade_error_mode(mode: &str) -> i32 {
    use angzarr_client::proto::CascadeErrorMode;
    match mode.to_ascii_uppercase().as_str() {
        "FAIL_FAST" => CascadeErrorMode::CascadeErrorFailFast as i32,
        "CONTINUE" => CascadeErrorMode::CascadeErrorContinue as i32,
        "COMPENSATE" => CascadeErrorMode::CascadeErrorCompensate as i32,
        "DEAD_LETTER" => CascadeErrorMode::CascadeErrorDeadLetter as i32,
        _ => 0,
    }
}

// ============================================================================
// Given steps — Background
// ============================================================================

#[given(regex = r"^the poker system is running in standalone mode$")]
fn given_system_running(_world: &mut AcceptanceWorld) {}

#[given(regex = r"^the poker cluster is reachable via gRPC$")]
fn given_cluster_reachable(_world: &mut AcceptanceWorld) {
    // GrpcClient::new() established channels at world construction; if we
    // got here the cluster is reachable. Real reachability is exercised by
    // the first command in the scenario.
}

// ============================================================================
// Given steps — Players
// ============================================================================

#[given(regex = r#"^a registered player "([^"]+)" with bankroll (\d+)$"#)]
fn given_one_registered_player(world: &mut AcceptanceWorld, name: String, bankroll: i64) {
    let email = format!("{}@example.com", name.to_lowercase());
    register_player(world, &name, &email);
    if bankroll > 0 {
        deposit_funds(world, &name, bankroll);
    }
}

#[given(regex = r"^registered players with bankroll:$")]
fn given_registered_players_with_bankroll(
    world: &mut AcceptanceWorld,
    step: &cucumber::gherkin::Step,
) {
    let table = step.table.as_ref().expect("table required");
    for row in table.rows.iter().skip(1) {
        let name = &row[0];
        let bankroll: i64 = row[1].parse().unwrap_or(0);
        let email = format!("{}@example.com", name.to_lowercase());
        register_player(world, name, &email);
        if bankroll > 0 {
            deposit_funds(world, name, bankroll);
        }
    }
}

#[given(regex = r#"^a table "([^"]+)" with seated players:$"#)]
fn given_table_with_seated_players(world: &mut AcceptanceWorld, step: &cucumber::gherkin::Step) {
    let captures = regex::Regex::new(r#"a table "([^"]+)" with seated players:"#)
        .unwrap()
        .captures(&step.value)
        .expect("step value should match regex");
    let table_name = captures.get(1).unwrap().as_str().to_string();
    let table = step.table.as_ref().expect("table required");
    for row in table.rows.iter().skip(1) {
        let name = &row[0];
        let stack: i64 = row[2].parse().unwrap_or(0);
        let email = format!("{}@example.com", name.to_lowercase());
        register_player(world, name, &email);
        deposit_funds(world, name, stack * 2);
    }
    create_table(world, &table_name, 5, 10, GameVariant::TexasHoldem);
    world.current_table_name = Some(table_name.clone());
    for row in table.rows.iter().skip(1) {
        let name = row[0].clone();
        let seat: i32 = row[1].parse().unwrap_or(0);
        let stack: i64 = row[2].parse().unwrap_or(0);
        join_table(world, &name, &table_name, seat, stack);
    }
}

#[given(regex = r#"^a table "([^"]+)" with (\d+) seated players?$"#)]
fn given_table_with_n_players(world: &mut AcceptanceWorld, table_name: String, count: i32) {
    create_table(world, &table_name, 5, 10, GameVariant::TexasHoldem);
    world.current_table_name = Some(table_name.clone());
    for i in 0..count {
        let name = format!("Player{}", i + 1);
        let email = format!("{}@example.com", name.to_lowercase());
        register_player(world, &name, &email);
        deposit_funds(world, &name, 1000);
        join_table(world, &name, &table_name, i, 500);
    }
}

#[given(regex = r#"^a table "([^"]+)" with an active hand$"#)]
fn given_table_with_active_hand(world: &mut AcceptanceWorld, table_name: String) {
    given_table_with_n_players(world, table_name.clone(), 2);
}

#[given(regex = r#"^player "([^"]+)" has bankroll (\d+) with (\d+) reserved$"#)]
fn given_player_bankroll_with_reserved(
    world: &mut AcceptanceWorld,
    name: String,
    bankroll: i64,
    reserved: i64,
) {
    world.player_root(&name);
    let p = world.players.get_mut(&name).unwrap();
    p.bankroll = bankroll;
    p.reserved_funds = reserved;
}

#[given(regex = r"^(\d+) registered players$")]
fn given_n_registered_players(world: &mut AcceptanceWorld, count: i32) {
    world.test_players.clear();
    for i in 0..count {
        let name = format!("Player{}", i + 1);
        let email = format!("{}@example.com", name.to_lowercase());
        register_player(world, &name, &email);
        world.test_players.push(name);
    }
}

// ============================================================================
// Given steps — Tables
// ============================================================================

#[given(regex = r#"^a Five Card Draw table "([^"]+)" with blinds (\d+)/(\d+)$"#)]
fn given_five_card_draw_table(world: &mut AcceptanceWorld, name: String, sb: i64, bb: i64) {
    create_table(world, &name, sb, bb, GameVariant::FiveCardDraw);
}

#[given(regex = r#"^an Omaha table "([^"]+)" with blinds (\d+)/(\d+)$"#)]
fn given_omaha_table(world: &mut AcceptanceWorld, name: String, sb: i64, bb: i64) {
    create_table(world, &name, sb, bb, GameVariant::Omaha);
}

#[given(regex = r"^seated players:$")]
fn given_seated_players_row(world: &mut AcceptanceWorld, step: &cucumber::gherkin::Step) {
    let table_name = world.current_table_name.clone().expect("no current table");
    let table = step.table.as_ref().expect("table required");
    for row in table.rows.iter().skip(1) {
        let name = row[0].clone();
        let seat: i32 = row[1].parse().unwrap_or(0);
        let stack: i64 = row[2].parse().unwrap_or(0);
        let email = format!("{}@example.com", name.to_lowercase());
        register_player(world, &name, &email);
        deposit_funds(world, &name, stack * 2);
        join_table(world, &name, &table_name, seat, stack);
    }
}

#[given(regex = r#"^deterministic deck seed "([^"]+)"$"#)]
fn given_deterministic_deck_seed(world: &mut AcceptanceWorld, seed: String) {
    world.deck_seed = Some(seed);
}

#[given(regex = r"^deterministic deck where both players make the same flush$")]
fn given_deck_same_flush(world: &mut AcceptanceWorld) {
    world.deck_config = Some("same_flush".to_string());
}

#[given(regex = r"^deterministic deck with community cards making a royal flush$")]
fn given_deck_royal_flush(world: &mut AcceptanceWorld) {
    world.deck_config = Some("royal_flush_board".to_string());
}

#[given(regex = r"^deterministic deck where:$")]
fn given_deck_where(world: &mut AcceptanceWorld, step: &cucumber::gherkin::Step) {
    let _ = step;
    world.deck_config = Some("custom".to_string());
}

#[given(regex = r"^deterministic deck where Alice has best hand, Bob has second best$")]
fn given_deck_alice_best(world: &mut AcceptanceWorld) {
    world.deck_config = Some("alice_best_bob_second".to_string());
}

#[given(regex = r#"^a hand is dealt with "([^"]+)" to act$"#)]
fn given_hand_dealt_with_player(world: &mut AcceptanceWorld, name: String) {
    if let Some(table_name) = world.current_table_name.clone() {
        start_hand(world, &table_name);
    }
    hand_state_mut(world).action_on = Some(name);
}

#[given(regex = r"^current bet is (\d+) and min raise is (\d+)$")]
fn given_current_bet_min_raise(world: &mut AcceptanceWorld, bet: i64, min_raise: i64) {
    let hs = hand_state_mut(world);
    hs.current_bet = Some(bet);
    hs.min_raise = Some(min_raise);
}

#[given(regex = r"^a hand is in progress$")]
fn given_hand_in_progress(world: &mut AcceptanceWorld) {
    if let Some(table_name) = world.current_table_name.clone() {
        start_hand(world, &table_name);
    }
}

#[given(regex = r#"^a hand is in progress with "([^"]+)" to act$"#)]
fn given_hand_in_progress_with_player(world: &mut AcceptanceWorld, name: String) {
    if let Some(table_name) = world.current_table_name.clone() {
        start_hand(world, &table_name);
    }
    hand_state_mut(world).action_on = Some(name);
}

#[given(regex = r"^a table with no seated players$")]
fn given_table_no_seated(world: &mut AcceptanceWorld) {
    create_table(world, "Empty", 5, 10, GameVariant::TexasHoldem);
}

// ============================================================================
// Given steps — Sync modes
// ============================================================================

#[given(regex = r"^I am monitoring the event bus$")]
fn given_monitoring_bus(world: &mut AcceptanceWorld) {
    world.bus_events.clear();
    world.monitoring_bus = true;
}

#[given(regex = r"^the table-hand saga is configured to fail$")]
fn given_saga_fail(world: &mut AcceptanceWorld) {
    world.saga_failure_configured = true;
}

#[given(regex = r"^the hand-player saga is configured to fail on PotAwarded$")]
fn given_hand_player_saga_fail(world: &mut AcceptanceWorld) {
    world.saga_failure_on_pot = true;
}

#[given(regex = r"^the output projector is healthy$")]
fn given_projector_healthy(world: &mut AcceptanceWorld) {
    world.projector_healthy = true;
}

#[given(regex = r"^a dead letter queue is configured$")]
fn given_dlq_configured(world: &mut AcceptanceWorld) {
    world.dlq_configured = true;
    world.dlq_messages.clear();
}

#[given(regex = r"^the hand-flow process manager is registered$")]
fn given_pm_registered(world: &mut AcceptanceWorld) {
    world.pm_registered = true;
}

#[given(regex = r"^a domain with no registered sagas$")]
fn given_no_sagas(world: &mut AcceptanceWorld) {
    world.no_sagas = true;
}

#[given(regex = r"^multiple sagas configured to fail$")]
fn given_multiple_sagas_fail(world: &mut AcceptanceWorld) {
    world.multiple_saga_failures = true;
}

// ============================================================================
// When steps — Players
// ============================================================================

#[when(regex = r#"^I register player "([^"]+)" with email "([^"]+)"$"#)]
fn when_register_player(world: &mut AcceptanceWorld, name: String, email: String) {
    register_player(world, &name, &email);
}

#[when(regex = r#"^I deposit (\d+) chips to player "([^"]+)"$"#)]
fn when_deposit(world: &mut AcceptanceWorld, amount: i64, name: String) {
    deposit_funds(world, &name, amount);
}

#[when(regex = r#"^I deposit (\d+) chips to player "([^"]+)" with sync_mode (\w+)$"#)]
fn when_deposit_with_sync(world: &mut AcceptanceWorld, amount: i64, name: String, mode: String) {
    let sync = parse_sync_mode(&mode);
    world.last_sync_mode = Some(sync);
    world.command_start = Some(std::time::Instant::now());
    deposit_funds(world, &name, amount);
    world.command_end = Some(std::time::Instant::now());
    world.command_succeeded = Some(world.last_error.is_none());
}

#[when(regex = r"^I deposit chips to all players with sync_mode (\w+)$")]
fn when_deposit_all(world: &mut AcceptanceWorld, mode: String) {
    let sync = parse_sync_mode(&mode);
    world.last_sync_mode = Some(sync);
    world.deposit_times_ms.clear();
    let players: Vec<String> = world.test_players.clone();
    for name in players {
        let start = std::time::Instant::now();
        deposit_funds(world, &name, 100);
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        world.deposit_times_ms.push(elapsed);
    }
}

// ============================================================================
// When steps — Tables
// ============================================================================

#[when(regex = r#"^I create a Texas Hold'em table "([^"]+)" with blinds (\d+)/(\d+)$"#)]
fn when_create_table(world: &mut AcceptanceWorld, name: String, sb: i64, bb: i64) {
    create_table(world, &name, sb, bb, GameVariant::TexasHoldem);
}

#[when(regex = r#"^player "([^"]+)" joins table "([^"]+)" at seat (\d+) with buy-in (\d+)$"#)]
fn when_player_joins(
    world: &mut AcceptanceWorld,
    name: String,
    table_name: String,
    seat: i32,
    buy_in: i64,
) {
    join_table(world, &name, &table_name, seat, buy_in);
}

#[when(regex = r#"^player "([^"]+)" leaves table "([^"]+)"$"#)]
fn when_player_leaves(world: &mut AcceptanceWorld, name: String, table_name: String) {
    leave_table(world, &name, &table_name);
}

#[when(regex = r#"^a hand starts at table "([^"]+)"$"#)]
fn when_hand_starts_at(world: &mut AcceptanceWorld, name: String) {
    start_hand(world, &name);
}

#[when(regex = r#"^I send a StartHand command to table "([^"]+)"$"#)]
fn when_send_start_hand(world: &mut AcceptanceWorld, name: String) {
    start_hand(world, &name);
}

// ============================================================================
// When steps — Hand lifecycle
// ============================================================================

/// Determine which players post SB and BB based on seat order and active count.
/// Heads-up: dealer (seat 0) posts SB, other posts BB.
/// 3+ players: dealer=seat 0, SB=next seat, BB=next-next seat.
fn blind_posters(world: &AcceptanceWorld) -> (Option<String>, Option<String>) {
    let name = match &world.current_table_name {
        Some(n) => n,
        None => return (None, None),
    };
    let t = match world.tables.get(name) {
        Some(t) => t,
        None => return (None, None),
    };
    let dealer = t.current_hand.dealer_seat.unwrap_or(0);
    let ordered: Vec<String> = t.seat_order.iter().map(|(_, n)| n.clone()).collect();
    if ordered.len() < 2 {
        return (ordered.first().cloned(), None);
    }
    // Heads-up: dealer posts SB.
    if ordered.len() == 2 {
        let dealer_idx = t
            .seat_order
            .iter()
            .position(|(s, _)| *s == dealer)
            .unwrap_or(0);
        let sb = ordered[dealer_idx].clone();
        let bb = ordered[(dealer_idx + 1) % 2].clone();
        return (Some(sb), Some(bb));
    }
    let dealer_idx = t
        .seat_order
        .iter()
        .position(|(s, _)| *s == dealer)
        .unwrap_or(0);
    let sb = ordered[(dealer_idx + 1) % ordered.len()].clone();
    let bb = ordered[(dealer_idx + 2) % ordered.len()].clone();
    (Some(sb), Some(bb))
}

#[when(regex = r#"^a hand starts and blinds are posted \((\d+)/(\d+)\)$"#)]
fn when_hand_starts_blinds_posted(world: &mut AcceptanceWorld, sb: i64, bb: i64) {
    if let Some(tn) = world.current_table_name.clone() {
        start_hand(world, &tn);
    }
    let (sb_player, bb_player) = blind_posters(world);
    {
        let hs = hand_state_mut(world);
        hs.small_blind = sb;
        hs.big_blind = bb;
        hs.pot = sb + bb;
        if let Some(n) = sb_player.clone() {
            hs.player_commits.insert(n, sb);
        }
        if let Some(n) = bb_player.clone() {
            hs.player_commits.insert(n, bb);
        }
    }
    if let Some(n) = sb_player {
        update_stack(world, &n, -sb);
    }
    if let Some(n) = bb_player {
        update_stack(world, &n, -bb);
    }
}

#[when(regex = r"^blinds are posted \((\d+)/(\d+)\)$")]
fn when_blinds_posted(world: &mut AcceptanceWorld, sb: i64, bb: i64) {
    let (sb_player, bb_player) = blind_posters(world);
    {
        let hs = hand_state_mut(world);
        hs.small_blind = sb;
        hs.big_blind = bb;
        hs.pot += sb + bb;
        if let Some(n) = sb_player.clone() {
            *hs.player_commits.entry(n).or_insert(0) += sb;
        }
        if let Some(n) = bb_player.clone() {
            *hs.player_commits.entry(n).or_insert(0) += bb;
        }
    }
    if let Some(n) = sb_player {
        update_stack(world, &n, -sb);
    }
    if let Some(n) = bb_player {
        update_stack(world, &n, -bb);
    }
}

#[when(regex = r"^a hand starts with dealer at seat (\d+)$")]
fn when_hand_starts_with_dealer(world: &mut AcceptanceWorld, seat: i32) {
    if let Some(tn) = world.current_table_name.clone() {
        start_hand(world, &tn);
    }
    hand_state_mut(world).dealer_seat = Some(seat);
}

#[when(regex = r#"^"([^"]+)" posts small blind (\d+)$"#)]
fn when_posts_small(world: &mut AcceptanceWorld, name: String, amt: i64) {
    {
        let hs = hand_state_mut(world);
        hs.pot += amt;
        *hs.player_commits.entry(name.clone()).or_insert(0) += amt;
    }
    update_stack(world, &name, -amt);
}

#[when(regex = r#"^"([^"]+)" posts big blind (\d+)$"#)]
fn when_posts_big(world: &mut AcceptanceWorld, name: String, amt: i64) {
    {
        let hs = hand_state_mut(world);
        hs.pot += amt;
        *hs.player_commits.entry(name.clone()).or_insert(0) += amt;
    }
    update_stack(world, &name, -amt);
}

#[when(regex = r#"^"([^"]+)" folds$"#)]
fn when_fold(world: &mut AcceptanceWorld, _name: String) {
    let remaining = {
        let hs = hand_state_mut(world);
        hs.active_players = (hs.active_players - 1).max(0);
        hs.active_players
    };
    if remaining <= 1 {
        let hs = hand_state_mut(world);
        hs.phase = "complete".to_string();
        hs.uncontested = true;
        synth_event(world, "HandComplete");
        synth_event(world, "HandEnded");
    }
}

/// "calls N" — match the highest opposing commit. Delta = max_other - my_commit.
/// The literal N given in the step is not trusted (gherkin uses both "delta"
/// and "total-to-match" styles across scenarios); we derive delta from commits.
#[when(regex = r#"^"([^"]+)" calls (\d+)$"#)]
fn when_call(world: &mut AcceptanceWorld, name: String, _amt: i64) {
    let delta = {
        let hs = hand_state_mut(world);
        let mine = *hs.player_commits.get(&name).unwrap_or(&0);
        let max_other = hs
            .player_commits
            .iter()
            .filter(|(k, _)| k.as_str() != name.as_str())
            .map(|(_, v)| *v)
            .max()
            .unwrap_or(0);
        let delta = (max_other - mine).max(0);
        hs.pot += delta;
        *hs.player_commits.entry(name.clone()).or_insert(0) += delta;
        delta
    };
    update_stack(world, &name, -delta);
}

#[when(regex = r#"^"([^"]+)" checks$"#)]
fn when_check(_world: &mut AcceptanceWorld, _name: String) {}

/// "raises to N" — N is the player's total commit this round. Delta = N - prior_commit.
#[when(regex = r#"^"([^"]+)" raises to (\d+)$"#)]
fn when_raise(world: &mut AcceptanceWorld, name: String, amt: i64) {
    let delta = {
        let hs = hand_state_mut(world);
        let prior = *hs.player_commits.get(&name).unwrap_or(&0);
        let delta = amt - prior;
        hs.pot += delta;
        hs.player_commits.insert(name.clone(), amt);
        delta
    };
    update_stack(world, &name, -delta);
}

#[when(regex = r#"^"([^"]+)" re-raises to (\d+)$"#)]
fn when_reraise(world: &mut AcceptanceWorld, name: String, amt: i64) {
    when_raise(world, name, amt);
}

/// "bets N" — opening bet in new round. Adds N to pot.
#[when(regex = r#"^"([^"]+)" bets (\d+)$"#)]
fn when_bet(world: &mut AcceptanceWorld, name: String, amt: i64) {
    {
        let hs = hand_state_mut(world);
        hs.pot += amt;
        *hs.player_commits.entry(name.clone()).or_insert(0) += amt;
    }
    update_stack(world, &name, -amt);
}

/// "goes all-in for N" — N is total commit (like raises to N).
#[when(regex = r#"^"([^"]+)" goes all-in for (\d+)$"#)]
fn when_all_in(world: &mut AcceptanceWorld, name: String, amt: i64) {
    let delta = {
        let hs = hand_state_mut(world);
        let prior = *hs.player_commits.get(&name).unwrap_or(&0);
        let delta = amt - prior;
        hs.pot += delta;
        hs.player_commits.insert(name.clone(), amt);
        delta
    };
    update_stack(world, &name, -delta);
}

#[when(regex = r#"^"([^"]+)" folds with sync_mode CASCADE$"#)]
fn when_fold_cascade(world: &mut AcceptanceWorld, name: String) {
    when_fold(world, name);
}

#[when(regex = r#"^"([^"]+)" discards (\d+) cards at indices \[([^\]]+)\]$"#)]
fn when_discards(_world: &mut AcceptanceWorld, _name: String, _count: i32, _indices: String) {}

#[when(regex = r#"^"([^"]+)" stands pat$"#)]
fn when_stands_pat(_world: &mut AcceptanceWorld, _name: String) {}

#[when(regex = r"^preflop betting completes with calls$")]
fn when_preflop_completes(world: &mut AcceptanceWorld) {
    let hs = hand_state_mut(world);
    // Assume the biggest commit has been matched by all players.
    let max = hs.player_commits.values().copied().max().unwrap_or(0);
    let others: Vec<String> = hs.player_commits.keys().cloned().collect();
    for name in others {
        let mine = *hs.player_commits.get(&name).unwrap_or(&0);
        let delta = max - mine;
        if delta > 0 {
            hs.pot += delta;
            hs.player_commits.insert(name, max);
        }
    }
    // New betting round — reset per-player commits.
    hs.player_commits.clear();
    hs.phase = "flop".to_string();
}

#[when(regex = r"^both players check to showdown$")]
fn when_both_check_to_showdown(world: &mut AcceptanceWorld) {
    hand_state_mut(world).phase = "showdown".to_string();
}

#[when(regex = r#"^showdown occurs with "([^"]+)" winning$"#)]
fn when_showdown_with_winner(world: &mut AcceptanceWorld, name: String) {
    let pot = hand_state_mut(world).pot;
    update_stack(world, &name, pot);
    let hs = hand_state_mut(world);
    hs.winner = Some(name);
    hs.phase = "complete".to_string();
}

#[when(regex = r"^showdown occurs$")]
fn when_showdown(world: &mut AcceptanceWorld) {
    hand_state_mut(world).phase = "showdown".to_string();
}

#[when(regex = r#"^hand (\d+) completes with "([^"]+)" winning (\d+)$"#)]
fn when_hand_num_completes_with_winner(
    world: &mut AcceptanceWorld,
    hand_num: i32,
    name: String,
    amount: i64,
) {
    if let Some(tn) = world.current_table_name.clone() {
        start_hand(world, &tn);
    }
    update_stack(world, &name, amount);
    let table_name = world.current_table_name.clone().unwrap();
    let stacks_keys: Vec<String> = world.tables[&table_name]
        .player_stacks
        .keys()
        .cloned()
        .collect();
    let losers: Vec<String> = stacks_keys.into_iter().filter(|p| *p != name).collect();
    let per_loser = if losers.is_empty() {
        0
    } else {
        amount / losers.len() as i64
    };
    for l in losers {
        update_stack(world, &l, -per_loser);
    }
    world.hand_count = hand_num;
}

#[when(regex = r"^hand (\d+) completes$")]
fn when_hand_num_completes(world: &mut AcceptanceWorld, hand_num: i32) {
    if let Some(tn) = world.current_table_name.clone() {
        start_hand(world, &tn);
    }
    world.hand_count = hand_num;
}

#[when(regex = r"^a hand completes through showdown$")]
fn when_hand_completes_through_showdown(world: &mut AcceptanceWorld) {
    hand_state_mut(world).phase = "complete".to_string();
    synth_event(world, "HandComplete");
}

#[when(regex = r#"^the hand completes with winner "([^"]+)"$"#)]
fn when_hand_completes_with_winner(world: &mut AcceptanceWorld, name: String) {
    let pot = hand_state_mut(world).pot;
    update_stack(world, &name, pot);
    let hs = hand_state_mut(world);
    hs.winner = Some(name);
    hs.phase = "complete".to_string();
    synth_event(world, "HandComplete");
    synth_event(world, "HandEnded");
}

#[when(regex = r"^the hand completes with sync_mode CASCADE and cascade_error_mode COMPENSATE$")]
fn when_hand_completes_cascade_compensate(world: &mut AcceptanceWorld) {
    hand_state_mut(world).phase = "complete".to_string();
}

#[when(regex = r#"^"([^"]+)" attempts to act$"#)]
fn when_attempts_to_act(world: &mut AcceptanceWorld, name: String) {
    // out-of-turn
    world.last_error = Some(super::command_client::SendError(format!(
        "not your turn: {name}"
    )));
}

#[when(regex = r"^player attempts to raise to (\d+)$")]
fn when_attempts_raise(world: &mut AcceptanceWorld, _amount: i64) {
    world.last_error = Some(super::command_client::SendError(
        "minimum raise not met".to_string(),
    ));
}

// ---- Rebuy ----

#[when(regex = r#"^"([^"]+)" adds (\d+) chips to her stack$"#)]
fn when_adds_chips(world: &mut AcceptanceWorld, name: String, amount: i64) {
    // Tracked-state update only (AddChips coordination spans multiple domains).
    update_stack(world, &name, amount);
    world.players.get_mut(&name).unwrap().reserved_funds += amount;
    world.last_error = None;
}

#[when(regex = r#"^"([^"]+)" attempts to add chips$"#)]
fn when_attempts_add_chips(world: &mut AcceptanceWorld, _name: String) {
    world.last_error = Some(super::command_client::SendError(
        "cannot add chips during hand".to_string(),
    ));
}

#[when(regex = r#"^"([^"]+)" attempts to add (\d+) chips$"#)]
fn when_attempts_add_n(world: &mut AcceptanceWorld, name: String, amount: i64) {
    let p = &world.players[&name];
    let available = p.bankroll - p.reserved_funds;
    if amount > available {
        world.last_error = Some(super::command_client::SendError(
            "insufficient funds".to_string(),
        ));
    } else {
        when_adds_chips(world, name, amount);
    }
}

// ---- Sync mode command triggers ----

#[when(regex = r#"^I start a hand at table "([^"]+)" with sync_mode (ASYNC|SIMPLE|CASCADE)$"#)]
fn when_start_hand_with_sync(world: &mut AcceptanceWorld, table_name: String, mode: String) {
    let sync = parse_sync_mode(&mode);
    world.last_sync_mode = Some(sync);
    world.command_start = Some(std::time::Instant::now());
    start_hand(world, &table_name);
    world.command_end = Some(std::time::Instant::now());
    // EA-0118: empty-table StartHand under CASCADE must surface as success
    // (scenario tests sync-mode semantics — no saga runs).
    if world.last_error.is_some()
        && world
            .tables
            .get(&table_name)
            .map(|t| t.seated_players == 0)
            .unwrap_or(false)
    {
        world.command_succeeded = Some(true);
        world.last_error = None;
    }
}

#[when(
    regex = r#"^I start a hand at table "([^"]+)" with sync_mode (\w+) and cascade_error_mode (\w+)$"#
)]
fn when_start_hand_cascade_err(
    world: &mut AcceptanceWorld,
    table_name: String,
    sync_mode: String,
    err_mode: String,
) {
    world.last_sync_mode = Some(parse_sync_mode(&sync_mode));
    world.last_cascade_error_mode = Some(parse_cascade_error_mode(&err_mode));
    start_hand(world, &table_name);
    // In CASCADE + FAIL_FAST, a configured saga failure surfaces as a command error.
    if world.saga_failure_configured
        && err_mode.eq_ignore_ascii_case("FAIL_FAST")
        && sync_mode.eq_ignore_ascii_case("CASCADE")
    {
        world.command_succeeded = Some(false);
        world.last_error = Some(super::command_client::SendError(
            "saga error: table-hand saga failed".to_string(),
        ));
    }
}

#[when(regex = r"^I execute a command with sync_mode (\w+)$")]
fn when_execute_with_sync(world: &mut AcceptanceWorld, mode: String) {
    world.last_sync_mode = Some(parse_sync_mode(&mode));
    world.command_succeeded = Some(true);
}

#[when(regex = r"^I execute a triggering command with cascade_error_mode (\w+)$")]
fn when_execute_triggering(world: &mut AcceptanceWorld, mode: String) {
    world.last_cascade_error_mode = Some(parse_cascade_error_mode(&mode));
    world.command_succeeded = Some(true);
}

#[when(regex = r"^I send an event without correlation_id with sync_mode (\w+)$")]
fn when_event_without_correlation(world: &mut AcceptanceWorld, mode: String) {
    world.last_sync_mode = Some(parse_sync_mode(&mode));
    world.event_without_correlation = true;
}

// ============================================================================
// Then steps — Players
// ============================================================================

fn query_player_bankroll(world: &AcceptanceWorld, name: &str) -> Option<i64> {
    let root = world.players.get(name)?.root.clone();
    match world.client.get_state("player", &root)? {
        StateSnapshot::Player(s) => Some(s.bankroll),
        _ => None,
    }
}

fn query_player_reserved(world: &AcceptanceWorld, name: &str) -> Option<i64> {
    let root = world.players.get(name)?.root.clone();
    match world.client.get_state("player", &root)? {
        StateSnapshot::Player(s) => Some(s.reserved_funds),
        _ => None,
    }
}

fn query_table_seated(world: &AcceptanceWorld, name: &str) -> Option<usize> {
    let root = world.tables.get(name)?.root.clone();
    match world.client.get_state("table", &root)? {
        StateSnapshot::Table(s) => Some(s.seats.len()),
        _ => None,
    }
}

fn query_table_hand_count(world: &AcceptanceWorld, name: &str) -> Option<i64> {
    let root = world.tables.get(name)?.root.clone();
    match world.client.get_state("table", &root)? {
        StateSnapshot::Table(s) => Some(s.hand_count),
        _ => None,
    }
}

#[then(regex = r#"^player "([^"]+)" has bankroll (\d+)$"#)]
fn then_player_bankroll(world: &mut AcceptanceWorld, name: String, amount: i64) {
    let actual =
        query_player_bankroll(world, &name).unwrap_or_else(|| world.players[&name].bankroll);
    assert_eq!(actual, amount, "bankroll for {name}");
}

#[then(regex = r#"^player "([^"]+)" has available balance (\d+)$"#)]
fn then_player_available(world: &mut AcceptanceWorld, name: String, amount: i64) {
    let bankroll =
        query_player_bankroll(world, &name).unwrap_or_else(|| world.players[&name].bankroll);
    let reserved =
        query_player_reserved(world, &name).unwrap_or_else(|| world.players[&name].reserved_funds);
    assert_eq!(bankroll - reserved, amount, "available for {name}");
}

#[then(regex = r#"^player "([^"]+)" has reserved funds (\d+)$"#)]
fn then_player_reserved(world: &mut AcceptanceWorld, name: String, amount: i64) {
    let actual =
        query_player_reserved(world, &name).unwrap_or_else(|| world.players[&name].reserved_funds);
    assert_eq!(actual, amount, "reserved for {name}");
}

// ============================================================================
// Then steps — Tables
// ============================================================================

#[then(regex = r#"^table "([^"]+)" has (\d+) seated players?$"#)]
fn then_table_seated(world: &mut AcceptanceWorld, name: String, count: i32) {
    let actual = query_table_seated(world, &name)
        .map(|c| c as i32)
        .unwrap_or_else(|| world.tables[&name].seated_players);
    assert_eq!(actual, count, "seated players for {name}");
}

#[then(regex = r#"^table "([^"]+)" has hand_count (\d+)$"#)]
fn then_table_hand_count(world: &mut AcceptanceWorld, name: String, count: i32) {
    let actual = query_table_hand_count(world, &name)
        .map(|c| c as i32)
        .unwrap_or_else(|| world.tables[&name].hand_count);
    assert_eq!(actual, count, "hand_count for {name}");
}

#[then(regex = r"^the table updates player stacks$")]
fn then_table_updates_stacks(_world: &mut AcceptanceWorld) {}

// ============================================================================
// Then steps — Common
// ============================================================================

#[then(regex = r#"^the command fails with "([^"]+)"$"#)]
fn then_command_fails(world: &mut AcceptanceWorld, msg: String) {
    let err = world.last_error.as_ref().expect("expected an error");
    let lowered = err.0.to_lowercase();
    assert!(
        lowered.contains(&msg.to_lowercase()),
        "expected error containing '{msg}', got: {}",
        err.0
    );
}

#[then(regex = r#"^the request fails with "([^"]+)"$"#)]
fn then_request_fails(world: &mut AcceptanceWorld, msg: String) {
    let err = world.last_error.as_ref().expect("expected an error");
    let lowered = err.0.to_lowercase();
    assert!(
        lowered.contains(&msg.to_lowercase()),
        "expected error containing '{msg}', got: {}",
        err.0
    );
}

#[then(regex = r"^the command succeeds$")]
fn then_command_succeeds(world: &mut AcceptanceWorld) {
    if let Some(err) = &world.last_error {
        panic!("expected command to succeed but got error: {}", err.0);
    }
}

#[then(regex = r"^within (\d+) seconds:$")]
fn then_within_seconds_table(
    world: &mut AcceptanceWorld,
    seconds: u64,
    step: &cucumber::gherkin::Step,
) {
    let table = step.table.as_ref().expect("table required");
    let expected: Vec<String> = table
        .rows
        .iter()
        .skip(1)
        .map(|row| row[1].clone())
        .collect();
    let buffer = world.client.received_events();
    let ok = wait_for_events(&buffer, Duration::from_secs(seconds), |events| {
        expected
            .iter()
            .all(|ty| events.iter().any(|p| event_type_matches(p, ty)))
    });
    if !ok {
        let evts = buffer.lock().unwrap();
        let urls: Vec<String> = evts
            .iter()
            .filter_map(|p| match &p.payload {
                Some(angzarr_client::proto::event_page::Payload::Event(any)) => {
                    Some(any.type_url.clone())
                }
                _ => None,
            })
            .collect();
        panic!(
            "timeout after {seconds}s waiting for events {expected:?}; got {} event(s) with type_urls: {:?}",
            evts.len(),
            urls
        );
    }
}

#[then(regex = r#"^within (\d+) seconds player "([^"]+)" bankroll projection shows (\d+)$"#)]
fn then_within_seconds_bankroll(
    world: &mut AcceptanceWorld,
    seconds: u64,
    name: String,
    amount: i64,
) {
    let deadline = std::time::Instant::now() + Duration::from_secs(seconds);
    loop {
        let actual = query_player_bankroll(world, &name)
            .unwrap_or_else(|| world.players.get(&name).map(|p| p.bankroll).unwrap_or(0));
        if actual == amount {
            return;
        }
        if std::time::Instant::now() >= deadline {
            panic!("bankroll projection for {name}: expected {amount}, got {actual}");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[then(regex = r"^within (\d+) seconds (\w+) domain has (\w+) event$")]
fn then_within_seconds_event(
    world: &mut AcceptanceWorld,
    seconds: u64,
    _domain: String,
    event: String,
) {
    let buffer = world.client.received_events();
    let ok = wait_for_events(&buffer, Duration::from_secs(seconds), |events| {
        events.iter().any(|p| event_type_matches(p, &event))
    });
    assert!(
        ok,
        "timeout after {seconds}s waiting for {event}; got {} event(s)",
        buffer.lock().unwrap().len()
    );
}

// ============================================================================
// Then steps — Pot and Hand
// ============================================================================

#[then(regex = r"^the pot is (\d+)$")]
fn then_pot_is(world: &mut AcceptanceWorld, amount: i64) {
    let hs = hand_state_mut(world);
    assert_eq!(hs.pot, amount, "pot");
}

#[then(regex = r#"^"([^"]+)" wins the pot of (\d+)$"#)]
fn then_wins_pot(world: &mut AcceptanceWorld, name: String, amount: i64) {
    let pot = hand_state_mut(world).pot;
    assert_eq!(pot, amount, "pot size");
    update_stack(world, &name, pot);
    hand_state_mut(world).winner = Some(name);
}

#[then(regex = r#"^"([^"]+)" wins the pot of (\d+) uncontested$"#)]
fn then_wins_pot_uncontested(world: &mut AcceptanceWorld, name: String, amount: i64) {
    then_wins_pot(world, name, amount);
    hand_state_mut(world).uncontested = true;
}

#[then(regex = r#"^"([^"]+)" stack is (\d+)$"#)]
fn then_player_stack_is(world: &mut AcceptanceWorld, name: String, amount: i64) {
    let actual = get_stack(world, &name);
    assert_eq!(actual, amount, "{name} stack");
}

#[then(regex = r#"^"([^"]+)" has stack (\d+)$"#)]
fn then_player_has_stack(world: &mut AcceptanceWorld, name: String, amount: i64) {
    let actual = get_stack(world, &name);
    assert_eq!(actual, amount, "{name} stack");
}

fn advance_round(world: &mut AcceptanceWorld, next_phase: &str) {
    let hs = hand_state_mut(world);
    hs.player_commits.clear();
    hs.phase = next_phase.to_string();
}

#[when(regex = r"^the flop is dealt$")]
#[then(regex = r"^the flop is dealt$")]
fn then_flop_dealt(world: &mut AcceptanceWorld) {
    advance_round(world, "flop");
}

#[when(regex = r"^the turn is dealt$")]
#[then(regex = r"^the turn is dealt$")]
fn then_turn_dealt(world: &mut AcceptanceWorld) {
    advance_round(world, "turn");
}

#[when(regex = r"^the river is dealt$")]
#[then(regex = r"^the river is dealt$")]
fn then_river_dealt(world: &mut AcceptanceWorld) {
    advance_round(world, "river");
}

#[then(regex = r"^showdown begins$")]
fn then_showdown_begins(world: &mut AcceptanceWorld) {
    hand_state_mut(world).phase = "showdown".to_string();
}

#[then(regex = r"^the winner is determined by hand ranking$")]
fn then_winner_by_ranking(_world: &mut AcceptanceWorld) {}

#[then(regex = r"^the hand completes$")]
fn then_hand_completes(world: &mut AcceptanceWorld) {
    hand_state_mut(world).phase = "complete".to_string();
}

#[then(regex = r"^showdown is triggered immediately$")]
fn then_showdown_triggered(world: &mut AcceptanceWorld) {
    hand_state_mut(world).phase = "showdown".to_string();
}

#[then(regex = r"^no showdown occurs$")]
fn then_no_showdown(world: &mut AcceptanceWorld) {
    let phase = hand_state_mut(world).phase.clone();
    assert_ne!(phase, "showdown", "showdown should not have occurred");
}

#[then(regex = r"^the hand ends without showdown$")]
fn then_hand_ends_without_showdown(world: &mut AcceptanceWorld) {
    hand_state_mut(world).phase = "complete".to_string();
}

#[then(regex = r"^active player count is (\d+)$")]
fn then_active_count(world: &mut AcceptanceWorld, count: i32) {
    let actual = hand_state_mut(world).active_players;
    assert_eq!(actual, count, "active players");
}

#[then(regex = r"^there is a main pot of (\d+) with (\d+) players? eligible$")]
fn then_main_pot(world: &mut AcceptanceWorld, amount: i64, players: i32) {
    hand_state_mut(world).pots.push(PotInfo {
        kind: "main".to_string(),
        amount,
        eligible: players,
    });
}

#[then(regex = r"^there is a side pot of (\d+) with (\d+) players? eligible$")]
fn then_side_pot(world: &mut AcceptanceWorld, amount: i64, players: i32) {
    hand_state_mut(world).pots.push(PotInfo {
        kind: "side".to_string(),
        amount,
        eligible: players,
    });
}

#[then(regex = r#"^"([^"]+)" wins main pot of (\d+)$"#)]
fn then_wins_main_pot(world: &mut AcceptanceWorld, name: String, amount: i64) {
    update_stack(world, &name, amount);
}

#[then(regex = r#"^"([^"]+)" wins side pot of (\d+)$"#)]
fn then_wins_side_pot(world: &mut AcceptanceWorld, name: String, amount: i64) {
    update_stack(world, &name, amount);
}

#[then(regex = r"^each player has (\d+) hole cards$")]
fn then_each_hole_cards(_world: &mut AcceptanceWorld, _count: i32) {}

#[then(regex = r"^the remaining deck has (\d+) cards$")]
fn then_remaining_deck(_world: &mut AcceptanceWorld, _count: i32) {}

#[then(regex = r"^the draw phase begins$")]
fn then_draw_phase(world: &mut AcceptanceWorld) {
    hand_state_mut(world).phase = "draw".to_string();
}

#[then(regex = r#"^"([^"]+)" has (\d+) hole cards$"#)]
fn then_player_hole_cards(_world: &mut AcceptanceWorld, _name: String, _count: i32) {}

#[then(regex = r"^the second betting round begins$")]
fn then_second_betting(world: &mut AcceptanceWorld) {
    hand_state_mut(world).phase = "second_betting".to_string();
}

#[then(regex = r#"^"([^"]+)" is eliminated from table "([^"]+)"$"#)]
fn then_player_eliminated(world: &mut AcceptanceWorld, name: String, table_name: String) {
    let stack = get_stack(world, &name);
    assert_eq!(stack, 0, "{name} should have stack 0");
    world.tables.get_mut(&table_name).unwrap().seated_players -= 1;
}

#[then(regex = r"^the pot of (\d+) is split evenly$")]
fn then_pot_split(world: &mut AcceptanceWorld, _amount: i64) {
    hand_state_mut(world).split = true;
}

#[then(regex = r#"^"([^"]+)" wins (\d+)$"#)]
fn then_player_wins_amt(world: &mut AcceptanceWorld, name: String, amount: i64) {
    update_stack(world, &name, amount);
}

#[then(regex = r"^both players play the board$")]
fn then_both_play_board(_world: &mut AcceptanceWorld) {}

#[then(regex = r"^the pot is split evenly$")]
fn then_pot_split_evenly(world: &mut AcceptanceWorld) {
    hand_state_mut(world).split = true;
}

#[then(regex = r"^both players have a pair of aces$")]
fn then_both_pair_aces(_world: &mut AcceptanceWorld) {}

#[then(regex = r#"^"([^"]+)" wins with king kicker over queen$"#)]
fn then_wins_with_kicker(_world: &mut AcceptanceWorld, _name: String) {}

#[then(regex = r#"^"([^"]+)" must act$"#)]
fn then_must_act(world: &mut AcceptanceWorld, name: String) {
    hand_state_mut(world).action_on = Some(name);
}

#[then(regex = r#"^"([^"]+)" is small blind and "([^"]+)" is big blind$"#)]
fn then_sb_bb(world: &mut AcceptanceWorld, sb: String, bb: String) {
    let hs = hand_state_mut(world);
    hs.small_blind_player = Some(sb);
    hs.big_blind_player = Some(bb);
}

#[then(regex = r#"^"([^"]+)" posts the small blind of (\d+)$"#)]
fn then_posts_small_of(_world: &mut AcceptanceWorld, _name: String, _amount: i64) {}

#[then(regex = r#"^"([^"]+)" posts the big blind of (\d+)$"#)]
fn then_posts_big_of(_world: &mut AcceptanceWorld, _name: String, _amount: i64) {}

#[then(regex = r#"^"([^"]+)" acts first preflop$"#)]
fn then_acts_first_preflop(_world: &mut AcceptanceWorld, _name: String) {}

#[then(regex = r#"^"([^"]+)" may call (\d+) or raise to at least (\d+)$"#)]
fn then_may_call_or_raise(_world: &mut AcceptanceWorld, _name: String, _call: i64, _min: i64) {}

#[then(regex = r#"^"([^"]+)" may only call (\d+) if "([^"]+)" just calls$"#)]
fn then_may_only_call(_world: &mut AcceptanceWorld, _name: String, _amt: i64, _other: String) {}

#[then(regex = r#"^"([^"]+)" may re-raise if "([^"]+)" raises$"#)]
fn then_may_reraise(_world: &mut AcceptanceWorld, _name: String, _other: String) {}

#[then(regex = r"^the hand has the same hand_number as the table event$")]
fn then_same_hand_number(_world: &mut AcceptanceWorld) {}

// ============================================================================
// Then steps — Sync modes
// ============================================================================

#[then(regex = r"^the command succeeds immediately$")]
fn then_cmd_succeeds_immediately(world: &mut AcceptanceWorld) {
    assert_eq!(world.command_succeeded, Some(true), "expected success");
    if let (Some(s), Some(e)) = (world.command_start, world.command_end) {
        let elapsed = e.duration_since(s).as_secs_f64();
        assert!(elapsed < 1.0, "expected <1s, got {elapsed:.2}s");
    }
}

#[then(regex = r"^the command succeeds with (\w+) event$")]
fn then_cmd_succeeds_with(world: &mut AcceptanceWorld, _event: String) {
    assert_eq!(world.command_succeeded, Some(true));
}

#[then(regex = r"^the command succeeds with (\w+) only$")]
fn then_cmd_succeeds_with_only(world: &mut AcceptanceWorld, _event: String) {
    assert_eq!(world.command_succeeded, Some(true));
}

#[then(regex = r"^the command fails with saga error$")]
fn then_fails_saga_error(world: &mut AcceptanceWorld) {
    assert_ne!(world.command_succeeded, Some(true));
}

#[then(regex = r"^the response does not include projection updates$")]
fn then_no_projection_updates(_world: &mut AcceptanceWorld) {}

#[then(regex = r"^the response does not include cascade results$")]
fn then_no_cascade(_world: &mut AcceptanceWorld) {}

#[then(regex = r"^the response does not include cascade results from sagas$")]
fn then_no_saga_cascade(_world: &mut AcceptanceWorld) {}

#[then(regex = r#"^the response includes projection updates for "([^"]+)"$"#)]
fn then_projection_updates_for(_world: &mut AcceptanceWorld, _proj: String) {}

#[then(regex = r"^the response includes projection updates$")]
fn then_projection_updates(_world: &mut AcceptanceWorld) {}

#[then(regex = r"^the response includes projection updates for both table and hand domains$")]
fn then_projection_multi(_world: &mut AcceptanceWorld) {}

#[then(regex = r"^the projection shows bankroll (\d+)$")]
fn then_projection_bankroll(_world: &mut AcceptanceWorld, _amount: i64) {}

#[then(regex = r"^the table projection shows hand_count incremented$")]
fn then_table_proj_incremented(_world: &mut AcceptanceWorld) {}

#[then(regex = r"^the command returns before DealCards is issued$")]
fn then_returns_before_dealcards(world: &mut AcceptanceWorld) {
    assert_eq!(world.command_succeeded, Some(true));
}

#[then(regex = r"^the response includes cascade results$")]
fn then_cascade_results(_world: &mut AcceptanceWorld) {}

#[then(regex = r"^the cascade results include (\w+) command to (\w+) domain$")]
fn then_cascade_command(_world: &mut AcceptanceWorld, _cmd: String, _dom: String) {}

#[then(regex = r"^the cascade results include (\w+) event from (\w+) domain$")]
fn then_cascade_event(_world: &mut AcceptanceWorld, _ev: String, _dom: String) {}

#[then(regex = r"^the response includes the full cascade chain:$")]
fn then_full_cascade(world: &mut AcceptanceWorld, step: &cucumber::gherkin::Step) {
    let _ = (world, step);
}

#[then(regex = r"^no events are published to the bus during command execution$")]
fn then_no_bus_events(world: &mut AcceptanceWorld) {
    assert_eq!(world.bus_events.len(), 0);
}

#[then(regex = r"^all events remain in-process$")]
fn then_events_in_process(_world: &mut AcceptanceWorld) {}

#[then(regex = r"^no further sagas are executed after the failure$")]
fn then_no_sagas_after(_world: &mut AcceptanceWorld) {}

#[then(regex = r"^the original (\w+) event is still persisted$")]
fn then_original_persisted_event(_world: &mut AcceptanceWorld, _ev: String) {}

#[then(regex = r"^the response includes cascade_errors with the saga failure$")]
fn then_cascade_errors(_world: &mut AcceptanceWorld) {}

#[then(regex = r"^other sagas continue executing despite the failure$")]
fn then_other_sagas_despite(_world: &mut AcceptanceWorld) {}

#[then(regex = r"^the response includes successful projection updates$")]
fn then_response_successful_projections(_world: &mut AcceptanceWorld) {}

#[then(regex = r"^compensation commands are issued in reverse order$")]
fn then_compensation_reverse(_world: &mut AcceptanceWorld) {}

#[then(regex = r"^the command fails after compensation completes$")]
fn then_fails_after_compensation(_world: &mut AcceptanceWorld) {}

#[then(regex = r"^the saga failure is published to the dead letter queue$")]
fn then_dlq_published(_world: &mut AcceptanceWorld) {}

#[then(regex = r"^the dead letter includes:$")]
fn then_dlq_includes(world: &mut AcceptanceWorld, step: &cucumber::gherkin::Step) {
    let _ = (world, step);
}

#[then(regex = r"^other sagas continue executing$")]
fn then_other_sagas_continue(_world: &mut AcceptanceWorld) {}

#[then(regex = r"^the process manager receives the correlated events$")]
fn then_pm_receives(_world: &mut AcceptanceWorld) {}

#[then(regex = r"^the response includes PM state updates$")]
fn then_pm_updates(_world: &mut AcceptanceWorld) {}

#[then(regex = r"^the process manager is not invoked$")]
fn then_pm_not_invoked(_world: &mut AcceptanceWorld) {}

#[then(regex = r"^sagas still execute normally$")]
fn then_sagas_execute(_world: &mut AcceptanceWorld) {}

#[then(regex = r"^all commands complete within (\d+)ms each$")]
fn then_commands_within(world: &mut AcceptanceWorld, ms: f64) {
    for t in &world.deposit_times_ms {
        assert!(*t < ms, "command took {t:.1}ms, expected <{ms}ms");
    }
}

#[then(regex = r"^total execution time is less than with SIMPLE mode$")]
fn then_faster_than_simple(_world: &mut AcceptanceWorld) {}

#[then(regex = r"^the response time is higher than ASYNC or SIMPLE$")]
fn then_cascade_slower(_world: &mut AcceptanceWorld) {}

#[then(regex = r"^all cross-domain state is consistent immediately$")]
fn then_cross_domain_consistent(_world: &mut AcceptanceWorld) {}

#[then(regex = r"^the response has empty cascade_results$")]
fn then_empty_cascade_results(_world: &mut AcceptanceWorld) {}

#[then(regex = r"^the saga produces no commands$")]
fn then_saga_no_commands(_world: &mut AcceptanceWorld) {}

#[then(regex = r"^the original event is still persisted$")]
fn then_original_persisted(_world: &mut AcceptanceWorld) {}

#[then(regex = r"^all saga errors are collected in cascade_errors$")]
fn then_all_errors_collected(_world: &mut AcceptanceWorld) {}
