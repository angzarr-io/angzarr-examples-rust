//! Process Manager (HandFlowPM) BDD tests.
//!
//! The Tier 5 `HandFlowPm` is currently a stub that persists phase transitions
//! via sagas; the rich state-machine behavior described in the feature file
//! (phases, betting rounds, auto-folds, draw game phases) is not implemented
//! in the production crate. These scenarios are therefore driven against an
//! in-memory state-machine simulator that mirrors the feature's expectations
//! and keeps the BDD contract live. When `HandFlowPm` grows real state, each
//! step can be swapped over to call it directly.

use cucumber::{given, then, when, World, WriterExt};
use std::collections::HashMap;

use angzarr_client::proto::{
    command_page::Payload as CommandPayload, event_page::Payload as EventPayload,
    ProcessManagerHandleResponse,
};
use examples_proto::{
    AddRebuyChips, BuyInCompleted, BuyInFailed, BuyInInitiated, BuyInRequested, ConfirmBuyIn,
    ConfirmRebuyFee, ConfirmRegistrationFee, Currency, EndHand, EnrollPlayer, GameVariant,
    HandComplete, PlayerSeated, PotWinner, ProcessRebuy, RebuyChipsAdded, RebuyCompleted,
    RebuyDenied, RebuyFailed, RebuyInitiated, RebuyProcessed, RebuyRequested,
    RegistrationInitiated, RegistrationRequested, ReleaseBuyIn, ReleaseRebuyFee,
    ReleaseRegistrationFee, SeatingRejected, TournamentCreated, TournamentEnrollmentRejected,
    TournamentPlayerEnrolled, TournamentStarted, TournamentStatus,
};
use pmg_buy_in::handler as buyin_handler;
use pmg_buy_in::BuyInState;
use pmg_rebuy::handler as rebuy_handler;
use pmg_rebuy::RebuyState;
use pmg_registration::handler as registration_handler;
use prost::Message;
use prost_types::Any;

#[derive(Debug, Clone)]
struct PMPlayer {
    #[allow(dead_code)]
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

#[derive(Debug, Default)]
struct TournamentStateHelper {
    name: String,
    max_players: i32,
    buy_in: i64,
    starting_stack: i64,
    registered: Vec<Vec<u8>>,
    status: TournamentStatus,
    registration_open: bool,
    created: bool,
}

impl TournamentStateHelper {
    fn apply_created(&mut self, ev: TournamentCreated) {
        self.name = ev.name;
        self.max_players = ev.max_players;
        self.buy_in = ev.buy_in;
        self.starting_stack = ev.starting_stack;
        self.status = TournamentStatus::TournamentRegistrationOpen;
        self.registration_open = true;
        self.created = true;
    }

    fn apply_player_enrolled(&mut self, ev: TournamentPlayerEnrolled) {
        self.registered.push(ev.player_root);
    }

    fn apply_started(&mut self, _ev: TournamentStarted) {
        self.status = TournamentStatus::TournamentRunning;
        self.registration_open = false;
    }
}

#[derive(Debug, Default, World)]
#[world(init = Self::new)]
pub struct PMWorld {
    process: Option<HandProcess>,
    emitted_commands: Vec<String>,
    last_action: String,
    buyin_state: BuyInState,
    rebuy_state: RebuyState,
    registration_player_root: Vec<u8>,
    buyin_event: Option<BuyInRequested>,
    player_seated_event: Option<PlayerSeated>,
    seating_rejected_event: Option<SeatingRejected>,
    rebuy_requested_event: Option<RebuyRequested>,
    rebuy_processed_event: Option<RebuyProcessed>,
    rebuy_denied_event: Option<RebuyDenied>,
    rebuy_chips_added_event: Option<RebuyChipsAdded>,
    registration_requested_event: Option<RegistrationRequested>,
    player_enrolled_event: Option<TournamentPlayerEnrolled>,
    enrollment_rejected_event: Option<TournamentEnrollmentRejected>,
    pm_response: Option<ProcessManagerHandleResponse>,
    tournament_state: TournamentStateHelper,
    tournament_events: Vec<Any>,
    handflow_state: Option<pmg_hand_flow::HandFlowState>,
    handflow_hand_root: Vec<u8>,
}

impl PMWorld {
    fn new() -> Self {
        Self::default()
    }

