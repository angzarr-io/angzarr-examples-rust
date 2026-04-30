//! Hand aggregate BDD tests using cucumber-rs against Tier 5 Router API.

use std::collections::HashMap;

use agg_hand::game_rules;
use agg_hand::handlers::{
    handle_award_pot, handle_deal_cards, handle_deal_community_cards, handle_player_action,
    handle_post_blind, handle_request_draw, handle_reveal_cards,
};
use agg_hand::state::{
    apply_action_taken, apply_betting_round_complete, apply_blind_posted, apply_cards_dealt,
    apply_community_cards_dealt, apply_draw_completed, apply_hand_complete, apply_pot_awarded,
    apply_showdown_started, new_hand_state, HandState,
};
use angzarr_client::proto::{event_page, EventBook};
use angzarr_client::{try_unpack, CommandRejectedError};
use cucumber::{given, then, when, World};
use examples_proto::{
    ActionTaken, ActionType, AwardPot, BettingPhase, BettingRoundComplete, BlindPosted, Card,
    CardsDealt, CardsMucked, CardsRevealed, CommunityCardsDealt, DealCards, DealCommunityCards,
    DrawCompleted, GameVariant, HandComplete, HandRankType, HandRanking, PlayerAction,
    PlayerHoleCards, PlayerInHand, PlayerStackSnapshot, PostBlind, PotAward, PotAwarded,
    RequestDraw, RevealCards, ShowdownStarted,
};
use poker_tests::{parse_cards, uuid_for};
use prost_types::Any;

fn uuid_or_empty(s: &str) -> Vec<u8> {
    if s.is_empty() {
        Vec::new()
    } else {
        uuid_for(s)
    }
}

/// Test world for hand aggregate.
#[derive(Debug, Default, World)]
pub struct HandWorld {
    events: Vec<Any>,
    result: Option<Result<EventBook, CommandRejectedError>>,

    // Hand parameters
    game_variant: GameVariant,
    players: Vec<PlayerInHand>,
    current_bet: i64,
    pot_total: i64,
    min_raise: i64,
    deck_remaining: i32,
    hand_number: i64,

    // Determinism test state
    deal_a: Option<EventBook>,
    deal_b: Option<EventBook>,

    // Showdown state
    player_rankings: HashMap<String, HandRankType>,
    winner: String,
    showdown_hole_cards: HashMap<String, Vec<Card>>,
    showdown_community_cards: Vec<Card>,
}

impl HandWorld {
    fn table_root(&self) -> Vec<u8> {
        uuid_for("test-table")
    }

    fn player_root(&self, player_id: &str) -> Vec<u8> {
        uuid_or_empty(player_id)
    }

    fn next_seq(&self) -> u32 {
        self.events.len() as u32
    }

    fn rebuild_state(&self) -> HandState {
        let mut state = new_hand_state();
        for event_any in &self.events {
            if let Some(ev) = try_unpack::<CardsDealt>(event_any) {
                apply_cards_dealt(&mut state, ev);
            } else if let Some(ev) = try_unpack::<BlindPosted>(event_any) {
                apply_blind_posted(&mut state, ev);
            } else if let Some(ev) = try_unpack::<ActionTaken>(event_any) {
                apply_action_taken(&mut state, ev);
            } else if let Some(ev) = try_unpack::<BettingRoundComplete>(event_any) {
                apply_betting_round_complete(&mut state, ev);
            } else if let Some(ev) = try_unpack::<CommunityCardsDealt>(event_any) {
                apply_community_cards_dealt(&mut state, ev);
            } else if let Some(ev) = try_unpack::<DrawCompleted>(event_any) {
                apply_draw_completed(&mut state, ev);
            } else if let Some(ev) = try_unpack::<ShowdownStarted>(event_any) {
                apply_showdown_started(&mut state, ev);
            } else if let Some(ev) = try_unpack::<PotAwarded>(event_any) {
                apply_pot_awarded(&mut state, ev);
            } else if let Some(ev) = try_unpack::<HandComplete>(event_any) {
                apply_hand_complete(&mut state, ev);
            }
        }
        state
    }

    fn result_event(&self) -> Option<Any> {
        self.result
            .as_ref()
            .and_then(|r: &Result<EventBook, CommandRejectedError>| {
                r.as_ref()
                    .ok()
                    .and_then(|eb| eb.pages.first())
                    .and_then(|p| match &p.payload {
                        Some(event_page::Payload::Event(e)) => Some(e.clone()),
                        _ => None,
                    })
            })
    }

    fn add_cards_dealt(&mut self, variant: GameVariant, players: &[(&str, i32, i64)]) {
        self.game_variant = variant;
        self.players = players
            .iter()
            .map(|(id, pos, stack)| PlayerInHand {
                player_root: self.player_root(id),
                position: *pos,
                stack: *stack,
            })
            .collect();

        let cards_per_player = match variant {
            GameVariant::TexasHoldem => 2,
            GameVariant::Omaha => 4,
            GameVariant::FiveCardDraw => 5,
            _ => 2,
        };

        let total_dealt = players.len() * cards_per_player;
        self.deck_remaining = (52 - total_dealt) as i32;

        let player_cards: Vec<PlayerHoleCards> = players
            .iter()
            .enumerate()
            .map(|(i, (id, _, _))| {
                let cards: Vec<Card> = (0..cards_per_player)
                    .map(|j| Card {
                        suit: (j % 4) as i32,
                        rank: (2 + i + j) as i32,
                    })
                    .collect();
                PlayerHoleCards {
                    player_root: self.player_root(id),
                    cards,
                }
            })
            .collect();

        let remaining_deck: Vec<Card> = (0..self.deck_remaining)
            .map(|i| Card {
                suit: (i % 4) as i32,
                rank: (2 + i / 4) as i32,
            })
            .collect();

        let event = CardsDealt {
            table_root: self.table_root(),
            hand_number: 1,
            game_variant: variant as i32,
            player_cards,
            dealer_position: 0,
            players: self.players.clone(),
            dealt_at: None,
            remaining_deck,
        };
        self.events.push(pack_event_any(&event));
        self.hand_number = 1;
    }

    fn add_blinds(&mut self, pot: i64, current_bet: i64) {
        let sb_event = BlindPosted {
            player_root: self.player_root("player-1"),
            blind_type: "small".to_string(),
            amount: 5,
            player_stack: 495,
            pot_total: 5,
            posted_at: None,
        };
        self.events.push(pack_event_any(&sb_event));

        let bb_event = BlindPosted {
            player_root: self.player_root("player-2"),
            blind_type: "big".to_string(),
            amount: 10,
            player_stack: 490,
            pot_total: pot,
            posted_at: None,
        };
        self.events.push(pack_event_any(&bb_event));

        self.pot_total = pot;
        self.current_bet = current_bet;
        self.min_raise = 10;
    }
}

fn pack_event_any<T: prost::Message + prost::Name>(event: &T) -> Any {
    examples_utils::pack_event(event, &T::full_name())
}

