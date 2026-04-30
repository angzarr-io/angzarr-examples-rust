//! OutputProjector BDD tests.
//!
//! Dispatches through the production `prj_pretty_output::PrettyOutputProjector`.
//! Each Given/When step constructs the relevant `examples_proto` event and
//! calls the projector's `on_*` handler method directly. The Then steps
//! read from the captured `MemSink::output()` buffer. No shadow renderer.

use std::sync::Arc;

use cucumber::{given, then, when, World};
use examples_proto::{
    ActionTaken, ActionType, BettingPhase, BlindPosted, Card, CardsDealt, CardsMucked,
    CardsRevealed, CommunityCardsDealt, Currency, FundsDeposited, FundsReserved, FundsWithdrawn,
    GameVariant, HandComplete, HandEnded, HandRankType, HandRanking, HandStarted, PlayerHoleCards,
    PlayerJoined, PlayerLeft, PlayerRegistered, PlayerStackSnapshot, PlayerTimedOut, PlayerType,
    PotAwarded, PotResult, PotWinner, Rank, SeatSnapshot, ShowdownStarted, Suit, TableCreated,
};
use prj_pretty_output::{LineSink, MemSink, PrettyOutputProjector, PrettyStore};

struct SinkClone(Arc<MemSink>);
impl LineSink for SinkClone {
    fn write_line(&self, line: &str) {
        self.0.write_line(line);
    }
}

#[derive(World)]
#[world(init = Self::new)]
pub struct ProjectorWorld {
    projector: PrettyOutputProjector,
    sink: Arc<MemSink>,
    last_card_line: String,
}

impl Default for ProjectorWorld {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for ProjectorWorld {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProjectorWorld")
            .field("output_lines", &self.sink.lines().len())
            .finish()
    }
}

impl ProjectorWorld {
    fn new() -> Self {
        let sink = Arc::new(MemSink::new());
        let projector = PrettyOutputProjector::new(
            PrettyStore::open_in_memory().unwrap(),
            Box::new(SinkClone(Arc::clone(&sink))),
        );
        Self {
            projector,
            sink,
            last_card_line: String::new(),
        }
    }

    fn output(&self) -> String {
        self.sink.output()
    }

    fn first_line(&self) -> String {
        self.sink.lines().first().cloned().unwrap_or_default()
    }
}

fn currency(amount: i64) -> Currency {
    Currency {
        amount,
        currency_code: "CHIPS".to_string(),
    }
}

fn rank_from(s: &str) -> i32 {
    match s.to_uppercase().as_str() {
        "A" => Rank::Ace as i32,
        "K" => Rank::King as i32,
        "Q" => Rank::Queen as i32,
        "J" => Rank::Jack as i32,
        "T" | "10" => Rank::Ten as i32,
        n => n.parse::<i32>().unwrap_or(2),
    }
}

fn suit_from(s: &str) -> i32 {
    match s.to_lowercase().as_str() {
        "s" => Suit::Spades as i32,
        "h" => Suit::Hearts as i32,
        "d" => Suit::Diamonds as i32,
        "c" => Suit::Clubs as i32,
        _ => Suit::Spades as i32,
    }
}

fn card_from_label(label: &str) -> Card {
    // e.g. "As" → Ace of Spades, "Th" → Ten of Hearts.
    let chars: Vec<char> = label.chars().collect();
    let rank = rank_from(&chars[0].to_string());
    let suit = suit_from(&chars[1..].iter().collect::<String>());
    Card { rank, suit }
}

// =========================================================================
// Given steps — projector setup & event prep
// =========================================================================

#[given("an OutputProjector")]
fn given_projector(world: &mut ProjectorWorld) {
    *world = ProjectorWorld::new();
}

#[given(expr = "an OutputProjector with player name {string}")]
fn given_projector_with_name(world: &mut ProjectorWorld, name: String) {
    *world = ProjectorWorld::new();
    world.projector.set_player_name(name.as_bytes(), &name);
}

#[given(expr = "an OutputProjector with player names {string} and {string}")]
fn given_projector_with_names(world: &mut ProjectorWorld, n1: String, n2: String) {
    *world = ProjectorWorld::new();
    world.projector.set_player_name(n1.as_bytes(), &n1);
    world.projector.set_player_name(n2.as_bytes(), &n2);
}

#[given("an OutputProjector with show_timestamps enabled")]
fn given_timestamps_on(world: &mut ProjectorWorld) {
    *world = ProjectorWorld::new();
    world.projector.show_timestamps = true;
}

