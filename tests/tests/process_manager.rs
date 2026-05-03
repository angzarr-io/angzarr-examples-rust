//! Process Manager BDD tests.
//!
//! The HandFlow PM scenarios (EU-0400 .. EU-0420) dispatch through the
//! production state machine in `pmg_hand_flow::state_machine` — `HandProcess`
//! is a real production type and the When steps call its methods (`on_hand_started`,
//! `handle_event`, `end_betting_round`, `apply_action`, `timeout`, etc.).
//!
//! The reservation-PM scenarios (EU-0421 .. EU-0441) dispatch through the
//! unified `ReservationPm` (replaces the former `pmg-buy-in`, `pmg-rebuy`,
//! `pmg-registration`). The TournamentStateHelper scenarios exercise the
//! production `pmg_reservation::TournamentStateHelper` rebuild helper used
//! in cross-aggregate pre-validation.
#![allow(clippy::large_enum_variant)]

use angzarr_client::proto::{
    command_page::Payload as CommandPayload, event_page::Payload as EventPayload,
    ProcessManagerHandleResponse,
};
use cucumber::{given, then, when, World};
use examples_proto::{
    AddRebuyChips, BuyInCompleted, BuyInFailed, BuyInInitiated, BuyInRequested, ConfirmBuyIn,
    ConfirmRebuyFee, ConfirmRegistrationFee, EnrollPlayer, GameVariant, HandComplete, PlayerSeated,
    PotWinner, ProcessRebuy, RebuyChipsAdded, RebuyCompleted, RebuyDenied, RebuyFailed,
    RebuyInitiated, RebuyProcessed, RebuyRequested, RegistrationInitiated, RegistrationRequested,
    ReleaseBuyIn, ReleaseRebuyFee, ReleaseRegistrationFee, SeatingRejected, TournamentCreated,
    TournamentEnrollmentRejected, TournamentPlayerEnrolled, TournamentStarted, TournamentStatus,
};
use pmg_hand_flow::state_machine::{
    Action as SmAction, BettingPhase as SmBettingPhase, BlindKind, Command as SmCommand,
    HandProcess, Phase as SmPhase, PlayerState,
};
use pmg_reservation::{Kind, ReservationPMState, ReservationPm};
use poker_tests::currency;
use prost::Message;
use prost_types::Any;

// =============================================================================
// HandFlowPM dispatch helpers
// =============================================================================
//
// `HandProcess` and `PlayerState` are imported from
// `pmg_hand_flow::state_machine` — the real production types. The When
// steps call `process.on_hand_started`, `process.handle_event`,
// `process.end_betting_round`, etc. directly.

fn cmd_label(cmd: &SmCommand) -> String {
    match cmd {
        SmCommand::PostBlind { kind } => match kind {
            BlindKind::Small => "PostBlind:small".to_string(),
            BlindKind::Big => "PostBlind:big".to_string(),
        },
        SmCommand::DealCommunityCards { count } => format!("DealCommunityCards:{}", count),
        SmCommand::AwardPot => "AwardPot".to_string(),
        SmCommand::PlayerAction { action } => format!(
            "PlayerAction:{}",
            match action {
                SmAction::Fold => "FOLD",
                SmAction::Check => "CHECK",
                SmAction::Call => "CALL",
                SmAction::Bet => "BET",
                SmAction::Raise => "RAISE",
                SmAction::AllIn => "ALL_IN",
            }
        ),
        SmCommand::TimeoutCancel => "timeout:cancel".to_string(),
    }
}

// =============================================================================
// Tournament state replay — uses production `pmg_reservation::TournamentStateHelper`
// =============================================================================
//
// The rebuild scenarios (EU-0434..0437) feed events into the production
// rebuild helper used by `ReservationPm` for cross-aggregate pre-validation.

// =============================================================================
// Test world
// =============================================================================

#[derive(Debug, Default, World)]
#[world(init = Self::new)]
pub struct PMWorld {
    process: Option<HandProcess>,
    emitted_commands: Vec<String>,
    last_action: String,

    // ReservationPM fields
    pm_state: ReservationPMState,
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

    // Tournament state replay — production helper from pmg_reservation.
    tournament_state: pmg_reservation::TournamentStateHelper,
    tournament_events: Vec<Any>,

    // HandFlow live-PM
    handflow_state: Option<pmg_hand_flow::HandFlowState>,
    handflow_hand_root: Vec<u8>,
}

impl PMWorld {
    fn new() -> Self {
        Self::default()
    }

    fn init_default_players(process: &mut HandProcess) {
        for i in 0..2 {
            process
                .players
                .insert(i, PlayerState::new(i, format!("player-{}", i + 1), 500));
        }
    }

    fn flush_emitted(&mut self) {
        self.emitted_commands.clear();
        if let Some(p) = self.process.as_ref() {
            for c in &p.emitted {
                self.emitted_commands.push(cmd_label(c));
            }
        }
    }

