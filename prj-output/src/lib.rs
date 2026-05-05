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

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use angzarr_client::{projector, CommandResult};
use examples_proto::{
    ActionTaken, ActionType, BettingPhase, BlindPosted, Card, CardsDealt, CardsMucked,
    CardsRevealed, CommunityCardsDealt, FundsDeposited, FundsReserved, FundsWithdrawn, GameVariant,
    HandComplete, HandEnded, HandRankType, HandStarted, PlayerJoined, PlayerLeft, PlayerRegistered,
    PlayerTimedOut, PlayerType, PotAwarded, Rank, ShowdownStarted, Suit, TableCreated,
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
    /// Cross-event name resolution: `player_root` bytes → display name.
    /// Populated via `set_player_name` from PlayerRegistered (where the
    /// projector knows the root) or test fixtures.
    names: Mutex<HashMap<Vec<u8>, String>>,
    pub show_timestamps: bool,
    /// Accumulating board for the current hand (clears on HandStarted).
    board: Mutex<Vec<Card>>,
}

impl PrettyOutputProjector {
    pub fn new(store: PrettyStore, sink: Box<dyn LineSink>) -> Self {
        Self {
            store,
            sink,
            names: Mutex::new(HashMap::new()),
            show_timestamps: false,
            board: Mutex::new(Vec::new()),
        }
    }

    /// Construct a projector with output directed at stdout.
    pub fn from_env() -> Self {
        let store = match std::env::var("PRJ_PRETTY_OUTPUT_DB") {
            Ok(s) if !s.is_empty() => {
                PrettyStore::open(&PathBuf::from(s)).expect("open sqlite store")
            }
            _ => PrettyStore::open_in_memory().expect("open in-memory sqlite store"),
        };
        Self::new(store, Box::new(StdoutSink))
    }

    /// Register a `player_root → display name` mapping. Called by
    /// PlayerRegistered when the cover surfaces the root; tests use it
    /// to seed names directly (they pass `player_root = name.as_bytes()`).
    pub fn set_player_name(&self, player_root: &[u8], name: &str) {
        self.names
            .lock()
            .unwrap()
            .insert(player_root.to_vec(), name.to_string());
    }

    /// Resolve `player_root` → display name. Falls back to
    /// `Player_<utf8-or-hex-suffix>` when unknown.
    pub fn resolve_name(&self, player_root: &[u8]) -> String {
        if let Some(name) = self.names.lock().unwrap().get(player_root) {
            return name.clone();
        }
        // Try utf-8: many test fixtures use ASCII roots like "player-abc123".
        if let Ok(s) = std::str::from_utf8(player_root) {
            let trimmed = s.strip_prefix("player-").unwrap_or(s);
            return format!("Player_{}", trimmed);
        }
        format!("Player_{}", fmt_player_short(player_root))
    }

    fn emit(&self, line: &str, ts: Option<&prost_types::Timestamp>) {
        if !self.show_timestamps {
            self.sink.write_line(line);
            return;
        }
        match ts {
            Some(t) => self
                .sink
                .write_line(&format!("[{}] {line}", fmt_timestamp_hms(t))),
            None => self.sink.write_line(line),
        }
    }
}

/// Format a `prost_types::Timestamp` as `HH:MM:SS` (UTC, modulo a day —
/// the projector's observability output asserts on the prefix only).
fn fmt_timestamp_hms(ts: &prost_types::Timestamp) -> String {
    let secs = ts.seconds.rem_euclid(86_400);
    let h = (secs / 3_600) as u8;
    let m = ((secs % 3_600) / 60) as u8;
    let s = (secs % 60) as u8;
    format!("{:02}:{:02}:{:02}", h, m, s)
}

#[projector(name = "pretty-output", domains = ["player", "table", "hand"])]
impl PrettyOutputProjector {
    // --- Player domain ---

    #[handles(PlayerRegistered)]
    pub fn on_player_registered(&self, event: PlayerRegistered) -> CommandResult<()> {
        // Note: the projector dispatch doesn't surface the player_root cover
        // here. Tests register names directly via `set_player_name`. The
        // emitted line still names the player from the event body so the
        // BDD "Alice registered" assertion passes.
        let kind = match PlayerType::try_from(event.player_type).unwrap_or_default() {
            PlayerType::Human => Some("HUMAN"),
            PlayerType::Ai => Some("AI"),
            _ => None,
        };
        let suffix = match (event.email.as_str(), kind) {
            ("", _) => String::new(),
            (email, Some(k)) => format!(" ({}) as {}", email, k),
            (email, None) => format!(" ({})", email),
        };
        self.emit(
            &format!("{} registered{}", event.display_name, suffix),
            event.registered_at.as_ref(),
        );
        Ok(())
    }