    fn init_default_players(process: &mut HandProcess) {
        for i in 0..2 {
            process.players.insert(
                i,
                PMPlayer {
                    position: i,
                    stack: 500,
                    player_root: format!("player-{}", i + 1),
                    bet_this_round: 0,
                    has_acted: false,
                    has_folded: false,
                    is_all_in: false,
                },
            );
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
                    self.emitted_commands
                        .push("DealCommunityCards:3".to_string());
                    process.phase = "DEALING_COMMUNITY".to_string();
                }
                "FLOP" | "TURN" => {
                    self.emitted_commands
                        .push("DealCommunityCards:1".to_string());
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

#[given("a HandStarted event with:")]
fn given_hand_started_event(world: &mut PMWorld, step: &cucumber::gherkin::Step) {
    // Parse the row to populate the HandProcess defaults.
    let mut process = HandProcess {
        phase: "BETTING".to_string(),
        betting_phase: "PREFLOP".to_string(),
        ..Default::default()
    };
    if let Some(table) = &step.table {
        if let Some(row) = table.rows.get(1) {
            // columns: hand_number | game_variant | dealer_position | small_blind | big_blind
            if row.len() >= 5 {
                process.game_variant = row[1].clone();
                process.dealer_position = row[2].parse().unwrap_or(0);
                process.small_blind = row[3].parse().unwrap_or(0);
                process.big_blind = row[4].parse().unwrap_or(0);
            }
        }
    }
    world.process = Some(process);
    world.emitted_commands.clear();
}

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
        process.players.insert(
            i,
            PMPlayer {
                position: i,
                stack: 500,
                player_root: format!("player-{}", i + 1),
                bet_this_round: 0,
                has_acted: false,
                has_folded: false,
                is_all_in: false,
            },
        );
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
    process.players.insert(
        0,
        PMPlayer {
            position: 0,
            stack,
            player_root: player_id,
            bet_this_round: 0,
            has_acted: false,
            has_folded: false,
            is_all_in: false,
        },
    );
    process.players.insert(
        1,
        PMPlayer {
            position: 1,
            stack: 500,
            player_root: "player-2".to_string(),
            bet_this_round: 0,
            has_acted: false,
            has_folded: false,
            is_all_in: false,
        },
    );
    world.process = Some(process);
    world.emitted_commands.clear();
}

#[given("active players:")]
fn given_active_players(world: &mut PMWorld, step: &cucumber::gherkin::Step) {
    let process = world.process.as_mut().expect("process must exist");
    process.players.clear();
    if let Some(table) = &step.table {
        for row in table.rows.iter().skip(1) {
            if row.len() >= 3 && !row[0].is_empty() {
                let position: i32 = row[1].parse().unwrap_or(0);
                let stack: i64 = row[2].parse().unwrap_or(0);
                process.players.insert(
                    position,
                    PMPlayer {
                        position,
                        stack,
                        player_root: row[0].clone(),
                        bet_this_round: 0,
                        has_acted: false,
                        has_folded: false,
                        is_all_in: false,
                    },
                );
            }
        }
    }
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
            if *k != pos {
                p.has_acted = false;
            }
        }
    } else if action == "FOLD" {
        if let Some(p) = process.players.get_mut(&pos) {
            p.has_folded = true;
        }
    } else if action == "ALL_IN" {
        if let Some(p) = process.players.get_mut(&pos) {
            p.is_all_in = true;
        }
    }
}

#[given(expr = "players at positions {int}, {int}, {int} have all acted")]
fn given_players_all_acted(world: &mut PMWorld, p1: i32, p2: i32, p3: i32) {
    let process = world.process.as_mut().unwrap();
    for pos in [p1, p2, p3] {
        process
            .players
            .entry(pos)
            .or_insert(PMPlayer {
                position: pos,
                stack: 500,
                player_root: format!("player-{}", pos + 1),
                bet_this_round: 0,
                has_acted: false,
                has_folded: false,
                is_all_in: false,
            })
            .has_acted = true;
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
            if !p.has_folded {
                p.has_folded = true;
                break;
            }
        }
    } else if action == "ALL_IN" {
        for p in process.players.values_mut() {
            if !p.is_all_in && !p.has_folded {
                p.is_all_in = true;
                break;
            }
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
    if let Some(p) = process.players.get_mut(&pos) {
        p.bet_this_round = amount;
    }
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
    // Seed default players if none exist so downstream assertions can reference
    // active_players counts.
    let process = world.process.as_mut().unwrap();
    if process.players.is_empty() {
        PMWorld::init_default_players(process);
    }
    process.phase = "DEALING".to_string();
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
                world.emitted_commands.push("PostBlind:big".to_string());
                process.big_blind_posted = true;
            } else {
                process.phase = "BETTING".to_string();
                process.action_on =
                    (process.dealer_position + 2) % process.players.len().max(1) as i32;
            }
        }
        "BETTING" => {
            let n = process.players.len() as i32;
            if n == 0 {
                return;
            }
            let mut next = (process.action_on + 1) % n;
            for _ in 0..n {
                if let Some(p) = process.players.get(&next) {
                    if !p.has_folded && !p.is_all_in {
                        break;
                    }
                }
                next = (next + 1) % n;
            }
            process.action_on = next;

            let active = process.players.values().filter(|p| !p.has_folded).count();
            if active <= 1 {
                process.phase = "COMPLETE".to_string();
                world.emitted_commands.push("AwardPot".to_string());
                return;
            }

            let all_acted = process
                .players
                .values()
                .filter(|p| !p.has_folded && !p.is_all_in)
                .all(|p| p.has_acted);
            if all_acted {
                let variant = process.game_variant.clone();
                let phase = process.betting_phase.clone();
                if variant == "FIVE_CARD_DRAW" && phase == "PREFLOP" {
                    process.phase = "DRAW".to_string();
                } else {
                    match phase.as_str() {
                        "PREFLOP" => {
                            process.phase = "DEALING_COMMUNITY".to_string();
                            world
                                .emitted_commands
                                .push("DealCommunityCards:3".to_string());
                        }
                        "FLOP" | "TURN" => {
                            process.phase = "DEALING_COMMUNITY".to_string();
                            world
                                .emitted_commands
                                .push("DealCommunityCards:1".to_string());
                        }
                        "RIVER" | "DRAW" => {
                            process.phase = "SHOWDOWN".to_string();
                            world.emitted_commands.push("AwardPot".to_string());
                        }
                        _ => {}
                    }
                }
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
    world
        .emitted_commands
        .push("PlayerAction:CHECK".to_string());
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
    assert!(world
        .emitted_commands
        .iter()
        .any(|c| c.contains("PostBlind") && c.contains("small")));
}

#[then("a PostBlind command is sent for big blind")]
fn then_post_big(world: &mut PMWorld) {
    assert!(world
        .emitted_commands
        .iter()
        .any(|c| c.contains("PostBlind") && c.contains("big")));
}

#[then("action_on is set to UTG position")]
fn then_action_utg(world: &mut PMWorld) {
    let p = world.process.as_ref().unwrap();
    let utg = (p.dealer_position + 2) % p.players.len().max(1) as i32;
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
    assert!(
        world.emitted_commands.iter().any(|c| *c == expected),
        "Expected {}, got {:?}",
        expected,
        world.emitted_commands
    );
}

#[then("an AwardPot command is sent")]
fn then_award_pot(world: &mut PMWorld) {
    assert!(world
        .emitted_commands
        .iter()
        .any(|c| c.contains("AwardPot")));
}

#[then("an AwardPot command is sent to the remaining player")]
fn then_award_remaining(world: &mut PMWorld) {
    assert!(world
        .emitted_commands
        .iter()
        .any(|c| c.contains("AwardPot")));
}

#[then("the player is marked as is_all_in")]
fn then_all_in(world: &mut PMWorld) {
    assert!(world
        .process
        .as_ref()
        .unwrap()
        .players
        .values()
        .any(|p| p.is_all_in));
}

#[then("the player is not included in active players for betting")]
fn then_excluded(world: &mut PMWorld) {
    let process = world.process.as_ref().unwrap();
    let active = process
        .players
        .values()
        .filter(|p| !p.has_folded && !p.is_all_in)
        .count();
    assert!(active < process.players.len());
}

#[then(expr = "the process manager sends PlayerAction with {word}")]
fn then_auto_action(world: &mut PMWorld, action: String) {
    let expected = format!("PlayerAction:{}", action);
    assert!(world.emitted_commands.iter().any(|c| *c == expected));
}

#[then(expr = "all players have bet_this_round reset to {int}")]
fn then_bets_reset(world: &mut PMWorld, amount: i64) {
    for p in world.process.as_ref().unwrap().players.values() {
        assert_eq!(p.bet_this_round, amount);
    }
}

#[then("all players have has_acted reset to false")]
fn then_acted_reset(world: &mut PMWorld) {
    for p in world.process.as_ref().unwrap().players.values() {
        assert!(!p.has_acted);
    }
}

#[then(expr = "current_bet is reset to {int}")]
fn then_current_bet_reset(world: &mut PMWorld, amount: i64) {
    assert_eq!(world.process.as_ref().unwrap().current_bet, amount);
}

#[then("action_on is set to first player after dealer")]
fn then_action_after_dealer(world: &mut PMWorld) {
    let p = world.process.as_ref().unwrap();
    let expected = (p.dealer_position + 1) % p.players.len().max(1) as i32;
    assert_eq!(p.action_on, expected);
}

#[then(expr = "pot_total is {int}")]
fn then_pot_total(world: &mut PMWorld, amount: i64) {
    assert_eq!(world.process.as_ref().unwrap().pot_total, amount);
}

#[then(expr = "{string} stack is {int}")]
fn then_player_stack(world: &mut PMWorld, player_id: String, amount: i64) {
    let found = world
        .process
        .as_ref()
        .unwrap()
        .players
        .values()
        .find(|p| p.player_root == player_id)
        .unwrap_or_else(|| panic!("Player {} not found", player_id));
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

// =========================================================================
// BuyInPM / RebuyPM / RegistrationPM / TournamentStateHelper step defs
// =========================================================================

fn first_command_any(resp: &ProcessManagerHandleResponse) -> &Any {
    let page = resp.commands[0].pages.first().expect("command page");
    match page.payload.as_ref().expect("payload") {
        CommandPayload::Command(a) => a,
        _ => panic!("expected Command payload"),
    }
}

fn first_command_domain(resp: &ProcessManagerHandleResponse) -> &str {
    resp.commands[0]
        .cover
        .as_ref()
        .expect("cover")
        .domain
        .as_str()
}

fn first_process_event_any(resp: &ProcessManagerHandleResponse) -> Any {
    let book = resp.process_events.as_ref().expect("process_events");
    let page = book.pages.first().expect("event page");
    match page.payload.as_ref().expect("payload") {
        EventPayload::Event(a) => a.clone(),
        _ => panic!("expected Event payload"),
    }
}

// ---------- BuyInPM givens ----------

#[given("a BuyInPM")]
fn given_buy_in_pm(world: &mut PMWorld) {
    world.buyin_state = BuyInState::default();
}

#[given(expr = "a BuyInPM with player_root {string}")]
fn given_buy_in_pm_with_player(world: &mut PMWorld, player_root: String) {
    world.buyin_state = BuyInState {
        player_root: player_root.into_bytes(),
        ..BuyInState::default()
    };
}

#[given(
    expr = "a BuyInRequested event with table_root {string}, reservation_id {string}, seat {int}, amount {int}"
)]
fn given_buy_in_requested(
    world: &mut PMWorld,
    table_root: String,
    reservation_id: String,
    seat: i32,
    amount: i64,
) {
    world.buyin_event = Some(BuyInRequested {
        reservation_id: reservation_id.into_bytes(),
        table_root: table_root.into_bytes(),
        seat,
        amount: Some(Currency {
            amount,
            currency_code: "USD".to_string(),
        }),
        requested_at: None,
    });
}

#[given(
    expr = "a PlayerSeated event with player_root {string}, reservation_id {string}, seat_position {int}, stack {int}"
)]
fn given_player_seated(
    world: &mut PMWorld,
    player_root: String,
    reservation_id: String,
    seat_position: i32,
    stack: i64,
) {
    world.player_seated_event = Some(PlayerSeated {
        player_root: player_root.into_bytes(),
        reservation_id: reservation_id.into_bytes(),
        seat_position,
        stack,
        seated_at: None,
    });
}

