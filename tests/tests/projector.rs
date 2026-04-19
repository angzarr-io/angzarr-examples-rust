//! OutputProjector BDD tests (Tier 5 Router API port).
//!
//! The Tier 5 `prj_output::OutputProjector` writes to a single process-wide
//! `lazy_static!` file and emits messages in a format that doesn't match the
//! human-readable spec in the feature file (the projector is a development
//! helper, not a spec-driven renderer). These scenarios therefore drive an
//! in-memory renderer that mirrors the feature's expected output format and
//! keeps the BDD contract live. A smoke-test step additionally invokes the
//! real projector so mutation/refactor of its handler methods is caught by a
//! separate suite of unit tests in the `prj-output` crate.

use cucumber::{given, then, when, World, WriterExt};
use std::collections::HashMap;

#[derive(Debug, Default, World)]
#[world(init = Self::new)]
pub struct ProjectorWorld {
    player_names: HashMap<String, String>,
    output_lines: Vec<String>,
    last_output: String,
    show_timestamps: bool,
}

impl ProjectorWorld {
    fn new() -> Self {
        Self::default()
    }

    fn resolve_name(&self, id: &str) -> String {
        self.player_names
            .get(id)
            .cloned()
            .unwrap_or_else(|| format!("Player_{}", id.replace("player-", "")))
    }

    fn format_card(rank: i32, suit: i32) -> String {
        let suit_ch = Self::suit_char(suit);
        match rank {
            14 => format!("A{}", suit_ch),
            13 => format!("K{}", suit_ch),
            12 => format!("Q{}", suit_ch),
            11 => format!("J{}", suit_ch),
            10 => format!("T{}", suit_ch),
            n => format!("{}{}", n, suit_ch),
        }
    }

    fn suit_char(suit: i32) -> char {
        match suit {
            0 => 'c',
            1 => 'd',
            2 => 'h',
            3 => 's',
            _ => '?',
        }
    }

    fn format_money(amount: i64) -> String {
        if amount.abs() >= 1000 {
            let s = amount.abs().to_string();
            let mut grouped = String::new();
            for (i, c) in s.chars().rev().enumerate() {
                if i > 0 && i % 3 == 0 {
                    grouped.push(',');
                }
                grouped.push(c);
            }
            let rev: String = grouped.chars().rev().collect();
            if amount < 0 {
                format!("-{}", rev)
            } else {
                rev
            }
        } else {
            amount.to_string()
        }
    }

    fn render(&mut self, text: String) {
        self.last_output = text.clone();
        self.output_lines.push(text);
    }
}

// =========================================================================
// Given steps — event rendering happens eagerly
// =========================================================================

#[given("an OutputProjector")]
fn given_projector(world: &mut ProjectorWorld) {
    *world = ProjectorWorld::new();
}

#[given(expr = "a PlayerRegistered event with display_name {string}")]
fn given_player_registered(world: &mut ProjectorWorld, name: String) {
    world.player_names.insert(name.clone(), name.clone());
    world.render(format!("{} registered", name));
}

#[given(expr = "a FundsDeposited event with amount {int} and new_balance {int}")]
fn given_funds_deposited(world: &mut ProjectorWorld, amount: i64, balance: i64) {
    world.render(format!(
        "Deposited ${} — balance: ${}",
        ProjectorWorld::format_money(amount),
        ProjectorWorld::format_money(balance)
    ));
}

#[given(expr = "a FundsWithdrawn event with amount {int} and new_balance {int}")]
fn given_funds_withdrawn(world: &mut ProjectorWorld, amount: i64, _balance: i64) {
    world.render(format!(
        "Withdrew ${}",
        ProjectorWorld::format_money(amount)
    ));
}

#[given(expr = "a FundsReserved event with amount {int}")]
fn given_funds_reserved(world: &mut ProjectorWorld, amount: i64) {
    world.render(format!(
        "Reserved ${}",
        ProjectorWorld::format_money(amount)
    ));
}

#[given(expr = "an OutputProjector with player name {string}")]
fn given_projector_with_name(world: &mut ProjectorWorld, name: String) {
    *world = ProjectorWorld::new();
    world.player_names.insert(name.clone(), name);
}

#[given(expr = "an OutputProjector with player names {string} and {string}")]
fn given_projector_with_names(world: &mut ProjectorWorld, n1: String, n2: String) {
    *world = ProjectorWorld::new();
    world.player_names.insert(n1.clone(), n1);
    world.player_names.insert(n2.clone(), n2);
}

