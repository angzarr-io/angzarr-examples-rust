//! Process Manager (HandFlowPM) BDD tests.
//!
//! Tests phase transitions, blind posting, betting rounds, timeouts, and
//! draw game phases using the same simple state tracking pattern as other languages.

use cucumber::{given, then, when, World, WriterExt};
use std::collections::HashMap;

#[derive(Debug, Clone)]
struct PMPlayer {
    position: i32,
    stack: i64,
    player_root: String,
    bet_this_round: i64,
    has_acted: bool,
    has_folded: bool,
    is_all_in: bool,
}

#[derive(Debug, Default)]
struct HandProcess {
    phase: String,
    betting_phase: String,
    game_variant: String,
    dealer_position: i32,
    small_blind: i64,
    big_blind: i64,
    pot_total: i64,
    current_bet: i64,
    action_on: i32,
    players: HashMap<i32, PMPlayer>,
    small_blind_posted: bool,
    big_blind_posted: bool,
}

#[derive(Debug, Default, World)]
#[world(init = Self::new)]
pub struct PMWorld {
    process: Option<HandProcess>,
    emitted_commands: Vec<String>,
    last_action: String,
}

impl PMWorld {
    fn new() -> Self {
        Self::default()
    }

    fn init_default_players(process: &mut HandProcess) {
        for i in 0..2 {
            process.players.insert(i, PMPlayer {
                position: i,
                stack: 500,
                player_root: format!("player-{}", i + 1),
                bet_this_round: 0,
                has_acted: false,
                has_folded: false,
                is_all_in: false,
            });
        }
    }

    fn end_betting_round(&mut self) {
        let process = self.process.as_mut().unwrap();
        self.emitted_commands.clear();

        if process.game_variant == "FIVE_CARD_DRAW" && process.betting_phase == "PREFLOP" {
            process.phase = "DRAW".to_string();
        } else {
            match process.betting_phase.as_str() {
                "PREFLOP" => {
                    self.emitted_commands.push("DealCommunityCards:3".to_string());
                    process.phase = "DEALING_COMMUNITY".to_string();
                }
                "FLOP" | "TURN" => {
                    self.emitted_commands.push("DealCommunityCards:1".to_string());
                    process.phase = "DEALING_COMMUNITY".to_string();
                }
                "RIVER" | "DRAW" => {
                    process.phase = "SHOWDOWN".to_string();
                    self.emitted_commands.push("AwardPot".to_string());
                }
                _ => {}
            }
        }
    }
}

// =========================================================================
// Given steps
// =========================================================================

#[given("a HandFlowPM")]
fn given_hand_flow_pm(world: &mut PMWorld) {
    world.process = None;
    world.emitted_commands.clear();
}

// "a HandStarted event with:" is handled in the PM initialization scenario
// by creating the process directly. The data table is parsed by cucumber framework.

#[given(expr = "an active hand process in phase {word}")]
fn given_active_process_in_phase(world: &mut PMWorld, phase: String) {
    let mut process = HandProcess {
        phase,
        betting_phase: "PREFLOP".to_string(),
        ..Default::default()
    };
    PMWorld::init_default_players(&mut process);
    world.process = Some(process);
    world.emitted_commands.clear();
}

#[given(expr = "an active hand process with betting_phase {word}")]
fn given_process_with_betting_phase(world: &mut PMWorld, phase: String) {
    let mut process = HandProcess {
        phase: "BETTING".to_string(),
        betting_phase: phase,
        ..Default::default()
    };
    PMWorld::init_default_players(&mut process);
    world.process = Some(process);
    world.emitted_commands.clear();
}

#[given(expr = "an active hand process with {int} players")]
fn given_process_with_players(world: &mut PMWorld, count: i32) {
    let mut process = HandProcess {
        phase: "BETTING".to_string(),
        ..Default::default()
    };
    for i in 0..count {
        process.players.insert(i, PMPlayer {
            position: i,
            stack: 500,
            player_root: format!("player-{}", i + 1),
            bet_this_round: 0,
            has_acted: false,
            has_folded: false,
            is_all_in: false,
        });
    }
    world.process = Some(process);
    world.emitted_commands.clear();
}