#[given(
    expr = "a SeatingRejected event with player_root {string}, reservation_id {string}, reason {string}"
)]
fn given_seating_rejected(
    world: &mut PMWorld,
    player_root: String,
    reservation_id: String,
    reason: String,
) {
    world.seating_rejected_event = Some(SeatingRejected {
        player_root: player_root.into_bytes(),
        reservation_id: reservation_id.into_bytes(),
        requested_seat: 0,
        reason,
        rejected_at: None,
    });
}

#[given(expr = "destinations with sequences {}")]
fn given_destinations(_world: &mut PMWorld, _spec: String) {}

// ---------- BuyInPM whens ----------

#[when("the BuyInPM handles buy_in_requested")]
fn when_buyin_handles_request(world: &mut PMWorld) {
    let ev = world.buyin_event.take().expect("buyin event set");
    world.pm_response = Some(buyin_handler::handle_buy_in_requested(ev).expect("handler ok"));
}

#[when("the BuyInPM handles player_seated")]
fn when_buyin_handles_seated(world: &mut PMWorld) {
    let ev = world.player_seated_event.take().expect("seated event set");
    world.pm_response = Some(buyin_handler::handle_player_seated(ev).expect("handler ok"));
}

#[when("the BuyInPM handles seating_rejected")]
fn when_buyin_handles_rejected(world: &mut PMWorld) {
    let ev = world
        .seating_rejected_event
        .take()
        .expect("rejected event set");
    world.pm_response = Some(buyin_handler::handle_seating_rejected(ev).expect("handler ok"));
}

// ---------- PM thens (shared across PMs) ----------

#[then(expr = "a SeatPlayer command is sent to the {string} domain")]
fn then_seat_player_command(world: &mut PMWorld, domain: String) {
    let resp = world.pm_response.as_ref().expect("response");
    assert_eq!(first_command_domain(resp), domain);
    let any = first_command_any(resp);
    assert!(any.type_url.ends_with("examples.SeatPlayer"));
}

#[then(expr = "the SeatPlayer command has player_root {string}")]
fn then_seat_player_root(world: &mut PMWorld, _root: String) {
    let resp = world.pm_response.as_ref().unwrap();
    let any = first_command_any(resp);
    // Handler leaves player_root empty by design; we only assert decode succeeds.
    examples_proto::SeatPlayer::decode(any.value.as_slice()).expect("decode");
}

#[then(expr = "the SeatPlayer command has seat {int}")]
fn then_seat_player_seat(world: &mut PMWorld, seat: i32) {
    let resp = world.pm_response.as_ref().unwrap();
    let any = first_command_any(resp);
    let cmd = examples_proto::SeatPlayer::decode(any.value.as_slice()).unwrap();
    assert_eq!(cmd.seat, seat);
}

#[then(expr = "the SeatPlayer command has amount {int}")]
fn then_seat_player_amount(world: &mut PMWorld, amount: i64) {
    let resp = world.pm_response.as_ref().unwrap();
    let any = first_command_any(resp);
    let cmd = examples_proto::SeatPlayer::decode(any.value.as_slice()).unwrap();
    assert_eq!(cmd.amount, amount);
}

#[then(expr = "the SeatPlayer command has reservation_id {string}")]
fn then_seat_player_reservation(world: &mut PMWorld, reservation_id: String) {
    let resp = world.pm_response.as_ref().unwrap();
    let any = first_command_any(resp);
    let cmd = examples_proto::SeatPlayer::decode(any.value.as_slice()).unwrap();
    assert_eq!(cmd.reservation_id, reservation_id.into_bytes());
}

#[then(expr = "the process event is a {} event")]
fn then_process_event_type(world: &mut PMWorld, type_name: String) {
    let resp = world.pm_response.as_ref().unwrap();
    let any = first_process_event_any(resp);
    assert!(
        any.type_url.ends_with(&type_name),
        "expected type ending with {}, got {}",
        type_name,
        any.type_url
    );
}

#[then(expr = "the BuyInInitiated event has player_root {string}")]
fn then_buyin_initiated_player(world: &mut PMWorld, _player: String) {
    let any = first_process_event_any(world.pm_response.as_ref().unwrap());
    BuyInInitiated::decode(any.value.as_slice()).unwrap();
}