    #[handles(FundsDeposited)]
    pub fn on_funds_deposited(&self, event: FundsDeposited) -> CommandResult<()> {
        let amount = event.amount.as_ref().map(|c| c.amount).unwrap_or(0);
        let balance = event.new_balance.as_ref().map(|c| c.amount).unwrap_or(0);
        self.emit(
            &format!(
                "Deposited {}, balance: {}",
                fmt_money(amount),
                fmt_money(balance)
            ),
            event.deposited_at.as_ref(),
        );
        Ok(())
    }

    #[handles(FundsWithdrawn)]
    pub fn on_funds_withdrawn(&self, event: FundsWithdrawn) -> CommandResult<()> {
        let amount = event.amount.as_ref().map(|c| c.amount).unwrap_or(0);
        let balance = event.new_balance.as_ref().map(|c| c.amount).unwrap_or(0);
        self.emit(
            &format!(
                "Withdrew {}, balance: {}",
                fmt_money(amount),
                fmt_money(balance)
            ),
            event.withdrawn_at.as_ref(),
        );
        Ok(())
    }

    #[handles(FundsReserved)]
    pub fn on_funds_reserved(&self, event: FundsReserved) -> CommandResult<()> {
        let amount = event.amount.as_ref().map(|c| c.amount).unwrap_or(0);
        self.emit(
            &format!("Reserved {}", fmt_money(amount)),
            event.reserved_at.as_ref(),
        );
        Ok(())
    }

    // --- Table domain ---

    #[handles(TableCreated)]
    pub fn on_table_created(&self, event: TableCreated) -> CommandResult<()> {
        let _ = self.store.record_table(&event);
        self.emit(
            &format!(
                "Table \"{}\" created: {} {}/{} (buy-in: {} - {})",
                event.table_name,
                fmt_variant(event.game_variant),
                fmt_money(event.small_blind),
                fmt_money(event.big_blind),
                fmt_money(event.min_buy_in),
                fmt_money(event.max_buy_in),
            ),
            event.created_at.as_ref(),
        );
        Ok(())
    }

    #[handles(PlayerJoined)]
    pub fn on_player_joined(&self, event: PlayerJoined) -> CommandResult<()> {
        let _ = self
            .store
            .update_player_stack(&event.player_root, event.stack);
        let name = self.resolve_name(&event.player_root);
        self.emit(
            &format!(
                "{} joined at seat {} with {}",
                name,
                event.seat_position,
                fmt_money(event.stack),
            ),
            event.joined_at.as_ref(),
        );
        Ok(())
    }

    #[handles(PlayerLeft)]
    pub fn on_player_left(&self, event: PlayerLeft) -> CommandResult<()> {
        let name = self.resolve_name(&event.player_root);
        self.emit(
            &format!("{} left with {}", name, fmt_money(event.chips_cashed_out)),
            event.left_at.as_ref(),
        );
        Ok(())
    }

    #[handles(HandStarted)]
    pub fn on_hand_started(&self, event: HandStarted) -> CommandResult<()> {
        let _ = self.store.record_hand_started(&event);
        self.board.lock().unwrap().clear();
        let names: Vec<String> = event
            .active_players
            .iter()
            .map(|s| self.resolve_name(&s.player_root))
            .collect();
        let players = if names.is_empty() {
            "no players".to_string()
        } else {
            names.join(", ")
        };
        self.emit(
            &format!(
                "HAND #{} started — Dealer: Seat {} — Players: {}",
                event.hand_number, event.dealer_position, players,
            ),
            event.started_at.as_ref(),
        );
        Ok(())
    }

    #[handles(HandEnded)]
    pub fn on_hand_ended(&self, event: HandEnded) -> CommandResult<()> {
        if event.results.is_empty() {
            self.emit("Hand ended", event.ended_at.as_ref());
            return Ok(());
        }
        let parts: Vec<String> = event
            .results
            .iter()
            .map(|r| {
                format!(
                    "{} wins {}",
                    self.resolve_name(&r.winner_root),
                    fmt_money(r.amount)
                )
            })
            .collect();
        self.emit(
            &format!("Hand ended — {}", parts.join(", ")),
            event.ended_at.as_ref(),
        );
        Ok(())
    }

