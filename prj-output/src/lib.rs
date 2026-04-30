//! Pretty Output Projector — enriches events for human-readable observability.
//!
//! Extends the original `OutputProjector` with:
//! - SQLite-backed state (hands, seats, table metadata, community board)
//!   accumulated from the events the projector observes
//! - Money formatting (`$1,000`), card rendering (`As Kh`), action verbs
//!   (`folds`, `calls $10`, `raises to $30`, `all-in $500`), uppercase blind
//!   types, phase-specific community card labels.
//!
//! Note on identity resolution. The projector dispatch only sees the event
//! body, not its cover. Events like `PlayerRegistered` therefore cannot be
//! bound to a `player_root` (bytes) through the projector alone — the proto
//! body carries no root. The best identity we can surface in `ActionTaken` /
//! `BlindPosted` / `PotAwarded` is the 8-char hex prefix of `player_root`.
//! Where richer joinable state *is* available (hand roster, table stakes,
//! community cards), we persist it to SQLite and reference it from later
//! renderings.
//!
//! Upstream has a general log projector (`angzarr-prj-log`) that uses
//! `prost-reflect` for domain-agnostic pretty-printing. This example exists
//! to demonstrate the `#[projector]` macro and multi-domain subscription on
//! a fixed set of poker events.

use std::path::PathBuf;
use std::sync::Mutex;

use angzarr_client::{projector, CommandResult};
use examples_proto::{
    ActionTaken, ActionType, BettingPhase, BlindPosted, Card, CardsDealt, CommunityCardsDealt,
    FundsDeposited, GameVariant, HandComplete, HandStarted, PlayerJoined, PlayerRegistered,
    PlayerType, PotAwarded, Rank, Suit, TableCreated,
};
use rusqlite::{params, Connection};

// ---------------------------------------------------------------------------
// Formatting helpers — stateless, reused across event handlers.
// ---------------------------------------------------------------------------

pub fn fmt_money(amount: i64) -> String {
    let mut buf = String::new();
    let s = amount.abs().to_string();
    let bytes = s.as_bytes();
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 && (bytes.len() - i).is_multiple_of(3) {
            buf.push(',');
        }
        buf.push(*b as char);
    }
    if amount < 0 {
        format!("-${}", buf)
    } else {
        format!("${}", buf)
    }
}

pub fn fmt_card(card: &Card) -> String {
    let rank = match Rank::try_from(card.rank).unwrap_or(Rank::Two) {
        Rank::Ace => 'A',
        Rank::King => 'K',
        Rank::Queen => 'Q',
        Rank::Jack => 'J',
        Rank::Ten => 'T',
        Rank::Nine => '9',
        Rank::Eight => '8',
        Rank::Seven => '7',
        Rank::Six => '6',
        Rank::Five => '5',
        Rank::Four => '4',
        Rank::Three => '3',
        Rank::Two => '2',
        _ => '?',
    };
    let suit = match Suit::try_from(card.suit).unwrap_or(Suit::Spades) {
        Suit::Spades => 's',
        Suit::Hearts => 'h',
        Suit::Diamonds => 'd',
        Suit::Clubs => 'c',
        _ => '?',
    };
    format!("{}{}", rank, suit)
}

pub fn fmt_cards(cards: &[Card]) -> String {
    let parts: Vec<String> = cards.iter().map(fmt_card).collect();
    format!("[{}]", parts.join(" "))
}

pub fn fmt_player_short(player_root: &[u8]) -> String {
    if player_root.len() >= 4 {
        hex::encode(&player_root[..4])
    } else {
        hex::encode(player_root)
    }
}

pub fn fmt_variant(variant: i32) -> &'static str {
    match GameVariant::try_from(variant).unwrap_or_default() {
        GameVariant::TexasHoldem => "TEXAS_HOLDEM",
        GameVariant::Omaha => "OMAHA",
        GameVariant::FiveCardDraw => "FIVE_CARD_DRAW",
        _ => "UNKNOWN",
    }
}

pub fn fmt_phase_label(phase: i32) -> &'static str {
    match BettingPhase::try_from(phase).unwrap_or_default() {
        BettingPhase::Preflop => "Preflop",
        BettingPhase::Flop => "Flop",
        BettingPhase::Turn => "Turn",
        BettingPhase::River => "River",
        BettingPhase::Draw => "Draw",
        BettingPhase::Showdown => "Showdown",
        _ => "?",
    }
}