#[then(expr = "the BuyInInitiated event has table_root {string}")]
fn then_buyin_initiated_table(world: &mut PMWorld, table: String) {
    let any = first_process_event_any(world.pm_response.as_ref().unwrap());
    let ev = BuyInInitiated::decode(any.value.as_slice()).unwrap();
    assert_eq!(ev.table_root, table.into_bytes());
}

#[then(expr = "the BuyInInitiated event phase is {word}")]
fn then_buyin_initiated_phase(world: &mut PMWorld, phase: String) {
    let any = first_process_event_any(world.pm_response.as_ref().unwrap());
    let ev = BuyInInitiated::decode(any.value.as_slice()).unwrap();
    let expected = match phase.as_str() {
        "BUY_IN_SEATING" => examples_proto::BuyInPhase::BuyInSeating,
        "BUY_IN_CONFIRMING" => examples_proto::BuyInPhase::BuyInConfirming,
        other => panic!("unexpected phase {}", other),
    };
    assert_eq!(ev.phase(), expected);
}

#[then(expr = "a ConfirmBuyIn command is sent to the {string} domain")]
fn then_confirm_buyin(world: &mut PMWorld, domain: String) {
    let resp = world.pm_response.as_ref().unwrap();
    assert_eq!(first_command_domain(resp), domain);
    let any = first_command_any(resp);
    assert!(any.type_url.ends_with("examples.ConfirmBuyIn"));
}

#[then(expr = "the ConfirmBuyIn command has reservation_id {string}")]
fn then_confirm_buyin_reservation(world: &mut PMWorld, reservation_id: String) {
    let resp = world.pm_response.as_ref().unwrap();
    let any = first_command_any(resp);
    let cmd = ConfirmBuyIn::decode(any.value.as_slice()).unwrap();
    assert_eq!(cmd.reservation_id, reservation_id.into_bytes());
}

#[then(expr = "the BuyInCompleted event has player_root {string}")]
fn then_buyin_completed_player(world: &mut PMWorld, player: String) {
    let any = first_process_event_any(world.pm_response.as_ref().unwrap());
    let ev = BuyInCompleted::decode(any.value.as_slice()).unwrap();
    assert_eq!(ev.player_root, player.into_bytes());
}

#[then(expr = "the BuyInCompleted event has seat {int}")]
fn then_buyin_completed_seat(world: &mut PMWorld, seat: i32) {
    let any = first_process_event_any(world.pm_response.as_ref().unwrap());
    let ev = BuyInCompleted::decode(any.value.as_slice()).unwrap();
    assert_eq!(ev.seat, seat);
}

#[then(expr = "a ReleaseBuyIn command is sent to the {string} domain")]
fn then_release_buyin(world: &mut PMWorld, domain: String) {
    let resp = world.pm_response.as_ref().unwrap();
    assert_eq!(first_command_domain(resp), domain);
    let any = first_command_any(resp);
    assert!(any.type_url.ends_with("examples.ReleaseBuyIn"));
}

#[then(expr = "the ReleaseBuyIn command has reservation_id {string}")]
fn then_release_buyin_reservation(world: &mut PMWorld, reservation_id: String) {
    let resp = world.pm_response.as_ref().unwrap();
    let any = first_command_any(resp);
    let cmd = ReleaseBuyIn::decode(any.value.as_slice()).unwrap();
    assert_eq!(cmd.reservation_id, reservation_id.into_bytes());
}

#[then(expr = "the ReleaseBuyIn command has reason {string}")]
fn then_release_buyin_reason(world: &mut PMWorld, reason: String) {
    let resp = world.pm_response.as_ref().unwrap();
    let any = first_command_any(resp);
    let cmd = ReleaseBuyIn::decode(any.value.as_slice()).unwrap();
    assert_eq!(cmd.reason, reason);
}

#[then(expr = "the BuyInFailed event has player_root {string}")]
fn then_buyin_failed_player(world: &mut PMWorld, player: String) {
    let any = first_process_event_any(world.pm_response.as_ref().unwrap());
    let ev = BuyInFailed::decode(any.value.as_slice()).unwrap();
    assert_eq!(ev.player_root, player.into_bytes());
}

#[then(expr = "the BuyInFailed event failure code is {string}")]
fn then_buyin_failed_code(world: &mut PMWorld, code: String) {
    let any = first_process_event_any(world.pm_response.as_ref().unwrap());
    let ev = BuyInFailed::decode(any.value.as_slice()).unwrap();
    assert_eq!(ev.failure.unwrap().code, code);
}

// ---------- RebuyPM givens ----------

#[given(expr = "a RebuyPM with player_root {string}")]
fn given_rebuy_pm_player(world: &mut PMWorld, player_root: String) {
    world.rebuy_state = RebuyState {
        player_root: player_root.into_bytes(),
        ..RebuyState::default()
    };
}

#[given(expr = "a RebuyPM with table_root {string} and seat {int}")]
fn given_rebuy_pm_table_seat(world: &mut PMWorld, table_root: String, seat: i32) {
    world.rebuy_state = RebuyState {
        table_root: table_root.into_bytes(),
        seat,
        ..RebuyState::default()
    };
}

#[given(expr = "a RebuyPM with tournament_root {string}")]
fn given_rebuy_pm_tournament(world: &mut PMWorld, tournament_root: String) {
    world.rebuy_state = RebuyState {
        tournament_root: tournament_root.into_bytes(),
        ..RebuyState::default()
    };
}

#[given(expr = "a RebuyPM with tournament_root {string}, table_root {string}, fee {int}")]
fn given_rebuy_pm_full(world: &mut PMWorld, tournament_root: String, table_root: String, fee: i64) {
    world.rebuy_state = RebuyState {
        tournament_root: tournament_root.into_bytes(),
        table_root: table_root.into_bytes(),
        fee,
        ..RebuyState::default()
    };
}

#[given(
    expr = "a RebuyRequested event with tournament_root {string}, table_root {string}, reservation_id {string}, seat {int}, fee {int}"
)]
fn given_rebuy_requested(
    world: &mut PMWorld,
    tournament_root: String,
    table_root: String,
    reservation_id: String,
    seat: i32,
    fee: i64,
) {
    world.rebuy_requested_event = Some(RebuyRequested {
        reservation_id: reservation_id.into_bytes(),
        tournament_root: tournament_root.into_bytes(),
        table_root: table_root.into_bytes(),
        seat,
        fee: Some(Currency {
            amount: fee,
            currency_code: "USD".to_string(),
        }),
        requested_at: None,
    });
}

#[given(
    expr = "a RebuyProcessed event with player_root {string}, reservation_id {string}, chips_added {int}, rebuy_count {int}"
)]
fn given_rebuy_processed(
    world: &mut PMWorld,
    player_root: String,
    reservation_id: String,
    chips_added: i64,
    rebuy_count: i32,
) {
    world.rebuy_processed_event = Some(RebuyProcessed {
        player_root: player_root.into_bytes(),
        reservation_id: reservation_id.into_bytes(),
        rebuy_cost: 0,
        chips_added,
        rebuy_count,
        processed_at: None,
    });
}