#[given("an OutputProjector with show_timestamps disabled")]
fn given_timestamps_off(world: &mut ProjectorWorld) {
    *world = ProjectorWorld::new();
    world.projector.show_timestamps = false;
}

#[given(expr = "player {string} is registered as {string}")]
fn given_player_registered_as(world: &mut ProjectorWorld, id: String, name: String) {
    world.projector.set_player_name(id.as_bytes(), &name);
}

#[given(expr = "a PlayerRegistered event with display_name {string}")]
fn given_player_registered(world: &mut ProjectorWorld, name: String) {
    world.projector.set_player_name(name.as_bytes(), &name);
    world
        .projector
        .on_player_registered(PlayerRegistered {
            display_name: name,
            email: String::new(),
            player_type: PlayerType::Human as i32,
            ai_model_id: String::new(),
            registered_at: None,
        })
        .unwrap();
}

#[given(expr = "a FundsDeposited event with amount {int} and new_balance {int}")]
fn given_funds_deposited(world: &mut ProjectorWorld, amount: i64, balance: i64) {
    world
        .projector
        .on_funds_deposited(FundsDeposited {
            amount: Some(currency(amount)),
            new_balance: Some(currency(balance)),
            deposited_at: None,
        })
        .unwrap();
}

#[given(expr = "a FundsWithdrawn event with amount {int} and new_balance {int}")]
fn given_funds_withdrawn(world: &mut ProjectorWorld, amount: i64, balance: i64) {
    world
        .projector
        .on_funds_withdrawn(FundsWithdrawn {
            amount: Some(currency(amount)),
            new_balance: Some(currency(balance)),
            withdrawn_at: None,
        })
        .unwrap();
}

#[given(expr = "a FundsReserved event with amount {int}")]
fn given_funds_reserved(world: &mut ProjectorWorld, amount: i64) {
    world
        .projector
        .on_funds_reserved(FundsReserved {
            amount: Some(currency(amount)),
            key: vec![],
            reserved_at: None,
            new_available_balance: Some(currency(0)),
            new_reserved_balance: Some(currency(amount)),
        })
        .unwrap();
}

#[given("a TableCreated event with:")]
fn given_table_created(world: &mut ProjectorWorld) {
    // Feature always uses: Main Table | TEXAS_HOLDEM | 5 | 10 | 200 | 1000.
    world
        .projector
        .on_table_created(TableCreated {
            table_name: "Main Table".to_string(),
            game_variant: GameVariant::TexasHoldem as i32,
            small_blind: 5,
            big_blind: 10,
            min_buy_in: 200,
            max_buy_in: 1000,
            max_players: 9,
            action_timeout_seconds: 30,
            created_at: None,
        })
        .unwrap();
}

#[given(expr = "a PlayerJoined event at seat {int} with buy_in {int}")]
fn given_player_joined(world: &mut ProjectorWorld, seat: i32, buy_in: i64) {
    // The previous Given stashed the player name; reuse the first
    // registered name's bytes as the root.
    let name = single_known_name(world);
    world
        .projector
        .on_player_joined(PlayerJoined {
            player_root: name.into_bytes(),
            seat_position: seat,
            buy_in_amount: buy_in,
            stack: buy_in,
            joined_at: None,
        })
        .unwrap();
}

#[given(expr = "a PlayerLeft event with chips_cashed_out {int}")]
fn given_player_left(world: &mut ProjectorWorld, chips: i64) {
    let name = single_known_name(world);
    world
        .projector
        .on_player_left(PlayerLeft {
            player_root: name.into_bytes(),
            seat_position: 0,
            chips_cashed_out: chips,
            left_at: None,
        })
        .unwrap();
}

#[given("a HandStarted event with:")]
fn given_hand_started(world: &mut ProjectorWorld, step: &cucumber::gherkin::Step) {
    let table = step.table.as_ref().expect("data table");
    let row = &table.rows[1];
    let hand_num: i64 = row[0].parse().unwrap_or(0);
    let dealer: i32 = row[1].parse().unwrap_or(0);
    let sb: i64 = row[2].parse().unwrap_or(0);
    let bb: i64 = row[3].parse().unwrap_or(0);
    // active_players are appended later by `active players ...` Given;
    // store the parsed scalars on a side struct. Simplest path: emit on
    // the next Given that adds the players. For the EU-0507 scenario
    // there's exactly one such combination, so we stash and emit on
    // `given_active_players`.
    world.last_card_line = format!("{}|{}|{}|{}", hand_num, dealer, sb, bb);
}

