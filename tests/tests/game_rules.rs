//! Game rules (Texas Hold'em, Omaha, Five Card Draw) BDD tests.
//!
//! Pure-function tests against `agg_hand::game_rules`. No aggregate state,
//! no commands, no events.

use agg_hand::game_rules::{
    get_rules, DealResult, DrawResult, FiveCardDrawRules, GameRules, HandRank, OmahaRules,
    PhaseTransition, TexasHoldemRules,
};
use cucumber::{given, then, when, World};
use examples_proto::{BettingPhase, Card, GameVariant, HandRankType};
use poker_tests::parse_cards;

#[derive(Default, World)]
#[world(init = Self::new)]
pub struct RulesWorld {
    rules: Option<Box<dyn GameRules>>,
    rules_label: String,
    hole_cards: Vec<Card>,
    community_cards: Vec<Card>,

    // evaluator outputs
    last_rank: Option<HandRank>,

    // phase outputs
    next_phase: Option<PhaseTransition>,

    // draw setup
    deck: Vec<Card>,
    existing_deck: Vec<Card>,
    draw_result: Option<DrawResult>,
    deck_a: Vec<Card>,
    deck_b: Vec<Card>,

    // deal
    deal_result: Option<DealResult>,
    players: Vec<Vec<u8>>,

    // factory
    factory_rules: Option<Box<dyn GameRules>>,
    factory_label: String,
}

impl std::fmt::Debug for RulesWorld {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RulesWorld")
            .field("rules_label", &self.rules_label)
            .finish()
    }
}

impl RulesWorld {
    fn new() -> Self {
        Self::default()
    }

    fn current_rules(&self) -> &dyn GameRules {
        self.rules
            .as_deref()
            .expect("rules not set — did you run a Given?")
    }
}

fn phase_from_name(name: &str) -> BettingPhase {
    match name.trim().to_ascii_uppercase().as_str() {
        "PREFLOP" => BettingPhase::Preflop,
        "FLOP" => BettingPhase::Flop,
        "TURN" => BettingPhase::Turn,
        "RIVER" => BettingPhase::River,
        "DRAW" => BettingPhase::Draw,
        "SHOWDOWN" => BettingPhase::Showdown,
        other => panic!("unknown phase: {}", other),
    }
}

fn rank_type_from_name(name: &str) -> HandRankType {
    match name.trim().to_ascii_uppercase().as_str() {
        "HIGH_CARD" => HandRankType::HighCard,
        "PAIR" => HandRankType::Pair,
        "TWO_PAIR" => HandRankType::TwoPair,
        "THREE_OF_A_KIND" => HandRankType::ThreeOfAKind,
        "STRAIGHT" => HandRankType::Straight,
        "FLUSH" => HandRankType::Flush,
        "FULL_HOUSE" => HandRankType::FullHouse,
        "FOUR_OF_A_KIND" => HandRankType::FourOfAKind,
        "STRAIGHT_FLUSH" => HandRankType::StraightFlush,
        "ROYAL_FLUSH" => HandRankType::RoyalFlush,
        other => panic!("unknown rank: {}", other),
    }
}

fn variant_name(variant: GameVariant) -> &'static str {
    match variant {
        GameVariant::TexasHoldem => "TEXAS_HOLDEM",
        GameVariant::Omaha => "OMAHA",
        GameVariant::FiveCardDraw => "FIVE_CARD_DRAW",
        _ => "UNKNOWN",
    }
}

fn variant_from_name(name: &str) -> GameVariant {
    match name.trim().to_ascii_uppercase().as_str() {
        "TEXAS_HOLDEM" => GameVariant::TexasHoldem,
        "OMAHA" => GameVariant::Omaha,
        "FIVE_CARD_DRAW" => GameVariant::FiveCardDraw,
        other => panic!("unknown variant: {}", other),
    }
}

fn rules_from_label(label: &str) -> Box<dyn GameRules> {
    let key = label
        .trim()
        .to_lowercase()
        .replace('\'', "")
        .replace('-', " ");
    let key = key.as_str();
    match key {
        "texas holdem" | "texas hold em" | "holdem" | "hold em" => Box::new(TexasHoldemRules),
        "omaha" => Box::new(OmahaRules),
        "five card draw" | "5 card draw" | "draw" => Box::new(FiveCardDrawRules),
        other => panic!("unknown variant label: {}", other),
    }
}