// ---------------------------------------------------------------------------
// Persistence — SQLite-backed accumulator for cross-event context.
// ---------------------------------------------------------------------------

/// Thin wrapper around `rusqlite::Connection` that runs the schema once.
pub struct PrettyStore {
    conn: Mutex<Connection>,
}

impl PrettyStore {
    pub fn open(path: &PathBuf) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        let store = Self {
            conn: Mutex::new(conn),
        };
        store.init_schema()?;
        Ok(store)
    }

    pub fn open_in_memory() -> rusqlite::Result<Self> {
        let conn = Connection::open_in_memory()?;
        let store = Self {
            conn: Mutex::new(conn),
        };
        store.init_schema()?;
        Ok(store)
    }

    fn init_schema(&self) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS tables (
                name           TEXT PRIMARY KEY,
                game_variant   TEXT NOT NULL,
                small_blind    INTEGER NOT NULL,
                big_blind      INTEGER NOT NULL,
                min_buy_in     INTEGER NOT NULL,
                max_buy_in     INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS hands (
                hand_root             BLOB PRIMARY KEY,
                hand_number           INTEGER NOT NULL,
                dealer_position       INTEGER NOT NULL,
                small_blind_position  INTEGER NOT NULL,
                big_blind_position    INTEGER NOT NULL,
                small_blind           INTEGER NOT NULL,
                big_blind             INTEGER NOT NULL,
                variant               TEXT NOT NULL,
                phase                 TEXT NOT NULL DEFAULT 'started'
            );

            CREATE TABLE IF NOT EXISTS hand_seats (
                hand_root     BLOB NOT NULL,
                position      INTEGER NOT NULL,
                player_root   BLOB NOT NULL,
                stack_start   INTEGER NOT NULL,
                PRIMARY KEY (hand_root, position)
            );

            CREATE TABLE IF NOT EXISTS board (
                hand_root     BLOB PRIMARY KEY,
                phase         TEXT NOT NULL,
                cards         TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS pot_totals (
                hand_root     BLOB PRIMARY KEY,
                pot           INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS players (
                player_root   BLOB PRIMARY KEY,
                last_stack    INTEGER NOT NULL DEFAULT 0
            );
            "#,
        )?;
        Ok(())
    }

    pub fn record_table(&self, ev: &TableCreated) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO tables(name, game_variant, small_blind, big_blind, min_buy_in, max_buy_in) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                ev.table_name,
                fmt_variant(ev.game_variant),
                ev.small_blind,
                ev.big_blind,
                ev.min_buy_in,
                ev.max_buy_in,
            ],
        )?;
        Ok(())
    }

    pub fn record_hand_started(&self, ev: &HandStarted) -> rusqlite::Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT OR REPLACE INTO hands(hand_root, hand_number, dealer_position, small_blind_position, big_blind_position, small_blind, big_blind, variant, phase) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'dealing')",
            params![
                ev.hand_root,
                ev.hand_number,
                ev.dealer_position,
                ev.small_blind_position,
                ev.big_blind_position,
                ev.small_blind,
                ev.big_blind,
                fmt_variant(ev.game_variant),
            ],
        )?;
        tx.execute(
            "DELETE FROM hand_seats WHERE hand_root = ?1",
            params![ev.hand_root],
        )?;
        for seat in &ev.active_players {
            tx.execute(
                "INSERT INTO hand_seats(hand_root, position, player_root, stack_start) VALUES (?1, ?2, ?3, ?4)",
                params![ev.hand_root, seat.position, seat.player_root, seat.stack],
            )?;
            tx.execute(
                "INSERT OR REPLACE INTO players(player_root, last_stack) VALUES (?1, ?2)",
                params![seat.player_root, seat.stack],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn update_hand_phase(&self, hand_root: &[u8], phase: &str) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE hands SET phase = ?1 WHERE hand_root = ?2",
            params![phase, hand_root],
        )?;
        Ok(())
    }

    pub fn record_board(
        &self,
        hand_root: &[u8],
        phase: &str,
        cards: &[Card],
    ) -> rusqlite::Result<()> {
        let cards_str = fmt_cards(cards);
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO board(hand_root, phase, cards) VALUES (?1, ?2, ?3)",
            params![hand_root, phase, cards_str],
        )?;
        Ok(())
    }

    pub fn record_pot_total(&self, hand_root: &[u8], pot: i64) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO pot_totals(hand_root, pot) VALUES (?1, ?2)",
            params![hand_root, pot],
        )?;
        Ok(())
    }

    pub fn update_player_stack(&self, player_root: &[u8], new_stack: i64) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO players(player_root, last_stack) VALUES (?1, ?2)",
            params![player_root, new_stack],
        )?;
        Ok(())
    }

    /// Lookup helper: hand_root present?
    pub fn known_hand(&self, hand_root: &[u8]) -> bool {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT 1 FROM hands WHERE hand_root = ?1",
            params![hand_root],
            |_| Ok(()),
        )
        .is_ok()
    }

    pub fn known_player(&self, player_root: &[u8]) -> bool {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT 1 FROM players WHERE player_root = ?1",
            params![player_root],
            |_| Ok(()),
        )
        .is_ok()
    }
}