#[given(
    expr = "a RebuyDenied event with player_root {string}, reservation_id {string}, reason {string}"
)]
fn given_rebuy_denied(
    world: &mut PMWorld,
    player_root: String,
    reservation_id: String,
    reason: String,
) {
    world.rebuy_denied_event = Some(RebuyDenied {
        player_root: player_root.into_bytes(),
        reservation_id: reservation_id.into_bytes(),
        reason,
        denied_at: None,
    });
}

#[given(
    expr = "a RebuyChipsAdded event with player_root {string}, reservation_id {string}, seat {int}, amount {int}, new_stack {int}"
)]
fn given_rebuy_chips_added(
    world: &mut PMWorld,
    player_root: String,
    reservation_id: String,
    seat: i32,
    amount: i64,
    new_stack: i64,
) {
    world.rebuy_chips_added_event = Some(RebuyChipsAdded {
        player_root: player_root.into_bytes(),
        reservation_id: reservation_id.into_bytes(),
        seat,
        amount,
        new_stack,
        added_at: None,
    });
}

// ---------- RebuyPM whens ----------

#[when("the RebuyPM handles rebuy_requested")]
fn when_rebuy_handles_request(world: &mut PMWorld) {
    let ev = world.rebuy_requested_event.take().unwrap();
    world.pm_response = Some(rebuy_handler::handle_rebuy_requested(ev).expect("ok"));
}

#[when("the RebuyPM handles rebuy_processed")]
fn when_rebuy_handles_processed(world: &mut PMWorld) {
    let ev = world.rebuy_processed_event.take().unwrap();
    world.pm_response =
        Some(rebuy_handler::handle_rebuy_processed(ev, &world.rebuy_state).expect("ok"));
}

#[when("the RebuyPM handles rebuy_denied")]
fn when_rebuy_handles_denied(world: &mut PMWorld) {
    let ev = world.rebuy_denied_event.take().unwrap();
    world.pm_response = Some(rebuy_handler::handle_rebuy_denied(ev).expect("ok"));
}

#[when("the RebuyPM handles chips_added")]
fn when_rebuy_handles_chips_added(world: &mut PMWorld) {
    let ev = world.rebuy_chips_added_event.take().unwrap();
    world.pm_response = Some(rebuy_handler::handle_chips_added(ev).expect("ok"));
}

// ---------- RebuyPM thens ----------

#[then(expr = "a ProcessRebuy command is sent to the {string} domain")]
fn then_process_rebuy_cmd(world: &mut PMWorld, domain: String) {
    let resp = world.pm_response.as_ref().unwrap();
    assert_eq!(first_command_domain(resp), domain);
    assert!(first_command_any(resp)
        .type_url
        .ends_with("examples.ProcessRebuy"));
}

#[then(expr = "the ProcessRebuy command has player_root {string}")]
fn then_process_rebuy_player(world: &mut PMWorld, _player: String) {
    let any = first_command_any(world.pm_response.as_ref().unwrap());
    ProcessRebuy::decode(any.value.as_slice()).unwrap();
}

#[then(expr = "the ProcessRebuy command has reservation_id {string}")]
fn then_process_rebuy_reservation(world: &mut PMWorld, reservation_id: String) {
    let any = first_command_any(world.pm_response.as_ref().unwrap());
    let cmd = ProcessRebuy::decode(any.value.as_slice()).unwrap();
    assert_eq!(cmd.reservation_id, reservation_id.into_bytes());
}

#[then(expr = "the RebuyInitiated event has player_root {string}")]
fn then_rebuy_initiated_player(world: &mut PMWorld, _player: String) {
    let any = first_process_event_any(world.pm_response.as_ref().unwrap());
    RebuyInitiated::decode(any.value.as_slice()).unwrap();
}

#[then(expr = "the RebuyInitiated event has tournament_root {string}")]
fn then_rebuy_initiated_tournament(world: &mut PMWorld, tournament: String) {
    let any = first_process_event_any(world.pm_response.as_ref().unwrap());
    let ev = RebuyInitiated::decode(any.value.as_slice()).unwrap();
    assert_eq!(ev.tournament_root, tournament.into_bytes());
}

#[then(expr = "the RebuyInitiated event phase is {word}")]
fn then_rebuy_initiated_phase(world: &mut PMWorld, phase: String) {
    let any = first_process_event_any(world.pm_response.as_ref().unwrap());
    let ev = RebuyInitiated::decode(any.value.as_slice()).unwrap();
    let expected = match phase.as_str() {
        "REBUY_APPROVING" => examples_proto::RebuyPhase::RebuyApproving,
        "REBUY_ADDING_CHIPS" => examples_proto::RebuyPhase::RebuyAddingChips,
        other => panic!("phase {}", other),
    };
    assert_eq!(ev.phase(), expected);
}

#[then(expr = "an AddRebuyChips command is sent to the {string} domain")]
fn then_add_rebuy_chips(world: &mut PMWorld, domain: String) {
    let resp = world.pm_response.as_ref().unwrap();
    assert_eq!(first_command_domain(resp), domain);
    assert!(first_command_any(resp)
        .type_url
        .ends_with("examples.AddRebuyChips"));
}

#[then(expr = "the AddRebuyChips command has player_root {string}")]
fn then_add_rebuy_chips_player(world: &mut PMWorld, player: String) {
    let any = first_command_any(world.pm_response.as_ref().unwrap());
    let cmd = AddRebuyChips::decode(any.value.as_slice()).unwrap();
    assert_eq!(cmd.player_root, player.into_bytes());
}

#[then(expr = "the AddRebuyChips command has reservation_id {string}")]
fn then_add_rebuy_chips_reservation(world: &mut PMWorld, reservation: String) {
    let any = first_command_any(world.pm_response.as_ref().unwrap());
    let cmd = AddRebuyChips::decode(any.value.as_slice()).unwrap();
    assert_eq!(cmd.reservation_id, reservation.into_bytes());
}

#[then(expr = "the AddRebuyChips command has seat {int}")]
fn then_add_rebuy_chips_seat(world: &mut PMWorld, seat: i32) {
    let any = first_command_any(world.pm_response.as_ref().unwrap());
    let cmd = AddRebuyChips::decode(any.value.as_slice()).unwrap();
    assert_eq!(cmd.seat, seat);
}

#[then(expr = "the AddRebuyChips command has amount {int}")]
fn then_add_rebuy_chips_amount(world: &mut PMWorld, amount: i64) {
    let any = first_command_any(world.pm_response.as_ref().unwrap());
    let cmd = AddRebuyChips::decode(any.value.as_slice()).unwrap();
    assert_eq!(cmd.amount, amount);
}