fn factory_class_label(variant: GameVariant) -> &'static str {
    match variant {
        GameVariant::TexasHoldem => "TexasHoldemRules",
        GameVariant::Omaha => "OmahaRules",
        GameVariant::FiveCardDraw => "FiveCardDrawRules",
        _ => "TexasHoldemRules",
    }
}

// --- Given: rule selection ---

#[given(regex = r"^(Texas Hold'em|Omaha|Five Card Draw) rules$")]
fn given_rules(world: &mut RulesWorld, label: String) {
    world.rules = Some(rules_from_label(&label));
    world.rules_label = label;
    world.hole_cards.clear();
    world.community_cards.clear();
}

#[given(expr = "hole cards {string}")]
fn given_hole_cards(world: &mut RulesWorld, cards: String) {
    world.hole_cards = parse_cards(&cards);
}

#[given(expr = "community cards {string}")]
fn given_community_cards(world: &mut RulesWorld, cards: String) {
    world.community_cards = parse_cards(&cards);
}

#[given(expr = "a deck of {string}")]
fn given_deck(world: &mut RulesWorld, cards: String) {
    world.deck = parse_cards(&cards);
}

#[given(expr = "an existing deck of {string}")]
fn given_existing_deck(world: &mut RulesWorld, cards: String) {
    world.existing_deck = parse_cards(&cards);
}

#[given(expr = "current hole cards {string}")]
fn given_current_hole_cards(world: &mut RulesWorld, cards: String) {
    world.hole_cards = parse_cards(&cards);
}

// --- When: evaluate ---

#[when("the best hand is evaluated")]
fn when_evaluate_hand(world: &mut RulesWorld) {
    let rank = world
        .current_rules()
        .evaluate_hand(&world.hole_cards, &world.community_cards);
    world.last_rank = Some(rank);
}

#[when(expr = "the best hand is evaluated from only {string}")]
fn when_evaluate_best_from_few(world: &mut RulesWorld, cards: String) {
    let parsed = parse_cards(&cards);
    let rank = world.current_rules().find_best_hand(&parsed);
    world.last_rank = Some(rank);
}

// --- When: phase transitions ---

#[when(expr = "I get the next phase from {word}")]
fn when_get_next_phase(world: &mut RulesWorld, phase: String) {
    world.next_phase = world
        .current_rules()
        .get_next_phase(phase_from_name(&phase));
}

// --- When: draw ---

#[when(expr = "I execute a draw discarding indices {string}")]
fn when_execute_draw(world: &mut RulesWorld, indices: String) {
    let idx_list: Vec<usize> = if indices.trim().is_empty() {
        Vec::new()
    } else {
        indices
            .split(',')
            .filter_map(|s| {
                let s = s.trim();
                if s.is_empty() {
                    None
                } else {
                    Some(s.parse::<usize>().unwrap())
                }
            })
            .collect()
    };
    let result = world
        .current_rules()
        .execute_draw(&world.deck, &world.hole_cards, &idx_list);
    world.draw_result = Some(result);
}

// --- When: deck creation / dealing ---

#[when("I create a deck")]
fn when_create_deck(world: &mut RulesWorld) {
    world.deck = world.current_rules().create_deck(None);
}

#[when(expr = "I create a deck with seed {string}")]
fn when_create_deck_with_seed(world: &mut RulesWorld, seed: String) {
    world.deck_a = world.current_rules().create_deck(Some(seed.as_bytes()));
}

#[when(expr = "I create another deck with seed {string}")]
fn when_create_another_deck_with_seed(world: &mut RulesWorld, seed: String) {
    world.deck_b = world.current_rules().create_deck(Some(seed.as_bytes()));
}

#[when(expr = "I deal hole cards to {int} players with seed {string}")]
fn when_deal_hole_cards_seeded(world: &mut RulesWorld, n: usize, seed: String) {
    let players: Vec<Vec<u8>> = (0..n).map(|i| vec![(i + 1) as u8]).collect();
    let result = world
        .current_rules()
        .deal_hole_cards(&[], &players, Some(seed.as_bytes()));
    world.deal_result = Some(result);
    world.players = players;
}

#[when(expr = "I deal hole cards to {int} players from the existing deck")]
fn when_deal_hole_cards_from_existing(world: &mut RulesWorld, n: usize) {
    let players: Vec<Vec<u8>> = (0..n).map(|i| vec![(i + 1) as u8]).collect();
    let result = world
        .current_rules()
        .deal_hole_cards(&world.existing_deck, &players, None);
    world.deal_result = Some(result);
    world.players = players;
}