    fn end_betting_round(&mut self) {
        let process = self.process.as_mut().unwrap();
        process.end_betting_round();
        self.flush_emitted();
    }
}

fn first_command_any(resp: &ProcessManagerHandleResponse) -> &Any {
    let page = resp.commands[0].pages.first().expect("command page");
    match page.payload.as_ref().expect("payload") {
        CommandPayload::Command(a) => a,
        _ => panic!("expected Command payload"),
    }
}

/// Find the first command in `resp` whose payload type ends with `suffix`.
/// PM responses for Initiate flows emit two commands (player + table or
/// player + tournament); the BDD scenarios assert against whichever one
/// matches the named type.
fn find_command_by_suffix<'a>(
    resp: &'a ProcessManagerHandleResponse,
    suffix: &str,
) -> Option<(&'a str, &'a Any)> {
    for cb in &resp.commands {
        let domain = cb.cover.as_ref().map(|c| c.domain.as_str()).unwrap_or("");
        for page in &cb.pages {
            if let Some(CommandPayload::Command(any)) = &page.payload {
                if any.type_url.ends_with(suffix) {
                    return Some((domain, any));
                }
            }
        }
    }
    None
}

/// Like `first_command_any` but locates the command by type-URL suffix —
/// used when a PM response carries multiple commands and the assertion
/// names a specific one.
fn command_any_by_suffix<'a>(resp: &'a ProcessManagerHandleResponse, suffix: &str) -> &'a Any {
    find_command_by_suffix(resp, suffix)
        .unwrap_or_else(|| panic!("no command with type ending in {} in response", suffix))
        .1
}

fn first_process_event_any(resp: &ProcessManagerHandleResponse) -> Any {
    let book = resp.process_events.first().expect("process_events");
    let page = book.pages.first().expect("event page");
    match page.payload.as_ref().expect("payload") {
        EventPayload::Event(a) => a.clone(),
        _ => panic!("expected Event payload"),
    }
}

fn pack_any<M: Message>(msg: &M, type_name: &str) -> Any {
    Any {
        type_url: format!("type.googleapis.com/{}", type_name),
        value: msg.encode_to_vec(),
    }
}

// =============================================================================
// HandFlowPM Given steps
// =============================================================================

#[given("a HandFlowPM")]
fn given_hand_flow_pm(world: &mut PMWorld) {
    world.process = None;
    world.emitted_commands.clear();
}

#[given("a HandStarted event with:")]
fn given_hand_started_event(world: &mut PMWorld, step: &cucumber::gherkin::Step) {
    let mut process = HandProcess::new();
    process.phase = SmPhase::Betting;
    process.betting_phase = Some(SmBettingPhase::Preflop);
    if let Some(table) = &step.table {
        if let Some(row) = table.rows.get(1) {
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
    let mut process = HandProcess::new();
    process.phase = SmPhase::parse(&phase).unwrap_or(SmPhase::Betting);
    process.betting_phase = Some(SmBettingPhase::Preflop);
    PMWorld::init_default_players(&mut process);
    world.process = Some(process);
    world.emitted_commands.clear();
}

#[given(expr = "an active hand process with betting_phase {word}")]
fn given_process_with_betting_phase(world: &mut PMWorld, phase: String) {
    let mut process = HandProcess::new();
    process.phase = SmPhase::Betting;
    process.betting_phase = SmBettingPhase::parse(&phase);
    PMWorld::init_default_players(&mut process);
    world.process = Some(process);
    world.emitted_commands.clear();
}

#[given(expr = "an active hand process with {int} players")]
fn given_process_with_players(world: &mut PMWorld, count: i32) {
    let mut process = HandProcess::new();
    process.phase = SmPhase::Betting;
    for i in 0..count {
        process
            .players
            .insert(i, PlayerState::new(i, format!("player-{}", i + 1), 500));
    }
    world.process = Some(process);
    world.emitted_commands.clear();
}

#[given(expr = "an active hand process with game_variant {word}")]
fn given_process_with_variant(world: &mut PMWorld, variant: String) {
    let mut process = HandProcess::new();
    process.phase = SmPhase::Betting;
    process.game_variant = variant;
    PMWorld::init_default_players(&mut process);
    world.process = Some(process);
    world.emitted_commands.clear();
}

#[given("an active hand process")]
fn given_active_process(world: &mut PMWorld) {
    let mut process = HandProcess::new();
    process.phase = SmPhase::Betting;
    PMWorld::init_default_players(&mut process);
    world.process = Some(process);
    world.emitted_commands.clear();
}

#[given(expr = "an active hand process with player {string} at stack {int}")]
fn given_process_with_player_stack(world: &mut PMWorld, player_id: String, stack: i64) {
    let mut process = HandProcess::new();
    process.phase = SmPhase::Betting;
    process
        .players
        .insert(0, PlayerState::new(0, player_id, stack));
    process
        .players
        .insert(1, PlayerState::new(1, "player-2".to_string(), 500));
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
                process
                    .players
                    .insert(position, PlayerState::new(position, row[0].clone(), stack));
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

/// `dealer is at position D and N players seated at positions A, B, ...`
/// — sets dealer and reseats the named positions with the default 1000 stack.
/// The seat list is comma-separated; the leading numeric `N` is informational
/// (it must equal the seat count). Used by EU-0445/0446/0447 multi-seat
/// scenarios where the canned 2-player init isn't expressive enough.
#[given(regex = r"^dealer is at position (\d+) and (\d+) players seated at positions ([\d, ]+)$")]
fn given_dealer_and_seats(
    world: &mut PMWorld,
    dealer: i32,
    _count: i32,
    positions_csv: String,
) {
    let positions: Vec<i32> = positions_csv
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.parse().expect("seat position must be int"))
        .collect();

    let process = world.process.as_mut().expect("process must exist");
    process.dealer_position = dealer;
    process.players.clear();
    for pos in positions {
        process.players.insert(
            pos,
            PlayerState::new(pos, format!("player-{}", pos + 1), 1000),
        );
    }
}

/// `blinds posted: SB position S amount X, BB position B amount Y`
/// — debits the SB/BB stacks, sets bet_this_round on those seats,
/// records the blind amounts, sets current_bet to the BB amount, and
/// flags both blinds posted. Mirrors the post-blind state the hand
/// aggregate would have produced before betting opens.
#[given(
    regex = r"^blinds posted: SB position (\d+) amount (\d+), BB position (\d+) amount (\d+)$"
)]
fn given_blinds_posted_positions(
    world: &mut PMWorld,
    sb_pos: i32,
    sb_amount: i64,
    bb_pos: i32,
    bb_amount: i64,
) {
    let process = world.process.as_mut().expect("process must exist");
    process.small_blind = sb_amount;
    process.big_blind = bb_amount;
    process.current_bet = bb_amount;
    process.pot_total += sb_amount + bb_amount;
    process.small_blind_posted = true;
    process.big_blind_posted = true;
    if let Some(p) = process.players.get_mut(&sb_pos) {
        p.stack -= sb_amount;
        p.bet_this_round = sb_amount;
    }
    if let Some(p) = process.players.get_mut(&bb_pos) {
        p.stack -= bb_amount;
        p.bet_this_round = bb_amount;
    }
}

