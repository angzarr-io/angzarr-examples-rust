//! Cucumber World for acceptance tests.
//!
//! Mirrors Python's `context.players/tables/hands` bookkeeping — scenarios
//! send commands via `CommandClient` and assert against tracked state maps.

// Test-only infrastructure shared between the `acceptance` (cluster, gated
// by the `acceptance-test` feature) and `poker_game_unit` (in-process)
// targets. Different targets exercise different fields; the dead-code lint
// can't see across feature gates, so silence it here rather than per-item.
#![allow(dead_code)]

use std::collections::HashMap;

use cucumber::World;

use super::command_client::{create_client, CommandClient, SendError};

/// Tracked player info, keyed by display name.
#[derive(Debug, Clone, Default)]
pub struct PlayerHandle {
    pub root: Vec<u8>,
    pub sequence: u32,
    pub bankroll: i64,
    pub reserved_funds: i64,
}

/// Tracked table info, keyed by table name.
#[derive(Debug, Clone, Default)]
pub struct TableHandle {
    pub root: Vec<u8>,
    pub sequence: u32,
    pub seated_players: i32,
    pub hand_count: i32,
    pub game_variant: i32,
    pub player_stacks: HashMap<String, i64>,
    /// Players in order they were seated (by seat index ascending).
    pub seat_order: Vec<(i32, String)>,
    pub current_hand: CurrentHand,
}

#[derive(Debug, Clone, Default)]
pub struct CurrentHand {
    pub pot: i64,
    pub active_players: i32,
    pub phase: String,
    pub small_blind: i64,
    pub big_blind: i64,
    pub small_blind_player: Option<String>,
    pub big_blind_player: Option<String>,
    pub dealer_seat: Option<i32>,
    pub action_on: Option<String>,
    pub current_bet: Option<i64>,
    pub min_raise: Option<i64>,
    pub winner: Option<String>,
    pub uncontested: bool,
    pub split: bool,
    pub pots: Vec<PotInfo>,
    /// Per-player running total committed to the pot this hand.
    /// Used so that "raises to N" means total-commit N (delta N - prior_commit).
    pub player_commits: HashMap<String, i64>,
}

#[derive(Debug, Clone)]
pub struct PotInfo {
    pub kind: String,
    pub amount: i64,
    pub eligible: i32,
}

#[derive(Debug, Clone, Default)]
pub struct HandHandle {
    pub root: Vec<u8>,
}

#[derive(World)]
#[world(init = Self::new)]
pub struct AcceptanceWorld {
    pub client: Box<dyn CommandClient>,
    pub players: HashMap<String, PlayerHandle>,
    pub tables: HashMap<String, TableHandle>,
    pub hands: HashMap<String, HandHandle>,
    pub last_response: Option<angzarr_client::proto::EventBook>,
    pub last_error: Option<SendError>,
    pub last_sync_mode: Option<i32>,
    pub last_cascade_error_mode: Option<i32>,
    pub current_hand_root: Option<Vec<u8>>,
    pub current_table_name: Option<String>,
    pub deck_seed: Option<String>,
    pub deck_config: Option<String>,
    pub hand_count: i32,
    pub command_start: Option<std::time::Instant>,
    pub command_end: Option<std::time::Instant>,
    pub command_succeeded: Option<bool>,
    pub bus_events: Vec<String>,
    pub monitoring_bus: bool,
    pub saga_failure_configured: bool,
    pub saga_failure_on_pot: bool,
    pub projector_healthy: bool,
    pub dlq_configured: bool,
    pub dlq_messages: Vec<String>,
    pub pm_registered: bool,
    pub no_sagas: bool,
    pub multiple_saga_failures: bool,
    pub test_players: Vec<String>,
    pub deposit_times_ms: Vec<f64>,
    pub event_without_correlation: bool,
    pub correlation_id: String,
}

impl std::fmt::Debug for AcceptanceWorld {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AcceptanceWorld")
            .field("players", &self.players.len())
            .field("tables", &self.tables.len())
            .field("hands", &self.hands.len())
            .field("last_error", &self.last_error)
            .finish()
    }
}

impl AcceptanceWorld {
    fn new() -> Self {
        Self {
            client: create_client(),
            players: HashMap::new(),
            tables: HashMap::new(),
            hands: HashMap::new(),
            last_response: None,
            last_error: None,
            last_sync_mode: None,
            last_cascade_error_mode: None,
            current_hand_root: None,
            current_table_name: None,
            deck_seed: None,
            deck_config: None,
            hand_count: 0,
            command_start: None,
            command_end: None,
            command_succeeded: None,
            bus_events: Vec::new(),
            monitoring_bus: false,
            saga_failure_configured: false,
            saga_failure_on_pot: false,
            projector_healthy: false,
            dlq_configured: false,
            dlq_messages: Vec::new(),
            pm_registered: false,
            no_sagas: false,
            multiple_saga_failures: false,
            test_players: Vec::new(),
            deposit_times_ms: Vec::new(),
            event_without_correlation: false,
            correlation_id: String::new(),
        }
    }

    pub fn player_root(&mut self, name: &str) -> Vec<u8> {
        self.players
            .entry(name.to_string())
            .or_insert_with(|| PlayerHandle {
                root: new_uuid_bytes(),
                ..Default::default()
            })
            .root
            .clone()
    }

    pub fn table_root(&mut self, name: &str) -> Vec<u8> {
        self.tables
            .entry(name.to_string())
            .or_insert_with(|| TableHandle {
                root: new_uuid_bytes(),
                game_variant: 1, // TEXAS_HOLDEM
                ..Default::default()
            })
            .root
            .clone()
    }
}

pub fn new_uuid_bytes() -> Vec<u8> {
    uuid::Uuid::new_v4().as_bytes().to_vec()
}

pub fn pack_command<M: prost::Message>(msg: &M, type_name: &str) -> prost_types::Any {
    prost_types::Any {
        type_url: format!("type.googleapis.com/{}", type_name),
        value: msg.encode_to_vec(),
    }
}