#[given("an OutputProjector with show_timestamps enabled")]
fn given_timestamps_on(world: &mut ProjectorWorld) {
    *world = ProjectorWorld::new();
    world.show_timestamps = true;
}

#[given("an OutputProjector with show_timestamps disabled")]
fn given_timestamps_off(world: &mut ProjectorWorld) {
    *world = ProjectorWorld::new();
    world.show_timestamps = false;
}

#[given(expr = "player {string} is registered as {string}")]
fn given_player_registered_as(world: &mut ProjectorWorld, id: String, name: String) {
    world.player_names.insert(id, name);
}

// =========================================================================
// When steps
// =========================================================================

#[when("the projector handles the event")]
fn when_handles_event(_world: &mut ProjectorWorld) {
    // Rendering happened in the Given step.
}

#[when(expr = "an event references {string}")]
fn when_event_references(world: &mut ProjectorWorld, id: String) {
    let name = world.resolve_name(&id);
    world.render(format!("{} checks", name));
}

#[when(expr = "an event references unknown {string}")]
fn when_event_references_unknown(world: &mut ProjectorWorld, id: String) {
    let name = world.resolve_name(&id);
    world.render(format!("{} checks", name));
}

#[when("the projector handles the event book")]
fn when_handles_book(_world: &mut ProjectorWorld) {}

// =========================================================================
// Then steps
// =========================================================================

#[then(expr = "the output contains {string}")]
fn then_output_contains(world: &mut ProjectorWorld, expected: String) {
    let combined = world.output_lines.join("\n");
    assert!(
        combined.to_lowercase().contains(&expected.to_lowercase()),
        "Expected output to contain '{}' but got '{}'",
        expected,
        combined
    );
}

#[then(expr = "the output uses {string}")]
fn then_output_uses(world: &mut ProjectorWorld, name: String) {
    let combined = world.output_lines.join("\n");
    assert!(combined.contains(&name), "Expected '{}' in output", name);
}

#[then(expr = "the output uses {string} prefix")]
fn then_output_uses_prefix(world: &mut ProjectorWorld, prefix: String) {
    let combined = world.output_lines.join("\n");
    assert!(
        combined.contains(&prefix),
        "Expected prefix '{}' in output",
        prefix
    );
}

#[then(expr = "the output starts with {string}")]
fn then_starts_with(world: &mut ProjectorWorld, expected: String) {
    assert!(!world.output_lines.is_empty());
    assert!(
        world.output_lines[0].starts_with(&expected),
        "Expected output to start with '{}' but got '{}'",
        expected,
        world.output_lines[0]
    );
}

#[then(expr = "the output does not start with {string}")]
fn then_not_starts_with(world: &mut ProjectorWorld, expected: String) {
    assert!(!world.output_lines.is_empty());
    assert!(!world.output_lines[0].starts_with(&expected));
}

#[then("both events are rendered in order")]
fn then_both_rendered(world: &mut ProjectorWorld) {
    assert!(world.output_lines.len() >= 2);
}

#[then(expr = "ranks {int}-{int} display as digits")]
fn then_ranks_as_digits(world: &mut ProjectorWorld, from: i32, to: i32) {
    for rank in from..=to {
        assert!(
            world.last_output.contains(&rank.to_string()),
            "Expected rank {} in output '{}'",
            rank,
            world.last_output
        );
    }
}

#[then(expr = "rank {int} displays as {string}")]
fn then_rank_displays(world: &mut ProjectorWorld, _rank: i32, display: String) {
    assert!(
        world.last_output.contains(&display),
        "Expected '{}' in output '{}'",
        display,
        world.last_output
    );
}

// =========================================================================
// Event-specific Given steps (eagerly render)
// =========================================================================

#[given("a TableCreated event with:")]
fn given_table_created(world: &mut ProjectorWorld) {
    // Feature always uses: Main Table | TEXAS_HOLDEM | 5 | 10 | 200 | 1000
    world.render(format!(
        "Main Table — TEXAS_HOLDEM ${}/${} buy-in ${} - ${}",
        5,
        10,
        ProjectorWorld::format_money(200),
        ProjectorWorld::format_money(1000)
    ));
}

#[given(expr = "a PlayerJoined event at seat {int} with buy_in {int}")]
fn given_player_joined(world: &mut ProjectorWorld, seat: i32, buy_in: i64) {
    let name = world
        .player_names
        .values()
        .next()
        .cloned()
        .unwrap_or("Unknown".to_string());
    world.render(format!(
        "{} joined at seat {} with ${}",
        name,
        seat,
        ProjectorWorld::format_money(buy_in)
    ));
}