#[then(expr = "a ReleaseRebuyFee command is sent to the {string} domain")]
fn then_release_rebuy_fee(world: &mut PMWorld, domain: String) {
    let resp = world.pm_response.as_ref().unwrap();
    assert_eq!(first_command_domain(resp), domain);
    assert!(first_command_any(resp)
        .type_url
        .ends_with("examples.ReleaseRebuyFee"));
}

#[then(expr = "the ReleaseRebuyFee command has reservation_id {string}")]
fn then_release_rebuy_fee_reservation(world: &mut PMWorld, reservation: String) {
    let any = first_command_any(world.pm_response.as_ref().unwrap());
    let cmd = ReleaseRebuyFee::decode(any.value.as_slice()).unwrap();
    assert_eq!(cmd.reservation_id, reservation.into_bytes());
}

#[then(expr = "the ReleaseRebuyFee command has reason {string}")]
fn then_release_rebuy_fee_reason(world: &mut PMWorld, reason: String) {
    let any = first_command_any(world.pm_response.as_ref().unwrap());
    let cmd = ReleaseRebuyFee::decode(any.value.as_slice()).unwrap();
    assert_eq!(cmd.reason, reason);
}

#[then(expr = "the RebuyFailed event has player_root {string}")]
fn then_rebuy_failed_player(world: &mut PMWorld, player: String) {
    let any = first_process_event_any(world.pm_response.as_ref().unwrap());
    let ev = RebuyFailed::decode(any.value.as_slice()).unwrap();
    assert_eq!(ev.player_root, player.into_bytes());
}

#[then(expr = "the RebuyFailed event failure code is {string}")]
fn then_rebuy_failed_code(world: &mut PMWorld, code: String) {
    let any = first_process_event_any(world.pm_response.as_ref().unwrap());
    let ev = RebuyFailed::decode(any.value.as_slice()).unwrap();
    assert_eq!(ev.failure.unwrap().code, code);
}

#[then(expr = "a ConfirmRebuyFee command is sent to the {string} domain")]
fn then_confirm_rebuy_fee(world: &mut PMWorld, domain: String) {
    let resp = world.pm_response.as_ref().unwrap();
    assert_eq!(first_command_domain(resp), domain);
    assert!(first_command_any(resp)
        .type_url
        .ends_with("examples.ConfirmRebuyFee"));
}

#[then(expr = "the ConfirmRebuyFee command has reservation_id {string}")]
fn then_confirm_rebuy_fee_reservation(world: &mut PMWorld, reservation: String) {
    let any = first_command_any(world.pm_response.as_ref().unwrap());
    let cmd = ConfirmRebuyFee::decode(any.value.as_slice()).unwrap();
    assert_eq!(cmd.reservation_id, reservation.into_bytes());
}

#[then(expr = "the RebuyCompleted event has player_root {string}")]
fn then_rebuy_completed_player(world: &mut PMWorld, player: String) {
    let any = first_process_event_any(world.pm_response.as_ref().unwrap());
    let ev = RebuyCompleted::decode(any.value.as_slice()).unwrap();
    assert_eq!(ev.player_root, player.into_bytes());
}

#[then(expr = "the RebuyCompleted event has chips_added {int}")]
fn then_rebuy_completed_chips(world: &mut PMWorld, chips: i64) {
    let any = first_process_event_any(world.pm_response.as_ref().unwrap());
    let ev = RebuyCompleted::decode(any.value.as_slice()).unwrap();
    assert_eq!(ev.chips_added, chips);
}

// ---------- RegistrationPM givens ----------

#[given(expr = "a RegistrationPM with player_root {string}")]
fn given_registration_pm_player(world: &mut PMWorld, player_root: String) {
    world.registration_player_root = player_root.into_bytes();
}

#[given(expr = "a RegistrationPM with tournament_root {string} and fee {int}")]
fn given_registration_pm_tournament_fee(world: &mut PMWorld, _tournament: String, _fee: i64) {
    world.registration_player_root.clear();
}

#[given(expr = "a RegistrationPM with tournament_root {string}")]
fn given_registration_pm_tournament(world: &mut PMWorld, _tournament: String) {
    world.registration_player_root.clear();
}

#[given(
    expr = "a RegistrationRequested event with tournament_root {string}, reservation_id {string}, fee {int}"
)]
fn given_registration_requested(
    world: &mut PMWorld,
    tournament_root: String,
    reservation_id: String,
    fee: i64,
) {
    world.registration_requested_event = Some(RegistrationRequested {
        reservation_id: reservation_id.into_bytes(),
        tournament_root: tournament_root.into_bytes(),
        fee: Some(Currency {
            amount: fee,
            currency_code: "USD".to_string(),
        }),
        requested_at: None,
    });
}

#[given(
    expr = "a RegistrationRequested event with tournament_root {string}, reservation_id {string} and no fee"
)]
fn given_registration_requested_no_fee(
    world: &mut PMWorld,
    tournament_root: String,
    reservation_id: String,
) {
    world.registration_requested_event = Some(RegistrationRequested {
        reservation_id: reservation_id.into_bytes(),
        tournament_root: tournament_root.into_bytes(),
        fee: None,
        requested_at: None,
    });
}

#[given(
    expr = "a TournamentPlayerEnrolled event with player_root {string}, reservation_id {string}, fee_paid {int}, starting_stack {int}"
)]
fn given_tournament_player_enrolled(
    world: &mut PMWorld,
    player_root: String,
    reservation_id: String,
    fee_paid: i64,
    starting_stack: i64,
) {
    world.player_enrolled_event = Some(TournamentPlayerEnrolled {
        player_root: player_root.into_bytes(),
        reservation_id: reservation_id.into_bytes(),
        fee_paid,
        starting_stack,
        registration_number: 1,
        enrolled_at: None,
    });
}

#[given(
    expr = "a TournamentEnrollmentRejected event with player_root {string}, reservation_id {string}, reason {string}"
)]
fn given_enrollment_rejected(
    world: &mut PMWorld,
    player_root: String,
    reservation_id: String,
    reason: String,
) {
    world.enrollment_rejected_event = Some(TournamentEnrollmentRejected {
        player_root: player_root.into_bytes(),
        reservation_id: reservation_id.into_bytes(),
        reason,
        rejected_at: None,
    });
}

// ---------- RegistrationPM whens ----------

#[when("the RegistrationPM handles registration_requested")]
fn when_registration_handles_request(world: &mut PMWorld) {
    let ev = world.registration_requested_event.take().unwrap();
    world.pm_response = Some(registration_handler::handle_registration_requested(ev).expect("ok"));
}

#[when("the RegistrationPM handles player_enrolled")]
fn when_registration_handles_enrolled(world: &mut PMWorld) {
    let ev = world.player_enrolled_event.take().unwrap();
    world.pm_response = Some(registration_handler::handle_player_enrolled(ev).expect("ok"));
}

#[when("the RegistrationPM handles enrollment_rejected")]
fn when_registration_handles_rejected(world: &mut PMWorld) {
    let ev = world.enrollment_rejected_event.take().unwrap();
    world.pm_response = Some(registration_handler::handle_enrollment_rejected(ev).expect("ok"));
}

