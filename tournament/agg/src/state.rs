//! Tournament aggregate state and event appliers.

use std::collections::{HashMap, HashSet};

use examples_proto::{
    BagAndTagComplete, BlindLevel, BlindLevelAdvanced, BountyAwarded, ColorUpCompleted,
    GameVariant, HandForHandEnded, HandForHandRoundComplete, HandForHandStarted,
    MixedGameVariantRotated, NewHandsHalted, NoShowDetected, PenaltyIssued,
    PenaltyRoundsDecremented, PenaltySeverity, PlayerDisqualified, PlayerEliminated,
    PlayerMovedTables, PlayerReEntered, RebuyConfig, RebuyDenied, RebuyProcessed,
    RegistrationClosed, RegistrationOpened, TournamentCompleted, TournamentCreated,
    TournamentEnrollmentRejected, TournamentPaused, TournamentPlayerEnrolled, TournamentResumed,
    TournamentStarted, TournamentStatus,
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
    // Hand-for-hand (TDA Rule 12) — bubble synchronisation state.
    pub hand_for_hand: bool,
    pub hand_for_hand_round: i32,
    pub hand_for_hand_pending_tables: HashSet<Vec<u8>>,
    pub hand_for_hand_active_tables: HashSet<Vec<u8>>,
    // Chip economy — total chips currently in play (TDA Rule 24A/24C
    // conservation). Updated by color-up, re-entry, no-show, and
    // disqualification appliers so audits can trace the chip ledger.
    pub total_chips_in_play: i64,
    // Penalty register — players currently serving a TDA Rule 71
    // penalty. Strings keyed by player_root_hex; values are the
    // remaining rounds (for MISSED_ROUND) or 1 (for MISSED_HAND).
    pub active_penalties: HashMap<String, i32>,
    pub penalty_severity: HashMap<String, PenaltySeverity>,
    // Bounty register — eliminator player_root_hex → cumulative chip
    // bounty paid (RP-22 / WSOP Rule 39).
    pub bounty_totals: HashMap<String, i64>,
    // Bag-and-tag snapshots — per-player end-of-day state (WSOP Rule
    // 122). Populated by BagAndTagComplete; consulted on resume.
    pub bag_snapshots: HashMap<String, BagSnapshot>,
    // No-show register — players ruled no-show after the first-break
    // deadline (WSOP Rule 16). Their chips have been removed from
    // total_chips_in_play; buy-in is held externally for refund.
    pub no_show_players: HashSet<String>,
    // New-hand halt — operator-issued stop (WSOP Rule 125). When set,
    // tables block subsequent StartHand commands.
    pub new_hands_halted: bool,
    // Mixed-game rotation index — TDA RP-18 / HORSE cycle position.
    // Cycles through GameVariant variants on RotateMixedGameVariant.
    pub mixed_game_index: i32,
}

#[derive(Debug, Clone, Default)]
pub struct BagSnapshot {
    pub stack: i64,
    pub table_root: Vec<u8>,
    pub seat: i32,
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

    pub fn is_hand_for_hand(&self) -> bool {
        self.hand_for_hand
    }

    pub fn is_new_hands_halted(&self) -> bool {
        self.new_hands_halted
    }

    pub fn is_no_show(&self, player_root_hex: &str) -> bool {
        self.no_show_players.contains(player_root_hex)
    }

