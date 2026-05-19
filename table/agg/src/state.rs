//! Table aggregate state and event appliers.

use std::collections::HashMap;

use examples_proto::{
    BlindDodgePenalty, ChipsAdded, GameVariant, HandEnded, HandStarted, PlayerJoined, PlayerLeft,
    PlayerSatIn, PlayerSatOut, PlayerSeated, RebuyChipsAdded, SeatingRejected, TableCreated,
    TableHandForHandEnded, TableHandForHandRoundComplete, TableHandForHandWaiting,
};

#[derive(Debug, Clone)]
pub struct SeatState {
    pub position: i32,
    pub player_root: Vec<u8>,
    pub stack: i64,
    pub is_active: bool,
    pub is_sitting_out: bool,
}

#[derive(Debug, Default, Clone)]
pub struct TableState {
    pub table_id: String,
    pub table_name: String,
    pub game_variant: GameVariant,
    pub small_blind: i64,
    pub big_blind: i64,
    pub min_buy_in: i64,
    pub max_buy_in: i64,
    pub max_players: i32,
    pub action_timeout_seconds: i32,
    pub seats: HashMap<i32, SeatState>,
    pub dealer_position: i32,
    pub hand_count: i64,
    pub current_hand_root: Vec<u8>,
    pub status: String,
    /// TDA Rule 12 — tournament hand-for-hand sync. While true the
    /// table blocks new `StartHand` commands until the tournament
    /// fans out a fresh `EnterTableHandForHand`. Toggled by
    /// `apply_table_hand_for_hand_entered` / `apply_table_hand_for_hand_ended`.
    pub hand_for_hand: bool,
    /// PR #12 / EU-1185 — running missed-round penalty count per player,
    /// updated by `apply_blind_dodge_penalty`. Key is player_root.
    pub missed_round_count_by_player: HashMap<Vec<u8>, i32>,
}

impl TableState {
    pub fn exists(&self) -> bool {
        !self.table_id.is_empty()
    }

    pub fn player_count(&self) -> usize {
        self.seats.len()
    }

    pub fn active_player_count(&self) -> usize {
        self.seats.values().filter(|s| !s.is_sitting_out).count()
    }

    pub fn find_seat_position_by_player(&self, player_root: &[u8]) -> Option<i32> {
        let player_hex = hex::encode(player_root);
        self.seats.iter().find_map(|(pos, seat)| {
            if hex::encode(&seat.player_root) == player_hex {
                Some(*pos)
            } else {
                None
            }
        })
    }

    pub fn find_seat_by_player(&self, player_root: &[u8]) -> Option<&SeatState> {
        let player_hex = hex::encode(player_root);
        self.seats
            .values()
            .find(|seat| hex::encode(&seat.player_root) == player_hex)
    }

    pub fn next_available_seat(&self) -> Option<i32> {
        (0..self.max_players).find(|i| !self.seats.contains_key(i))
    }
}

// --- Event appliers ---

pub fn apply_table_created(state: &mut TableState, event: TableCreated) {
    state.table_id = format!("table_{}", event.table_name);
    state.table_name = event.table_name;
    state.game_variant = GameVariant::try_from(event.game_variant).unwrap_or_default();
    state.small_blind = event.small_blind;
    state.big_blind = event.big_blind;
    state.min_buy_in = event.min_buy_in;
    state.max_buy_in = event.max_buy_in;
    state.max_players = event.max_players;
    state.action_timeout_seconds = event.action_timeout_seconds;
    state.dealer_position = 0;
    state.hand_count = 0;
    state.status = "waiting".to_string();
}

pub fn apply_player_joined(state: &mut TableState, event: PlayerJoined) {
    state.seats.insert(
        event.seat_position,
        SeatState {
            position: event.seat_position,
            player_root: event.player_root,
            stack: event.stack,
            is_active: true,
            is_sitting_out: false,
        },
    );
}

pub fn apply_player_left(state: &mut TableState, event: PlayerLeft) {
    state.seats.remove(&event.seat_position);
}

pub fn apply_player_sat_out(state: &mut TableState, event: PlayerSatOut) {
    if let Some(pos) = state.find_seat_position_by_player(&event.player_root) {
        if let Some(seat) = state.seats.get_mut(&pos) {
            seat.is_sitting_out = true;
        }
    }
}

pub fn apply_player_sat_in(state: &mut TableState, event: PlayerSatIn) {
    if let Some(pos) = state.find_seat_position_by_player(&event.player_root) {
        if let Some(seat) = state.seats.get_mut(&pos) {
            seat.is_sitting_out = false;
        }
    }
}

pub fn apply_hand_started(state: &mut TableState, event: HandStarted) {
    state.current_hand_root = event.hand_root;
    state.hand_count = event.hand_number;
    state.dealer_position = event.dealer_position;
    state.status = "in_hand".to_string();
}

pub fn apply_hand_ended(state: &mut TableState, event: HandEnded) {
    state.current_hand_root.clear();
    state.status = "waiting".to_string();
    for (player_hex, delta) in &event.stack_changes {
        for seat in state.seats.values_mut() {
            if hex::encode(&seat.player_root) == *player_hex {
                seat.stack += delta;
                break;
            }
        }
    }
}

pub fn apply_chips_added(state: &mut TableState, event: ChipsAdded) {
    if let Some(pos) = state.find_seat_position_by_player(&event.player_root) {
        if let Some(seat) = state.seats.get_mut(&pos) {
            seat.stack = event.new_stack;
        }
    }
}

// --- PM-orchestrated events ---

pub fn apply_player_seated(state: &mut TableState, event: PlayerSeated) {
    state.seats.insert(
        event.seat_position,
        SeatState {
            position: event.seat_position,
            player_root: event.player_root,
            stack: event.stack,
            is_active: true,
            is_sitting_out: false,
        },
    );
}

pub fn apply_seating_rejected(_state: &mut TableState, _event: SeatingRejected) {}

pub fn apply_rebuy_chips_added(state: &mut TableState, event: RebuyChipsAdded) {
    if let Some(seat) = state.seats.get_mut(&event.seat) {
        seat.stack = event.new_stack;
    }
}

pub fn apply_table_hand_for_hand_entered(state: &mut TableState, _event: TableHandForHandWaiting) {
    state.hand_for_hand = true;
}

pub fn apply_table_hand_for_hand_hand_completed(
    _state: &mut TableState,
    _event: TableHandForHandRoundComplete,
) {
    // Tournament aggregate tracks per-table progress via
    // RecordTableHandComplete. The per-table aggregate has nothing
    // additional to store here.
}

pub fn apply_table_hand_for_hand_ended(state: &mut TableState, _event: TableHandForHandEnded) {
    state.hand_for_hand = false;
}

// PR #12 / EU-1185 — record the chip forfeit + missed-round increment.
// Mirrors Python `apply_blind_dodge_penalty` in `table.py:1196`.
pub fn apply_blind_dodge_penalty(state: &mut TableState, event: BlindDodgePenalty) {
    let key = event.player_root.clone();
    let prior = *state.missed_round_count_by_player.get(&key).unwrap_or(&0);
    state
        .missed_round_count_by_player
        .insert(key, prior + event.missed_round_count);
    for seat in state.seats.values_mut() {
        if seat.player_root == event.player_root {
            seat.stack = (seat.stack - event.chips_forfeited).max(0);
            break;
        }
    }
}