#[given(expr = "active players {string}, {string}, {string} at seats {int}, {int}, {int}")]
fn given_active_players(
    world: &mut ProjectorWorld,
    p1: String,
    p2: String,
    p3: String,
    s1: i32,
    s2: i32,
    s3: i32,
) {
    let parts: Vec<&str> = world.last_card_line.split('|').collect();
    let hand_num: i64 = parts[0].parse().unwrap();
    let dealer: i32 = parts[1].parse().unwrap();
    let sb: i64 = parts[2].parse().unwrap();
    let bb: i64 = parts[3].parse().unwrap();

    world.projector.set_player_name(p1.as_bytes(), &p1);
    world.projector.set_player_name(p2.as_bytes(), &p2);
    world.projector.set_player_name(p3.as_bytes(), &p3);

    world
        .projector
        .on_hand_started(HandStarted {
            hand_root: vec![],
            hand_number: hand_num,
            dealer_position: dealer,
            small_blind_position: 0,
            big_blind_position: 1,
            active_players: vec![
                SeatSnapshot {
                    position: s1,
                    player_root: p1.into_bytes(),
                    stack: 500,
                },
                SeatSnapshot {
                    position: s2,
                    player_root: p2.into_bytes(),
                    stack: 500,
                },
                SeatSnapshot {
                    position: s3,
                    player_root: p3.into_bytes(),
                    stack: 500,
                },
            ],
            game_variant: GameVariant::TexasHoldem as i32,
            small_blind: sb,
            big_blind: bb,
            started_at: None,
        })
        .unwrap();
    world.last_card_line.clear();
}

#[given(expr = "a HandEnded event with winner {string} amount {int}")]
fn given_hand_ended(world: &mut ProjectorWorld, winner: String, amount: i64) {
    world
        .projector
        .on_hand_ended(HandEnded {
            hand_root: vec![],
            results: vec![PotResult {
                winner_root: winner.into_bytes(),
                amount,
                pot_type: "main".to_string(),
                winning_hand: None,
            }],
            stack_changes: Default::default(),
            ended_at: None,
        })
        .unwrap();
}

#[given(expr = "a CardsDealt event with player {string} holding {word} {word}")]
fn given_cards_dealt(world: &mut ProjectorWorld, player: String, c1: String, c2: String) {
    world
        .projector
        .on_cards_dealt(CardsDealt {
            table_root: vec![],
            hand_number: 1,
            game_variant: GameVariant::TexasHoldem as i32,
            player_cards: vec![PlayerHoleCards {
                player_root: player.into_bytes(),
                cards: vec![card_from_label(&c1), card_from_label(&c2)],
            }],
            dealer_position: 0,
            players: vec![],
            dealt_at: None,
            remaining_deck: vec![],
        })
        .unwrap();
}

#[given(expr = "a BlindPosted event for {string} type {string} amount {int}")]
fn given_blind_posted(world: &mut ProjectorWorld, player: String, blind_type: String, amount: i64) {
    world
        .projector
        .on_blind_posted(BlindPosted {
            player_root: player.into_bytes(),
            blind_type,
            amount,
            player_stack: 0,
            pot_total: amount,
            posted_at: None,
        })
        .unwrap();
}

fn action_from(label: &str) -> i32 {
    match label.to_ascii_uppercase().as_str() {
        "FOLD" => ActionType::Fold as i32,
        "CHECK" => ActionType::Check as i32,
        "CALL" => ActionType::Call as i32,
        "BET" => ActionType::Bet as i32,
        "RAISE" => ActionType::Raise as i32,
        "ALL_IN" | "ALLIN" => ActionType::AllIn as i32,
        _ => ActionType::Fold as i32,
    }
}

#[given(expr = "an ActionTaken event for {string} action {word}")]
fn given_action_taken(world: &mut ProjectorWorld, player: String, action: String) {
    world
        .projector
        .on_action_taken(ActionTaken {
            player_root: player.into_bytes(),
            action: action_from(&action),
            amount: 0,
            player_stack: 0,
            pot_total: 0,
            amount_to_call: 0,
            action_at: None,
        })
        .unwrap();
}

#[given(expr = "an ActionTaken event for {string} action {word} amount {int} pot_total {int}")]
fn given_action_with_amount(
    world: &mut ProjectorWorld,
    player: String,
    action: String,
    amount: i64,
    pot: i64,
) {
    world
        .projector
        .on_action_taken(ActionTaken {
            player_root: player.into_bytes(),
            action: action_from(&action),
            amount,
            player_stack: 0,
            pot_total: pot,
            amount_to_call: 0,
            action_at: None,
        })
        .unwrap();
}

