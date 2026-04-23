//! Betting-round iteration BDD tests.
//!
//! Exercises `BettingRoundTester` + `MockPlayer` — a test-only harness that
//! mirrors the `betting_round()` driver from run_game.py. No production Rust
//! aggregate is involved; this pins iteration behavior (fold skipping, all-in
//! aggressor termination) against gherkin scenarios.

use std::collections::BTreeMap;

use cucumber::{given, then, when, World};

// Action types (mirroring poker_types.ActionType).
const FOLD: i32 = 1;
const CHECK: i32 = 2;
const CALL: i32 = 3;
const BET: i32 = 4;
const RAISE: i32 = 5;

#[derive(Debug, Clone)]
struct MockPlayer {
    stack: i64,
    bet: i64,
    folded: bool,
    all_in: bool,
}

impl MockPlayer {
    fn new(stack: i64) -> Self {
        Self {
            stack,
            bet: 0,
            folded: false,
            all_in: false,
        }
    }
}

#[derive(Debug, Default)]
struct BettingRoundTester {
    players: BTreeMap<i32, MockPlayer>,
    current_bet: i64,
    pot: i64,
    actions: Vec<(i32, i64)>,
    action_index: usize,
    seats_asked_to_act: Vec<i32>,
}

impl BettingRoundTester {
    fn new(players: BTreeMap<i32, MockPlayer>) -> Self {
        Self {
            players,
            ..Default::default()
        }
    }

    fn set_actions(&mut self, actions: Vec<(i32, i64)>) {
        self.actions = actions;
        self.action_index = 0;
    }

    fn get_action(&mut self, seat: i32) -> (i32, i64) {
        self.seats_asked_to_act.push(seat);
        if self.action_index < self.actions.len() {
            let a = self.actions[self.action_index];
            self.action_index += 1;
            a
        } else {
            (FOLD, 0)
        }
    }

    fn all_seats(&self) -> Vec<i32> {
        self.players.keys().copied().collect()
    }

    fn active_seats(&self) -> Vec<i32> {
        self.all_seats()
            .into_iter()
            .filter(|s| {
                let p = &self.players[s];
                !p.folded && !p.all_in
            })
            .collect()
    }

    fn next_active_seat(&self, current: i32) -> Option<i32> {
        let all = self.all_seats();
        let active: Vec<i32> = self.active_seats();
        if active.is_empty() {
            return None;
        }
        let idx = all.iter().position(|&s| s == current)?;
        for i in 1..=all.len() {
            let next_s = all[(idx + i) % all.len()];
            if active.contains(&next_s) {
                return Some(next_s);
            }
        }
        None
    }