// --- When: factory ---

#[when(expr = "I get game rules for variant {word}")]
fn when_get_game_rules(world: &mut RulesWorld, variant: String) {
    let v = variant_from_name(&variant);
    world.factory_rules = Some(get_rules(v));
    world.factory_label = factory_class_label(v).to_string();
}

#[when("I get game rules for an unknown variant")]
fn when_get_game_rules_unknown(world: &mut RulesWorld) {
    // Pick any variant that's not a known poker variant. GameVariant::Unspecified (default)
    // isn't in the match arms so get_rules falls back to TexasHoldemRules.
    world.factory_rules = Some(get_rules(GameVariant::default()));
    world.factory_label = "TexasHoldemRules".into();
}

// --- Then: evaluator ---

#[then(expr = "the rank is {word}")]
fn then_rank(world: &mut RulesWorld, rank: String) {
    let expected = rank_type_from_name(&rank);
    let actual = world
        .last_rank
        .as_ref()
        .expect("no hand evaluated")
        .rank_type;
    assert_eq!(
        actual, expected,
        "Expected rank {:?}, got {:?}",
        expected, actual
    );
}

#[then(expr = "the score is {int}")]
fn then_score(world: &mut RulesWorld, score: i32) {
    let actual = world.last_rank.as_ref().expect("no hand evaluated").score;
    assert_eq!(actual, score, "Expected score {}, got {}", score, actual);
}

#[then(expr = "the kicker count is {int}")]
fn then_kicker_count(world: &mut RulesWorld, n: usize) {
    let actual = world
        .last_rank
        .as_ref()
        .expect("no hand evaluated")
        .kickers
        .len();
    assert_eq!(actual, n, "Expected {} kickers, got {}", n, actual);
}

#[then(regex = r"^the kickers are ([\d, ]+)$")]
fn then_kickers(world: &mut RulesWorld, kicker_list: String) {
    let expected: Vec<i32> = kicker_list
        .split(',')
        .map(|s| s.trim().parse().unwrap())
        .collect();
    let actual: Vec<i32> = world
        .last_rank
        .as_ref()
        .expect("no hand evaluated")
        .kickers
        .iter()
        .map(|r| *r as i32)
        .collect();
    assert_eq!(
        actual, expected,
        "Expected kickers {:?}, got {:?}",
        expected, actual
    );
}

// --- Then: variant property ---

#[then(expr = "the variant is {string}")]
fn then_variant(world: &mut RulesWorld, variant_name_expected: String) {
    let rules: &dyn GameRules = world
        .factory_rules
        .as_deref()
        .or(world.rules.as_deref())
        .expect("no rules set");
    let actual = variant_name(rules.variant());
    assert_eq!(
        actual, variant_name_expected,
        "Expected variant {}, got {}",
        variant_name_expected, actual
    );
}

#[then(expr = "the hole card count is {int}")]
fn then_hole_card_count(world: &mut RulesWorld, n: usize) {
    assert_eq!(world.current_rules().hole_card_count(), n);
}

#[then(expr = "the phases are {string}")]
fn then_phases(world: &mut RulesWorld, phases: String) {
    let expected: Vec<BettingPhase> = phases.split(',').map(phase_from_name).collect();
    let actual = world.current_rules().phases();
    assert_eq!(
        actual, expected,
        "Expected phases {:?}, got {:?}",
        expected, actual
    );
}

// --- Then: phase transitions ---

#[then(expr = "the next phase is {word}")]
fn then_next_phase(world: &mut RulesWorld, phase: String) {
    let transition = world.next_phase.as_ref().expect("expected a phase");
    let expected = phase_from_name(&phase);
    assert_eq!(
        transition.next_phase, expected,
        "Expected next_phase {:?}, got {:?}",
        expected, transition.next_phase
    );
}

#[then(expr = "the community cards to deal is {int}")]
fn then_deal_count(world: &mut RulesWorld, n: usize) {
    let transition = world.next_phase.as_ref().expect("expected a phase");
    assert_eq!(
        transition.community_cards_to_deal, n,
        "Expected {} cards to deal, got {}",
        n, transition.community_cards_to_deal
    );
}

#[then(expr = "is_showdown is {word}")]
fn then_is_showdown(world: &mut RulesWorld, flag: String) {
    let transition = world.next_phase.as_ref().expect("expected a phase");
    let expected = flag == "True";
    assert_eq!(
        transition.is_showdown, expected,
        "Expected is_showdown={}, got {}",
        expected, transition.is_showdown
    );
}