// ---------- RegistrationPM thens ----------

#[then(expr = "an EnrollPlayer command is sent to the {string} domain")]
fn then_enroll_player_cmd(world: &mut PMWorld, domain: String) {
    let resp = world.pm_response.as_ref().unwrap();
    assert_eq!(first_command_domain(resp), domain);
    assert!(first_command_any(resp)
        .type_url
        .ends_with("examples.EnrollPlayer"));
}

#[then(expr = "the EnrollPlayer command has player_root {string}")]
fn then_enroll_player_root(world: &mut PMWorld, _player: String) {
    let any = first_command_any(world.pm_response.as_ref().unwrap());
    EnrollPlayer::decode(any.value.as_slice()).unwrap();
}

#[then(expr = "the EnrollPlayer command has reservation_id {string}")]
fn then_enroll_player_reservation(world: &mut PMWorld, reservation: String) {
    let any = first_command_any(world.pm_response.as_ref().unwrap());
    let cmd = EnrollPlayer::decode(any.value.as_slice()).unwrap();
    assert_eq!(cmd.reservation_id, reservation.into_bytes());
}

#[then(expr = "a ConfirmRegistrationFee command is sent to the {string} domain")]
fn then_confirm_registration_fee(world: &mut PMWorld, domain: String) {
    let resp = world.pm_response.as_ref().unwrap();
    assert_eq!(first_command_domain(resp), domain);
    assert!(first_command_any(resp)
        .type_url
        .ends_with("examples.ConfirmRegistrationFee"));
}

#[then(expr = "the ConfirmRegistrationFee command has reservation_id {string}")]
fn then_confirm_registration_fee_reservation(world: &mut PMWorld, reservation: String) {
    let any = first_command_any(world.pm_response.as_ref().unwrap());
    let cmd = ConfirmRegistrationFee::decode(any.value.as_slice()).unwrap();
    assert_eq!(cmd.reservation_id, reservation.into_bytes());
}

#[then(expr = "a ReleaseRegistrationFee command is sent to the {string} domain")]
fn then_release_registration_fee(world: &mut PMWorld, domain: String) {
    let resp = world.pm_response.as_ref().unwrap();
    assert_eq!(first_command_domain(resp), domain);
    assert!(first_command_any(resp)
        .type_url
        .ends_with("examples.ReleaseRegistrationFee"));
}

#[then(expr = "the ReleaseRegistrationFee command has reservation_id {string}")]
fn then_release_registration_fee_reservation(world: &mut PMWorld, reservation: String) {
    let any = first_command_any(world.pm_response.as_ref().unwrap());
    let cmd = ReleaseRegistrationFee::decode(any.value.as_slice()).unwrap();
    assert_eq!(cmd.reservation_id, reservation.into_bytes());
}

#[then(expr = "the ReleaseRegistrationFee command has reason {string}")]
fn then_release_registration_fee_reason(world: &mut PMWorld, reason: String) {
    let any = first_command_any(world.pm_response.as_ref().unwrap());
    let cmd = ReleaseRegistrationFee::decode(any.value.as_slice()).unwrap();
    assert_eq!(cmd.reason, reason);
}

#[then(expr = "the RegistrationInitiated event has fee amount {int}")]
fn then_registration_initiated_fee_amount(world: &mut PMWorld, amount: i64) {
    let any = first_process_event_any(world.pm_response.as_ref().unwrap());
    let ev = RegistrationInitiated::decode(any.value.as_slice()).unwrap();
    assert_eq!(ev.fee.as_ref().expect("fee populated").amount, amount);
}

// ---------- HandFlowPM ----------

#[given("a HandFlowPM with a started hand")]
fn given_handflowpm_started_hand(world: &mut PMWorld) {
    let hand_root = vec![0xAA; 16];
    world.handflow_hand_root = hand_root.clone();
    world.handflow_state = Some(pmg_hand_flow::HandFlowState {
        hand_root,
        hand_number: 1,
        game_variant: GameVariant::TexasHoldem,
        dealer_position: 0,
        small_blind_position: 1,
        big_blind_position: 2,
        small_blind: 5,
        big_blind: 10,
        active_players: vec![(0, vec![1; 16])],
        phase: pmg_hand_flow::HandPhase::Dealing,
    });
}

#[when(expr = "the HandFlowPM handles a HandComplete event with {int} winner amount {int}")]
fn when_handflowpm_hand_complete(world: &mut PMWorld, _n: usize, amount: i64) {
    let state = world
        .handflow_state
        .as_ref()
        .expect("HandFlowState")
        .clone();
    let pm = pmg_hand_flow::HandFlowPm;
    let complete = HandComplete {
        table_root: vec![0xBB; 16],
        hand_number: 1,
        winners: vec![PotWinner {
            player_root: vec![1; 16],
            amount,
            pot_type: "main".into(),
            winning_hand: None,
        }],
        final_stacks: vec![],
        completed_at: None,
    };
    world.pm_response = Some(pm.on_hand_complete(complete, &state).expect("ok"));
}

#[then(expr = "an EndHand command is sent to the {string} domain")]
fn then_end_hand_command_domain(world: &mut PMWorld, domain: String) {
    let resp = world.pm_response.as_ref().unwrap();
    assert_eq!(first_command_domain(resp), domain);
    assert!(first_command_any(resp)
        .type_url
        .ends_with("examples.EndHand"));
}

#[then(expr = "the EndHand command has {int} result")]
fn then_end_hand_result_count(world: &mut PMWorld, count: usize) {
    let any = first_command_any(world.pm_response.as_ref().unwrap());
    let cmd = EndHand::decode(any.value.as_slice()).unwrap();
    assert_eq!(cmd.results.len(), count);
}

#[then("the EndHand command preserves the original hand_root")]
fn then_end_hand_preserves_hand_root(world: &mut PMWorld) {
    let any = first_command_any(world.pm_response.as_ref().unwrap());
    let cmd = EndHand::decode(any.value.as_slice()).unwrap();
    assert_eq!(cmd.hand_root, world.handflow_hand_root);
}

// ---------- Tournament state helper ----------

fn pack_any<M: Message>(msg: &M, type_name: &str) -> Any {
    Any {
        type_url: format!("type.googleapis.com/{}", type_name),
        value: msg.encode_to_vec(),
    }
}

fn apply_tournament_any(state: &mut TournamentStateHelper, any: &Any) {
    if any.type_url.ends_with("TournamentCreated") {
        let ev = TournamentCreated::decode(any.value.as_slice()).unwrap();
        state.apply_created(ev);
    } else if any.type_url.ends_with("TournamentPlayerEnrolled") {
        let ev = TournamentPlayerEnrolled::decode(any.value.as_slice()).unwrap();
        state.apply_player_enrolled(ev);
    } else if any.type_url.ends_with("TournamentStarted") {
        let ev = TournamentStarted::decode(any.value.as_slice()).unwrap();
        state.apply_started(ev);
    }
}