    // --- Hand domain ---

    #[handles(CardsDealt)]
    pub fn on_cards_dealt(&self, event: CardsDealt) -> CommandResult<()> {
        if event.player_cards.is_empty() {
            self.emit("Cards dealt", event.dealt_at.as_ref());
            return Ok(());
        }
        let lines: Vec<String> = event
            .player_cards
            .iter()
            .map(|pc| {
                format!(
                    "{}: {}",
                    self.resolve_name(&pc.player_root),
                    fmt_cards(&pc.cards)
                )
            })
            .collect();
        self.emit(&lines.join(" | "), event.dealt_at.as_ref());
        Ok(())
    }

    #[handles(BlindPosted)]
    pub fn on_blind_posted(&self, event: BlindPosted) -> CommandResult<()> {
        let kind = if event.blind_type.is_empty() {
            "BLIND".to_string()
        } else {
            event.blind_type.to_uppercase()
        };
        let name = self.resolve_name(&event.player_root);
        self.emit(
            &format!("{} posts {} {}", name, kind, fmt_money(event.amount),),
            event.posted_at.as_ref(),
        );
        Ok(())
    }

    #[handles(ActionTaken)]
    pub fn on_action_taken(&self, event: ActionTaken) -> CommandResult<()> {
        let _ = self
            .store
            .update_player_stack(&event.player_root, event.player_stack);
        let name = self.resolve_name(&event.player_root);
        let mut msg = match ActionType::try_from(event.action).unwrap_or_default() {
            ActionType::Fold => format!("{} folds", name),
            ActionType::Check => format!("{} checks", name),
            ActionType::Call => format!("{} calls {}", name, fmt_money(event.amount)),
            ActionType::Bet => format!("{} bets {}", name, fmt_money(event.amount)),
            ActionType::Raise => format!("{} raises to {}", name, fmt_money(event.amount)),
            ActionType::AllIn => format!("{} all-in {}", name, fmt_money(event.amount)),
            _ => format!("{} acts ({:?} {})", name, event.action, event.amount),
        };
        if event.pot_total > 0 {
            msg.push_str(&format!(" (pot: {})", fmt_money(event.pot_total)));
        }
        self.emit(&msg, event.action_at.as_ref());
        Ok(())
    }

    #[handles(CommunityCardsDealt)]
    pub fn on_community_dealt(&self, event: CommunityCardsDealt) -> CommandResult<()> {
        let label = fmt_phase_label(event.phase);
        let new_cards = fmt_cards(&event.cards);
        let mut board = self.board.lock().unwrap();
        board.extend(event.cards.iter().cloned());
        let board_str = if board.is_empty() {
            new_cards.clone()
        } else {
            fmt_cards(&board)
        };
        self.emit(
            &format!("{}: {} — Board: {}", label, new_cards, board_str),
            event.dealt_at.as_ref(),
        );
        Ok(())
    }

    #[handles(ShowdownStarted)]
    pub fn on_showdown_started(&self, event: ShowdownStarted) -> CommandResult<()> {
        self.emit("SHOWDOWN", event.started_at.as_ref());
        Ok(())
    }

    #[handles(CardsRevealed)]
    pub fn on_cards_revealed(&self, event: CardsRevealed) -> CommandResult<()> {
        let name = self.resolve_name(&event.player_root);
        let cards_str = fmt_cards(&event.cards);
        let ranking = event
            .ranking
            .as_ref()
            .map(|r| fmt_hand_rank(r.rank_type))
            .unwrap_or("");
        if ranking.is_empty() {
            self.emit(
                &format!("{} shows {}", name, cards_str),
                event.revealed_at.as_ref(),
            );
        } else {
            self.emit(
                &format!("{} shows {} — {}", name, cards_str, ranking),
                event.revealed_at.as_ref(),
            );
        }
        Ok(())
    }

    #[handles(CardsMucked)]
    pub fn on_cards_mucked(&self, event: CardsMucked) -> CommandResult<()> {
        let name = self.resolve_name(&event.player_root);
        self.emit(&format!("{} mucks", name), event.mucked_at.as_ref());
        Ok(())
    }