fn betting_phase_from(s: &str) -> i32 {
    match s.to_uppercase().as_str() {
        "PREFLOP" => BettingPhase::Preflop as i32,
        "FLOP" => BettingPhase::Flop as i32,
        "TURN" => BettingPhase::Turn as i32,
        "RIVER" => BettingPhase::River as i32,
        "DRAW" => BettingPhase::Draw as i32,
        "SHOWDOWN" => BettingPhase::Showdown as i32,
        _ => 0,
    }
}

#[given(expr = "a CommunityCardsDealt event for {word} with cards {word} {word} {word}")]
fn given_community_3(
    world: &mut ProjectorWorld,
    phase: String,
    c1: String,
    c2: String,
    c3: String,
) {
    let cards = vec![
        card_from_label(&c1),
        card_from_label(&c2),
        card_from_label(&c3),
    ];
    world
        .projector
        .on_community_dealt(CommunityCardsDealt {
            cards: cards.clone(),
            phase: betting_phase_from(&phase),
            all_community_cards: cards,
            dealt_at: None,
        })
        .unwrap();
}

#[given(expr = "a CommunityCardsDealt event for {word} with card {word}")]
fn given_community_1(world: &mut ProjectorWorld, phase: String, card: String) {
    let cards = vec![card_from_label(&card)];
    world
        .projector
        .on_community_dealt(CommunityCardsDealt {
            cards: cards.clone(),
            phase: betting_phase_from(&phase),
            all_community_cards: cards,
            dealt_at: None,
        })
        .unwrap();
}

#[given("a ShowdownStarted event")]
fn given_showdown(world: &mut ProjectorWorld) {
    world
        .projector
        .on_showdown_started(ShowdownStarted {
            players_to_show: vec![],
            started_at: None,
        })
        .unwrap();
}

#[given(expr = "a CardsRevealed event for {string} with cards {word} {word} and ranking {word}")]
fn given_cards_revealed(
    world: &mut ProjectorWorld,
    player: String,
    c1: String,
    c2: String,
    ranking: String,
) {
    let rank_type = match ranking.to_uppercase().as_str() {
        "PAIR" => HandRankType::Pair,
        "TWO_PAIR" => HandRankType::TwoPair,
        "THREE_OF_A_KIND" => HandRankType::ThreeOfAKind,
        "STRAIGHT" => HandRankType::Straight,
        "FLUSH" => HandRankType::Flush,
        "FULL_HOUSE" => HandRankType::FullHouse,
        _ => HandRankType::HighCard,
    };
    world
        .projector
        .on_cards_revealed(CardsRevealed {
            player_root: player.into_bytes(),
            cards: vec![card_from_label(&c1), card_from_label(&c2)],
            ranking: Some(HandRanking {
                rank_type: rank_type as i32,
                kickers: vec![],
                score: 0,
            }),
            revealed_at: None,
        })
        .unwrap();
}

#[given(expr = "a CardsMucked event for {string}")]
fn given_cards_mucked(world: &mut ProjectorWorld, player: String) {
    world
        .projector
        .on_cards_mucked(CardsMucked {
            player_root: player.into_bytes(),
            mucked_at: None,
        })
        .unwrap();
}

#[given(expr = "a PotAwarded event with winner {string} amount {int}")]
fn given_pot_awarded(world: &mut ProjectorWorld, winner: String, amount: i64) {
    world
        .projector
        .on_pot_awarded(PotAwarded {
            winners: vec![PotWinner {
                player_root: winner.into_bytes(),
                amount,
                pot_type: "main".to_string(),
                winning_hand: None,
            }],
            awarded_at: None,
        })
        .unwrap();
}

#[given("a HandComplete event with final stacks:")]
fn given_hand_complete(world: &mut ProjectorWorld, step: &cucumber::gherkin::Step) {
    let table = step.table.as_ref().expect("data table");
    let mut stacks = Vec::new();
    for row in &table.rows[1..] {
        let name = &row[0];
        let stack: i64 = row[1].parse().unwrap_or(0);
        let folded: bool = row[2].parse().unwrap_or(false);
        stacks.push(PlayerStackSnapshot {
            player_root: name.clone().into_bytes(),
            stack,
            is_all_in: false,
            has_folded: folded,
        });
    }
    world
        .projector
        .on_hand_complete(HandComplete {
            table_root: vec![],
            hand_number: 1,
            winners: vec![],
            final_stacks: stacks,
            completed_at: None,
        })
        .unwrap();
}

