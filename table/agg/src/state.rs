//! Table aggregate state and event appliers.

use std::collections::HashMap;

use examples_proto::{
    ChipsAdded, GameVariant, HandEnded, HandStarted, PlayerJoined, PlayerLeft, PlayerSatIn,
    PlayerSatOut, PlayerSeated, RebuyChipsAdded, SeatingRejected, TableCreated,
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

#[cfg(test)]
mod tests {
    use super::*;
    use examples_proto::GameVariant;

    fn created_state() -> TableState {
        let mut state = TableState::default();
        apply_table_created(
            &mut state,
            TableCreated {
                table_name: "MyTable".into(),
                game_variant: GameVariant::TexasHoldem as i32,
                small_blind: 5,
                big_blind: 10,
                min_buy_in: 100,
                max_buy_in: 1000,
                max_players: 6,
                action_timeout_seconds: 30,
                created_at: None,
            },
        );
        state
    }

    #[test]
    fn apply_table_created_initializes_identity_and_status() {
        let state = created_state();
        assert_eq!(state.table_id, "table_MyTable");
        assert_eq!(state.game_variant, GameVariant::TexasHoldem);
        assert_eq!(state.status, "waiting");
        assert_eq!(state.max_players, 6);
        assert!(state.exists());
    }

    #[test]
    fn apply_player_joined_records_seat() {
        let mut state = created_state();
        apply_player_joined(
            &mut state,
            PlayerJoined {
                player_root: vec![1, 2, 3],
                seat_position: 2,
                buy_in_amount: 500,
                stack: 500,
                joined_at: None,
            },
        );
        assert_eq!(state.player_count(), 1);
        assert_eq!(state.active_player_count(), 1);
        let pos = state.find_seat_position_by_player(&[1, 2, 3]).unwrap();
        assert_eq!(pos, 2);
        assert_eq!(state.find_seat_by_player(&[1, 2, 3]).unwrap().stack, 500);
        assert_eq!(state.next_available_seat(), Some(0));
    }

    #[test]
    fn apply_player_left_removes_seat() {
        let mut state = created_state();
        apply_player_joined(
            &mut state,
            PlayerJoined {
                player_root: vec![1],
                seat_position: 0,
                buy_in_amount: 100,
                stack: 100,
                joined_at: None,
            },
        );
        apply_player_left(
            &mut state,
            PlayerLeft {
                player_root: vec![1],
                seat_position: 0,
                chips_cashed_out: 50,
                left_at: None,
            },
        );
        assert_eq!(state.player_count(), 0);
    }

    #[test]
    fn sit_out_then_sit_in_toggles_flag() {
        let mut state = created_state();
        apply_player_joined(
            &mut state,
            PlayerJoined {
                player_root: vec![1],
                seat_position: 0,
                buy_in_amount: 100,
                stack: 100,
                joined_at: None,
            },
        );
        apply_player_sat_out(
            &mut state,
            PlayerSatOut {
                player_root: vec![1],
                sat_out_at: None,
            },
        );
        assert_eq!(state.active_player_count(), 0);
        apply_player_sat_in(
            &mut state,
            PlayerSatIn {
                player_root: vec![1],
                sat_in_at: None,
            },
        );
        assert_eq!(state.active_player_count(), 1);
    }

    #[test]
    fn hand_started_updates_current_hand_and_status() {
        let mut state = created_state();
        apply_hand_started(
            &mut state,
            HandStarted {
                hand_root: vec![9],
                hand_number: 1,
                dealer_position: 0,
                small_blind_position: 1,
                big_blind_position: 2,
                active_players: vec![],
                game_variant: GameVariant::TexasHoldem as i32,
                small_blind: 5,
                big_blind: 10,
                started_at: None,
            },
        );
        assert_eq!(state.status, "in_hand");
        assert_eq!(state.current_hand_root, vec![9]);
        assert_eq!(state.hand_count, 1);
    }

    #[test]
    fn hand_ended_applies_stack_changes_and_resets_status() {
        let mut state = created_state();
        apply_player_joined(
            &mut state,
            PlayerJoined {
                player_root: vec![1],
                seat_position: 0,
                buy_in_amount: 100,
                stack: 100,
                joined_at: None,
            },
        );
        state.current_hand_root = vec![9];
        state.status = "in_hand".to_string();
        let mut changes = std::collections::HashMap::new();
        changes.insert(hex::encode(&[1u8]), 50i64);
        apply_hand_ended(
            &mut state,
            HandEnded {
                hand_root: vec![9],
                results: vec![],
                stack_changes: changes,
                ended_at: None,
            },
        );
        assert_eq!(state.status, "waiting");
        assert!(state.current_hand_root.is_empty());
        assert_eq!(state.find_seat_by_player(&[1]).unwrap().stack, 150);
    }

    #[test]
    fn chips_added_updates_seat_stack() {
        let mut state = created_state();
        apply_player_joined(
            &mut state,
            PlayerJoined {
                player_root: vec![1],
                seat_position: 0,
                buy_in_amount: 100,
                stack: 100,
                joined_at: None,
            },
        );
        apply_chips_added(
            &mut state,
            ChipsAdded {
                player_root: vec![1],
                amount: 400,
                new_stack: 500,
                added_at: None,
            },
        );
        let _ = state.seats.get(&0).unwrap();
        assert_eq!(state.find_seat_by_player(&[1]).unwrap().stack, 500);
    }

    #[test]
    fn player_seated_inserts_seat_via_pm_flow() {
        let mut state = created_state();
        apply_player_seated(
            &mut state,
            PlayerSeated {
                player_root: vec![7],
                reservation_id: vec![],
                seat_position: 3,
                stack: 1000,
                seated_at: None,
            },
        );
        assert_eq!(state.player_count(), 1);
        assert_eq!(state.find_seat_by_player(&[7]).unwrap().position, 3);
    }

    #[test]
    fn seating_rejected_is_noop() {
        let state_before = created_state();
        let mut state = state_before.clone();
        apply_seating_rejected(
            &mut state,
            SeatingRejected {
                player_root: vec![1],
                reservation_id: vec![],
                requested_seat: 0,
                reason: String::new(),
                rejected_at: None,
            },
        );
        assert_eq!(state.seats.len(), state_before.seats.len());
    }

    #[test]
    fn rebuy_chips_added_updates_stack_at_seat() {
        let mut state = created_state();
        apply_player_joined(
            &mut state,
            PlayerJoined {
                player_root: vec![1],
                seat_position: 0,
                buy_in_amount: 50,
                stack: 50,
                joined_at: None,
            },
        );
        apply_rebuy_chips_added(
            &mut state,
            RebuyChipsAdded {
                player_root: vec![1],
                reservation_id: vec![],
                seat: 0,
                amount: 100,
                new_stack: 150,
                added_at: None,
            },
        );
        // and confirm through the convenience path too
        let _ = state.find_seat_by_player(&[1]);
        assert_eq!(state.seats.get(&0).unwrap().stack, 150);
    }
}