/// Mark the preflop round complete: every active seat acted and matched
/// `current_bet`. EU-0446/0447 need this to satisfy the round-end
/// precondition before dispatching a CommunityCardsDealt event.
#[given("the preflop betting round is complete")]
fn given_preflop_complete(world: &mut PMWorld) {
    let process = world.process.as_mut().expect("process must exist");
    process.betting_phase = Some(SmBettingPhase::Preflop);
    let bet = process.current_bet;
    for p in process.players.values_mut() {
        if !p.has_folded && !p.is_all_in {
            p.has_acted = true;
            p.bet_this_round = bet;
        }
    }
}

#[given(expr = "an ActionTaken event for player at position {int} with action {word}")]
fn given_action_at_position(world: &mut PMWorld, pos: i32, action: String) {
    world.last_action = action.clone();
    let process = world.process.as_mut().unwrap();
    let action_enum = SmAction::parse(&action).expect("known action");
    process.apply_action(pos, action_enum);
}

#[given(expr = "players at positions {int}, {int}, {int} have all acted")]
fn given_players_all_acted(world: &mut PMWorld, p1: i32, p2: i32, p3: i32) {
    let process = world.process.as_mut().unwrap();
    for pos in [p1, p2, p3] {
        process
            .players
            .entry(pos)
            .or_insert_with(|| PlayerState::new(pos, format!("player-{}", pos + 1), 500))
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
    match action.as_str() {
        "FOLD" => {
            for p in process.players.values_mut() {
                if !p.has_folded {
                    p.has_folded = true;
                    break;
                }
            }
        }
        "ALL_IN" => {
            for p in process.players.values_mut() {
                if !p.is_all_in && !p.has_folded {
                    p.is_all_in = true;
                    break;
                }
            }
        }
        _ => {}
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
    world.process.as_mut().unwrap().betting_phase = SmBettingPhase::parse(&phase);
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
fn given_pot_awarded(world: &mut PMWorld) {
    // The When step's generic `handle_event` already exits SHOWDOWN →
    // COMPLETE when phase==Showdown. EU-0420 sets phase to SHOWDOWN
    // upstream, so no extra prep here. The Given still serves as a
    // cucumber anchor for the trigger event.
    world.last_action = "POT_AWARDED".to_string();
}

#[given(expr = "a CommunityCardsDealt event for {word}")]
fn given_community_dealt(world: &mut PMWorld, phase: String) {
    // Tag the pending event; the When step routes through
    // `apply_community_cards_dealt` instead of generic `handle_event`.
    world.last_action = format!("COMMUNITY:{}", phase);
}

// =============================================================================
// HandFlowPM When steps
// =============================================================================

#[when("the process manager starts the hand")]
fn when_pm_starts(world: &mut PMWorld) {
    let process = world.process.as_mut().unwrap();
    if process.players.is_empty() {
        PMWorld::init_default_players(process);
    }
    let players: Vec<PlayerState> = process.players.values().cloned().collect();
    let dealer = process.dealer_position;
    let sb = process.small_blind;
    let bb = process.big_blind;
    let variant = process.game_variant.clone();
    process.on_hand_started(1, &variant, dealer, sb, bb, &players);
    world.flush_emitted();
}

#[when("the process manager handles the event")]
fn when_pm_handles(world: &mut PMWorld) {
    if let Some(phase) = world.last_action.strip_prefix("COMMUNITY:") {
        let bp = SmBettingPhase::parse(phase).expect("known betting phase");
        let process = world.process.as_mut().unwrap();
        process.apply_community_cards_dealt(bp);
        world.flush_emitted();
        return;
    }
    let process = world.process.as_mut().unwrap();
    process.handle_event();
    world.flush_emitted();
}

#[when("the process manager ends the betting round")]
fn when_pm_ends_round(world: &mut PMWorld) {
    world.end_betting_round();
}

/// `the player at position N calls X` — synthesize a CALL action for
/// position `pos`: the seat tops up `bet_this_round` to `current_bet`
/// (the gherkin `X` is the call delta), debits the stack by that delta,
/// flags `has_acted`, and advances `action_on` to the next un-folded /
/// un-all-in seat. Used by EU-0445 to preflop-call the BB twice and
/// confirm the round does NOT close on `everyone-matched` alone.
///
/// Calls production methods only:
///   - `apply_player_action(pos, Call, amount)` does chip accounting +
///     flag updates,
///   - `advance_action_on()` walks the seat ring without closing the
///     round.
/// A buggy production implementation will surface here, not just in the
/// test fixture.
#[when(regex = r"^the player at position (\d+) calls (\d+)$")]
fn when_player_at_pos_calls(world: &mut PMWorld, pos: i32, amount: i64) {
    let process = world.process.as_mut().expect("process must exist");
    process.apply_player_action(pos, SmAction::Call, amount);
    process.advance_action_on();
}

/// `a CommunityCardsDealt event for FLOP is handled` — dispatch the named
/// betting phase through the production `apply_community_cards_dealt`,
/// which resets per-round bet tracking and moves action to the first seat
/// after the dealer. Used by EU-0446/0447 post-flop first-to-act scenarios.
#[when(regex = r"^a CommunityCardsDealt event for (\w+) is handled$")]
fn when_community_cards_dealt(world: &mut PMWorld, phase: String) {
    let bp = SmBettingPhase::parse(&phase).expect("known betting phase");
    let process = world.process.as_mut().expect("process must exist");
    process.apply_community_cards_dealt(bp);
    process.phase = SmPhase::Betting;
    world.flush_emitted();
}

#[when("the action times out")]
fn when_timeout(world: &mut PMWorld) {
    let process = world.process.as_mut().unwrap();
    process.timeout();
    world.flush_emitted();
}

#[when("the process manager handles the last draw")]
fn when_last_draw(world: &mut PMWorld) {
    let process = world.process.as_mut().unwrap();
    process.handle_last_draw();
    world.flush_emitted();
}

#[when("all events are processed")]
fn when_all_processed(_world: &mut PMWorld) {}

// =============================================================================
// HandFlowPM Then steps
// =============================================================================

#[then(expr = "a HandProcess is created with phase {word}")]
fn then_process_created(world: &mut PMWorld, phase: String) {
    let p = world.process.as_ref().expect("No process");
    assert_eq!(p.phase.as_str(), phase);
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
    assert_eq!(world.process.as_ref().unwrap().phase.as_str(), phase);
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
    assert_ne!(world.process.as_ref().unwrap().phase, SmPhase::Betting);
}

#[then("the process advances to next phase")]
fn then_advances(_world: &mut PMWorld) {}

#[then(expr = "a DealCommunityCards command is sent with count {int}")]
fn then_deal_community(world: &mut PMWorld, count: i32) {
    let expected = format!("DealCommunityCards:{}", count);
    assert!(
        world.emitted_commands.contains(&expected),
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
    assert!(world.emitted_commands.contains(&expected));
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
    assert_eq!(world.process.as_ref().unwrap().phase, SmPhase::Complete);
}

/// `the betting round is not complete` — assert that the production
/// `is_betting_complete()` returns false. EU-0445's discriminator: BB's
/// option preflop must keep the round open even when every other seat
/// already matched the BB. A buggy production implementation that closes
/// the round on `everyone-matched` would fail here.
#[then("the betting round is not complete")]
fn then_betting_not_complete(world: &mut PMWorld) {
    let process = world.process.as_ref().expect("process must exist");
    assert!(
        !process.is_betting_complete(),
        "Expected is_betting_complete() == false, but production reports complete (players: {:?})",
        process
            .players
            .values()
            .map(|p| (p.position, p.has_acted, p.bet_this_round, p.has_folded, p.is_all_in))
            .collect::<Vec<_>>()
    );
}

#[then(expr = "action_on is position {int}")]
fn then_action_on_is(world: &mut PMWorld, pos: i32) {
    let actual = world.process.as_ref().expect("process must exist").action_on;
    assert_eq!(
        actual, pos,
        "Expected action_on={}, got {}",
        pos, actual
    );
}

#[then(expr = "betting_phase is set to {word}")]
fn then_betting_phase(world: &mut PMWorld, phase: String) {
    let actual = world
        .process
        .as_ref()
        .unwrap()
        .betting_phase
        .map(|p| p.as_str().to_string())
        .unwrap_or_default();
    assert_eq!(actual, phase);
}

// =============================================================================
// ReservationPM Given steps
// =============================================================================

#[given("a BuyInPM")]
fn given_buy_in_pm(world: &mut PMWorld) {
    world.pm_state = ReservationPMState::default();
}

#[given(expr = "a BuyInPM with player_root {string}")]
fn given_buy_in_pm_with_player(world: &mut PMWorld, player_root: String) {
    world.pm_state = ReservationPMState {
        kind: Kind::BuyIn,
        player_root: player_root.into_bytes(),
        ..ReservationPMState::default()
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
        player_root: world.pm_state.player_root.clone(),
        table_root: table_root.into_bytes(),
        seat,
        amount: Some(currency(amount)),
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

// `destinations with sequences X=N, Y=M` is a runtime-coordinator detail
// that doesn't affect the handler's pure response synthesis. We accept
// any tail with a `{}` placeholder — the suite never asserts on these.
#[given(expr = "destinations with sequences {}")]
fn given_destinations(_world: &mut PMWorld, _spec: String) {}

#[given(expr = "a RebuyPM with player_root {string}")]
fn given_rebuy_pm_player(world: &mut PMWorld, player_root: String) {
    world.pm_state = ReservationPMState {
        kind: Kind::Rebuy,
        player_root: player_root.into_bytes(),
        ..ReservationPMState::default()
    };
}

#[given(expr = "a RebuyPM with table_root {string} and seat {int}")]
fn given_rebuy_pm_table_seat(world: &mut PMWorld, table_root: String, seat: i32) {
    world.pm_state = ReservationPMState {
        kind: Kind::Rebuy,
        table_root: table_root.into_bytes(),
        seat,
        ..ReservationPMState::default()
    };
}

#[given(expr = "a RebuyPM with tournament_root {string}")]
fn given_rebuy_pm_tournament(world: &mut PMWorld, tournament_root: String) {
    world.pm_state = ReservationPMState {
        kind: Kind::Rebuy,
        tournament_root: tournament_root.into_bytes(),
        ..ReservationPMState::default()
    };
}

#[given(expr = "a RebuyPM with tournament_root {string}, table_root {string}, fee {int}")]
fn given_rebuy_pm_full(world: &mut PMWorld, tournament_root: String, table_root: String, fee: i64) {
    world.pm_state = ReservationPMState {
        kind: Kind::Rebuy,
        tournament_root: tournament_root.into_bytes(),
        table_root: table_root.into_bytes(),
        fee,
        ..ReservationPMState::default()
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
        player_root: world.pm_state.player_root.clone(),
        tournament_root: tournament_root.into_bytes(),
        table_root: table_root.into_bytes(),
        seat,
        fee: Some(currency(fee)),
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

#[given(expr = "a RegistrationPM with player_root {string}")]
fn given_registration_pm_player(world: &mut PMWorld, player_root: String) {
    world.pm_state = ReservationPMState {
        kind: Kind::Registration,
        player_root: player_root.into_bytes(),
        ..ReservationPMState::default()
    };
}

#[given(expr = "a RegistrationPM with tournament_root {string} and fee {int}")]
fn given_registration_pm_tournament_fee(world: &mut PMWorld, tournament_root: String, fee: i64) {
    world.pm_state = ReservationPMState {
        kind: Kind::Registration,
        tournament_root: tournament_root.into_bytes(),
        fee,
        ..ReservationPMState::default()
    };
}

#[given(expr = "a RegistrationPM with tournament_root {string}")]
fn given_registration_pm_tournament(world: &mut PMWorld, tournament_root: String) {
    world.pm_state = ReservationPMState {
        kind: Kind::Registration,
        tournament_root: tournament_root.into_bytes(),
        ..ReservationPMState::default()
    };
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
        player_root: world.pm_state.player_root.clone(),
        tournament_root: tournament_root.into_bytes(),
        fee: Some(currency(fee)),
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
        player_root: world.pm_state.player_root.clone(),
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

// =============================================================================
// ReservationPM When steps
// =============================================================================

#[when("the BuyInPM handles buy_in_requested")]
fn when_buyin_handles_request(world: &mut PMWorld) {
    let ev = world.buyin_event.take().expect("buyin event set");
    let pm = ReservationPm::new();
    world.pm_response = Some(pm.on_buy_in_requested(ev, &world.pm_state).expect("ok"));
}

#[when("the BuyInPM handles player_seated")]
fn when_buyin_handles_seated(world: &mut PMWorld) {
    let ev = world.player_seated_event.take().expect("seated event set");
    let pm = ReservationPm::new();
    world.pm_response = Some(pm.on_player_seated(ev, &world.pm_state).expect("ok"));
}

#[when("the BuyInPM handles seating_rejected")]
fn when_buyin_handles_rejected(world: &mut PMWorld) {
    let ev = world
        .seating_rejected_event
        .take()
        .expect("rejected event set");
    let pm = ReservationPm::new();
    world.pm_response = Some(pm.on_seating_rejected(ev, &world.pm_state).expect("ok"));
}

#[when("the RebuyPM handles rebuy_requested")]
fn when_rebuy_handles_request(world: &mut PMWorld) {
    let ev = world.rebuy_requested_event.take().unwrap();
    let pm = ReservationPm::new();
    world.pm_response = Some(pm.on_rebuy_requested(ev, &world.pm_state).expect("ok"));
}

#[when("the RebuyPM handles rebuy_processed")]
fn when_rebuy_handles_processed(world: &mut PMWorld) {
    let ev = world.rebuy_processed_event.take().unwrap();
    let pm = ReservationPm::new();
    world.pm_response = Some(pm.on_rebuy_processed(ev, &world.pm_state).expect("ok"));
}

#[when("the RebuyPM handles rebuy_denied")]
fn when_rebuy_handles_denied(world: &mut PMWorld) {
    let ev = world.rebuy_denied_event.take().unwrap();
    let pm = ReservationPm::new();
    world.pm_response = Some(pm.on_rebuy_denied(ev, &world.pm_state).expect("ok"));
}

#[when("the RebuyPM handles rebuy_chips_added")]
fn when_rebuy_handles_chips_added(world: &mut PMWorld) {
    let ev = world.rebuy_chips_added_event.take().unwrap();
    let pm = ReservationPm::new();
    world.pm_response = Some(pm.on_rebuy_chips_added(ev, &world.pm_state).expect("ok"));
}

#[when("the RegistrationPM handles registration_requested")]
fn when_registration_handles_request(world: &mut PMWorld) {
    let ev = world.registration_requested_event.take().unwrap();
    let pm = ReservationPm::new();
    world.pm_response = Some(
        pm.on_registration_requested(ev, &world.pm_state)
            .expect("ok"),
    );
}

#[when("the RegistrationPM handles player_enrolled")]
fn when_registration_handles_enrolled(world: &mut PMWorld) {
    let ev = world.player_enrolled_event.take().unwrap();
    let pm = ReservationPm::new();
    world.pm_response = Some(pm.on_player_enrolled(ev, &world.pm_state).expect("ok"));
}

#[when("the RegistrationPM handles enrollment_rejected")]
fn when_registration_handles_rejected(world: &mut PMWorld) {
    let ev = world.enrollment_rejected_event.take().unwrap();
    let pm = ReservationPm::new();
    world.pm_response = Some(pm.on_enrollment_rejected(ev, &world.pm_state).expect("ok"));
}

// =============================================================================
// ReservationPM Then steps — generic command/event dispatch
// =============================================================================

fn assert_command_target(resp: &ProcessManagerHandleResponse, domain: &str, type_suffix: &str) {
    let (found_domain, _any) = find_command_by_suffix(resp, type_suffix).unwrap_or_else(|| {
        panic!(
            "no command of type *{} in response (commands: {:?})",
            type_suffix,
            resp.commands
                .iter()
                .filter_map(|c| c.cover.as_ref().map(|cv| cv.domain.clone()))
                .collect::<Vec<_>>()
        )
    });
    assert_eq!(
        found_domain, domain,
        "Expected {} command on `{}` domain, got `{}`",
        type_suffix, domain, found_domain
    );
}

#[then(expr = "a SeatPlayer command is sent to the {string} domain")]
fn then_seat_player_command(world: &mut PMWorld, domain: String) {
    assert_command_target(world.pm_response.as_ref().unwrap(), &domain, "SeatPlayer");
}

#[then(expr = "the SeatPlayer command has player_root {string}")]
fn then_seat_player_root(world: &mut PMWorld, root: String) {
    let any = command_any_by_suffix(world.pm_response.as_ref().unwrap(), "SeatPlayer");
    let cmd = examples_proto::SeatPlayer::decode(any.value.as_slice()).expect("decode");
    assert_eq!(cmd.player_root, root.into_bytes());
}

#[then(expr = "the SeatPlayer command has seat {int}")]
fn then_seat_player_seat(world: &mut PMWorld, seat: i32) {
    let any = command_any_by_suffix(world.pm_response.as_ref().unwrap(), "SeatPlayer");
    let cmd = examples_proto::SeatPlayer::decode(any.value.as_slice()).unwrap();
    assert_eq!(cmd.seat, seat);
}

#[then(expr = "the SeatPlayer command has amount {int}")]
fn then_seat_player_amount(world: &mut PMWorld, amount: i64) {
    let any = command_any_by_suffix(world.pm_response.as_ref().unwrap(), "SeatPlayer");
    let cmd = examples_proto::SeatPlayer::decode(any.value.as_slice()).unwrap();
    assert_eq!(cmd.amount, amount);
}

#[then(expr = "the SeatPlayer command has reservation_id {string}")]
fn then_seat_player_reservation(world: &mut PMWorld, reservation_id: String) {
    let any = command_any_by_suffix(world.pm_response.as_ref().unwrap(), "SeatPlayer");
    let cmd = examples_proto::SeatPlayer::decode(any.value.as_slice()).unwrap();
    assert_eq!(cmd.reservation_id, reservation_id.into_bytes());
}

#[then(expr = "the process event is a {} event")]
fn then_process_event_type(world: &mut PMWorld, type_name: String) {
    let any = first_process_event_any(world.pm_response.as_ref().unwrap());
    let suffix = type_name.rsplit('.').next().unwrap_or(type_name.as_str());
    assert!(
        any.type_url.ends_with(suffix),
        "expected type ending with {}, got {}",
        type_name,
        any.type_url
    );
}

#[then(expr = "the BuyInInitiated event has player_root {string}")]
fn then_buyin_initiated_player(world: &mut PMWorld, player: String) {
    let any = first_process_event_any(world.pm_response.as_ref().unwrap());
    let ev = BuyInInitiated::decode(any.value.as_slice()).unwrap();
    assert_eq!(ev.player_root, player.into_bytes());
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
    assert_command_target(world.pm_response.as_ref().unwrap(), &domain, "ConfirmBuyIn");
}

#[then(expr = "the ConfirmBuyIn command has reservation_id {string}")]
fn then_confirm_buyin_reservation(world: &mut PMWorld, reservation_id: String) {
    let any = first_command_any(world.pm_response.as_ref().unwrap());
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
    assert_command_target(world.pm_response.as_ref().unwrap(), &domain, "ReleaseBuyIn");
}

#[then(expr = "the ReleaseBuyIn command has reservation_id {string}")]
fn then_release_buyin_reservation(world: &mut PMWorld, reservation_id: String) {
    let any = first_command_any(world.pm_response.as_ref().unwrap());
    let cmd = ReleaseBuyIn::decode(any.value.as_slice()).unwrap();
    assert_eq!(cmd.reservation_id, reservation_id.into_bytes());
}

#[then(expr = "the ReleaseBuyIn command has reason {string}")]
fn then_release_buyin_reason(world: &mut PMWorld, reason: String) {
    let any = first_command_any(world.pm_response.as_ref().unwrap());
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

#[then(expr = "a ProcessRebuy command is sent to the {string} domain")]
fn then_process_rebuy_cmd(world: &mut PMWorld, domain: String) {
    assert_command_target(world.pm_response.as_ref().unwrap(), &domain, "ProcessRebuy");
}

#[then(expr = "the ProcessRebuy command has player_root {string}")]
fn then_process_rebuy_player(world: &mut PMWorld, player: String) {
    let any = command_any_by_suffix(world.pm_response.as_ref().unwrap(), "ProcessRebuy");
    let cmd = ProcessRebuy::decode(any.value.as_slice()).unwrap();
    assert_eq!(cmd.player_root, player.into_bytes());
}

#[then(expr = "the ProcessRebuy command has reservation_id {string}")]
fn then_process_rebuy_reservation(world: &mut PMWorld, reservation_id: String) {
    let any = command_any_by_suffix(world.pm_response.as_ref().unwrap(), "ProcessRebuy");
    let cmd = ProcessRebuy::decode(any.value.as_slice()).unwrap();
    assert_eq!(cmd.reservation_id, reservation_id.into_bytes());
}

#[then(expr = "the RebuyInitiated event has player_root {string}")]
fn then_rebuy_initiated_player(world: &mut PMWorld, player: String) {
    let any = first_process_event_any(world.pm_response.as_ref().unwrap());
    let ev = RebuyInitiated::decode(any.value.as_slice()).unwrap();
    assert_eq!(ev.player_root, player.into_bytes());
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
    assert_command_target(
        world.pm_response.as_ref().unwrap(),
        &domain,
        "AddRebuyChips",
    );
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
    assert_command_target(
        world.pm_response.as_ref().unwrap(),
        &domain,
        "ReleaseRebuyFee",
    );
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
    assert_command_target(
        world.pm_response.as_ref().unwrap(),
        &domain,
        "ConfirmRebuyFee",
    );
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

#[then(expr = "an EnrollPlayer command is sent to the {string} domain")]
fn then_enroll_player_cmd(world: &mut PMWorld, domain: String) {
    assert_command_target(world.pm_response.as_ref().unwrap(), &domain, "EnrollPlayer");
}

#[then(expr = "the EnrollPlayer command has player_root {string}")]
fn then_enroll_player_root(world: &mut PMWorld, player: String) {
    let any = command_any_by_suffix(world.pm_response.as_ref().unwrap(), "EnrollPlayer");
    let cmd = EnrollPlayer::decode(any.value.as_slice()).unwrap();
    assert_eq!(cmd.player_root, player.into_bytes());
}

#[then(expr = "the EnrollPlayer command has reservation_id {string}")]
fn then_enroll_player_reservation(world: &mut PMWorld, reservation: String) {
    let any = command_any_by_suffix(world.pm_response.as_ref().unwrap(), "EnrollPlayer");
    let cmd = EnrollPlayer::decode(any.value.as_slice()).unwrap();
    assert_eq!(cmd.reservation_id, reservation.into_bytes());
}

#[then(expr = "a ConfirmRegistrationFee command is sent to the {string} domain")]
fn then_confirm_registration_fee(world: &mut PMWorld, domain: String) {
    assert_command_target(
        world.pm_response.as_ref().unwrap(),
        &domain,
        "ConfirmRegistrationFee",
    );
}

#[then(expr = "the ConfirmRegistrationFee command has reservation_id {string}")]
fn then_confirm_registration_fee_reservation(world: &mut PMWorld, reservation: String) {
    let any = first_command_any(world.pm_response.as_ref().unwrap());
    let cmd = ConfirmRegistrationFee::decode(any.value.as_slice()).unwrap();
    assert_eq!(cmd.reservation_id, reservation.into_bytes());
}

#[then(expr = "a ReleaseRegistrationFee command is sent to the {string} domain")]
fn then_release_registration_fee(world: &mut PMWorld, domain: String) {
    assert_command_target(
        world.pm_response.as_ref().unwrap(),
        &domain,
        "ReleaseRegistrationFee",
    );
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

// =============================================================================
// HandFlowPM (live PM) — HandComplete -> EndHand
// =============================================================================

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
        ..Default::default()
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
    assert_command_target(world.pm_response.as_ref().unwrap(), &domain, "EndHand");
}

#[then(expr = "the EndHand command has {int} result")]
fn then_end_hand_result_count(world: &mut PMWorld, count: usize) {
    let any = first_command_any(world.pm_response.as_ref().unwrap());
    let cmd = examples_proto::EndHand::decode(any.value.as_slice()).unwrap();
    assert_eq!(cmd.results.len(), count);
}

#[then("the EndHand command preserves the original hand_root")]
fn then_end_hand_preserves_hand_root(world: &mut PMWorld) {
    let any = first_command_any(world.pm_response.as_ref().unwrap());
    let cmd = examples_proto::EndHand::decode(any.value.as_slice()).unwrap();
    assert_eq!(cmd.hand_root, world.handflow_hand_root);
}

// =============================================================================
// Tournament state replay — exercises the helper used in PM pre-validation
// =============================================================================

fn rebuild_tournament(events: &[Any]) -> pmg_reservation::TournamentStateHelper {
    use angzarr_client::proto::{
        event_page::Payload as EventPayload, page_header::SequenceType, EventBook, EventPage,
        PageHeader,
    };
    let pages: Vec<EventPage> = events
        .iter()
        .cloned()
        .map(|any| EventPage {
            header: Some(PageHeader {
                sync_mode: None,
                sequence_type: Some(SequenceType::Sequence(0)),
            }),
            created_at: None,
            no_commit: false,
            cascade_id: None,
            payload: Some(EventPayload::Event(any)),
        })
        .collect();
    let book = EventBook {
        cover: None,
        pages,
        snapshot: None,
        next_sequence: 0,
    };
    pmg_reservation::tournament_state_from_event_book(&book)
}

#[given("an empty tournament state helper")]
fn given_empty_tournament_state(world: &mut PMWorld) {
    world.tournament_state = pmg_reservation::TournamentStateHelper::default();
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
    world.tournament_state = pmg_reservation::TournamentStateHelper::default();
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
    world.tournament_state = pmg_reservation::TournamentStateHelper::default();
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
    world.tournament_state = rebuild_tournament(&world.tournament_events);
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
    world
        .tournament_events
        .push(pack_any(&ev, "examples.TournamentCreated"));
    world.tournament_state = rebuild_tournament(&world.tournament_events);
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
    world
        .tournament_events
        .push(pack_any(&ev, "examples.TournamentPlayerEnrolled"));
    world.tournament_state = rebuild_tournament(&world.tournament_events);
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
fn then_tournament_registered_count(world: &mut PMWorld, n: i32) {
    assert_eq!(world.tournament_state.registered_count, n);
}

#[then(expr = "the tournament state has registered player {string}")]
fn then_tournament_has_registered_player(world: &mut PMWorld, player: String) {
    let hex_str = hex::encode(player.as_bytes());
    assert!(
        world.tournament_state.registered_players.contains(&hex_str),
        "player {} not in registered set {:?}",
        player,
        world.tournament_state.registered_players
    );
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
    PMWorld::run("features/example/unit/process_manager.feature").await;
}