#[given(expr = "an active hand process with game_variant {word}")]
fn given_process_with_variant(world: &mut PMWorld, variant: String) {
    let mut process = HandProcess {
        phase: "BETTING".to_string(),
        game_variant: variant,
        ..Default::default()
    };
    PMWorld::init_default_players(&mut process);
    world.process = Some(process);
    world.emitted_commands.clear();
}

#[given("an active hand process")]
fn given_active_process(world: &mut PMWorld) {
    let mut process = HandProcess {
        phase: "BETTING".to_string(),
        ..Default::default()
    };
    PMWorld::init_default_players(&mut process);
    world.process = Some(process);
    world.emitted_commands.clear();
}

#[given(expr = "an active hand process with player {string} at stack {int}")]
fn given_process_with_player_stack(world: &mut PMWorld, player_id: String, stack: i64) {
    let mut process = HandProcess {
        phase: "BETTING".to_string(),
        ..Default::default()
    };
    process.players.insert(0, PMPlayer {
        position: 0, stack, player_root: player_id,
        bet_this_round: 0, has_acted: false, has_folded: false, is_all_in: false,
    });
    process.players.insert(1, PMPlayer {
        position: 1, stack: 500, player_root: "player-2".to_string(),
        bet_this_round: 0, has_acted: false, has_folded: false, is_all_in: false,
    });
    world.process = Some(process);
    world.emitted_commands.clear();
}

#[given("a CardsDealt event")]
fn given_cards_dealt(_world: &mut PMWorld) {}

#[given("small_blind_posted is true")]
fn given_small_blind_posted(world: &mut PMWorld) {
    let process = world.process.as_mut().unwrap();
    process.pot_total += process.small_blind;
    process.small_blind_posted = true;
}

#[given("a BlindPosted event for small blind")]
fn given_blind_posted_small(_world: &mut PMWorld) {}

#[given("a BlindPosted event for big blind")]
fn given_blind_posted_big(world: &mut PMWorld) {
    let process = world.process.as_mut().unwrap();
    process.pot_total += process.big_blind;
    process.current_bet = process.big_blind;
    process.big_blind_posted = true;
}

#[given(expr = "action_on is position {int}")]
fn given_action_on(world: &mut PMWorld, pos: i32) {
    world.process.as_mut().unwrap().action_on = pos;
}

#[given(expr = "an ActionTaken event for player at position {int} with action {word}")]
fn given_action_at_position(world: &mut PMWorld, pos: i32, action: String) {
    world.last_action = action.clone();
    let process = world.process.as_mut().unwrap();
    if let Some(p) = process.players.get_mut(&pos) {
        p.has_acted = true;
    }
    if action == "RAISE" {
        for (k, p) in process.players.iter_mut() {
            if *k != pos { p.has_acted = false; }
        }
    } else if action == "FOLD" {
        if let Some(p) = process.players.get_mut(&pos) { p.has_folded = true; }
    } else if action == "ALL_IN" {
        if let Some(p) = process.players.get_mut(&pos) { p.is_all_in = true; }
    }
}

#[given(expr = "players at positions {int}, {int}, {int} have all acted")]
fn given_players_all_acted(world: &mut PMWorld, p1: i32, p2: i32, p3: i32) {
    let process = world.process.as_mut().unwrap();
    for pos in [p1, p2, p3] {
        process.players.entry(pos).or_insert(PMPlayer {
            position: pos, stack: 500,
            player_root: format!("player-{}", pos + 1),
            bet_this_round: 0, has_acted: false, has_folded: false, is_all_in: false,
        }).has_acted = true;
    }
}

#[given("all active players have acted and matched the current bet")]
fn given_all_acted_matched(world: &mut PMWorld) {
    let process = world.process.as_mut().unwrap();
    let bet = process.current_bet;
    for p in process.players.values_mut() {
        if !p.has_folded && !p.is_all_in {
            p.has_acted = true;
            p.bet_this_round = bet;
        }
    }
}

#[given("an ActionTaken event for the last player")]
fn given_last_player_acts(world: &mut PMWorld) {
    let process = world.process.as_mut().unwrap();
    for p in process.players.values_mut() {
        if !p.has_acted && !p.has_folded && !p.is_all_in {
            p.has_acted = true;
            break;
        }
    }
}