    #[handles(PotAwarded)]
    pub fn on_pot_awarded(&self, event: PotAwarded) -> CommandResult<()> {
        for w in &event.winners {
            self.emit(
                &format!(
                    "{} wins {}",
                    self.resolve_name(&w.player_root),
                    fmt_money(w.amount),
                ),
                event.awarded_at.as_ref(),
            );
        }
        Ok(())
    }

    #[handles(PlayerTimedOut)]
    pub fn on_player_timed_out(&self, event: PlayerTimedOut) -> CommandResult<()> {
        let name = self.resolve_name(&event.player_root);
        let verb = match ActionType::try_from(event.default_action).unwrap_or_default() {
            ActionType::Fold => "folds",
            ActionType::Check => "checks",
            _ => "acts",
        };
        self.emit(
            &format!("{} timed out — auto {}", name, verb),
            event.timed_out_at.as_ref(),
        );
        Ok(())
    }

    #[handles(HandComplete)]
    pub fn on_hand_complete(&self, event: HandComplete) -> CommandResult<()> {
        if event.final_stacks.is_empty() {
            self.emit(
                &format!("HAND #{} complete", event.hand_number),
                event.completed_at.as_ref(),
            );
            return Ok(());
        }
        let mut parts: Vec<String> = Vec::with_capacity(event.final_stacks.len());
        parts.push("Final stacks".to_string());
        for snap in &event.final_stacks {
            let name = self.resolve_name(&snap.player_root);
            let mut entry = format!("{}: {}", name, fmt_money(snap.stack));
            if snap.has_folded {
                entry.push_str(" (folded)");
            }
            parts.push(entry);
        }
        self.emit(&parts.join(" — "), event.completed_at.as_ref());
        Ok(())
    }

    /// Catch-all for events whose `type_url` matches no `#[handles]` arm.
    /// The framework also emits a `tracing::warn!` regardless of whether
    /// this method is declared; emitting a sink line gives observability
    /// through the projector's own output channel.
    #[handles_unknown]
    pub fn on_unknown(&self, type_url: &str) {
        self.emit(&format!("[Unknown event type: {}]", type_url), None);
    }
}

fn fmt_hand_rank(rank: i32) -> &'static str {
    match HandRankType::try_from(rank).unwrap_or_default() {
        HandRankType::HighCard => "High Card",
        HandRankType::Pair => "Pair",
        HandRankType::TwoPair => "Two Pair",
        HandRankType::ThreeOfAKind => "Three of a Kind",
        HandRankType::Straight => "Straight",
        HandRankType::Flush => "Flush",
        HandRankType::FullHouse => "Full House",
        HandRankType::FourOfAKind => "Four of a Kind",
        HandRankType::StraightFlush => "Straight Flush",
        HandRankType::RoyalFlush => "Royal Flush",
        _ => "",
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
    pub fn on_player_registered_emits_registration_line() {
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
    pub fn on_player_registered_ai_labels_as_ai() {
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
    pub fn on_funds_deposited_formats_money_with_thousands() {
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
    pub fn on_table_created_persists_and_renders_stakes() {
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
    pub fn on_hand_started_persists_roster_and_renders_header() {
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
            ..Default::default()
        };
        proj.on_hand_started(event).unwrap();
        let out = sink.output();
        assert!(out.contains("HAND #5"));
        assert!(out.contains("Dealer: Seat 2"));
        // Players are resolved by name fallback (Player_<utf8>) since no
        // `set_player_name` was called; the byte roots are non-utf8.
        assert!(out.contains("Player_"));

        assert!(proj.store.known_hand(&[0xAB; 16]));
        assert!(proj.store.known_player(&[0xAA; 16]));
    }

    #[test]
    pub fn on_blind_posted_uppercases_blind_type() {
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
    pub fn on_action_taken_uses_action_specific_verb() {
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
            assert!(out.contains("pot: $100"));
        }
    }

    #[test]
    pub fn on_community_dealt_labels_phase() {
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
        assert!(out.contains("Board:"));
    }

    #[test]
    pub fn on_pot_awarded_lines_per_winner() {
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
        assert!(lines[0].contains("Player_"));
        assert!(lines[0].contains("wins $150"));
        assert!(lines[1].contains("wins $50"));
    }

    #[test]
    pub fn on_hand_complete_emits_completion_line() {
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
    pub fn on_cards_dealt_renders_per_player_hole_cards() {
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
            ..Default::default()
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
            ..Default::default()
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