// ---------------------------------------------------------------------------
// Output sink — stdout by default, swappable for tests.
// ---------------------------------------------------------------------------

pub trait LineSink: Send + Sync {
    fn write_line(&self, line: &str);
}

pub struct StdoutSink;
impl LineSink for StdoutSink {
    fn write_line(&self, line: &str) {
        println!("{}", line);
    }
}

pub struct MemSink {
    lines: Mutex<Vec<String>>,
}

impl MemSink {
    pub fn new() -> Self {
        Self {
            lines: Mutex::new(Vec::new()),
        }
    }

    pub fn output(&self) -> String {
        self.lines.lock().unwrap().join("\n")
    }

    pub fn lines(&self) -> Vec<String> {
        self.lines.lock().unwrap().clone()
    }
}

impl Default for MemSink {
    fn default() -> Self {
        Self::new()
    }
}

impl LineSink for MemSink {
    fn write_line(&self, line: &str) {
        self.lines.lock().unwrap().push(line.to_string());
    }
}

// ---------------------------------------------------------------------------
// Projector — the #[projector] impl.
// ---------------------------------------------------------------------------

pub struct PrettyOutputProjector {
    store: PrettyStore,
    sink: Box<dyn LineSink>,
}

impl PrettyOutputProjector {
    pub fn new(store: PrettyStore, sink: Box<dyn LineSink>) -> Self {
        Self { store, sink }
    }

    /// Construct a projector with output directed at stdout. Uses
    /// on-disk SQLite at `$PRJ_PRETTY_OUTPUT_DB` when the env var is set;
    /// falls back to an in-memory store when unset (the distroless
    /// runtime image has no writable filesystem path the `nonroot` user
    /// can rely on, and the projector is read-only at the boundary —
    /// scenarios that need persistence set the env var explicitly).
    pub fn from_env() -> Self {
        let store = match std::env::var("PRJ_PRETTY_OUTPUT_DB") {
            Ok(s) if !s.is_empty() => {
                PrettyStore::open(&PathBuf::from(s)).expect("open sqlite store")
            }
            _ => PrettyStore::open_in_memory().expect("open in-memory sqlite store"),
        };
        Self::new(store, Box::new(StdoutSink))
    }

    fn emit(&self, line: &str) {
        self.sink.write_line(line);
    }
}

#[projector(name = "pretty-output", domains = ["player", "table", "hand"])]
impl PrettyOutputProjector {
    // --- Player domain ---

    #[handles(PlayerRegistered)]
    fn on_player_registered(&self, event: PlayerRegistered) -> CommandResult<()> {
        let kind = match PlayerType::try_from(event.player_type).unwrap_or_default() {
            PlayerType::Human => "HUMAN",
            PlayerType::Ai => "AI",
            _ => "UNKNOWN",
        };
        self.emit(&format!(
            "{} registered ({}) as {}",
            event.display_name, event.email, kind
        ));
        Ok(())
    }

