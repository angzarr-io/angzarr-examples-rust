//! Tournament aggregate state and event appliers.

use std::collections::HashMap;

use examples_proto::{
    BlindLevel, BlindLevelAdvanced, GameVariant, PlayerEliminated, RebuyConfig, RebuyDenied,
    RebuyProcessed, RegistrationClosed, RegistrationOpened, TournamentCompleted, TournamentCreated,
    TournamentEnrollmentRejected, TournamentPaused, TournamentPlayerEnrolled, TournamentResumed,
    TournamentStatus,
};

#[derive(Debug, Clone, Default)]
pub struct PlayerRegistration {
    pub player_root: Vec<u8>,
    pub fee_paid: i64,
    pub starting_stack: i64,
    pub rebuys_used: i32,
    pub addon_taken: bool,
    pub table_assignment: i32,
    pub seat_assignment: i32,
}

#[derive(Debug, Default, Clone)]
pub struct TournamentState {
    pub tournament_id: String,
    pub name: String,
    pub game_variant: GameVariant,
    pub status: TournamentStatus,
    pub buy_in: i64,
    pub starting_stack: i64,
    pub max_players: i32,
    pub min_players: i32,
    pub rebuy_config: Option<RebuyConfig>,
    pub blind_structure: Vec<BlindLevel>,
    pub current_level: i32,
    pub registered_players: HashMap<String, PlayerRegistration>,
    pub players_remaining: i32,
    pub total_prize_pool: i64,
}

impl TournamentState {
    pub fn exists(&self) -> bool {
        !self.tournament_id.is_empty()
    }

    pub fn is_registration_open(&self) -> bool {
        self.status == TournamentStatus::TournamentRegistrationOpen
    }

    pub fn is_running(&self) -> bool {
        self.status == TournamentStatus::TournamentRunning
    }

    pub fn has_capacity(&self) -> bool {
        (self.registered_players.len() as i32) < self.max_players
    }

    pub fn is_player_registered(&self, player_root_hex: &str) -> bool {
        self.registered_players.contains_key(player_root_hex)
    }

    pub fn can_rebuy(&self, player_root_hex: &str) -> bool {
        if !self.is_running() {
            return false;
        }

        let Some(rebuy_config) = &self.rebuy_config else {
            return false;
        };

        if !rebuy_config.enabled {
            return false;
        }

        if rebuy_config.rebuy_level_cutoff > 0
            && self.current_level > rebuy_config.rebuy_level_cutoff
        {
            return false;
        }

        if let Some(registration) = self.registered_players.get(player_root_hex) {
            if rebuy_config.max_rebuys > 0 && registration.rebuys_used >= rebuy_config.max_rebuys {
                return false;
            }
        }

        true
    }
}

// --- Event appliers ---

pub fn apply_created(state: &mut TournamentState, event: TournamentCreated) {
    state.tournament_id = format!("tournament_{}", event.name);
    state.name = event.name;
    state.game_variant = GameVariant::try_from(event.game_variant).unwrap_or_default();
    state.status = TournamentStatus::TournamentCreated;
    state.buy_in = event.buy_in;
    state.starting_stack = event.starting_stack;
    state.max_players = event.max_players;
    state.min_players = event.min_players;
    state.rebuy_config = event.rebuy_config;
    state.blind_structure = event.blind_structure;
    state.current_level = 1;
}

pub fn apply_registration_opened(state: &mut TournamentState, _event: RegistrationOpened) {
    state.status = TournamentStatus::TournamentRegistrationOpen;
}

pub fn apply_registration_closed(_state: &mut TournamentState, _event: RegistrationClosed) {}

pub fn apply_player_enrolled(state: &mut TournamentState, event: TournamentPlayerEnrolled) {
    let player_root_hex = hex::encode(&event.player_root);
    state.registered_players.insert(
        player_root_hex,
        PlayerRegistration {
            player_root: event.player_root,
            fee_paid: event.fee_paid,
            starting_stack: event.starting_stack,
            rebuys_used: 0,
            addon_taken: false,
            table_assignment: 0,
            seat_assignment: 0,
        },
    );
    state.total_prize_pool += event.fee_paid;
    state.players_remaining = state.registered_players.len() as i32;
}