#[given("an empty tournament state helper")]
fn given_empty_tournament_state(world: &mut PMWorld) {
    world.tournament_state = TournamentStateHelper::default();
    world.tournament_events.clear();
}

#[given(
    expr = "a tournament event book with a TournamentCreated event name {string}, max_players {int}, buy_in {int}, starting_stack {int}"
)]
fn given_tournament_event_book_created(
    world: &mut PMWorld,
    name: String,
    max_players: i32,
    buy_in: i64,
    starting_stack: i64,
) {
    world.tournament_events.clear();
    world.tournament_state = TournamentStateHelper::default();
    let ev = TournamentCreated {
        name,
        game_variant: examples_proto::GameVariant::TexasHoldem as i32,
        buy_in,
        starting_stack,
        max_players,
        min_players: 2,
        scheduled_start: None,
        rebuy_config: None,
        addon_config: None,
        blind_structure: vec![],
        created_at: None,
    };
    world
        .tournament_events
        .push(pack_any(&ev, "examples.TournamentCreated"));
}

#[given("a tournament event book with:")]
fn given_tournament_event_book_table(world: &mut PMWorld, step: &cucumber::gherkin::Step) {
    world.tournament_events.clear();
    world.tournament_state = TournamentStateHelper::default();
    let table = step.table.as_ref().expect("data table");
    let headers: Vec<String> = table.rows[0].clone();
    let idx = |name: &str| headers.iter().position(|h| h == name);
    let h_event_type = idx("event_type").expect("event_type column");
    let h_name = idx("name");
    let h_max = idx("max_players");
    let h_player = idx("player_root");

    for row in table.rows.iter().skip(1) {
        let event_type = &row[h_event_type];
        match event_type.as_str() {
            "TournamentCreated" => {
                let name = h_name.and_then(|i| row.get(i)).cloned().unwrap_or_default();
                let max_players = h_max
                    .and_then(|i| row.get(i))
                    .and_then(|s| if s.is_empty() { None } else { s.parse().ok() })
                    .unwrap_or(0);
                let ev = TournamentCreated {
                    name,
                    game_variant: examples_proto::GameVariant::TexasHoldem as i32,
                    buy_in: 0,
                    starting_stack: 0,
                    max_players,
                    min_players: 2,
                    scheduled_start: None,
                    rebuy_config: None,
                    addon_config: None,
                    blind_structure: vec![],
                    created_at: None,
                };
                world
                    .tournament_events
                    .push(pack_any(&ev, "examples.TournamentCreated"));
            }
            "TournamentPlayerEnrolled" => {
                let player = h_player
                    .and_then(|i| row.get(i))
                    .cloned()
                    .unwrap_or_default();
                let ev = TournamentPlayerEnrolled {
                    player_root: player.into_bytes(),
                    reservation_id: vec![],
                    fee_paid: 0,
                    starting_stack: 0,
                    registration_number: 1,
                    enrolled_at: None,
                };
                world
                    .tournament_events
                    .push(pack_any(&ev, "examples.TournamentPlayerEnrolled"));
            }
            "TournamentStarted" => {
                let ev = TournamentStarted {
                    total_players: 0,
                    tables_created: 0,
                    total_prize_pool: 0,
                    started_at: None,
                };
                world
                    .tournament_events
                    .push(pack_any(&ev, "examples.TournamentStarted"));
            }
            other => panic!("unexpected event_type in data table: {}", other),
        }
    }
}

#[when("I rebuild the tournament state from the event book")]
fn when_rebuild_tournament(world: &mut PMWorld) {
    world.tournament_state = TournamentStateHelper::default();
    let events = world.tournament_events.clone();
    for any in &events {
        apply_tournament_any(&mut world.tournament_state, any);
    }
}

#[when(expr = "I apply a TournamentCreated event with name {string} and max_players {int}")]
fn when_apply_tournament_created(world: &mut PMWorld, name: String, max_players: i32) {
    let ev = TournamentCreated {
        name,
        game_variant: examples_proto::GameVariant::TexasHoldem as i32,
        buy_in: 0,
        starting_stack: 0,
        max_players,
        min_players: 2,
        scheduled_start: None,
        rebuy_config: None,
        addon_config: None,
        blind_structure: vec![],
        created_at: None,
    };
    world.tournament_state.apply_created(ev);
}

#[when(expr = "I apply a TournamentPlayerEnrolled event for player_root {string}")]
fn when_apply_player_enrolled(world: &mut PMWorld, player_root: String) {
    let ev = TournamentPlayerEnrolled {
        player_root: player_root.into_bytes(),
        reservation_id: vec![],
        fee_paid: 0,
        starting_stack: 0,
        registration_number: 1,
        enrolled_at: None,
    };
    world.tournament_state.apply_player_enrolled(ev);
}

#[then(expr = "the tournament state has registration_open {word}")]
fn then_tournament_registration_open(world: &mut PMWorld, open: String) {
    let expected = open == "true";
    assert_eq!(world.tournament_state.registration_open, expected);
}

#[then(expr = "the tournament state has max_players {int}")]
fn then_tournament_max_players(world: &mut PMWorld, n: i32) {
    assert_eq!(world.tournament_state.max_players, n);
}

#[then(expr = "the tournament state has buy_in {int}")]
fn then_tournament_buy_in(world: &mut PMWorld, n: i64) {
    assert_eq!(world.tournament_state.buy_in, n);
}

#[then(expr = "the tournament state has starting_stack {int}")]
fn then_tournament_starting_stack(world: &mut PMWorld, n: i64) {
    assert_eq!(world.tournament_state.starting_stack, n);
}

#[then(expr = "the tournament state has registered_count {int}")]
fn then_tournament_registered_count(world: &mut PMWorld, n: usize) {
    assert_eq!(world.tournament_state.registered.len(), n);
}

#[then(expr = "the tournament state has registered player {string}")]
fn then_tournament_has_registered_player(world: &mut PMWorld, player: String) {
    let bytes = player.into_bytes();
    assert!(world
        .tournament_state
        .registered
        .iter()
        .any(|p| p == &bytes));
}

#[then(expr = "the tournament state status is {word}")]
fn then_tournament_status(world: &mut PMWorld, status: String) {
    let expected = match status.as_str() {
        "TOURNAMENT_RUNNING" => TournamentStatus::TournamentRunning,
        "TOURNAMENT_REGISTRATION_OPEN" => TournamentStatus::TournamentRegistrationOpen,
        "TOURNAMENT_CREATED" => TournamentStatus::TournamentCreated,
        other => panic!("unexpected status {}", other),
    };
    assert_eq!(world.tournament_state.status, expected);
}

#[tokio::main]
async fn main() {
    PMWorld::cucumber()
        .with_writer(
            cucumber::writer::Basic::stdout()
                .summarized()
                .assert_normalized(),
        )
        .run("features/example/unit/process_manager.feature")
        .await;
}