    #[handles(FundsDeposited)]
    fn on_funds_deposited(&self, event: FundsDeposited) -> CommandResult<()> {
        let amount = event.amount.as_ref().map(|c| c.amount).unwrap_or(0);
        let balance = event.new_balance.as_ref().map(|c| c.amount).unwrap_or(0);
        self.emit(&format!(
            "Deposited {}, balance: {}",
            fmt_money(amount),
            fmt_money(balance)
        ));
        Ok(())
    }

    // --- Table domain ---

    #[handles(TableCreated)]
    fn on_table_created(&self, event: TableCreated) -> CommandResult<()> {
        let _ = self.store.record_table(&event);
        self.emit(&format!(
            "Table created: {} — {} ({}/{}, buy-in {} - {})",
            event.table_name,
            fmt_variant(event.game_variant),
            fmt_money(event.small_blind),
            fmt_money(event.big_blind),
            fmt_money(event.min_buy_in),
            fmt_money(event.max_buy_in),
        ));
        Ok(())
    }

    #[handles(PlayerJoined)]
    fn on_player_joined(&self, event: PlayerJoined) -> CommandResult<()> {
        let _ = self
            .store
            .update_player_stack(&event.player_root, event.stack);
        self.emit(&format!(
            "Player {} joined at seat {} with {} buy-in (stack {})",
            fmt_player_short(&event.player_root),
            event.seat_position,
            fmt_money(event.buy_in_amount),
            fmt_money(event.stack),
        ));
        Ok(())
    }

    #[handles(HandStarted)]
    fn on_hand_started(&self, event: HandStarted) -> CommandResult<()> {
        let _ = self.store.record_hand_started(&event);
        let roster: Vec<String> = event
            .active_players
            .iter()
            .map(|s| {
                format!(
                    "seat {}: {} ({})",
                    s.position,
                    fmt_player_short(&s.player_root),
                    fmt_money(s.stack)
                )
            })
            .collect();
        self.emit(&format!(
            "HAND #{} — {} | Dealer: seat {} | SB seat {} {} / BB seat {} {} | {}",
            event.hand_number,
            fmt_variant(event.game_variant),
            event.dealer_position,
            event.small_blind_position,
            fmt_money(event.small_blind),
            event.big_blind_position,
            fmt_money(event.big_blind),
            roster.join(", ")
        ));
        Ok(())
    }

    // --- Hand domain ---

    #[handles(CardsDealt)]
    fn on_cards_dealt(&self, event: CardsDealt) -> CommandResult<()> {
        for pc in &event.player_cards {
            self.emit(&format!(
                "Hole cards dealt to {}: {}",
                fmt_player_short(&pc.player_root),
                fmt_cards(&pc.cards)
            ));
        }
        if event.player_cards.is_empty() {
            self.emit(&format!(
                "Cards dealt (hand #{}, {} players)",
                event.hand_number,
                event.players.len()
            ));
        }
        Ok(())
    }

    #[handles(BlindPosted)]
    fn on_blind_posted(&self, event: BlindPosted) -> CommandResult<()> {
        let kind = event.blind_type.to_uppercase();
        self.emit(&format!(
            "{} posts {} {} (pot {})",
            fmt_player_short(&event.player_root),
            kind,
            fmt_money(event.amount),
            fmt_money(event.pot_total)
        ));
        Ok(())
    }

    #[handles(ActionTaken)]
    fn on_action_taken(&self, event: ActionTaken) -> CommandResult<()> {
        let _ = self
            .store
            .update_player_stack(&event.player_root, event.player_stack);
        let player = fmt_player_short(&event.player_root);
        let verb = match ActionType::try_from(event.action).unwrap_or_default() {
            ActionType::Fold => format!("{} folds", player),
            ActionType::Check => format!("{} checks", player),
            ActionType::Call => format!("{} calls {}", player, fmt_money(event.amount)),
            ActionType::Bet => format!("{} bets {}", player, fmt_money(event.amount)),
            ActionType::Raise => {
                format!("{} raises to {}", player, fmt_money(event.amount))
            }
            ActionType::AllIn => format!("{} all-in {}", player, fmt_money(event.amount)),
            _ => format!("{} acts ({:?} {})", player, event.action, event.amount),
        };
        self.emit(&format!("{} (pot {})", verb, fmt_money(event.pot_total)));
        Ok(())
    }