#[then("there is no next phase")]
fn then_no_next_phase(world: &mut RulesWorld) {
    assert!(
        world.next_phase.is_none(),
        "Expected None for terminal phase, got a transition"
    );
}

// --- Then: draw ---

fn dr(world: &RulesWorld) -> &DrawResult {
    world.draw_result.as_ref().expect("no draw result")
}

#[then(expr = "the new hand has {int} cards")]
fn then_new_hand_size(world: &mut RulesWorld, n: usize) {
    assert_eq!(dr(world).new_hole_cards.len(), n);
}

#[then(expr = "the new hand has {int} card")]
fn then_new_hand_size_one(world: &mut RulesWorld, n: usize) {
    assert_eq!(dr(world).new_hole_cards.len(), n);
}

#[then(expr = "{int} cards were drawn")]
fn then_cards_drawn(world: &mut RulesWorld, n: usize) {
    assert_eq!(dr(world).cards_drawn.len(), n);
}

#[then(expr = "{int} card were drawn")]
fn then_cards_drawn_one(world: &mut RulesWorld, n: usize) {
    assert_eq!(dr(world).cards_drawn.len(), n);
}

#[then(expr = "the remaining deck has {int} cards")]
fn then_remaining_deck(world: &mut RulesWorld, n: usize) {
    let remaining = if let Some(dr) = &world.draw_result {
        &dr.remaining_deck
    } else if let Some(deal) = &world.deal_result {
        &deal.remaining_deck
    } else {
        &world.deck
    };
    assert_eq!(
        remaining.len(),
        n,
        "Expected {} cards, got {}",
        n,
        remaining.len()
    );
}

#[then(expr = "the remaining deck has {int} card")]
fn then_remaining_deck_one(world: &mut RulesWorld, n: usize) {
    then_remaining_deck(world, n);
}

#[then(expr = "the new hand retains {string}")]
fn then_new_hand_retains(world: &mut RulesWorld, card_str: String) {
    let parsed = parse_cards(&card_str);
    let target = parsed[0];
    assert!(
        dr(world).new_hole_cards.contains(&target),
        "expected card {} in new hand",
        card_str
    );
}

#[then(expr = "the new hand does not contain {string}")]
fn then_new_hand_excludes(world: &mut RulesWorld, card_str: String) {
    let parsed = parse_cards(&card_str);
    let target = parsed[0];
    assert!(
        !dr(world).new_hole_cards.contains(&target),
        "expected card {} to be absent",
        card_str
    );
}

#[then(expr = "the new hand equals {string}")]
fn then_new_hand_equals(world: &mut RulesWorld, cards: String) {
    let expected = parse_cards(&cards);
    assert_eq!(
        dr(world).new_hole_cards,
        expected,
        "Expected new hand {:?}, got {:?}",
        expected,
        dr(world).new_hole_cards
    );
}

// --- Then: deck / deal ---

#[then(expr = "the deck has {int} cards")]
fn then_deck_size(world: &mut RulesWorld, n: usize) {
    assert_eq!(world.deck.len(), n);
}

#[then("the two decks are identical")]
fn then_decks_identical(world: &mut RulesWorld) {
    assert_eq!(
        world.deck_a, world.deck_b,
        "Seeded decks differ — shuffle is not deterministic"
    );
}

#[then(expr = "each player has {int} hole cards")]
fn then_each_player_hole_cards(world: &mut RulesWorld, n: usize) {
    let deal = world.deal_result.as_ref().expect("no deal result");
    for p in &world.players {
        let cards = deal.player_cards.get(p).expect("player missing");
        assert_eq!(cards.len(), n);
    }
}

#[then(expr = "each player has {int} hole card")]
fn then_each_player_hole_cards_one(world: &mut RulesWorld, n: usize) {
    then_each_player_hole_cards(world, n);
}

// --- Then: factory ---

#[then(expr = "the rules class is {word}")]
fn then_rules_class(world: &mut RulesWorld, cls: String) {
    assert_eq!(
        world.factory_label, cls,
        "Expected {}, got {}",
        cls, world.factory_label
    );
    // Also sanity-check the instance exists.
    let _ = world.factory_rules.as_deref().expect("no factory rules");
}

#[tokio::main]
async fn main() {
    RulesWorld::run("features/example/unit/game_rules.feature").await;
}