pub fn apply_enrollment_rejected(
    _state: &mut TournamentState,
    _event: TournamentEnrollmentRejected,
) {
}

pub fn apply_rebuy_processed(state: &mut TournamentState, event: RebuyProcessed) {
    let player_root_hex = hex::encode(&event.player_root);
    if let Some(registration) = state.registered_players.get_mut(&player_root_hex) {
        registration.rebuys_used = event.rebuy_count;
    }
    state.total_prize_pool += event.rebuy_cost;
}

pub fn apply_rebuy_denied(_state: &mut TournamentState, _event: RebuyDenied) {}

pub fn apply_blind_advanced(state: &mut TournamentState, event: BlindLevelAdvanced) {
    state.current_level = event.level;
}

pub fn apply_player_eliminated(state: &mut TournamentState, event: PlayerEliminated) {
    let player_root_hex = hex::encode(&event.player_root);
    state.registered_players.remove(&player_root_hex);
    state.players_remaining = state.registered_players.len() as i32;
}

pub fn apply_paused(state: &mut TournamentState, _event: TournamentPaused) {
    state.status = TournamentStatus::TournamentPaused;
}

pub fn apply_resumed(state: &mut TournamentState, _event: TournamentResumed) {
    state.status = TournamentStatus::TournamentRunning;
}