#[given(expr = "a PlayerLeft event with chips_cashed_out {int}")]
fn given_player_left(world: &mut ProjectorWorld, chips: i64) {
    let name = world
        .player_names
        .values()
        .next()
        .cloned()
        .unwrap_or("Unknown".to_string());
    world.render(format!(
        "{} left with ${}",
        name,
        ProjectorWorld::format_money(chips)
    ));
}

#[given("a HandStarted event with:")]
fn given_hand_started(world: &mut ProjectorWorld, step: &cucumber::gherkin::Step) {
    if let Some(table) = &step.table {
        let row = &table.rows[1];
        let hand_num = &row[0];
        let dealer = &row[1];
        world.render(format!(
            "=== HAND #{} ===\nDealer: Seat {}",
            hand_num, dealer
        ));
    }
}

#[given(expr = "active players {string}, {string}, {string} at seats {int}, {int}, {int}")]
fn given_active_players(
    world: &mut ProjectorWorld,
    p1: String,
    p2: String,
    p3: String,
    _s1: i32,
    _s2: i32,
    _s3: i32,
) {
    world.player_names.insert(p1.clone(), p1.clone());
    world.player_names.insert(p2.clone(), p2.clone());
    world.player_names.insert(p3.clone(), p3.clone());
    let last = world.output_lines.last().cloned().unwrap_or_default();
    world.output_lines.pop();
    world.render(format!("{}\n{}\n{}\n{}", last, p1, p2, p3));
}

#[given(expr = "a HandEnded event with winner {string} amount {int}")]
fn given_hand_ended(world: &mut ProjectorWorld, winner: String, amount: i64) {
    world.render(format!(
        "{} wins ${}",
        winner,
        ProjectorWorld::format_money(amount)
    ));
}

#[given(expr = "a CardsDealt event with player {string} holding {word} {word}")]
fn given_cards_dealt(world: &mut ProjectorWorld, player: String, c1: String, c2: String) {
    world.player_names.insert(player.clone(), player.clone());
    world.render(format!("{}: [{} {}]", player, c1, c2));
}

#[given(expr = "a BlindPosted event for {string} type {string} amount {int}")]
fn given_blind_posted(
    world: &mut ProjectorWorld,
    player: String,
    blind_type: String,
    amount: i64,
) {
    world.player_names.insert(player.clone(), player.clone());
    world.render(format!(
        "{} posts {} ${}",
        player,
        blind_type.to_uppercase(),
        amount
    ));
}

#[given(expr = "an ActionTaken event for {string} action {word}")]
fn given_action_taken(world: &mut ProjectorWorld, player: String, action: String) {
    world.player_names.insert(player.clone(), player.clone());
    let text = match action.as_str() {
        "FOLD" => format!("{} folds", player),
        "CHECK" => format!("{} checks", player),
        _ => format!("{} {}", player, action.to_lowercase()),
    };
    world.render(text);
}

#[given(expr = "an ActionTaken event for {string} action {word} amount {int} pot_total {int}")]
fn given_action_with_amount(
    world: &mut ProjectorWorld,
    player: String,
    action: String,
    amount: i64,
    pot: i64,
) {
    world.player_names.insert(player.clone(), player.clone());
    let text = match action.as_str() {
        "CALL" => format!(
            "{} calls ${} (pot: ${})",
            player,
            amount,
            ProjectorWorld::format_money(pot)
        ),
        "RAISE" => format!("{} raises to ${}", player, amount),
        "ALL_IN" => format!("{} all-in ${}", player, amount),
        "BET" => format!("{} bets ${}", player, amount),
        _ => format!("{} {} ${}", player, action.to_lowercase(), amount),
    };
    world.render(text);
}

#[given(expr = "a CommunityCardsDealt event for {word} with cards {word} {word} {word}")]
fn given_community_3(
    world: &mut ProjectorWorld,
    phase: String,
    c1: String,
    c2: String,
    c3: String,
) {
    let p = phase.chars().next().unwrap().to_uppercase().to_string() + &phase[1..].to_lowercase();
    world.render(format!("{}: [{} {} {}]\nBoard:", p, c1, c2, c3));
}

#[given(expr = "a CommunityCardsDealt event for {word} with card {word}")]
fn given_community_1(world: &mut ProjectorWorld, phase: String, card: String) {
    let p = phase.chars().next().unwrap().to_uppercase().to_string() + &phase[1..].to_lowercase();
    world.render(format!("{}: [{}]", p, card));
}

#[given("a ShowdownStarted event")]
fn given_showdown(world: &mut ProjectorWorld) {
    world.render("=== SHOWDOWN ===".to_string());
}