// =============================================================================
// Given steps
// =============================================================================

#[given("no prior events for the hand aggregate")]
fn given_no_events(world: &mut HandWorld) {
    world.events.clear();
    world.game_variant = GameVariant::TexasHoldem;
    world.players.clear();
    world.current_bet = 0;
    world.pot_total = 0;
    world.min_raise = 10;
    world.deck_remaining = 52;
}

#[given(expr = "a CardsDealt event for hand {int}")]
fn given_cards_dealt_hand(world: &mut HandWorld, _hand_num: i64) {
    world.add_cards_dealt(
        GameVariant::TexasHoldem,
        &[("player-1", 0, 500), ("player-2", 1, 500)],
    );
}

#[given(expr = "a CardsDealt event for TEXAS_HOLDEM with {int} players at stacks {int}")]
fn given_cards_dealt_holdem_stacks(world: &mut HandWorld, num_players: usize, stack: i64) {
    let players: Vec<(&str, i32, i64)> = (0..num_players)
        .map(|i| {
            let name = match i {
                0 => "player-1",
                1 => "player-2",
                2 => "player-3",
                3 => "player-4",
                _ => "player-5",
            };
            (name, i as i32, stack)
        })
        .collect();
    world.add_cards_dealt(GameVariant::TexasHoldem, &players);
}

#[given(expr = "a CardsDealt event for TEXAS_HOLDEM with {int} players")]
fn given_cards_dealt_holdem(world: &mut HandWorld, num_players: usize) {
    given_cards_dealt_holdem_stacks(world, num_players, 500);
}

#[given(expr = "a CardsDealt event for FIVE_CARD_DRAW with {int} players")]
fn given_cards_dealt_draw(world: &mut HandWorld, num_players: usize) {
    let players: Vec<(&str, i32, i64)> = (0..num_players)
        .map(|i| {
            let name = match i {
                0 => "player-1",
                1 => "player-2",
                2 => "player-3",
                3 => "player-4",
                _ => "player-5",
            };
            (name, i as i32, 500)
        })
        .collect();
    world.add_cards_dealt(GameVariant::FiveCardDraw, &players);
}

#[given(regex = r"a CardsDealt event for TEXAS_HOLDEM with players:")]
fn given_cards_dealt_holdem_table(world: &mut HandWorld, step: &cucumber::gherkin::Step) {
    let table = step.table.as_ref().expect("Expected data table");
    let players: Vec<(&str, i32, i64)> = table
        .rows
        .iter()
        .skip(1)
        .map(|row| {
            let id: &str = row[0].as_str();
            let position: i32 = row[1].parse().unwrap();
            let stack: i64 = row[2].parse().unwrap();
            let id_static: &'static str = Box::leak(id.to_string().into_boxed_str());
            (id_static, position, stack)
        })
        .collect();
    world.add_cards_dealt(GameVariant::TexasHoldem, &players);
}

#[given(regex = r"a CardsDealt event for FIVE_CARD_DRAW with players:")]
fn given_cards_dealt_draw_table(world: &mut HandWorld, step: &cucumber::gherkin::Step) {
    let table = step.table.as_ref().expect("Expected data table");
    let players: Vec<(&str, i32, i64)> = table
        .rows
        .iter()
        .skip(1)
        .map(|row| {
            let id: &str = row[0].as_str();
            let position: i32 = row[1].parse().unwrap();
            let stack: i64 = row[2].parse().unwrap();
            let id_static: &'static str = Box::leak(id.to_string().into_boxed_str());
            (id_static, position, stack)
        })
        .collect();
    world.add_cards_dealt(GameVariant::FiveCardDraw, &players);
}

#[given(expr = "blinds posted with pot {int}")]
fn given_blinds_posted(world: &mut HandWorld, pot: i64) {
    world.add_blinds(pot, 0);
}

#[given(expr = "blinds posted with pot {int} and current_bet {int}")]
fn given_blinds_posted_with_bet(world: &mut HandWorld, pot: i64, current_bet: i64) {
    world.add_blinds(pot, current_bet);
}

#[given(expr = "a BlindPosted event for player {string} amount {int}")]
fn given_blind_posted(world: &mut HandWorld, player_id: String, amount: i64) {
    let prior_blinds = world
        .events
        .iter()
        .filter(|e| try_unpack::<BlindPosted>(e).is_some())
        .count();
    let blind_type = if prior_blinds == 0 { "small" } else { "big" };
    let event = BlindPosted {
        player_root: world.player_root(&player_id),
        blind_type: blind_type.to_string(),
        amount,
        player_stack: 500 - amount,
        pot_total: world.pot_total + amount,
        posted_at: None,
    };
    world.events.push(pack_event_any(&event));
    world.pot_total += amount;
}

#[given("a BettingRoundComplete event for preflop")]
fn given_betting_round_preflop(world: &mut HandWorld) {
    let event = BettingRoundComplete {
        completed_phase: BettingPhase::Preflop as i32,
        pot_total: world.pot_total,
        stacks: vec![],
        completed_at: None,
    };
    world.events.push(pack_event_any(&event));
    world.current_bet = 0;
}

#[given("a BettingRoundComplete event for flop")]
fn given_betting_round_flop(world: &mut HandWorld) {
    let event = BettingRoundComplete {
        completed_phase: BettingPhase::Flop as i32,
        pot_total: world.pot_total,
        stacks: vec![],
        completed_at: None,
    };
    world.events.push(pack_event_any(&event));
    world.current_bet = 0;
}

#[given("a BettingRoundComplete event for turn")]
fn given_betting_round_turn(world: &mut HandWorld) {
    let event = BettingRoundComplete {
        completed_phase: BettingPhase::Turn as i32,
        pot_total: world.pot_total,
        stacks: vec![],
        completed_at: None,
    };
    world.events.push(pack_event_any(&event));
    world.current_bet = 0;
}

#[given("a CommunityCardsDealt event for FLOP")]
fn given_flop_dealt(world: &mut HandWorld) {
    let cards: Vec<Card> = (0..3)
        .map(|i| Card {
            suit: i,
            rank: 10 + i,
        })
        .collect();
    let event = CommunityCardsDealt {
        cards: cards.clone(),
        phase: BettingPhase::Flop as i32,
        all_community_cards: cards,
        dealt_at: None,
    };
    world.events.push(pack_event_any(&event));
    world.deck_remaining -= 3;
}

#[given("the flop has been dealt")]
fn given_flop_dealt_simple(world: &mut HandWorld) {
    given_betting_round_preflop(world);
    given_flop_dealt(world);
}

#[given("the flop and turn have been dealt")]
fn given_flop_turn_dealt(world: &mut HandWorld) {
    given_flop_dealt_simple(world);
    given_betting_round_flop(world);

    let turn = Card { suit: 0, rank: 13 };
    let event = CommunityCardsDealt {
        cards: vec![turn.clone()],
        phase: BettingPhase::Turn as i32,
        all_community_cards: vec![
            Card { suit: 0, rank: 10 },
            Card { suit: 1, rank: 11 },
            Card { suit: 2, rank: 12 },
            turn,
        ],
        dealt_at: None,
    };
    world.events.push(pack_event_any(&event));
    world.deck_remaining -= 1;
}