    pub fn is_on_penalty(&self, player_root_hex: &str) -> bool {
        self.active_penalties.contains_key(player_root_hex)
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

pub fn apply_started(state: &mut TournamentState, _event: TournamentStarted) {
    state.status = TournamentStatus::TournamentRunning;
    // Seed total_chips_in_play from registered stacks at start. Subsequent
    // appliers (color-up, re-entry, no-show, disqualification) adjust
    // the ledger; the conservation invariant is checked in tests.
    state.total_chips_in_play = state
        .registered_players
        .values()
        .map(|reg| reg.starting_stack)
        .sum();
}

pub fn apply_color_up_completed(state: &mut TournamentState, event: ColorUpCompleted) {
    // Conservation invariant (TDA Rule 24A/24C): total chips moves by
    // the rescue gain minus the race loss leftover. Mirrors the Python
    // applier semantics in `tournament/agg/handlers.py:apply_color_up_completed`.
    state.total_chips_in_play += event.chips_added_by_rescue - event.chips_removed_by_race;
}

pub fn apply_hand_for_hand_started(state: &mut TournamentState, event: HandForHandStarted) {
    state.hand_for_hand = true;
    state.hand_for_hand_round = 0;
    state.hand_for_hand_pending_tables = event.active_table_roots.iter().cloned().collect();
    state.hand_for_hand_active_tables = event.active_table_roots.into_iter().collect();
}

pub fn apply_player_moved_tables(state: &mut TournamentState, event: PlayerMovedTables) {
    // While in hand-for-hand the tournament emits `PlayerMovedTables`
    // with only `from_table_root` set as a per-table progress receipt
    // for `RecordTableHandComplete` (see `handle_record_table_hand_complete`).
    // Discard that table from the per-round pending set so the next
    // command's state replay sees the prior table's removal. Mirrors
    // the cpp `apply_player_moved_tables` in
    // `examples-cpp/.../tournament_state.cpp` which also performs the
    // discard. Outside hand-for-hand mode (e.g. seat-redraw rebalance)
    // the pending set is empty and the discard is a no-op.
    if !event.from_table_root.is_empty() {
        state
            .hand_for_hand_pending_tables
            .remove(&event.from_table_root);
    }
}

pub fn apply_hand_for_hand_round_complete(
    state: &mut TournamentState,
    event: HandForHandRoundComplete,
) {
    state.hand_for_hand_round = event.round_number;
    // Re-seed the pending set from active tables so the next
    // synchronised round can be tracked independently. Mirrors Python's
    // `apply_hand_for_hand_round_complete`.
    state.hand_for_hand_pending_tables = state.hand_for_hand_active_tables.clone();
}

pub fn apply_hand_for_hand_ended(state: &mut TournamentState, _event: HandForHandEnded) {
    state.hand_for_hand = false;
    state.hand_for_hand_pending_tables.clear();
    state.hand_for_hand_active_tables.clear();
}

pub fn apply_penalty_issued(state: &mut TournamentState, event: PenaltyIssued) {
    let key = hex::encode(&event.player_root);
    let severity = match event.r#type.as_str() {
        "MISSED_ROUND" => PenaltySeverity::MissedRound,
        "MISSED_HAND" => PenaltySeverity::MissedHand,
        "DISQUALIFIED" => PenaltySeverity::Disqualification,
        _ => PenaltySeverity::VerbalWarning,
    };
    state.penalty_severity.insert(key.clone(), severity);
    // Track round-counter only for non-verbal/non-DQ penalties; the
    // decrement saga ticks this down as rounds elapse.
    if matches!(
        severity,
        PenaltySeverity::MissedHand | PenaltySeverity::MissedRound
    ) {
        let rounds = if event.rounds > 0 { event.rounds } else { 1 };
        state.active_penalties.insert(key, rounds);
    }
}

pub fn apply_penalty_rounds_decremented(
    state: &mut TournamentState,
    event: PenaltyRoundsDecremented,
) {
    let key = hex::encode(&event.player_root);
    if event.rounds_remaining <= 0 {
        state.active_penalties.remove(&key);
        state.penalty_severity.remove(&key);
    } else {
        state.active_penalties.insert(key, event.rounds_remaining);
    }
}

pub fn apply_player_disqualified(state: &mut TournamentState, event: PlayerDisqualified) {
    let key = hex::encode(&event.player_root);
    state.registered_players.remove(&key);
    state.active_penalties.remove(&key);
    state.penalty_severity.remove(&key);
    state.players_remaining = state.registered_players.len() as i32;
    // DQ chips are removed from total_chips_in_play (Rule 71D).
    state.total_chips_in_play -= event.chips_removed;
}

pub fn apply_player_re_entered(state: &mut TournamentState, event: PlayerReEntered) {
    // Re-entry (TDA Rule 8B): forfeited chips removed, fresh stack
    // added. Mirrors Python applier behavior.
    let key = hex::encode(&event.player_root);
    state.total_chips_in_play -= event.chips_forfeited;
    state.total_chips_in_play += event.chips_added;
    // Restore registration with the fresh starting stack.
    state.registered_players.insert(
        key,
        PlayerRegistration {
            player_root: event.player_root,
            fee_paid: state.buy_in,
            starting_stack: event.chips_added,
            rebuys_used: 0,
            addon_taken: false,
            table_assignment: 0,
            seat_assignment: 0,
        },
    );
    state.players_remaining = state.registered_players.len() as i32;
}

pub fn apply_bounty_awarded(state: &mut TournamentState, event: BountyAwarded) {
    let key = hex::encode(&event.eliminator_root);
    *state.bounty_totals.entry(key).or_insert(0) += event.amount;
}

pub fn apply_no_show_detected(state: &mut TournamentState, event: NoShowDetected) {
    let key = hex::encode(&event.player_root);
    state.no_show_players.insert(key.clone());
    state.registered_players.remove(&key);
    state.players_remaining = state.registered_players.len() as i32;
    // Chips removed (WSOP Rule 16); buy-in held externally.
    state.total_chips_in_play -= event.chips_removed;
}

pub fn apply_new_hands_halted(state: &mut TournamentState, _event: NewHandsHalted) {
    state.new_hands_halted = true;
}

pub fn apply_bag_and_tag_complete(state: &mut TournamentState, event: BagAndTagComplete) {
    for snapshot in event.snapshots {
        state.bag_snapshots.insert(
            hex::encode(&snapshot.player_root),
            BagSnapshot {
                stack: snapshot.stack,
                table_root: snapshot.table_root,
                seat: snapshot.seat,
            },
        );
    }
}

pub fn apply_mixed_game_variant_rotated(
    state: &mut TournamentState,
    event: MixedGameVariantRotated,
) {
    state.game_variant = GameVariant::try_from(event.to_variant).unwrap_or_default();
    state.mixed_game_index += 1;
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
                ..Default::default()
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
    fn apply_player_moved_tables_discards_from_h4h_pending_set() {
        // `PlayerMovedTables` doubles as the per-table progress receipt
        // for `RecordTableHandComplete` while in hand-for-hand. The
        // applier must remove `from_table_root` from the pending set
        // so the next replay sees the prior table's discard.
        let mut state = created_state(None);
        apply_hand_for_hand_started(
            &mut state,
            HandForHandStarted {
                started_at: None,
                active_table_roots: vec![vec![0xaa], vec![0xbb]],
            },
        );
        assert_eq!(state.hand_for_hand_pending_tables.len(), 2);
        apply_player_moved_tables(
            &mut state,
            PlayerMovedTables {
                from_table_root: vec![0xaa],
                ..Default::default()
            },
        );
        assert_eq!(state.hand_for_hand_pending_tables.len(), 1);
        assert!(!state.hand_for_hand_pending_tables.contains(&vec![0xaa]));
        assert!(state.hand_for_hand_pending_tables.contains(&vec![0xbb]));
    }

    #[test]
    fn apply_player_moved_tables_with_empty_from_table_root_is_noop_for_pending() {
        // Saga-side rebalance emits may carry an empty `from_table_root`
        // when the bytes are not yet stamped; the applier must not
        // accidentally erase the empty-vec key from the pending set
        // (HashSet `remove` would treat empty Vec as a valid key).
        let mut state = created_state(None);
        apply_hand_for_hand_started(
            &mut state,
            HandForHandStarted {
                started_at: None,
                active_table_roots: vec![vec![0xaa]],
            },
        );
        apply_player_moved_tables(
            &mut state,
            PlayerMovedTables {
                from_table_root: vec![],
                ..Default::default()
            },
        );
        assert_eq!(state.hand_for_hand_pending_tables.len(), 1);
        assert!(state.hand_for_hand_pending_tables.contains(&vec![0xaa]));
    }
}