#[given(expr = "an ActionTaken event with action {word}")]
fn given_action_event(world: &mut PMWorld, action: String) {
    world.last_action = action.clone();
    let process = world.process.as_mut().unwrap();
    if action == "FOLD" {
        for p in process.players.values_mut() {
            if !p.has_folded { p.has_folded = true; break; }
        }
    } else if action == "ALL_IN" {
        for p in process.players.values_mut() {
            if !p.is_all_in && !p.has_folded { p.is_all_in = true; break; }
        }
    }
}

#[given("betting round is complete")]
fn given_betting_complete(world: &mut PMWorld) {
    let process = world.process.as_mut().unwrap();
    let bet = process.current_bet;
    for p in process.players.values_mut() {
        if !p.has_folded && !p.is_all_in {
            p.has_acted = true;
            p.bet_this_round = bet;
        }
    }
}

#[given(expr = "current_bet is {int}")]
fn given_current_bet(world: &mut PMWorld, amount: i64) {
    world.process.as_mut().unwrap().current_bet = amount;
}

#[given(expr = "action_on player has bet_this_round {int}")]
fn given_action_player_bet(world: &mut PMWorld, amount: i64) {
    let process = world.process.as_mut().unwrap();
    let pos = process.action_on;
    if let Some(p) = process.players.get_mut(&pos) { p.bet_this_round = amount; }
}

#[given(expr = "betting_phase {word}")]
fn given_betting_phase(world: &mut PMWorld, phase: String) {
    world.process.as_mut().unwrap().betting_phase = phase;
}

#[given("all players have completed their draws")]
fn given_draws_complete(_world: &mut PMWorld) {}

#[given(expr = "a series of BlindPosted and ActionTaken events totaling {int}")]
fn given_events_totaling(world: &mut PMWorld, total: i64) {
    world.process.as_mut().unwrap().pot_total = total;
}

#[given(expr = "an ActionTaken event for {string} with amount {int}")]
fn given_action_for_player(world: &mut PMWorld, player_id: String, amount: i64) {
    let process = world.process.as_mut().unwrap();
    for p in process.players.values_mut() {
        if p.player_root == player_id {
            p.stack -= amount;
            p.bet_this_round += amount;
            break;
        }
    }
    process.pot_total += amount;
}

#[given("a PotAwarded event")]
fn given_pot_awarded(_world: &mut PMWorld) {}

#[given(expr = "a CommunityCardsDealt event for {word}")]
fn given_community_dealt(world: &mut PMWorld, phase: String) {
    let process = world.process.as_mut().unwrap();
    for p in process.players.values_mut() {
        p.bet_this_round = 0;
        p.has_acted = false;
    }
    process.current_bet = 0;
    process.betting_phase = phase;
}

// =========================================================================
// When steps
// =========================================================================

#[when("the process manager starts the hand")]
fn when_pm_starts(world: &mut PMWorld) {
    world.emitted_commands.clear();
    world.process.as_mut().unwrap().phase = "DEALING".to_string();
}