    fn betting_round(&mut self, first_to_act: i32, preflop: bool) {
        let active = self.active_seats();
        if active.len() < 2 {
            return;
        }

        if !preflop {
            for p in self.players.values_mut() {
                p.bet = 0;
            }
            self.current_bet = 0;
        }

        let mut current_seat = if active.contains(&first_to_act) {
            first_to_act
        } else {
            active[0]
        };

        let mut acted: std::collections::HashSet<i32> = std::collections::HashSet::new();
        let mut last_aggressor: Option<i32> = None;

        loop {
            let player_folded = self.players[&current_seat].folded;
            let player_all_in = self.players[&current_seat].all_in;

            if player_folded || player_all_in {
                match self.next_active_seat(current_seat) {
                    Some(s) => {
                        current_seat = s;
                        continue;
                    }
                    None => break,
                }
            }

            let active = self.active_seats();
            if active.len() <= 1 {
                break;
            }

            let all_bets_matched = active
                .iter()
                .all(|s| self.players[s].bet == self.current_bet);

            let effective_last_aggressor = match last_aggressor {
                Some(a) if active.contains(&a) => Some(a),
                _ => None,
            };

            if acted.contains(&current_seat) && all_bets_matched {
                if effective_last_aggressor.is_none()
                    || Some(current_seat) == effective_last_aggressor
                {
                    break;
                }
            }

            let (mut action, mut amount) = self.get_action(current_seat);
            let player_stack = self.players[&current_seat].stack;
            let player_bet = self.players[&current_seat].bet;
            let to_call = (self.current_bet - player_bet).max(0);

            if action == CALL && to_call == 0 {
                action = CHECK;
                amount = 0;
            }
            if action == CHECK && to_call > 0 {
                action = CALL;
                amount = to_call;
            }
            if action == BET && self.current_bet > 0 {
                action = RAISE;
            }

            if action == RAISE {
                let min_raise_to = self.current_bet * 2;
                if amount < min_raise_to {
                    if to_call > 0 && player_stack >= to_call {
                        action = CALL;
                        amount = to_call;
                    } else if to_call == 0 {
                        action = CHECK;
                        amount = 0;
                    } else {
                        action = FOLD;
                        amount = 0;
                    }
                }
            }

            if action == BET || action == RAISE {
                let max_bet = player_stack + player_bet;
                if amount > max_bet {
                    amount = max_bet;
                }
                if action == RAISE && amount < self.current_bet * 2 {
                    if to_call > 0 && player_stack >= to_call {
                        action = CALL;
                        amount = to_call;
                    } else if player_stack > 0 {
                        action = CALL;
                        amount = player_stack;
                    } else {
                        action = FOLD;
                        amount = 0;
                    }
                }
            }

            if action == CALL && to_call > player_stack {
                amount = player_stack;
            }

            let player = self.players.get_mut(&current_seat).unwrap();
            match action {
                a if a == FOLD => {
                    player.folded = true;
                }
                a if a == CALL => {
                    let call_amount = (self.current_bet - player.bet).min(player.stack);
                    player.stack -= call_amount;
                    player.bet += call_amount;
                    self.pot += call_amount;
                    if player.stack == 0 {
                        player.all_in = true;
                    }
                }
                a if a == BET || a == RAISE => {
                    let bet_amount = amount - player.bet;
                    player.stack -= bet_amount;
                    player.bet = amount;
                    self.pot += bet_amount;
                    self.current_bet = amount;
                    last_aggressor = Some(current_seat);
                    if player.stack == 0 {
                        player.all_in = true;
                    }
                }
                _ => {}
            }

            acted.insert(current_seat);

            match self.next_active_seat(current_seat) {
                Some(s) => current_seat = s,
                None => break,
            }
            if self.active_seats().len() < 2 {
                break;
            }
        }
    }
}

fn action_from_label(label: &str) -> i32 {
    match label.trim().to_ascii_uppercase().as_str() {
        "FOLD" => FOLD,
        "CHECK" => CHECK,
        "CALL" => CALL,
        "BET" => BET,
        "RAISE" => RAISE,
        other => panic!("Unknown action label: {}", other),
    }
}

#[derive(Debug, Default, World)]
pub struct BettingWorld {
    tester: Option<BettingRoundTester>,
}

impl BettingWorld {
    fn tester_mut(&mut self) -> &mut BettingRoundTester {
        self.tester.as_mut().expect("tester not initialized")
    }

    fn tester(&self) -> &BettingRoundTester {
        self.tester.as_ref().expect("tester not initialized")
    }
}

#[given(expr = "a {int}-player table with big_blind {int}")]
fn given_n_player_table(world: &mut BettingWorld, count: i32, _bb: i64) {
    let mut players = BTreeMap::new();
    for i in 0..count {
        players.insert(i, MockPlayer::new(1000));
    }
    world.tester = Some(BettingRoundTester::new(players));
}

#[given("a table with players:")]
fn given_table_with_players(world: &mut BettingWorld, step: &cucumber::gherkin::Step) {
    let table = step.table.as_ref().expect("expected data table");
    let headings = &table.rows[0];
    let seat_col = headings.iter().position(|h| h == "seat").unwrap();
    let stack_col = headings.iter().position(|h| h == "stack").unwrap();

    let mut players = BTreeMap::new();
    for row in table.rows.iter().skip(1) {
        let seat: i32 = row[seat_col].parse().unwrap();
        let stack: i64 = row[stack_col].parse().unwrap();
        players.insert(seat, MockPlayer::new(stack));
    }
    world.tester = Some(BettingRoundTester::new(players));
}