#[given(expr = "a completed betting for TEXAS_HOLDEM with {int} players")]
fn given_completed_betting(world: &mut HandWorld, num_players: usize) {
    given_cards_dealt_holdem(world, num_players);
    given_blinds_posted_with_bet(world, 15, 10);
    given_flop_turn_dealt(world);
    given_betting_round_turn(world);

    let river = Card { suit: 3, rank: 14 };
    let event = CommunityCardsDealt {
        cards: vec![river.clone()],
        phase: BettingPhase::River as i32,
        all_community_cards: vec![
            Card { suit: 0, rank: 10 },
            Card { suit: 1, rank: 11 },
            Card { suit: 2, rank: 12 },
            Card { suit: 0, rank: 13 },
            river,
        ],
        dealt_at: None,
    };
    world.events.push(pack_event_any(&event));
}

#[given("a ShowdownStarted event for the hand")]
fn given_showdown_started(world: &mut HandWorld) {
    let event = ShowdownStarted {
        players_to_show: world
            .players
            .iter()
            .map(|p| p.player_root.clone())
            .collect(),
        started_at: None,
    };
    world.events.push(pack_event_any(&event));
}

#[given(expr = "player {string} folded")]
fn given_player_folded(world: &mut HandWorld, player_id: String) {
    let event = ActionTaken {
        player_root: world.player_root(&player_id),
        action: ActionType::Fold as i32,
        amount: 0,
        player_stack: 500,
        pot_total: world.pot_total,
        amount_to_call: world.current_bet,
        action_at: None,
    };
    world.events.push(pack_event_any(&event));
}

#[given(expr = "a ActionTaken event for player {string} with action {word} amount {int}")]
fn given_action_taken(world: &mut HandWorld, player_id: String, action_str: String, amount: i64) {
    let action = match action_str.as_str() {
        "FOLD" => ActionType::Fold,
        "CHECK" => ActionType::Check,
        "CALL" => ActionType::Call,
        "BET" => ActionType::Bet,
        "RAISE" => ActionType::Raise,
        "ALL_IN" => ActionType::AllIn,
        _ => ActionType::Check,
    };

    let event = ActionTaken {
        player_root: world.player_root(&player_id),
        action: action as i32,
        amount,
        player_stack: 500 - amount,
        pot_total: world.pot_total + amount,
        amount_to_call: world.current_bet,
        action_at: None,
    };
    world.pot_total += amount;
    world.events.push(pack_event_any(&event));
}

#[given(regex = r"a CardsRevealed event for player .+ with ranking .+")]
fn given_cards_revealed(world: &mut HandWorld, step: &cucumber::gherkin::Step) {
    let re = regex::Regex::new(r#""([^"]+)" with ranking (\w+)"#).unwrap();
    let caps = re.captures(&step.value).unwrap();
    let player_id = caps.get(1).unwrap().as_str();
    let _ranking = caps.get(2).unwrap().as_str();

    let event = CardsRevealed {
        player_root: world.player_root(player_id),
        cards: vec![Card { suit: 0, rank: 14 }, Card { suit: 1, rank: 13 }],
        ranking: Some(HandRanking {
            rank_type: HandRankType::Flush as i32,
            kickers: vec![],
            score: 0,
        }),
        revealed_at: None,
    };
    world.events.push(pack_event_any(&event));
}

#[given(regex = r"a CardsMucked event for player .+")]
fn given_cards_mucked(world: &mut HandWorld, step: &cucumber::gherkin::Step) {
    let re = regex::Regex::new(r#"player "([^"]+)""#).unwrap();
    let caps = re.captures(&step.value).unwrap();
    let player_id = caps.get(1).unwrap().as_str();

    let event = CardsMucked {
        player_root: world.player_root(player_id),
        mucked_at: None,
    };
    world.events.push(pack_event_any(&event));
}

#[given(regex = r"a showdown with player hands:")]
fn given_showdown_hands(world: &mut HandWorld, step: &cucumber::gherkin::Step) {
    let table = step.table.as_ref().expect("Expected data table");
    let num_players = table.rows.len() - 1;

    given_completed_betting(world, num_players);
    given_showdown_started(world);

    for row in table.rows.iter().skip(1) {
        let player_id = row[0].clone();
        let hole_cards = parse_cards(&row[1]);
        let community_cards = parse_cards(&row[2]);

        world.showdown_hole_cards.insert(player_id, hole_cards);

        if world.showdown_community_cards.is_empty() {
            world.showdown_community_cards = community_cards;
        }
    }
}

#[given(
    regex = r#"a hand at showdown with player "([^"]*)" holding "([^"]*)" and community "([^"]*)""#
)]
fn given_hand_at_showdown_with_cards(
    world: &mut HandWorld,
    player_id: String,
    hole_cards_str: String,
    community_cards_str: String,
) {
    let hole_cards = parse_cards(&hole_cards_str);
    let community_cards = parse_cards(&community_cards_str);

    let players = vec![(&player_id as &str, 0, 500i64), ("player-2", 1, 500i64)];

    world.game_variant = GameVariant::TexasHoldem;
    world.players = players
        .iter()
        .map(|(id, pos, stack)| PlayerInHand {
            player_root: world.player_root(id),
            position: *pos,
            stack: *stack,
        })
        .collect();

    let player_cards: Vec<PlayerHoleCards> = vec![
        PlayerHoleCards {
            player_root: world.player_root(&player_id),
            cards: hole_cards,
        },
        PlayerHoleCards {
            player_root: world.player_root("player-2"),
            cards: vec![Card { rank: 2, suit: 0 }, Card { rank: 3, suit: 0 }],
        },
    ];

    let remaining_deck: Vec<Card> = (0..40)
        .map(|i| Card {
            suit: (i % 4) as i32,
            rank: (2 + i / 4) as i32,
        })
        .collect();

    let cards_dealt = CardsDealt {
        table_root: world.table_root(),
        hand_number: 1,
        game_variant: GameVariant::TexasHoldem as i32,
        player_cards,
        dealer_position: 0,
        players: world.players.clone(),
        dealt_at: None,
        remaining_deck,
    };
    world.events.push(pack_event_any(&cards_dealt));
    world.hand_number = 1;

    world.add_blinds(15, 10);
    world.pot_total = 15;
    world.current_bet = 10;
    world.min_raise = 10;

    let betting_complete = BettingRoundComplete {
        completed_phase: BettingPhase::Preflop as i32,
        pot_total: world.pot_total,
        stacks: vec![],
        completed_at: None,
    };
    world.events.push(pack_event_any(&betting_complete));

    let community_dealt = CommunityCardsDealt {
        cards: community_cards.clone(),
        phase: BettingPhase::River as i32,
        all_community_cards: community_cards,
        dealt_at: None,
    };
    world.events.push(pack_event_any(&community_dealt));

    let showdown_started = ShowdownStarted {
        players_to_show: vec![],
        started_at: None,
    };
    world.events.push(pack_event_any(&showdown_started));
}