pub fn apply_completed(state: &mut TournamentState, _event: TournamentCompleted) {
    state.status = TournamentStatus::TournamentCompleted;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_rebuy_config(enabled: bool, max_rebuys: i32, cutoff: i32) -> RebuyConfig {
        RebuyConfig {
            enabled,
            max_rebuys,
            rebuy_level_cutoff: cutoff,
            stack_threshold: 5_000,
            rebuy_cost: 100,
            rebuy_chips: 1_000,
        }
    }

    fn created_state(with_rebuy: Option<RebuyConfig>) -> TournamentState {
        let mut state = TournamentState::default();
        apply_created(
            &mut state,
            TournamentCreated {
                name: "Spring Classic".into(),
                game_variant: GameVariant::TexasHoldem as i32,
                buy_in: 500,
                starting_stack: 10_000,
                max_players: 2,
                min_players: 2,
                scheduled_start: None,
                rebuy_config: with_rebuy,
                addon_config: None,
                blind_structure: vec![BlindLevel {
                    level: 1,
                    small_blind: 25,
                    big_blind: 50,
                    ante: 0,
                    duration_minutes: 20,
                }],
                created_at: None,
            },
        );
        state
    }

    fn enroll(state: &mut TournamentState, player_root: Vec<u8>, fee_paid: i64) -> String {
        let hex_key = hex::encode(&player_root);
        apply_player_enrolled(
            state,
            TournamentPlayerEnrolled {
                player_root,
                reservation_id: vec![1, 2, 3],
                fee_paid,
                starting_stack: 10_000,
                registration_number: 1,
                enrolled_at: None,
            },
        );
        hex_key
    }

    #[test]
    fn exists_is_false_on_default_and_true_after_create() {
        let state = TournamentState::default();
        assert!(!state.exists());

        let created = created_state(None);
        assert!(created.exists());
        assert_eq!(created.tournament_id, "tournament_Spring Classic");
    }

    #[test]
    fn is_registration_open_only_when_status_matches() {
        let mut state = created_state(None);
        assert!(!state.is_registration_open());
        apply_registration_opened(&mut state, RegistrationOpened { opened_at: None });
        assert!(state.is_registration_open());
        state.status = TournamentStatus::TournamentRunning;
        assert!(!state.is_registration_open());
    }

    #[test]
    fn is_running_only_when_status_matches() {
        let mut state = created_state(None);
        assert!(!state.is_running());
        state.status = TournamentStatus::TournamentRunning;
        assert!(state.is_running());
        state.status = TournamentStatus::TournamentPaused;
        assert!(!state.is_running());
    }

    #[test]
    fn has_capacity_tracks_registered_player_count() {
        let mut state = created_state(None);
        assert!(state.has_capacity());
        enroll(&mut state, vec![0xaa], 500);
        assert!(state.has_capacity());
        enroll(&mut state, vec![0xbb], 500);
        // max_players is 2; now at capacity
        assert!(!state.has_capacity());
    }

    #[test]
    fn is_player_registered_reflects_map_contents() {
        let mut state = created_state(None);
        let hex_key = enroll(&mut state, vec![0xde, 0xad], 500);
        assert!(state.is_player_registered(&hex_key));
        assert!(!state.is_player_registered("00ff"));
    }

    #[test]
    fn can_rebuy_false_when_not_running() {
        let config = sample_rebuy_config(true, 3, 5);
        let mut state = created_state(Some(config));
        let hex_key = enroll(&mut state, vec![0x11], 500);
        // status is TournamentCreated after apply_created
        assert!(!state.can_rebuy(&hex_key));
    }

    #[test]
    fn can_rebuy_false_when_no_rebuy_config() {
        let mut state = created_state(None);
        let hex_key = enroll(&mut state, vec![0x22], 500);
        state.status = TournamentStatus::TournamentRunning;
        assert!(!state.can_rebuy(&hex_key));
    }

    #[test]
    fn can_rebuy_false_when_disabled_in_config() {
        let config = sample_rebuy_config(false, 3, 5);
        let mut state = created_state(Some(config));
        let hex_key = enroll(&mut state, vec![0x33], 500);
        state.status = TournamentStatus::TournamentRunning;
        assert!(!state.can_rebuy(&hex_key));
    }

    #[test]
    fn can_rebuy_false_when_past_level_cutoff() {
        let config = sample_rebuy_config(true, 3, 4);
        let mut state = created_state(Some(config));
        let hex_key = enroll(&mut state, vec![0x44], 500);
        state.status = TournamentStatus::TournamentRunning;
        state.current_level = 5;
        assert!(!state.can_rebuy(&hex_key));
    }

    #[test]
    fn can_rebuy_false_when_at_max_rebuys() {
        let config = sample_rebuy_config(true, 2, 10);
        let mut state = created_state(Some(config));
        let hex_key = enroll(&mut state, vec![0x55], 500);
        state.status = TournamentStatus::TournamentRunning;
        state
            .registered_players
            .get_mut(&hex_key)
            .expect("player exists")
            .rebuys_used = 2;
        assert!(!state.can_rebuy(&hex_key));
    }

    #[test]
    fn can_rebuy_happy_path_true() {
        let config = sample_rebuy_config(true, 3, 10);
        let mut state = created_state(Some(config));
        let hex_key = enroll(&mut state, vec![0x66], 500);
        state.status = TournamentStatus::TournamentRunning;
        state.current_level = 2;
        assert!(state.can_rebuy(&hex_key));
    }

    #[test]
    fn can_rebuy_true_when_cutoff_zero_disabled_check() {
        // rebuy_level_cutoff == 0 means cutoff check is skipped
        let config = sample_rebuy_config(true, 3, 0);
        let mut state = created_state(Some(config));
        let hex_key = enroll(&mut state, vec![0x77], 500);
        state.status = TournamentStatus::TournamentRunning;
        state.current_level = 99;
        assert!(state.can_rebuy(&hex_key));
    }

    #[test]
    fn can_rebuy_true_when_max_zero_unlimited() {
        // max_rebuys == 0 means unlimited
        let config = sample_rebuy_config(true, 0, 10);
        let mut state = created_state(Some(config));
        let hex_key = enroll(&mut state, vec![0x88], 500);
        state.status = TournamentStatus::TournamentRunning;
        state
            .registered_players
            .get_mut(&hex_key)
            .expect("player exists")
            .rebuys_used = 100;
        assert!(state.can_rebuy(&hex_key));
    }

    #[test]
    fn apply_created_sets_identity_status_and_initial_level() {
        let state = created_state(None);
        assert_eq!(state.tournament_id, "tournament_Spring Classic");
        assert_eq!(state.name, "Spring Classic");
        assert_eq!(state.game_variant, GameVariant::TexasHoldem);
        assert_eq!(state.status, TournamentStatus::TournamentCreated);
        assert_eq!(state.buy_in, 500);
        assert_eq!(state.starting_stack, 10_000);
        assert_eq!(state.max_players, 2);
        assert_eq!(state.min_players, 2);
        assert_eq!(state.current_level, 1);
        assert_eq!(state.blind_structure.len(), 1);
        assert!(state.rebuy_config.is_none());
    }

    #[test]
    fn apply_registration_opened_sets_status() {
        let mut state = created_state(None);
        apply_registration_opened(&mut state, RegistrationOpened { opened_at: None });
        assert_eq!(state.status, TournamentStatus::TournamentRegistrationOpen);
        assert!(state.is_registration_open());
    }

    #[test]
    fn apply_registration_closed_is_noop() {
        let mut state = created_state(None);
        apply_registration_opened(&mut state, RegistrationOpened { opened_at: None });
        let before = state.status;
        let prize_before = state.total_prize_pool;
        let count_before = state.registered_players.len();
        apply_registration_closed(
            &mut state,
            RegistrationClosed {
                total_registrations: 0,
                closed_at: None,
            },
        );
        // appl_registration_closed in state.rs is a noop; status unchanged
        assert_eq!(state.status, before);
        assert_eq!(state.total_prize_pool, prize_before);
        assert_eq!(state.registered_players.len(), count_before);
    }

    #[test]
    fn apply_player_enrolled_adds_player_and_updates_pool_and_remaining() {
        let mut state = created_state(None);
        let hex_key = enroll(&mut state, vec![0xab, 0xcd], 500);
        assert_eq!(state.registered_players.len(), 1);
        assert!(state.registered_players.contains_key(&hex_key));
        let reg = state.registered_players.get(&hex_key).unwrap();
        assert_eq!(reg.fee_paid, 500);
        assert_eq!(reg.starting_stack, 10_000);
        assert_eq!(reg.rebuys_used, 0);
        assert_eq!(state.total_prize_pool, 500);
        assert_eq!(state.players_remaining, 1);

        // second enrollment accumulates
        enroll(&mut state, vec![0xef], 500);
        assert_eq!(state.registered_players.len(), 2);
        assert_eq!(state.total_prize_pool, 1000);
        assert_eq!(state.players_remaining, 2);
    }

    #[test]
    fn apply_enrollment_rejected_is_noop() {
        let mut state = created_state(None);
        enroll(&mut state, vec![0x01], 500);
        let prize_before = state.total_prize_pool;
        let count_before = state.registered_players.len();
        let remaining_before = state.players_remaining;
        apply_enrollment_rejected(
            &mut state,
            TournamentEnrollmentRejected {
                player_root: vec![0x02],
                reservation_id: vec![],
                reason: "full".into(),
                rejected_at: None,
            },
        );
        assert_eq!(state.total_prize_pool, prize_before);
        assert_eq!(state.registered_players.len(), count_before);
        assert_eq!(state.players_remaining, remaining_before);
    }

    #[test]
    fn apply_rebuy_processed_updates_rebuys_used_and_prize_pool() {
        let mut state = created_state(Some(sample_rebuy_config(true, 3, 10)));
        let player_root = vec![0x99, 0x11];
        let hex_key = enroll(&mut state, player_root.clone(), 500);
        let prize_before = state.total_prize_pool;

        apply_rebuy_processed(
            &mut state,
            RebuyProcessed {
                player_root: player_root.clone(),
                reservation_id: vec![],
                rebuy_cost: 100,
                chips_added: 1_000,
                rebuy_count: 1,
                processed_at: None,
            },
        );
        assert_eq!(
            state.registered_players.get(&hex_key).unwrap().rebuys_used,
            1
        );
        assert_eq!(state.total_prize_pool, prize_before + 100);

        apply_rebuy_processed(
            &mut state,
            RebuyProcessed {
                player_root,
                reservation_id: vec![],
                rebuy_cost: 150,
                chips_added: 1_000,
                rebuy_count: 2,
                processed_at: None,
            },
        );
        assert_eq!(
            state.registered_players.get(&hex_key).unwrap().rebuys_used,
            2
        );
        assert_eq!(state.total_prize_pool, prize_before + 250);
    }

    #[test]
    fn apply_rebuy_processed_for_unknown_player_still_updates_pool() {
        let mut state = created_state(Some(sample_rebuy_config(true, 3, 10)));
        assert_eq!(state.total_prize_pool, 0);
        apply_rebuy_processed(
            &mut state,
            RebuyProcessed {
                player_root: vec![0xff],
                reservation_id: vec![],
                rebuy_cost: 77,
                chips_added: 100,
                rebuy_count: 1,
                processed_at: None,
            },
        );
        assert_eq!(state.total_prize_pool, 77);
        assert!(state.registered_players.is_empty());
    }

    #[test]
    fn apply_rebuy_denied_is_noop() {
        let mut state = created_state(Some(sample_rebuy_config(true, 3, 10)));
        enroll(&mut state, vec![0x12], 500);
        let prize_before = state.total_prize_pool;
        let count_before = state.registered_players.len();
        apply_rebuy_denied(
            &mut state,
            RebuyDenied {
                player_root: vec![0x12],
                reservation_id: vec![],
                reason: "max_reached".into(),
                denied_at: None,
            },
        );
        assert_eq!(state.total_prize_pool, prize_before);
        assert_eq!(state.registered_players.len(), count_before);
    }

    #[test]
    fn apply_blind_advanced_updates_current_level() {
        let mut state = created_state(None);
        assert_eq!(state.current_level, 1);
        apply_blind_advanced(
            &mut state,
            BlindLevelAdvanced {
                level: 7,
                small_blind: 200,
                big_blind: 400,
                ante: 50,
                advanced_at: None,
            },
        );
        assert_eq!(state.current_level, 7);
    }

    #[test]
    fn apply_player_eliminated_removes_from_registered_and_decrements_remaining() {
        let mut state = created_state(None);
        let hex_key_a = enroll(&mut state, vec![0xaa], 500);
        let _hex_key_b = enroll(&mut state, vec![0xbb], 500);
        assert_eq!(state.players_remaining, 2);

        apply_player_eliminated(
            &mut state,
            PlayerEliminated {
                player_root: vec![0xaa],
                finish_position: 2,
                hand_root: vec![],
                payout: 0,
                eliminated_at: None,
            },
        );
        assert!(!state.registered_players.contains_key(&hex_key_a));
        assert_eq!(state.registered_players.len(), 1);
        assert_eq!(state.players_remaining, 1);
    }

    #[test]
    fn apply_paused_transitions_to_paused() {
        let mut state = created_state(None);
        state.status = TournamentStatus::TournamentRunning;
        apply_paused(
            &mut state,
            TournamentPaused {
                reason: "break".into(),
                paused_at: None,
            },
        );
        assert_eq!(state.status, TournamentStatus::TournamentPaused);
    }

    #[test]
    fn apply_resumed_transitions_to_running() {
        let mut state = created_state(None);
        state.status = TournamentStatus::TournamentPaused;
        apply_resumed(&mut state, TournamentResumed { resumed_at: None });
        assert_eq!(state.status, TournamentStatus::TournamentRunning);
        assert!(state.is_running());
    }

    #[test]
    fn apply_completed_transitions_to_completed() {
        let mut state = created_state(None);
        state.status = TournamentStatus::TournamentRunning;
        apply_completed(
            &mut state,
            TournamentCompleted {
                winner_root: vec![0xaa],
                total_prize_pool: 1_000,
                results: vec![],
                completed_at: None,
            },
        );
        assert_eq!(state.status, TournamentStatus::TournamentCompleted);
    }
}