#[given(expr = "big_blind {int}")]
fn given_big_blind(_world: &mut BettingWorld, _bb: i64) {}

#[given(
    expr = "blinds posted: {string} seat {int} amount {int} and {string} seat {int} amount {int}"
)]
fn given_blinds_posted(
    world: &mut BettingWorld,
    _sb_name: String,
    sb_seat: i32,
    sb_amt: i64,
    _bb_name: String,
    bb_seat: i32,
    bb_amt: i64,
) {
    let t = world.tester_mut();
    if let Some(p) = t.players.get_mut(&sb_seat) {
        p.bet = sb_amt;
    }
    if let Some(p) = t.players.get_mut(&bb_seat) {
        p.bet = bb_amt;
    }
    t.current_bet = bb_amt;
    t.pot = sb_amt + bb_amt;
}

#[given("scripted actions:")]
fn given_scripted_actions(world: &mut BettingWorld, step: &cucumber::gherkin::Step) {
    let table = step.table.as_ref().expect("expected data table");
    let headings = &table.rows[0];
    let action_col = headings.iter().position(|h| h == "action").unwrap();
    let amount_col = headings.iter().position(|h| h == "amount").unwrap();
    let actions: Vec<(i32, i64)> = table
        .rows
        .iter()
        .skip(1)
        .map(|row| {
            let a = action_from_label(&row[action_col]);
            let amt: i64 = row[amount_col].parse().unwrap();
            (a, amt)
        })
        .collect();
    world.tester_mut().set_actions(actions);
}

#[when(expr = "I run a preflop betting round starting at seat {int}")]
fn when_run_preflop(world: &mut BettingWorld, seat: i32) {
    world.tester_mut().betting_round(seat, true);
}

#[then(regex = r"^the seats asked to act are \[([^\]]*)\]$")]
fn then_seats_asked_csv(world: &mut BettingWorld, list: String) {
    let expected: Vec<i32> = list.split(',').map(|s| s.trim().parse().unwrap()).collect();
    let actual = &world.tester().seats_asked_to_act;
    assert_eq!(
        actual, &expected,
        "Expected {:?}, got {:?}",
        expected, actual
    );
}

#[then(expr = "seat {int} is asked to act after seat {int}")]
fn then_seat_after(world: &mut BettingWorld, after: i32, before: i32) {
    let order = &world.tester().seats_asked_to_act;
    let before_idx = order
        .iter()
        .position(|&s| s == before)
        .unwrap_or_else(|| panic!("seat {} never asked to act: {:?}", before, order));
    assert!(
        order[before_idx + 1..].contains(&after),
        "seat {} did not appear after seat {}; full order: {:?}",
        after,
        before,
        order
    );
}

#[then(expr = "seat {int} is asked to act exactly {int} time")]
fn then_seat_asked_exact_one(world: &mut BettingWorld, seat: i32, n: usize) {
    let count = world
        .tester()
        .seats_asked_to_act
        .iter()
        .filter(|&&s| s == seat)
        .count();
    assert_eq!(
        count,
        n,
        "seat {} asked {} times, expected {}. full order: {:?}",
        seat,
        count,
        n,
        world.tester().seats_asked_to_act
    );
}

#[then(expr = "seat {int} is asked to act exactly {int} times")]
fn then_seat_asked_exact(world: &mut BettingWorld, seat: i32, n: usize) {
    then_seat_asked_exact_one(world, seat, n);
}

#[then(expr = "seat {int} is all-in")]
fn then_seat_all_in(world: &mut BettingWorld, seat: i32) {
    let p = &world.tester().players[&seat];
    assert!(p.all_in, "seat {} is not all-in", seat);
}

#[then(expr = "seat {int} has stack {int}")]
fn then_seat_stack(world: &mut BettingWorld, seat: i32, stack: i64) {
    let p = &world.tester().players[&seat];
    assert_eq!(
        p.stack, stack,
        "seat {} has stack {}, expected {}",
        seat, p.stack, stack
    );
}

#[tokio::main]
async fn main() {
    BettingWorld::run("features/example/unit/betting_round.feature").await;
}