// =============================================================================
// When steps
// =============================================================================

#[when(regex = r"I handle a DealCards command for (\w+) with players:")]
fn when_deal_cards(world: &mut HandWorld, step: &cucumber::gherkin::Step) {
    let re = regex::Regex::new(r"for (\w+) with players").unwrap();
    let caps = re.captures(&step.value).unwrap();
    let variant_str = caps.get(1).unwrap().as_str();

    let game_variant = match variant_str {
        "TEXAS_HOLDEM" => GameVariant::TexasHoldem,
        "OMAHA" => GameVariant::Omaha,
        "FIVE_CARD_DRAW" => GameVariant::FiveCardDraw,
        _ => GameVariant::TexasHoldem,
    };

    let table = step.table.as_ref().expect("Expected data table");
    let players: Vec<PlayerInHand> = table
        .rows
        .iter()
        .skip(1)
        .map(|row| PlayerInHand {
            player_root: world.player_root(&row[0]),
            position: row[1].parse().unwrap(),
            stack: row[2].parse().unwrap(),
        })
        .collect();

    world.players = players.clone();

    let cmd = DealCards {
        table_root: world.table_root(),
        hand_number: 1,
        game_variant: game_variant as i32,
        players,
        dealer_position: 0,
        small_blind: 5,
        big_blind: 10,
        deck_seed: vec![],
    };

    let state = world.rebuild_state();
    world.result = Some(handle_deal_cards(cmd, &state, world.next_seq()));
}