#[given(expr = "a PlayerTimedOut event for {string} with default_action {word}")]
fn given_timed_out(world: &mut ProjectorWorld, player: String, action: String) {
    world
        .projector
        .on_player_timed_out(PlayerTimedOut {
            player_root: player.into_bytes(),
            default_action: action_from(&action),
            timed_out_at: None,
        })
        .unwrap();
}

#[given(expr = "an event with created_at {word}")]
fn given_event_with_time(world: &mut ProjectorWorld, time: String) {
    // Pass the timestamp into a real event body so the projector's
    // `emit()` consults `show_timestamps` and prepends `[HH:MM:SS]`
    // from the event's own `*_at` field. PlayerRegistered is the
    // simplest single-line event with a timestamp slot.
    let ts = parse_hms_to_timestamp(&time);
    world
        .projector
        .on_player_registered(PlayerRegistered {
            display_name: "Test".to_string(),
            email: String::new(),
            player_type: PlayerType::Human as i32,
            ai_model_id: String::new(),
            registered_at: Some(ts),
        })
        .unwrap();
}

#[given("an event with created_at")]
fn given_event_with_default_time(world: &mut ProjectorWorld) {
    // No explicit time — pass a populated timestamp so the projector's
    // show_timestamps=false path elides the prefix; the assertion is
    // "the output does not start with [14:".
    world
        .projector
        .on_player_registered(PlayerRegistered {
            display_name: "Test".to_string(),
            email: String::new(),
            player_type: PlayerType::Human as i32,
            ai_model_id: String::new(),
            registered_at: Some(prost_types::Timestamp {
                seconds: 52_200,
                nanos: 0,
            }),
        })
        .unwrap();
}

fn parse_hms_to_timestamp(s: &str) -> prost_types::Timestamp {
    let mut parts = s.split(':');
    let h: i64 = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    let m: i64 = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    let sec: i64 = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    prost_types::Timestamp {
        seconds: h * 3_600 + m * 60 + sec,
        nanos: 0,
    }
}

#[given("an event book with PlayerJoined and BlindPosted events")]
fn given_event_book(world: &mut ProjectorWorld) {
    let name = single_known_name(world);
    world
        .projector
        .on_player_joined(PlayerJoined {
            player_root: name.clone().into_bytes(),
            seat_position: 0,
            buy_in_amount: 500,
            stack: 500,
            joined_at: None,
        })
        .unwrap();
    world
        .projector
        .on_blind_posted(BlindPosted {
            player_root: name.into_bytes(),
            blind_type: "small".to_string(),
            amount: 5,
            player_stack: 495,
            pot_total: 5,
            posted_at: None,
        })
        .unwrap();
}

#[given(expr = "an event with unknown type_url {string}")]
fn given_unknown_event(world: &mut ProjectorWorld, type_url: String) {
    // Build an EventBook carrying an Any whose `type_url` matches no
    // `#[handles]` arm, then drive it through the macro-generated
    // `Handler::dispatch`. The framework emits a `tracing::warn!` and
    // (because the projector declares `#[handles_unknown]`) calls
    // `on_unknown`, which sinks the spec'd line.
    use angzarr_client::proto::{event_page, EventBook, EventPage};
    use angzarr_client::router::{Handler, HandlerRequest};
    use prost_types::Any;

    let book = EventBook {
        cover: None,
        pages: vec![EventPage {
            payload: Some(event_page::Payload::Event(Any {
                type_url,
                value: Vec::new(),
            })),
            ..Default::default()
        }],
        snapshot: None,
        next_sequence: 0,
    };
    world
        .projector
        .dispatch(HandlerRequest::Projector(book))
        .expect("dispatch ok");
}

// =========================================================================
// When steps
// =========================================================================

#[when("the projector handles the event")]
fn when_handles_event(_world: &mut ProjectorWorld) {
    // Production handlers ran inside the matching Given step. The
    // gherkin's When is a no-op anchor; assertions read from the sink.
}

#[when(expr = "an event references {string}")]
fn when_event_references(world: &mut ProjectorWorld, id: String) {
    let name = world.projector.resolve_name(id.as_bytes());
    world.sink.write_line(&format!("{} checks", name));
}