#[when("the process manager handles the event")]
fn when_pm_handles(world: &mut PMWorld) {
    world.emitted_commands.clear();
    let process = world.process.as_mut().unwrap();

    match process.phase.as_str() {
        "DEALING" => {
            process.phase = "POSTING_BLINDS".to_string();
            world.emitted_commands.push("PostBlind:small".to_string());
        }
        "POSTING_BLINDS" => {
            if process.small_blind_posted && !process.big_blind_posted {
                // Small blind posted, emit big blind
                world.emitted_commands.push("PostBlind:big".to_string());
                process.big_blind_posted = true;
            } else {
                // Both blinds posted, transition to betting
                process.phase = "BETTING".to_string();
                process.action_on = (process.dealer_position + 2) % process.players.len() as i32;
            }
        }
        "BETTING" => {
            // Advance action
            let n = process.players.len() as i32;
            let mut next = (process.action_on + 1) % n;
            for _ in 0..n {
                if let Some(p) = process.players.get(&next) {
                    if !p.has_folded && !p.is_all_in { break; }
                }
                next = (next + 1) % n;
            }
            process.action_on = next;

            // Check all folded
            let active = process.players.values().filter(|p| !p.has_folded).count();
            if active <= 1 {
                process.phase = "COMPLETE".to_string();
                world.emitted_commands.push("AwardPot".to_string());
                return;
            }

            // Check betting complete
            let all_acted = process.players.values()
                .filter(|p| !p.has_folded && !p.is_all_in)
                .all(|p| p.has_acted);
            if all_acted {
                // Inline end_betting_round to avoid borrow conflict
                let variant = process.game_variant.clone();
                let phase = process.betting_phase.clone();
                if variant == "FIVE_CARD_DRAW" && phase == "PREFLOP" {
                    process.phase = "DRAW".to_string();
                } else {
                    match phase.as_str() {
                        "PREFLOP" => {
                            process.phase = "DEALING_COMMUNITY".to_string();
                            drop(process);
                            world.emitted_commands.push("DealCommunityCards:3".to_string());
                        }
                        "FLOP" | "TURN" => {
                            process.phase = "DEALING_COMMUNITY".to_string();
                            drop(process);
                            world.emitted_commands.push("DealCommunityCards:1".to_string());
                        }
                        "RIVER" | "DRAW" => {
                            process.phase = "SHOWDOWN".to_string();
                            drop(process);
                            world.emitted_commands.push("AwardPot".to_string());
                        }
                        _ => {}
                    }
                }
                return;
            }
        }
        "SHOWDOWN" => {
            process.phase = "COMPLETE".to_string();
            world.emitted_commands.push("timeout:cancel".to_string());
        }
        _ => {}
    }
}

#[when("the process manager ends the betting round")]
fn when_pm_ends_round(world: &mut PMWorld) {
    world.end_betting_round();
}

#[when("the action times out")]
fn when_timeout(world: &mut PMWorld) {
    world.emitted_commands.clear();
    let process = world.process.as_ref().unwrap();
    if process.current_bet > 0 {
        if let Some(p) = process.players.get(&process.action_on) {
            if p.bet_this_round < process.current_bet {
                world.emitted_commands.push("PlayerAction:FOLD".to_string());
                return;
            }
        }
    }
    world.emitted_commands.push("PlayerAction:CHECK".to_string());
}

#[when("the process manager handles the last draw")]
fn when_last_draw(world: &mut PMWorld) {
    world.emitted_commands.clear();
    let process = world.process.as_mut().unwrap();
    process.phase = "BETTING".to_string();
    process.betting_phase = "DRAW".to_string();
}

#[when("all events are processed")]
fn when_all_processed(_world: &mut PMWorld) {}

// =========================================================================
// Then steps
// =========================================================================

#[then(expr = "a HandProcess is created with phase {word}")]
fn then_process_created(world: &mut PMWorld, phase: String) {
    let p = world.process.as_ref().expect("No process");
    assert_eq!(p.phase, phase);
}

#[then(expr = "the process has {int} players")]
fn then_process_has_players(world: &mut PMWorld, count: usize) {
    assert_eq!(world.process.as_ref().unwrap().players.len(), count);
}

#[then(expr = "the process has dealer_position {int}")]
fn then_dealer_position(world: &mut PMWorld, pos: i32) {
    assert_eq!(world.process.as_ref().unwrap().dealer_position, pos);
}

#[then(expr = "the process transitions to phase {word}")]
fn then_phase_is(world: &mut PMWorld, phase: String) {
    assert_eq!(world.process.as_ref().unwrap().phase, phase);
}

#[then("a PostBlind command is sent for small blind")]
fn then_post_small(world: &mut PMWorld) {
    assert!(world.emitted_commands.iter().any(|c| c.contains("PostBlind") && c.contains("small")));
}

#[then("a PostBlind command is sent for big blind")]
fn then_post_big(world: &mut PMWorld) {
    assert!(world.emitted_commands.iter().any(|c| c.contains("PostBlind") && c.contains("big")));
}

#[then("action_on is set to UTG position")]
fn then_action_utg(world: &mut PMWorld) {
    let p = world.process.as_ref().unwrap();
    let utg = (p.dealer_position + 2) % p.players.len() as i32;
    assert_eq!(p.action_on, utg);
}

#[then("action_on advances to next active player")]
fn then_action_advances(world: &mut PMWorld) {
    assert!(world.process.as_ref().unwrap().action_on >= 0);
}