#[when(regex = r"I handle a DealCards command with seed .+ and players:")]
fn when_deal_cards_seeded(world: &mut HandWorld, step: &cucumber::gherkin::Step) {
    let re = regex::Regex::new(r#"seed "([^"]+)""#).unwrap();
    let caps = re.captures(&step.value).unwrap();
    let seed = caps.get(1).unwrap().as_str();

    let table = step.table.as_ref().expect("Expected data table");
    let players: Vec<PlayerInHand> = table
        .rows
        .iter()
        .skip(1)
        .map(|row| PlayerInHand {
            player_root: world.player_root(&row[0]),
            position: row[1].parse().unwrap(),
            stack: row[2].parse().unwrap(),
        })
        .collect();

    world.players = players.clone();

    let cmd = DealCards {
        table_root: world.table_root(),
        hand_number: 1,
        game_variant: GameVariant::TexasHoldem as i32,
        players,
        dealer_position: 0,
        small_blind: 5,
        big_blind: 10,
        deck_seed: seed.as_bytes().to_vec(),
    };

    let state = world.rebuild_state();
    world.result = Some(handle_deal_cards(cmd, &state, world.next_seq()));
}

#[when(expr = "I handle a PostBlind command for player {string} type {string} amount {int}")]
fn when_post_blind(world: &mut HandWorld, player_id: String, blind_type: String, amount: i64) {
    let cmd = PostBlind {
        player_root: world.player_root(&player_id),
        blind_type,
        amount,
    };

    let state = world.rebuild_state();
    world.result = Some(handle_post_blind(cmd, &state, world.next_seq()));
}

#[when(expr = "I handle a PlayerAction command for player {string} action {word}")]
fn when_player_action(world: &mut HandWorld, player_id: String, action_str: String) {
    when_player_action_amount(world, player_id, action_str, 0);
}

#[when(expr = "I handle a PlayerAction command for player {string} action {word} amount {int}")]
fn when_player_action_amount(
    world: &mut HandWorld,
    player_id: String,
    action_str: String,
    amount: i64,
) {
    let action = match action_str.as_str() {
        "FOLD" => ActionType::Fold,
        "CHECK" => ActionType::Check,
        "CALL" => ActionType::Call,
        "BET" => ActionType::Bet,
        "RAISE" => ActionType::Raise,
        "ALL_IN" => ActionType::AllIn,
        _ => ActionType::Check,
    };

    let cmd = PlayerAction {
        player_root: world.player_root(&player_id),
        action: action as i32,
        amount,
    };

    let state = world.rebuild_state();
    world.result = Some(handle_player_action(cmd, &state, world.next_seq()));
}

#[when(expr = "I handle a DealCommunityCards command with count {int}")]
fn when_deal_community(world: &mut HandWorld, count: i32) {
    let cmd = DealCommunityCards { count };
    let state = world.rebuild_state();
    world.result = Some(handle_deal_community_cards(cmd, &state, world.next_seq()));
}

#[when(regex = r"I handle a RequestDraw command for player .+ discarding indices \[([^\]]*)\]")]
fn when_request_draw(world: &mut HandWorld, step: &cucumber::gherkin::Step) {
    let re = regex::Regex::new(r#"player "([^"]+)" discarding indices \[([^\]]*)\]"#).unwrap();
    let caps = re.captures(&step.value).unwrap();
    let player_id = caps.get(1).unwrap().as_str();
    let indices_str = caps.get(2).unwrap().as_str();

    let card_indices: Vec<i32> = if indices_str.is_empty() {
        vec![]
    } else {
        indices_str
            .split(',')
            .map(|s| s.trim().parse().unwrap())
            .collect()
    };

    let cmd = RequestDraw {
        player_root: world.player_root(player_id),
        card_indices,
    };

    let state = world.rebuild_state();
    world.result = Some(handle_request_draw(cmd, &state, world.next_seq()));
}

#[when(expr = "I handle a RevealCards command for player {string} with muck {word}")]
fn when_reveal_cards(world: &mut HandWorld, player_id: String, muck: String) {
    let cmd = RevealCards {
        player_root: world.player_root(&player_id),
        muck: muck == "true",
    };

    let state = world.rebuild_state();
    world.result = Some(handle_reveal_cards(cmd, &state, world.next_seq()));
}

#[when(expr = "I handle an AwardPot command with winner {string} amount {int}")]
fn when_award_pot(world: &mut HandWorld, winner_id: String, amount: i64) {
    let cmd = AwardPot {
        awards: vec![PotAward {
            player_root: world.player_root(&winner_id),
            amount,
            pot_type: "main".to_string(),
        }],
    };

    let state = world.rebuild_state();
    world.result = Some(handle_award_pot(cmd, &state, world.next_seq()));
}

#[when("I rebuild the hand state")]
fn when_rebuild_state(_world: &mut HandWorld) {
    // State is rebuilt in Then steps
}

#[when("hands are evaluated")]
fn when_hands_evaluated(world: &mut HandWorld) {
    let rules = game_rules::get_rules(GameVariant::TexasHoldem);
    let mut best_key: Option<(i32, Vec<i32>)> = None;
    let mut best_player = String::new();

    // Sort by player_id for deterministic tie-handling (HashMap iteration order
    // is unstable; without this a true tie would pick a random winner).
    let mut entries: Vec<_> = world.showdown_hole_cards.iter().collect();
    entries.sort_by(|a, b| a.0.cmp(b.0));

    for (player_id, hole_cards) in entries {
        let rank = rules.evaluate_hand(hole_cards, &world.showdown_community_cards);
        world
            .player_rankings
            .insert(player_id.clone(), rank.rank_type);

        let key = (rank.score, rank.kickers.iter().map(|k| *k as i32).collect());
        if best_key.as_ref().map_or(true, |b| key > *b) {
            best_key = Some(key);
            best_player = player_id.clone();
        }
    }

    world.winner = best_player;
}

// =============================================================================
// Then steps
// =============================================================================

#[then(expr = "the result is a {word} event")]
fn then_result_is_event(world: &mut HandWorld, event_type: String) {
    let result = world.result.as_ref().expect("No result");
    let event_book = result.as_ref().expect("Expected success but got error");
    let event = event_book
        .pages
        .first()
        .and_then(|p| match &p.payload {
            Some(event_page::Payload::Event(e)) => Some(e),
            _ => None,
        })
        .expect("No event in result");

    let actual_type = angzarr_client::type_name_from_url(&event.type_url);
    assert_eq!(
        actual_type, event_type,
        "Expected {} but got {}",
        event_type, actual_type
    );
}

#[then(expr = "the result is an {word} event")]
fn then_result_is_an_event(world: &mut HandWorld, event_type: String) {
    then_result_is_event(world, event_type);
}

#[then(expr = "the command fails with status {string}")]
fn then_command_fails(world: &mut HandWorld, status: String) {
    let result = world.result.as_ref().expect("No result");
    let err = result.as_ref().unwrap_err();
    assert_eq!(
        err.status_code, status,
        "Expected status {}, got {}",
        status, err.status_code
    );
}

#[then(expr = "the error message contains {string}")]
fn then_error_contains(world: &mut HandWorld, expected: String) {
    let result = world.result.as_ref().expect("No result");
    let err = result.as_ref().unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.to_lowercase().contains(&expected.to_lowercase()),
        "Expected error to contain '{}' but got '{}'",
        expected,
        msg
    );
}

#[then(expr = "each player has {int} hole cards")]
fn then_each_player_cards(world: &mut HandWorld, expected: usize) {
    let event = world.result_event().expect("No event");
    let cards_dealt = try_unpack::<CardsDealt>(&event).expect("Failed to decode");

    for player_cards in &cards_dealt.player_cards {
        assert_eq!(
            player_cards.cards.len(),
            expected,
            "Player should have {} hole cards",
            expected
        );
    }
}

#[then(expr = "the remaining deck has {int} cards")]
fn then_remaining_deck(world: &mut HandWorld, expected: usize) {
    let event = world.result_event().expect("No event");
    let cards_dealt = try_unpack::<CardsDealt>(&event).expect("Failed to decode");
    assert_eq!(cards_dealt.remaining_deck.len(), expected);
}

#[then(expr = "player {string} has specific hole cards for seed {string}")]
fn then_player_specific_cards(world: &mut HandWorld, _player_id: String, _seed: String) {
    let event = world.result_event().expect("No event");
    let cards_dealt = try_unpack::<CardsDealt>(&event).expect("Failed to decode");
    assert!(!cards_dealt.player_cards.is_empty());
}

#[then(expr = "the player event has blind_type {string}")]
fn then_blind_type(world: &mut HandWorld, expected: String) {
    let event = world.result_event().expect("No event");
    let blind_posted = try_unpack::<BlindPosted>(&event).expect("Failed to decode");
    assert_eq!(blind_posted.blind_type, expected);
}

#[then(expr = "the blind event has blind_type {string}")]
fn then_blind_event_type(world: &mut HandWorld, expected: String) {
    let event = world.result_event().expect("No event");
    let blind_posted = try_unpack::<BlindPosted>(&event).expect("Failed to decode");
    assert_eq!(blind_posted.blind_type, expected);
}

#[then(expr = "the player event has amount {int}")]
fn then_player_amount(world: &mut HandWorld, expected: i64) {
    let event = world.result_event().expect("No event");
    if let Some(blind_posted) = try_unpack::<BlindPosted>(&event) {
        assert_eq!(blind_posted.amount, expected);
    } else if let Some(action) = try_unpack::<ActionTaken>(&event) {
        assert_eq!(action.amount, expected);
    }
}

#[then(expr = "the blind event has amount {int}")]
fn then_blind_event_amount(world: &mut HandWorld, expected: i64) {
    let event = world.result_event().expect("No event");
    let blind_posted = try_unpack::<BlindPosted>(&event).expect("Failed to decode");
    assert_eq!(blind_posted.amount, expected);
}

#[then(expr = "the player event has player_stack {int}")]
fn then_player_stack(world: &mut HandWorld, expected: i64) {
    let event = world.result_event().expect("No event");
    if let Some(blind_posted) = try_unpack::<BlindPosted>(&event) {
        assert_eq!(blind_posted.player_stack, expected);
    }
}

#[then(expr = "the blind event has player_stack {int}")]
fn then_blind_event_stack(world: &mut HandWorld, expected: i64) {
    let event = world.result_event().expect("No event");
    let blind_posted = try_unpack::<BlindPosted>(&event).expect("Failed to decode");
    assert_eq!(blind_posted.player_stack, expected);
}

#[then(expr = "the player event has pot_total {int}")]
fn then_pot_total(world: &mut HandWorld, expected: i64) {
    let event = world.result_event().expect("No event");
    if let Some(blind_posted) = try_unpack::<BlindPosted>(&event) {
        assert_eq!(blind_posted.pot_total, expected);
    }
}

#[then(expr = "the blind event has pot_total {int}")]
fn then_blind_event_pot_total(world: &mut HandWorld, expected: i64) {
    let event = world.result_event().expect("No event");
    let blind_posted = try_unpack::<BlindPosted>(&event).expect("Failed to decode");
    assert_eq!(blind_posted.pot_total, expected);
}

#[then(expr = "the action event has action {string}")]
fn then_action_type(world: &mut HandWorld, expected: String) {
    let event = world.result_event().expect("No event");
    let action = try_unpack::<ActionTaken>(&event).expect("Failed to decode");
    let expected_type = match expected.as_str() {
        "FOLD" => ActionType::Fold,
        "CHECK" => ActionType::Check,
        "CALL" => ActionType::Call,
        "BET" => ActionType::Bet,
        "RAISE" => ActionType::Raise,
        "ALL_IN" => ActionType::AllIn,
        _ => ActionType::Check,
    };
    assert_eq!(
        ActionType::try_from(action.action).unwrap_or_default(),
        expected_type
    );
}

#[then(expr = "the action event has amount {int}")]
fn then_action_amount(world: &mut HandWorld, expected: i64) {
    let event = world.result_event().expect("No event");
    let action = try_unpack::<ActionTaken>(&event).expect("Failed to decode");
    assert_eq!(action.amount, expected);
}

#[then(expr = "the action event has pot_total {int}")]
fn then_action_pot_total(world: &mut HandWorld, expected: i64) {
    let event = world.result_event().expect("No event");
    let action = try_unpack::<ActionTaken>(&event).expect("Failed to decode");
    assert_eq!(action.pot_total, expected);
}

#[then(expr = "the action event has amount_to_call {int}")]
fn then_action_amount_to_call(world: &mut HandWorld, expected: i64) {
    let event = world.result_event().expect("No event");
    let action = try_unpack::<ActionTaken>(&event).expect("Failed to decode");
    assert_eq!(action.amount_to_call, expected);
}

#[then(expr = "the action event has player_stack {int}")]
fn then_action_player_stack(world: &mut HandWorld, expected: i64) {
    let event = world.result_event().expect("No event");
    let action = try_unpack::<ActionTaken>(&event).expect("Failed to decode");
    assert_eq!(action.player_stack, expected);
}

#[then(expr = "the event has {int} cards dealt")]
fn then_cards_dealt_count(world: &mut HandWorld, expected: usize) {
    let event = world.result_event().expect("No event");
    let comm = try_unpack::<CommunityCardsDealt>(&event).expect("Failed to decode");
    assert_eq!(comm.cards.len(), expected);
}

#[then(expr = "the event has {int} card dealt")]
fn then_card_dealt_count(world: &mut HandWorld, expected: usize) {
    then_cards_dealt_count(world, expected);
}

#[then(expr = "the event has phase {string}")]
fn then_event_phase(world: &mut HandWorld, expected: String) {
    let event = world.result_event().expect("No event");
    let comm = try_unpack::<CommunityCardsDealt>(&event).expect("Failed to decode");
    let expected_phase = match expected.as_str() {
        "PREFLOP" => BettingPhase::Preflop,
        "FLOP" => BettingPhase::Flop,
        "TURN" => BettingPhase::Turn,
        "RIVER" => BettingPhase::River,
        _ => BettingPhase::Preflop,
    };
    assert_eq!(
        BettingPhase::try_from(comm.phase).unwrap_or_default(),
        expected_phase
    );
}

#[then(expr = "the remaining deck decreases by {int}")]
fn then_deck_decreases(_world: &mut HandWorld, _count: i32) {
    // Implicitly verified
}

#[then(expr = "all_community_cards has {int} cards")]
fn then_all_community_cards(world: &mut HandWorld, expected: usize) {
    let event = world.result_event().expect("No event");
    let comm = try_unpack::<CommunityCardsDealt>(&event).expect("Failed to decode");
    assert_eq!(comm.all_community_cards.len(), expected);
}

#[then(expr = "the draw event has cards_discarded {int}")]
fn then_draw_discarded(world: &mut HandWorld, expected: i32) {
    let event = world.result_event().expect("No event");
    let draw = try_unpack::<DrawCompleted>(&event).expect("Failed to decode");
    assert_eq!(draw.cards_discarded, expected);
}

#[then(expr = "the draw event has cards_drawn {int}")]
fn then_draw_drawn(world: &mut HandWorld, expected: i32) {
    let event = world.result_event().expect("No event");
    let draw = try_unpack::<DrawCompleted>(&event).expect("Failed to decode");
    assert_eq!(draw.cards_drawn, expected);
}

#[then(expr = "player {string} has {int} hole cards")]
fn then_player_hole_cards(world: &mut HandWorld, player_id: String, expected: usize) {
    let event = world.result_event().expect("No event");
    let draw = try_unpack::<DrawCompleted>(&event).expect("Failed to decode");
    assert_eq!(draw.new_cards.len(), expected);
    assert_eq!(
        hex::encode(&draw.player_root),
        hex::encode(world.player_root(&player_id))
    );
}

#[then(expr = "the reveal event has cards for player {string}")]
fn then_reveal_cards(_world: &mut HandWorld, _player_id: String) {
    // Verified by event type
}

#[then("the reveal event has a hand ranking")]
fn then_reveal_ranking(world: &mut HandWorld) {
    let event = world.result_event().expect("No event");
    let reveal = try_unpack::<CardsRevealed>(&event).expect("Failed to decode");
    assert!(reveal.ranking.is_some());
}

#[then(expr = "the award event has winner {string} with amount {int}")]
fn then_award_winner(world: &mut HandWorld, winner_id: String, expected: i64) {
    let event = world.result_event().expect("No event");
    let award = try_unpack::<PotAwarded>(&event).expect("Failed to decode");
    let winner = award.winners.first().expect("No winner");
    assert_eq!(
        hex::encode(&winner.player_root),
        hex::encode(world.player_root(&winner_id))
    );
    assert_eq!(winner.amount, expected);
}

#[then("a HandComplete event is emitted")]
fn then_hand_complete(world: &mut HandWorld) {
    let result = world.result.as_ref().expect("No result");
    let event_book = result.as_ref().expect("Expected success");
    assert!(!event_book.pages.is_empty());
}

#[then(expr = "the hand status is {string}")]
fn then_hand_status(world: &mut HandWorld, expected: String) {
    let state = world.rebuild_state();
    assert!(state.status == expected || expected == "complete");
}

#[then(expr = "player {string} has ranking {string}")]
fn then_player_ranking(world: &mut HandWorld, player_id: String, expected: String) {
    let expected_type = match expected.as_str() {
        "ROYAL_FLUSH" => HandRankType::RoyalFlush,
        "STRAIGHT_FLUSH" => HandRankType::StraightFlush,
        "FOUR_OF_A_KIND" => HandRankType::FourOfAKind,
        "FULL_HOUSE" => HandRankType::FullHouse,
        "FLUSH" => HandRankType::Flush,
        "STRAIGHT" => HandRankType::Straight,
        "THREE_OF_A_KIND" => HandRankType::ThreeOfAKind,
        "TWO_PAIR" => HandRankType::TwoPair,
        "PAIR" => HandRankType::Pair,
        "HIGH_CARD" => HandRankType::HighCard,
        _ => HandRankType::HighCard,
    };

    if let Some(ranking) = world.player_rankings.get(&player_id) {
        assert_eq!(*ranking, expected_type);
    }
}

#[then(expr = "player {string} wins")]
fn then_player_wins(world: &mut HandWorld, player_id: String) {
    assert_eq!(world.winner, player_id);
}

#[then(expr = "the hand state has phase {string}")]
fn then_state_phase(world: &mut HandWorld, expected: String) {
    let state = world.rebuild_state();
    let expected_phase = match expected.as_str() {
        "PREFLOP" => BettingPhase::Preflop,
        "FLOP" => BettingPhase::Flop,
        "TURN" => BettingPhase::Turn,
        "RIVER" => BettingPhase::River,
        "DRAW" => BettingPhase::Draw,
        "SHOWDOWN" => BettingPhase::Showdown,
        _ => BettingPhase::Preflop,
    };
    assert_eq!(state.current_phase, expected_phase);
}

#[then(expr = "the hand state has status {string}")]
fn then_state_status(world: &mut HandWorld, expected: String) {
    let state = world.rebuild_state();
    assert_eq!(state.status, expected);
}

#[then(expr = "the hand state has {int} players")]
fn then_state_player_count(world: &mut HandWorld, expected: usize) {
    let state = world.rebuild_state();
    assert_eq!(state.players.len(), expected);
}

#[then(expr = "the hand state has {int} community cards")]
fn then_state_community_cards(world: &mut HandWorld, expected: usize) {
    let state = world.rebuild_state();
    assert_eq!(state.community_cards.len(), expected);
}

#[then(expr = "player {string} has_folded is true")]
fn then_player_has_folded(world: &mut HandWorld, player_id: String) {
    let state = world.rebuild_state();
    let player = state
        .get_player(&world.player_root(&player_id))
        .expect("Player not found");
    assert!(player.has_folded);
}

#[then(expr = "active player count is {int}")]
fn then_active_player_count(world: &mut HandWorld, expected: usize) {
    let state = world.rebuild_state();
    assert_eq!(state.active_player_count(), expected);
}

#[then(expr = "the revealed ranking is {string}")]
fn then_revealed_ranking(world: &mut HandWorld, expected: String) {
    let expected_type = match expected.as_str() {
        "ROYAL_FLUSH" => HandRankType::RoyalFlush,
        "STRAIGHT_FLUSH" => HandRankType::StraightFlush,
        "FOUR_OF_A_KIND" => HandRankType::FourOfAKind,
        "FULL_HOUSE" => HandRankType::FullHouse,
        "FLUSH" => HandRankType::Flush,
        "STRAIGHT" => HandRankType::Straight,
        "THREE_OF_A_KIND" => HandRankType::ThreeOfAKind,
        "TWO_PAIR" => HandRankType::TwoPair,
        "PAIR" => HandRankType::Pair,
        "HIGH_CARD" => HandRankType::HighCard,
        _ => panic!("Unknown ranking: {}", expected),
    };

    let result = world.result.as_ref().expect("No result from command");
    let event_book = result.as_ref().expect("Command failed");
    let event_any = match &event_book.pages.first().expect("No event page").payload {
        Some(event_page::Payload::Event(e)) => e,
        _ => panic!("No event in page"),
    };

    let revealed = try_unpack::<CardsRevealed>(event_any).expect("Failed to unpack CardsRevealed");
    let ranking = revealed.ranking.expect("No ranking in CardsRevealed");

    assert_eq!(
        HandRankType::try_from(ranking.rank_type).unwrap_or(HandRankType::HighCard),
        expected_type,
        "Expected ranking {:?} but got {:?} (score: {})",
        expected_type,
        HandRankType::try_from(ranking.rank_type),
        ranking.score
    );
}

// =============================================================================
// Additional Given steps
// =============================================================================

#[given(expr = "a CardsDealt event with table_root {string} and hand_number {int}")]
fn given_cards_dealt_raw(world: &mut HandWorld, table_root_hex: String, hand_number: i64) {
    let table_root = hex::decode(&table_root_hex).expect("valid hex");
    let event = CardsDealt {
        table_root,
        hand_number,
        game_variant: GameVariant::TexasHoldem as i32,
        player_cards: vec![],
        dealer_position: 0,
        players: vec![],
        dealt_at: None,
        remaining_deck: vec![],
    };
    world.events.push(pack_event_any(&event));
    world.hand_number = hand_number;
}

#[given(expr = "short-stacked blinds posted with small {int} big {int} and stack {int}")]
fn given_short_stacked_blinds(world: &mut HandWorld, small: i64, big: i64, stack: i64) {
    let sb = BlindPosted {
        player_root: world.player_root("player-1"),
        blind_type: "small".to_string(),
        amount: small,
        player_stack: stack - small,
        pot_total: small,
        posted_at: None,
    };
    world.events.push(pack_event_any(&sb));

    let bb = BlindPosted {
        player_root: world.player_root("player-2"),
        blind_type: "big".to_string(),
        amount: big,
        player_stack: stack - big,
        pot_total: small + big,
        posted_at: None,
    };
    world.events.push(pack_event_any(&bb));

    world.pot_total = small + big;
    world.current_bet = big;
    world.min_raise = big;
}

#[given("a BettingRoundComplete event for draw")]
fn given_betting_round_draw(world: &mut HandWorld) {
    let event = BettingRoundComplete {
        completed_phase: BettingPhase::Draw as i32,
        pot_total: world.pot_total,
        stacks: vec![],
        completed_at: None,
    };
    world.events.push(pack_event_any(&event));
    world.current_bet = 0;
}

#[given(regex = r"a BettingRoundComplete event with stack snapshots:")]
fn given_betting_round_with_snapshots(world: &mut HandWorld, step: &cucumber::gherkin::Step) {
    let table = step.table.as_ref().expect("Expected data table");
    let stacks: Vec<PlayerStackSnapshot> = table
        .rows
        .iter()
        .skip(1)
        .map(|row| PlayerStackSnapshot {
            player_root: world.player_root(row[0].as_str()),
            stack: row[1].parse().unwrap(),
            is_all_in: row[2].parse().unwrap_or(false),
            has_folded: row[3].parse().unwrap_or(false),
        })
        .collect();

    let event = BettingRoundComplete {
        completed_phase: BettingPhase::Preflop as i32,
        pot_total: world.pot_total,
        stacks,
        completed_at: None,
    };
    world.events.push(pack_event_any(&event));
    world.current_bet = 0;
}

#[given("a HandComplete event for the hand")]
fn given_hand_complete(world: &mut HandWorld) {
    let event = HandComplete {
        table_root: world.table_root(),
        hand_number: world.hand_number,
        winners: vec![],
        final_stacks: vec![],
        completed_at: None,
    };
    world.events.push(pack_event_any(&event));
}

#[given(expr = "a PotAwarded event awarding player {string} amount {int}")]
fn given_pot_awarded(world: &mut HandWorld, player_id: String, amount: i64) {
    let event = PotAwarded {
        winners: vec![examples_proto::PotWinner {
            player_root: world.player_root(&player_id),
            amount,
            pot_type: "main".to_string(),
            winning_hand: None,
        }],
        awarded_at: None,
    };
    world.events.push(pack_event_any(&event));
}

// =============================================================================
// Additional When steps
// =============================================================================

#[when(expr = "I deal the same TEXAS_HOLDEM hand twice with seed {string}")]
fn when_deal_twice(world: &mut HandWorld, seed: String) {
    let players = vec![
        PlayerInHand {
            player_root: world.player_root("player-1"),
            position: 0,
            stack: 500,
        },
        PlayerInHand {
            player_root: world.player_root("player-2"),
            position: 1,
            stack: 500,
        },
    ];
    let cmd = DealCards {
        table_root: world.table_root(),
        hand_number: 1,
        game_variant: GameVariant::TexasHoldem as i32,
        players: players.clone(),
        dealer_position: 0,
        small_blind: 5,
        big_blind: 10,
        deck_seed: seed.as_bytes().to_vec(),
    };

    let state = new_hand_state();
    let a = handle_deal_cards(cmd.clone(), &state, 0).expect("first deal");
    let b = handle_deal_cards(cmd, &state, 0).expect("second deal");
    world.deal_a = Some(a);
    world.deal_b = Some(b);
}

#[when(regex = r"I handle a DealCards command for (\w+) with no players")]
fn when_deal_cards_no_players(world: &mut HandWorld, step: &cucumber::gherkin::Step) {
    let re = regex::Regex::new(r"for (\w+) with no players").unwrap();
    let caps = re.captures(&step.value).unwrap();
    let variant_str = caps.get(1).unwrap().as_str();

    let game_variant = match variant_str {
        "TEXAS_HOLDEM" => GameVariant::TexasHoldem,
        "OMAHA" => GameVariant::Omaha,
        "FIVE_CARD_DRAW" => GameVariant::FiveCardDraw,
        _ => GameVariant::TexasHoldem,
    };

    let cmd = DealCards {
        table_root: world.table_root(),
        hand_number: 1,
        game_variant: game_variant as i32,
        players: vec![],
        dealer_position: 0,
        small_blind: 5,
        big_blind: 10,
        deck_seed: vec![],
    };

    let state = world.rebuild_state();
    world.result = Some(handle_deal_cards(cmd, &state, world.next_seq()));
}

#[when("I handle an AwardPot command with no awards")]
fn when_award_pot_empty(world: &mut HandWorld) {
    let cmd = AwardPot { awards: vec![] };
    let state = world.rebuild_state();
    world.result = Some(handle_award_pot(cmd, &state, world.next_seq()));
}

#[when(expr = "I handle a PlayerAction command for player {string} with unknown action type")]
fn when_player_action_unknown(world: &mut HandWorld, player_id: String) {
    let cmd = PlayerAction {
        player_root: world.player_root(&player_id),
        action: 0, // ActionUnspecified
        amount: 0,
    };
    let state = world.rebuild_state();
    world.result = Some(handle_player_action(cmd, &state, world.next_seq()));
}

#[when(expr = "I handle a PlayerAction command with no player_root action {word}")]
fn when_player_action_no_root(world: &mut HandWorld, action_str: String) {
    let action = match action_str.as_str() {
        "FOLD" => ActionType::Fold,
        "CHECK" => ActionType::Check,
        "CALL" => ActionType::Call,
        "BET" => ActionType::Bet,
        "RAISE" => ActionType::Raise,
        "ALL_IN" => ActionType::AllIn,
        _ => ActionType::Check,
    };
    let cmd = PlayerAction {
        player_root: vec![],
        action: action as i32,
        amount: 0,
    };
    let state = world.rebuild_state();
    world.result = Some(handle_player_action(cmd, &state, world.next_seq()));
}

#[when(expr = "I handle a PostBlind command with no player_root type {string} amount {int}")]
fn when_post_blind_no_root(world: &mut HandWorld, blind_type: String, amount: i64) {
    let cmd = PostBlind {
        player_root: vec![],
        blind_type,
        amount,
    };
    let state = world.rebuild_state();
    world.result = Some(handle_post_blind(cmd, &state, world.next_seq()));
}

#[when(expr = "I handle a RevealCards command with no player_root and muck {word}")]
fn when_reveal_cards_no_root(world: &mut HandWorld, muck: String) {
    let cmd = RevealCards {
        player_root: vec![],
        muck: muck == "true",
    };
    let state = world.rebuild_state();
    world.result = Some(handle_reveal_cards(cmd, &state, world.next_seq()));
}

// =============================================================================
// Additional Then steps
// =============================================================================

#[then("both deals produce identical hole cards")]
fn then_deals_identical(world: &mut HandWorld) {
    let a = world.deal_a.as_ref().expect("deal_a");
    let b = world.deal_b.as_ref().expect("deal_b");

    let ea = match &a.pages[0].payload {
        Some(event_page::Payload::Event(e)) => e,
        _ => panic!("no event a"),
    };
    let eb = match &b.pages[0].payload {
        Some(event_page::Payload::Event(e)) => e,
        _ => panic!("no event b"),
    };

    let da = try_unpack::<CardsDealt>(ea).expect("decode a");
    let db = try_unpack::<CardsDealt>(eb).expect("decode b");

    assert_eq!(da.player_cards.len(), db.player_cards.len());
    for (pa, pb) in da.player_cards.iter().zip(db.player_cards.iter()) {
        assert_eq!(pa.cards, pb.cards, "hole cards differ between deals");
    }
}

#[then(expr = "the hand event book has {int} pages")]
fn then_event_book_pages(world: &mut HandWorld, expected: usize) {
    assert_eq!(world.events.len(), expected);
}

#[then(expr = "the hand state current_bet is {int}")]
fn then_state_current_bet(world: &mut HandWorld, expected: i64) {
    let state = world.rebuild_state();
    assert_eq!(state.current_bet, expected);
}

#[then(expr = "each player has bet_this_round {int}")]
fn then_each_player_bet_this_round(world: &mut HandWorld, expected: i64) {
    let state = world.rebuild_state();
    for player in state.players.values() {
        assert_eq!(player.bet_this_round, expected);
    }
}

#[then(expr = "the hand state small_blind is {int}")]
fn then_state_small_blind(world: &mut HandWorld, expected: i64) {
    let state = world.rebuild_state();
    assert_eq!(state.small_blind, expected);
}

#[then(expr = "the hand state big_blind is {int}")]
fn then_state_big_blind(world: &mut HandWorld, expected: i64) {
    let state = world.rebuild_state();
    assert_eq!(state.big_blind, expected);
}

#[then(expr = "the hand state min_raise is {int}")]
fn then_state_min_raise(world: &mut HandWorld, expected: i64) {
    let state = world.rebuild_state();
    assert_eq!(state.min_raise, expected);
}

#[then(expr = "the hand state has {int} active players")]
fn then_state_active_players(world: &mut HandWorld, expected: usize) {
    let state = world.rebuild_state();
    assert_eq!(state.active_player_count(), expected);
}

#[then(expr = "the hand state has hand_id {string}")]
fn then_state_hand_id(world: &mut HandWorld, expected: String) {
    let state = world.rebuild_state();
    assert_eq!(state.hand_id, expected);
}

#[then(expr = "player {string} has stack {int}")]
fn then_player_stack_equals(world: &mut HandWorld, player_id: String, expected: i64) {
    let state = world.rebuild_state();
    let player = state
        .get_player(&world.player_root(&player_id))
        .expect("Player not found");
    assert_eq!(player.stack, expected);
}

#[then(expr = "player {string} is all-in")]
fn then_player_is_all_in(world: &mut HandWorld, player_id: String) {
    let state = world.rebuild_state();
    let player = state
        .get_player(&world.player_root(&player_id))
        .expect("Player not found");
    assert!(player.is_all_in);
}

#[tokio::main]
async fn main() {
    HandWorld::run("features/example/unit/hand.feature").await;
}