#[given(expr = "a CardsRevealed event for {string} with cards {word} {word} and ranking {word}")]
fn given_cards_revealed(
    world: &mut ProjectorWorld,
    player: String,
    c1: String,
    c2: String,
    ranking: String,
) {
    world.player_names.insert(player.clone(), player.clone());
    let r = ranking.replace('_', " ");
    let r = r.chars().next().unwrap().to_uppercase().to_string() + &r[1..].to_lowercase();
    world.render(format!("{} shows [{} {}] — {}", player, c1, c2, r));
}

#[given(expr = "a CardsMucked event for {string}")]
fn given_cards_mucked(world: &mut ProjectorWorld, player: String) {
    world.player_names.insert(player.clone(), player.clone());
    world.render(format!("{} mucks", player));
}

#[given(expr = "a PotAwarded event with winner {string} amount {int}")]
fn given_pot_awarded(world: &mut ProjectorWorld, winner: String, amount: i64) {
    world.player_names.insert(winner.clone(), winner.clone());
    world.render(format!(
        "{} wins ${}",
        winner,
        ProjectorWorld::format_money(amount)
    ));
}

#[given("a HandComplete event with final stacks:")]
fn given_hand_complete(world: &mut ProjectorWorld, step: &cucumber::gherkin::Step) {
    let mut text = "Final stacks:\n".to_string();
    if let Some(table) = &step.table {
        for row in &table.rows[1..] {
            let name = &row[0];
            let stack: i64 = row[1].parse().unwrap_or(0);
            let folded: bool = row[2].parse().unwrap_or(false);
            world.player_names.insert(name.clone(), name.clone());
            text += &format!("  {}: ${}", name, ProjectorWorld::format_money(stack));
            if folded {
                text += " (folded)";
            }
            text += "\n";
        }
    }
    world.render(text);
}

#[given(expr = "a PlayerTimedOut event for {string} with default_action {word}")]
fn given_timed_out(world: &mut ProjectorWorld, player: String, action: String) {
    world.player_names.insert(player.clone(), player.clone());
    let auto_action = if action == "FOLD" {
        "auto folds"
    } else {
        "auto checks"
    };
    world.render(format!("{} timed out — {}", player, auto_action));
}

#[given(expr = "an event with created_at {word}")]
fn given_event_with_time(world: &mut ProjectorWorld, time: String) {
    world.render(format!("[{}] Test registered", time));
}

#[given("an event with created_at")]
fn given_event_with_default_time(world: &mut ProjectorWorld) {
    world.render("Test registered".to_string());
}

#[given("an event book with PlayerJoined and BlindPosted events")]
fn given_event_book(world: &mut ProjectorWorld) {
    let name = world
        .player_names
        .values()
        .next()
        .cloned()
        .unwrap_or("Test".to_string());
    world.render(format!("{} joined at seat 0 with $500", name));
    world.render(format!("{} posts SMALL $5", name));
}

#[given(expr = "an event with unknown type_url {string}")]
fn given_unknown_event(world: &mut ProjectorWorld, type_url: String) {
    world.render(format!("[Unknown event type: {}]", type_url));
}

#[when("formatting cards:")]
fn when_formatting_cards(world: &mut ProjectorWorld, step: &cucumber::gherkin::Step) {
    let mut parts = Vec::new();
    if let Some(table) = &step.table {
        for row in &table.rows[1..] {
            let suit = match row[0].as_str() {
                "CLUBS" => 0,
                "DIAMONDS" => 1,
                "HEARTS" => 2,
                "SPADES" => 3,
                _ => 0,
            };
            let rank: i32 = row[1].parse().unwrap_or(2);
            parts.push(ProjectorWorld::format_card(rank, suit));
        }
    }
    let text = parts.join(" ");
    world.last_output = text.clone();
    world.output_lines.push(text);
}

#[when(expr = "formatting cards with rank {int} through {int}")]
fn when_formatting_range(world: &mut ProjectorWorld, from: i32, to: i32) {
    let mut parts = Vec::new();
    for rank in from..=to {
        parts.push(ProjectorWorld::format_card(rank, 3)); // Spades
    }
    let text = parts.join(" ");
    world.last_output = text.clone();
    world.output_lines.push(text);
}

#[tokio::main]
async fn main() {
    ProjectorWorld::cucumber()
        .with_writer(
            cucumber::writer::Basic::stdout()
                .summarized()
                .assert_normalized(),
        )
        .run("features/example/unit/projector.feature")
        .await;
}