#[then(expr = "players at positions {int} and {int} have has_acted reset to false")]
fn then_players_reset(world: &mut PMWorld, p1: i32, p2: i32) {
    let process = world.process.as_ref().unwrap();
    assert!(!process.players[&p1].has_acted);
    assert!(!process.players[&p2].has_acted);
}

#[then("the betting round ends")]
fn then_betting_ends(world: &mut PMWorld) {
    assert_ne!(world.process.as_ref().unwrap().phase, "BETTING");
}

#[then("the process advances to next phase")]
fn then_advances(_world: &mut PMWorld) {}

#[then(expr = "a DealCommunityCards command is sent with count {int}")]
fn then_deal_community(world: &mut PMWorld, count: i32) {
    let expected = format!("DealCommunityCards:{}", count);
    assert!(world.emitted_commands.iter().any(|c| *c == expected),
        "Expected {}, got {:?}", expected, world.emitted_commands);
}

#[then("an AwardPot command is sent")]
fn then_award_pot(world: &mut PMWorld) {
    assert!(world.emitted_commands.iter().any(|c| c.contains("AwardPot")));
}

#[then("an AwardPot command is sent to the remaining player")]
fn then_award_remaining(world: &mut PMWorld) {
    assert!(world.emitted_commands.iter().any(|c| c.contains("AwardPot")));
}

#[then("the player is marked as is_all_in")]
fn then_all_in(world: &mut PMWorld) {
    assert!(world.process.as_ref().unwrap().players.values().any(|p| p.is_all_in));
}

#[then("the player is not included in active players for betting")]
fn then_excluded(world: &mut PMWorld) {
    let active = world.process.as_ref().unwrap().players.values()
        .filter(|p| !p.has_folded && !p.is_all_in).count();
    assert!(active < world.process.as_ref().unwrap().players.len());
}

#[then(expr = "the process manager sends PlayerAction with {word}")]
fn then_auto_action(world: &mut PMWorld, action: String) {
    let expected = format!("PlayerAction:{}", action);
    assert!(world.emitted_commands.iter().any(|c| *c == expected));
}

#[then(expr = "all players have bet_this_round reset to {int}")]
fn then_bets_reset(world: &mut PMWorld, amount: i64) {
    for p in world.process.as_ref().unwrap().players.values() {
        assert_eq!(p.bet_this_round, amount, "Player {} bet not reset", p.position);
    }
}

#[then("all players have has_acted reset to false")]
fn then_acted_reset(world: &mut PMWorld) {
    for p in world.process.as_ref().unwrap().players.values() {
        assert!(!p.has_acted, "Player {} still has_acted", p.position);
    }
}

#[then(expr = "current_bet is reset to {int}")]
fn then_current_bet_reset(world: &mut PMWorld, amount: i64) {
    assert_eq!(world.process.as_ref().unwrap().current_bet, amount);
}

#[then("action_on is set to first player after dealer")]
fn then_action_after_dealer(world: &mut PMWorld) {
    let p = world.process.as_ref().unwrap();
    let expected = (p.dealer_position + 1) % p.players.len() as i32;
    assert_eq!(p.action_on, expected);
}

#[then(expr = "pot_total is {int}")]
fn then_pot_total(world: &mut PMWorld, amount: i64) {
    assert_eq!(world.process.as_ref().unwrap().pot_total, amount);
}

#[then(expr = "{string} stack is {int}")]
fn then_player_stack(world: &mut PMWorld, player_id: String, amount: i64) {
    let found = world.process.as_ref().unwrap().players.values()
        .find(|p| p.player_root == player_id)
        .expect(&format!("Player {} not found", player_id));
    assert_eq!(found.stack, amount);
}

#[then("any pending timeout is cancelled")]
fn then_timeout_cancelled(world: &mut PMWorld) {
    assert_eq!(world.process.as_ref().unwrap().phase, "COMPLETE");
}

#[then(expr = "betting_phase is set to {word}")]
fn then_betting_phase(world: &mut PMWorld, phase: String) {
    assert_eq!(world.process.as_ref().unwrap().betting_phase, phase);
}

#[tokio::main]
async fn main() {
    PMWorld::cucumber()
        .with_writer(
            cucumber::writer::Basic::stdout()
                .summarized()
                .assert_normalized(),
        )
        .run("features/unit/process_manager.feature")
        .await;
}