    #[handles(CommunityCardsDealt)]
    fn on_community_dealt(&self, event: CommunityCardsDealt) -> CommandResult<()> {
        let label = fmt_phase_label(event.phase);
        let new_cards = fmt_cards(&event.cards);
        let board = fmt_cards(&event.all_community_cards);
        self.emit(&format!("{}: {}  (board: {})", label, new_cards, board));
        Ok(())
    }

    #[handles(PotAwarded)]
    fn on_pot_awarded(&self, event: PotAwarded) -> CommandResult<()> {
        for w in &event.winners {
            self.emit(&format!(
                "{} wins {} ({})",
                fmt_player_short(&w.player_root),
                fmt_money(w.amount),
                w.pot_type
            ));
        }
        Ok(())
    }

    #[handles(HandComplete)]
    fn on_hand_complete(&self, event: HandComplete) -> CommandResult<()> {
        self.emit(&format!("HAND #{} complete", event.hand_number));
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use examples_proto::{Currency, PotWinner, SeatSnapshot};
    use std::sync::Arc;

    struct SinkClone(Arc<MemSink>);
    impl LineSink for SinkClone {
        fn write_line(&self, line: &str) {
            self.0.write_line(line);
        }
    }

    fn make_projector() -> (PrettyOutputProjector, Arc<MemSink>) {
        let store = PrettyStore::open_in_memory().unwrap();
        let sink = Arc::new(MemSink::new());
        let proj = PrettyOutputProjector::new(store, Box::new(SinkClone(Arc::clone(&sink))));
        (proj, sink)
    }

    #[test]
    fn fmt_money_adds_dollar_and_thousands_separator() {
        assert_eq!(fmt_money(0), "$0");
        assert_eq!(fmt_money(5), "$5");
        assert_eq!(fmt_money(999), "$999");
        assert_eq!(fmt_money(1000), "$1,000");
        assert_eq!(fmt_money(12_345), "$12,345");
        assert_eq!(fmt_money(1_000_000), "$1,000,000");
        assert_eq!(fmt_money(-50), "-$50");
    }

    #[test]
    fn fmt_card_renders_rank_and_suit() {
        let ace_spades = Card {
            rank: Rank::Ace as i32,
            suit: Suit::Spades as i32,
        };
        assert_eq!(fmt_card(&ace_spades), "As");
        let ten_hearts = Card {
            rank: Rank::Ten as i32,
            suit: Suit::Hearts as i32,
        };
        assert_eq!(fmt_card(&ten_hearts), "Th");
    }

    #[test]
    fn fmt_cards_wraps_in_brackets() {
        let cards = vec![
            Card {
                rank: Rank::Ace as i32,
                suit: Suit::Spades as i32,
            },
            Card {
                rank: Rank::King as i32,
                suit: Suit::Hearts as i32,
            },
        ];
        assert_eq!(fmt_cards(&cards), "[As Kh]");
    }

    #[test]
    fn on_player_registered_emits_registration_line() {
        let (proj, sink) = make_projector();
        proj.on_player_registered(PlayerRegistered {
            display_name: "Alice".into(),
            email: "alice@example.com".into(),
            player_type: PlayerType::Human as i32,
            ai_model_id: String::new(),
            registered_at: None,
        })
        .unwrap();
        let out = sink.output();
        assert!(out.contains("Alice registered"));
        assert!(out.contains("HUMAN"));
    }

    #[test]
    fn on_player_registered_ai_labels_as_ai() {
        let (proj, sink) = make_projector();
        proj.on_player_registered(PlayerRegistered {
            display_name: "Bot".into(),
            email: "bot@x.io".into(),
            player_type: PlayerType::Ai as i32,
            ai_model_id: "v1".into(),
            registered_at: None,
        })
        .unwrap();
        assert!(sink.output().contains(" as AI"));
    }

    #[test]
    fn on_funds_deposited_formats_money_with_thousands() {
        let (proj, sink) = make_projector();
        proj.on_funds_deposited(FundsDeposited {
            amount: Some(Currency {
                amount: 1000,
                currency_code: "CHIPS".into(),
            }),
            new_balance: Some(Currency {
                amount: 1000,
                currency_code: "CHIPS".into(),
            }),
            deposited_at: None,
        })
        .unwrap();
        let out = sink.output();
        assert!(out.contains("$1,000"));
        assert!(out.contains("balance: $1,000"));
    }

    #[test]
    fn on_table_created_persists_and_renders_stakes() {
        let (proj, sink) = make_projector();
        let event = TableCreated {
            table_name: "Main".into(),
            game_variant: GameVariant::TexasHoldem as i32,
            small_blind: 5,
            big_blind: 10,
            min_buy_in: 200,
            max_buy_in: 1000,
            max_players: 6,
            action_timeout_seconds: 30,
            created_at: None,
        };
        proj.on_table_created(event).unwrap();
        let out = sink.output();
        assert!(out.contains("Main"));
        assert!(out.contains("TEXAS_HOLDEM"));
        assert!(out.contains("$5/$10"));
        assert!(out.contains("$200 - $1,000"));
        let conn = proj.store.conn.lock().unwrap();
        let variant: String = conn
            .query_row(
                "SELECT game_variant FROM tables WHERE name = 'Main'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(variant, "TEXAS_HOLDEM");
    }

    #[test]
    fn on_hand_started_persists_roster_and_renders_header() {
        let (proj, sink) = make_projector();
        let event = HandStarted {
            hand_root: vec![0xAB; 16],
            hand_number: 5,
            dealer_position: 2,
            small_blind_position: 0,
            big_blind_position: 1,
            active_players: vec![
                SeatSnapshot {
                    position: 0,
                    player_root: vec![0xAA; 16],
                    stack: 500,
                },
                SeatSnapshot {
                    position: 1,
                    player_root: vec![0xBB; 16],
                    stack: 500,
                },
                SeatSnapshot {
                    position: 2,
                    player_root: vec![0xCC; 16],
                    stack: 500,
                },
            ],
            game_variant: GameVariant::TexasHoldem as i32,
            small_blind: 5,
            big_blind: 10,
            started_at: None,
        };
        proj.on_hand_started(event).unwrap();
        let out = sink.output();
        assert!(out.contains("HAND #5"));
        assert!(out.contains("Dealer: seat 2"));
        assert!(out.contains("SB seat 0 $5"));
        assert!(out.contains("BB seat 1 $10"));
        assert!(out.contains("aaaaaaaa"));
        assert!(out.contains("bbbbbbbb"));
        assert!(out.contains("cccccccc"));

        assert!(proj.store.known_hand(&[0xAB; 16]));
        assert!(proj.store.known_player(&[0xAA; 16]));
    }

    #[test]
    fn on_blind_posted_uppercases_blind_type() {
        let (proj, sink) = make_projector();
        proj.on_blind_posted(BlindPosted {
            player_root: vec![0xDE, 0xAD, 0xBE, 0xEF],
            blind_type: "small".into(),
            amount: 5,
            player_stack: 495,
            pot_total: 5,
            posted_at: None,
        })
        .unwrap();
        let out = sink.output();
        assert!(out.contains("deadbeef"));
        assert!(out.contains("SMALL"));
        assert!(out.contains("$5"));
    }

    #[test]
    fn on_action_taken_uses_action_specific_verb() {
        let cases: Vec<(ActionType, &str, i64)> = vec![
            (ActionType::Fold, "folds", 0),
            (ActionType::Check, "checks", 0),
            (ActionType::Call, "calls $10", 10),
            (ActionType::Bet, "bets $20", 20),
            (ActionType::Raise, "raises to $30", 30),
            (ActionType::AllIn, "all-in $500", 500),
        ];
        for (action, expected_fragment, amount) in cases {
            let (proj, sink) = make_projector();
            proj.on_action_taken(ActionTaken {
                player_root: vec![0xAB; 4],
                action: action as i32,
                amount,
                player_stack: 0,
                pot_total: 100,
                amount_to_call: 0,
                action_at: None,
            })
            .unwrap();
            let out = sink.output();
            assert!(
                out.contains(expected_fragment),
                "output {:?} should contain {:?}",
                out,
                expected_fragment
            );
            assert!(out.contains("pot $100"));
        }
    }

    #[test]
    fn on_community_dealt_labels_phase() {
        let (proj, sink) = make_projector();
        let flop = vec![
            Card {
                rank: Rank::Ace as i32,
                suit: Suit::Hearts as i32,
            },
            Card {
                rank: Rank::King as i32,
                suit: Suit::Diamonds as i32,
            },
            Card {
                rank: Rank::Seven as i32,
                suit: Suit::Spades as i32,
            },
        ];
        proj.on_community_dealt(CommunityCardsDealt {
            cards: flop.clone(),
            phase: BettingPhase::Flop as i32,
            all_community_cards: flop,
            dealt_at: None,
        })
        .unwrap();
        let out = sink.output();
        assert!(out.contains("Flop: [Ah Kd 7s]"));
        assert!(out.contains("board:"));
    }

    #[test]
    fn on_pot_awarded_lines_per_winner() {
        let (proj, sink) = make_projector();
        proj.on_pot_awarded(PotAwarded {
            winners: vec![
                PotWinner {
                    player_root: vec![0x11, 0x22, 0x33, 0x44],
                    amount: 150,
                    pot_type: "main".into(),
                    winning_hand: None,
                },
                PotWinner {
                    player_root: vec![0x55, 0x66, 0x77, 0x88],
                    amount: 50,
                    pot_type: "side_1".into(),
                    winning_hand: None,
                },
            ],
            awarded_at: None,
        })
        .unwrap();
        let lines = sink.lines();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("11223344 wins $150 (main)"));
        assert!(lines[1].contains("55667788 wins $50 (side_1)"));
    }

    #[test]
    fn on_hand_complete_emits_completion_line() {
        let (proj, sink) = make_projector();
        proj.on_hand_complete(HandComplete {
            table_root: vec![],
            hand_number: 42,
            winners: vec![],
            final_stacks: vec![],
            completed_at: None,
        })
        .unwrap();
        assert!(sink.output().contains("HAND #42 complete"));
    }

    #[test]
    fn on_cards_dealt_renders_per_player_hole_cards() {
        let (proj, sink) = make_projector();
        let cards = vec![
            Card {
                rank: Rank::Ace as i32,
                suit: Suit::Spades as i32,
            },
            Card {
                rank: Rank::King as i32,
                suit: Suit::Hearts as i32,
            },
        ];
        proj.on_cards_dealt(CardsDealt {
            table_root: vec![],
            hand_number: 1,
            game_variant: GameVariant::TexasHoldem as i32,
            player_cards: vec![examples_proto::PlayerHoleCards {
                player_root: vec![0x99; 4],
                cards,
            }],
            dealer_position: 0,
            players: vec![],
            dealt_at: None,
            remaining_deck: vec![],
        })
        .unwrap();
        let out = sink.output();
        assert!(out.contains("99999999"));
        assert!(out.contains("[As Kh]"));
    }

    #[test]
    fn pretty_store_accumulates_state_across_events() {
        let (proj, _sink) = make_projector();
        proj.on_table_created(TableCreated {
            table_name: "T1".into(),
            game_variant: GameVariant::TexasHoldem as i32,
            small_blind: 1,
            big_blind: 2,
            min_buy_in: 100,
            max_buy_in: 500,
            max_players: 6,
            action_timeout_seconds: 30,
            created_at: None,
        })
        .unwrap();
        let hand_root = vec![0x01; 16];
        proj.on_hand_started(HandStarted {
            hand_root: hand_root.clone(),
            hand_number: 1,
            dealer_position: 0,
            small_blind_position: 1,
            big_blind_position: 2,
            active_players: vec![SeatSnapshot {
                position: 0,
                player_root: vec![0xEE; 16],
                stack: 400,
            }],
            game_variant: GameVariant::TexasHoldem as i32,
            small_blind: 1,
            big_blind: 2,
            started_at: None,
        })
        .unwrap();
        proj.store.record_board(&hand_root, "Flop", &[]).unwrap();
        proj.store.record_pot_total(&hand_root, 50).unwrap();
        proj.store.update_hand_phase(&hand_root, "flop").unwrap();

        let conn = proj.store.conn.lock().unwrap();
        let phase: String = conn
            .query_row(
                "SELECT phase FROM hands WHERE hand_root = ?1",
                params![hand_root],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(phase, "flop");
        let pot: i64 = conn
            .query_row(
                "SELECT pot FROM pot_totals WHERE hand_root = ?1",
                params![hand_root],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(pot, 50);
    }
}
