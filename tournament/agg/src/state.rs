//! Tournament aggregate state.

use std::collections::HashMap;
use std::sync::LazyLock;

use angzarr_client::proto::event_page::Payload;
use angzarr_client::proto::EventBook;
use angzarr_client::StateRouter;
use angzarr_client::UnpackAny;
use examples_proto::{
    BlindLevel, BlindLevelAdvanced, GameVariant, PlayerEliminated, RebuyConfig, RebuyDenied,
    RebuyProcessed, RegistrationClosed, RegistrationOpened, TournamentCompleted,
    TournamentCreated, TournamentEnrollmentRejected, TournamentPaused, TournamentPlayerEnrolled,
    TournamentResumed, TournamentState as ProtoTournamentState, TournamentStatus,
};

/// Player registration record.
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

/// Tournament aggregate state rebuilt from events.
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
    pub registered_players: HashMap<String, PlayerRegistration>, // player_root_hex -> registration
    pub players_remaining: i32,
    pub total_prize_pool: i64,
}

impl TournamentState {
    /// Check if the tournament exists.
    pub fn exists(&self) -> bool {
        !self.tournament_id.is_empty()
    }

    /// Check if registration is open.
    pub fn is_registration_open(&self) -> bool {
        self.status == TournamentStatus::TournamentRegistrationOpen
    }

    /// Check if tournament is running.
    pub fn is_running(&self) -> bool {
        self.status == TournamentStatus::TournamentRunning
    }

    /// Check if tournament has capacity for more players.
    pub fn has_capacity(&self) -> bool {
        (self.registered_players.len() as i32) < self.max_players
    }

    /// Check if a player is registered.
    pub fn is_player_registered(&self, player_root_hex: &str) -> bool {
        self.registered_players.contains_key(player_root_hex)
    }

    /// Check if rebuy is allowed for a player.
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

        // Check level cutoff
        if rebuy_config.rebuy_level_cutoff > 0 && self.current_level > rebuy_config.rebuy_level_cutoff
        {
            return false;
        }

        // Check max rebuys
        if let Some(registration) = self.registered_players.get(player_root_hex) {
            if rebuy_config.max_rebuys > 0 && registration.rebuys_used >= rebuy_config.max_rebuys {
                return false;
            }
        }

        true
    }
}

// Event applier functions

fn apply_created(state: &mut TournamentState, event: TournamentCreated) {
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

fn apply_registration_opened(state: &mut TournamentState, _event: RegistrationOpened) {
    state.status = TournamentStatus::TournamentRegistrationOpen;
}

fn apply_registration_closed(state: &mut TournamentState, _event: RegistrationClosed) {
    // Status will change to Running when tournament starts
}

fn apply_player_enrolled(state: &mut TournamentState, event: TournamentPlayerEnrolled) {
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

fn apply_enrollment_rejected(_state: &mut TournamentState, _event: TournamentEnrollmentRejected) {
    // No state change - just an event for the player
}

fn apply_rebuy_processed(state: &mut TournamentState, event: RebuyProcessed) {
    let player_root_hex = hex::encode(&event.player_root);
    if let Some(registration) = state.registered_players.get_mut(&player_root_hex) {
        registration.rebuys_used = event.rebuy_count;
    }
    state.total_prize_pool += event.rebuy_cost;
}

fn apply_rebuy_denied(_state: &mut TournamentState, _event: RebuyDenied) {
    // No state change
}

fn apply_blind_advanced(state: &mut TournamentState, event: BlindLevelAdvanced) {
    state.current_level = event.level;
}

fn apply_player_eliminated(state: &mut TournamentState, event: PlayerEliminated) {
    let player_root_hex = hex::encode(&event.player_root);
    state.registered_players.remove(&player_root_hex);
    state.players_remaining = state.registered_players.len() as i32;
}

fn apply_paused(state: &mut TournamentState, _event: TournamentPaused) {
    state.status = TournamentStatus::TournamentPaused;
}

fn apply_resumed(state: &mut TournamentState, _event: TournamentResumed) {
    state.status = TournamentStatus::TournamentRunning;
}

fn apply_completed(state: &mut TournamentState, _event: TournamentCompleted) {
    state.status = TournamentStatus::TournamentCompleted;
}

/// StateRouter for tournament state reconstruction.
pub static STATE_ROUTER: LazyLock<StateRouter<TournamentState>> = LazyLock::new(|| {
    StateRouter::new()
        .on::<TournamentCreated>(apply_created)
        .on::<RegistrationOpened>(apply_registration_opened)
        .on::<RegistrationClosed>(apply_registration_closed)
        .on::<TournamentPlayerEnrolled>(apply_player_enrolled)
        .on::<TournamentEnrollmentRejected>(apply_enrollment_rejected)
        .on::<RebuyProcessed>(apply_rebuy_processed)
        .on::<RebuyDenied>(apply_rebuy_denied)
        .on::<BlindLevelAdvanced>(apply_blind_advanced)
        .on::<PlayerEliminated>(apply_player_eliminated)
        .on::<TournamentPaused>(apply_paused)
        .on::<TournamentResumed>(apply_resumed)
        .on::<TournamentCompleted>(apply_completed)
});

/// Rebuild tournament state from event history.
pub fn rebuild_state(event_book: &EventBook) -> TournamentState {
    // Start from snapshot if available
    if let Some(snapshot) = &event_book.snapshot {
        if let Some(snapshot_any) = &snapshot.state {
            if let Ok(proto_state) = snapshot_any.unpack::<ProtoTournamentState>() {
                let mut state = apply_snapshot(&proto_state);
                // Apply events since snapshot
                for page in &event_book.pages {
                    if let Some(Payload::Event(event)) = &page.payload {
                        STATE_ROUTER.apply_single(&mut state, event);
                    }
                }
                return state;
            }
        }
    }

    STATE_ROUTER.with_event_book(event_book)
}

fn apply_snapshot(snapshot: &ProtoTournamentState) -> TournamentState {
    let mut registered_players = HashMap::new();
    for (key, proto_reg) in &snapshot.registered_players {
        registered_players.insert(
            key.clone(),
            PlayerRegistration {
                player_root: proto_reg.player_root.clone(),
                fee_paid: proto_reg.fee_paid,
                starting_stack: proto_reg.starting_stack,
                rebuys_used: proto_reg.rebuys_used,
                addon_taken: proto_reg.addon_taken,
                table_assignment: proto_reg.table_assignment,
                seat_assignment: proto_reg.seat_assignment,
            },
        );
    }

    TournamentState {
        tournament_id: snapshot.tournament_id.clone(),
        name: snapshot.name.clone(),
        game_variant: GameVariant::try_from(snapshot.game_variant).unwrap_or_default(),
        status: TournamentStatus::try_from(snapshot.status).unwrap_or_default(),
        buy_in: snapshot.buy_in,
        starting_stack: snapshot.starting_stack,
        max_players: snapshot.max_players,
        min_players: snapshot.min_players,
        rebuy_config: snapshot.rebuy_config.clone(),
        blind_structure: snapshot.blind_structure.clone(),
        current_level: snapshot.current_level,
        registered_players,
        players_remaining: snapshot.players_remaining,
        total_prize_pool: snapshot.total_prize_pool,
    }
}