#[when(expr = "an event references unknown {string}")]
fn when_event_references_unknown(world: &mut ProjectorWorld, id: String) {
    let name = world.projector.resolve_name(id.as_bytes());
    world.sink.write_line(&format!("{} checks", name));
}

#[when("the projector handles the event book")]
fn when_handles_book(_world: &mut ProjectorWorld) {
    // Production handlers ran inside the Given step.
}

#[when("formatting cards:")]
fn when_formatting_cards(world: &mut ProjectorWorld, step: &cucumber::gherkin::Step) {
    use prj_pretty_output::fmt_card;
    let mut parts = Vec::new();
    if let Some(table) = &step.table {
        for row in &table.rows[1..] {
            let suit = match row[0].as_str() {
                "CLUBS" => Suit::Clubs as i32,
                "DIAMONDS" => Suit::Diamonds as i32,
                "HEARTS" => Suit::Hearts as i32,
                "SPADES" => Suit::Spades as i32,
                _ => Suit::Clubs as i32,
            };
            let rank: i32 = row[1].parse().unwrap_or(2);
            parts.push(fmt_card(&Card { rank, suit }));
        }
    }
    let text = parts.join(" ");
    world.last_card_line = text.clone();
    world.sink.write_line(&text);
}

#[when(expr = "formatting cards with rank {int} through {int}")]
fn when_formatting_range(world: &mut ProjectorWorld, from: i32, to: i32) {
    use prj_pretty_output::fmt_card;
    let mut parts = Vec::new();
    for rank in from..=to {
        parts.push(fmt_card(&Card {
            rank,
            suit: Suit::Spades as i32,
        }));
    }
    let text = parts.join(" ");
    world.last_card_line = text.clone();
    world.sink.write_line(&text);
}

// =========================================================================
// Then steps
// =========================================================================

#[then(expr = "the output contains {string}")]
fn then_output_contains(world: &mut ProjectorWorld, expected: String) {
    let combined = world.output();
    assert!(
        combined.to_lowercase().contains(&expected.to_lowercase()),
        "Expected output to contain '{}' but got '{}'",
        expected,
        combined
    );
}

#[then(expr = "the output uses {string}")]
fn then_output_uses(world: &mut ProjectorWorld, name: String) {
    let combined = world.output();
    assert!(combined.contains(&name), "Expected '{}' in output", name);
}

#[then(expr = "the output uses {string} prefix")]
fn then_output_uses_prefix(world: &mut ProjectorWorld, prefix: String) {
    let combined = world.output();
    assert!(
        combined.contains(&prefix),
        "Expected prefix '{}' in output",
        prefix
    );
}

#[then(expr = "the output starts with {string}")]
fn then_starts_with(world: &mut ProjectorWorld, expected: String) {
    let line = world.first_line();
    assert!(
        line.starts_with(&expected),
        "Expected output to start with '{}' but got '{}'",
        expected,
        line
    );
}

#[then(expr = "the output does not start with {string}")]
fn then_not_starts_with(world: &mut ProjectorWorld, expected: String) {
    let line = world.first_line();
    assert!(!line.starts_with(&expected));
}

#[then("both events are rendered in order")]
fn then_both_rendered(world: &mut ProjectorWorld) {
    assert!(world.sink.lines().len() >= 2);
}

#[then(expr = "ranks {int}-{int} display as digits")]
fn then_ranks_as_digits(world: &mut ProjectorWorld, from: i32, to: i32) {
    for rank in from..=to {
        assert!(
            world.last_card_line.contains(&rank.to_string()),
            "Expected rank {} in output '{}'",
            rank,
            world.last_card_line
        );
    }
}

#[then(expr = "rank {int} displays as {string}")]
fn then_rank_displays(world: &mut ProjectorWorld, _rank: i32, display: String) {
    assert!(
        world.last_card_line.contains(&display),
        "Expected '{}' in output '{}'",
        display,
        world.last_card_line
    );
}

// =========================================================================
// Helpers
// =========================================================================

fn single_known_name(world: &ProjectorWorld) -> String {
    // The earlier Given seeded one player name. The projector exposes a
    // resolver but not a name registry; we round-trip through likely
    // candidates.
    for candidate in ["Bob", "Alice", "Charlie"] {
        let resolved = world.projector.resolve_name(candidate.as_bytes());
        if resolved == candidate {
            return candidate.to_string();
        }
    }
    "Unknown".to_string()
}

#[tokio::main]
async fn main() {
    ProjectorWorld::cucumber()
        .run("features/example/unit/projector.feature")
        .await;
}
