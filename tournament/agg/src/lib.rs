//! Tournament aggregate library.

pub mod handlers;
pub mod state;

pub use state::TournamentState;

use angzarr_client::proto::EventBook;
use angzarr_client::{aggregate, CommandResult};
use examples_proto::{
    AdvanceBlindLevel, BlindLevelAdvanced, CloseRegistration, CreateTournament, EliminatePlayer,
    EnrollPlayer, OpenRegistration, PauseTournament, PlayerEliminated, ProcessRebuy, RebuyDenied,
    RebuyProcessed, RegistrationClosed, RegistrationOpened, ResumeTournament, TournamentCompleted,
    TournamentCreated, TournamentEnrollmentRejected, TournamentPaused, TournamentPlayerEnrolled,
    TournamentResumed,
};

use crate::state::{
    apply_blind_advanced, apply_completed, apply_created, apply_enrollment_rejected, apply_paused,
    apply_player_eliminated, apply_player_enrolled, apply_rebuy_denied, apply_rebuy_processed,
    apply_registration_closed, apply_registration_opened, apply_resumed,
};

pub struct TournamentAggregate;

#[aggregate(domain = "tournament", state = TournamentState)]
impl TournamentAggregate {
    // --- Command handlers ---

    #[handles(CreateTournament)]
    fn on_create(
        &self,
        cmd: CreateTournament,
        state: &TournamentState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        handlers::handle_create_tournament(cmd, state, seq)
    }

    #[handles(OpenRegistration)]
    fn on_open_registration(
        &self,
        cmd: OpenRegistration,
        state: &TournamentState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        handlers::handle_open_registration(cmd, state, seq)
    }

    #[handles(CloseRegistration)]
    fn on_close_registration(
        &self,
        cmd: CloseRegistration,
        state: &TournamentState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        handlers::handle_close_registration(cmd, state, seq)
    }

    #[handles(EnrollPlayer)]
    fn on_enroll(
        &self,
        cmd: EnrollPlayer,
        state: &TournamentState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        handlers::handle_enroll_player(cmd, state, seq)
    }

    #[handles(ProcessRebuy)]
    fn on_rebuy(
        &self,
        cmd: ProcessRebuy,
        state: &TournamentState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        handlers::handle_process_rebuy(cmd, state, seq)
    }

    #[handles(AdvanceBlindLevel)]
    fn on_advance_blind(
        &self,
        cmd: AdvanceBlindLevel,
        state: &TournamentState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        handlers::handle_advance_blind_level(cmd, state, seq)
    }

    #[handles(EliminatePlayer)]
    fn on_eliminate(
        &self,
        cmd: EliminatePlayer,
        state: &TournamentState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        handlers::handle_eliminate_player(cmd, state, seq)
    }

    #[handles(PauseTournament)]
    fn on_pause(
        &self,
        cmd: PauseTournament,
        state: &TournamentState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        handlers::handle_pause_tournament(cmd, state, seq)
    }

    #[handles(ResumeTournament)]
    fn on_resume(
        &self,
        cmd: ResumeTournament,
        state: &TournamentState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        handlers::handle_resume_tournament(cmd, state, seq)
    }

    // --- Event appliers ---

    #[applies(TournamentCreated)]
    fn apply_created(state: &mut TournamentState, event: TournamentCreated) {
        apply_created(state, event);
    }

    #[applies(RegistrationOpened)]
    fn apply_registration_opened(state: &mut TournamentState, event: RegistrationOpened) {
        apply_registration_opened(state, event);
    }

    #[applies(RegistrationClosed)]
    fn apply_registration_closed(state: &mut TournamentState, event: RegistrationClosed) {
        apply_registration_closed(state, event);
    }

    #[applies(TournamentPlayerEnrolled)]
    fn apply_player_enrolled(state: &mut TournamentState, event: TournamentPlayerEnrolled) {
        apply_player_enrolled(state, event);
    }

    #[applies(TournamentEnrollmentRejected)]
    fn apply_enrollment_rejected(
        state: &mut TournamentState,
        event: TournamentEnrollmentRejected,
    ) {
        apply_enrollment_rejected(state, event);
    }

    #[applies(RebuyProcessed)]
    fn apply_rebuy_processed(state: &mut TournamentState, event: RebuyProcessed) {
        apply_rebuy_processed(state, event);
    }

    #[applies(RebuyDenied)]
    fn apply_rebuy_denied(state: &mut TournamentState, event: RebuyDenied) {
        apply_rebuy_denied(state, event);
    }

    #[applies(BlindLevelAdvanced)]
    fn apply_blind_advanced(state: &mut TournamentState, event: BlindLevelAdvanced) {
        apply_blind_advanced(state, event);
    }

    #[applies(PlayerEliminated)]
    fn apply_player_eliminated(state: &mut TournamentState, event: PlayerEliminated) {
        apply_player_eliminated(state, event);
    }

    #[applies(TournamentPaused)]
    fn apply_paused(state: &mut TournamentState, event: TournamentPaused) {
        apply_paused(state, event);
    }

    #[applies(TournamentResumed)]
    fn apply_resumed(state: &mut TournamentState, event: TournamentResumed) {
        apply_resumed(state, event);
    }

    #[applies(TournamentCompleted)]
    fn apply_completed(state: &mut TournamentState, event: TournamentCompleted) {
        apply_completed(state, event);
    }
}

#[cfg(test)]
mod applier_tests {
    //! Direct-call tests for each `#[applies]` method on `TournamentAggregate`.
    use super::*;
    use examples_proto::{BlindLevel, GameVariant, RebuyConfig, TournamentStatus};

    fn created_state() -> TournamentState {
        let mut s = TournamentState::default();
        apply_created(
            &mut s,
            TournamentCreated {
                name: "X".into(),
                game_variant: GameVariant::TexasHoldem as i32,
                buy_in: 500,
                starting_stack: 10_000,
                max_players: 4,
                min_players: 2,
                scheduled_start: None,
                rebuy_config: None,
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
        s
    }

    #[test]
    fn apply_created_delegates_to_state_fn() {
        let mut state = TournamentState::default();
        TournamentAggregate::apply_created(
            &mut state,
            TournamentCreated {
                name: "Test".into(),
                game_variant: GameVariant::TexasHoldem as i32,
                buy_in: 100,
                starting_stack: 5000,
                max_players: 2,
                min_players: 2,
                scheduled_start: None,
                rebuy_config: None,
                addon_config: None,
                blind_structure: vec![],
                created_at: None,
            },
        );
        assert_eq!(state.tournament_id, "tournament_Test");
        assert_eq!(state.current_level, 1);
        assert_eq!(state.status, TournamentStatus::TournamentCreated);
    }

    #[test]
    fn apply_registration_opened_delegates_to_state_fn() {
        let mut state = created_state();
        TournamentAggregate::apply_registration_opened(
            &mut state,
            RegistrationOpened { opened_at: None },
        );
        assert_eq!(state.status, TournamentStatus::TournamentRegistrationOpen);
    }

    #[test]
    fn apply_registration_closed_delegates_to_state_fn() {
        // apply_registration_closed is a no-op in state.rs; ensure the
        // aggregate's delegation call has identical observable behavior.
        let mut state_a = created_state();
        let mut state_b = state_a.clone();
        let event = RegistrationClosed {
            total_registrations: 0,
            closed_at: None,
        };
        TournamentAggregate::apply_registration_closed(&mut state_a, event.clone());
        apply_registration_closed(&mut state_b, event);
        assert_eq!(state_a.status, state_b.status);
        assert_eq!(state_a.total_prize_pool, state_b.total_prize_pool);
    }

    #[test]
    fn apply_player_enrolled_delegates_to_state_fn() {
        let mut state = created_state();
        TournamentAggregate::apply_player_enrolled(
            &mut state,
            TournamentPlayerEnrolled {
                player_root: vec![0xaa],
                reservation_id: vec![1, 2, 3],
                fee_paid: 500,
                starting_stack: 10_000,
                registration_number: 1,
                enrolled_at: None,
            },
        );
        assert_eq!(state.registered_players.len(), 1);
        assert_eq!(state.total_prize_pool, 500);
        assert_eq!(state.players_remaining, 1);
    }

    #[test]
    fn apply_enrollment_rejected_delegates_to_state_fn() {
        // No-op applier; compare identical behavior.
        let mut state_a = created_state();
        let mut state_b = state_a.clone();
        let event = TournamentEnrollmentRejected {
            player_root: vec![0x01],
            reservation_id: vec![],
            reason: "full".into(),
            rejected_at: None,
        };
        TournamentAggregate::apply_enrollment_rejected(&mut state_a, event.clone());
        apply_enrollment_rejected(&mut state_b, event);
        assert_eq!(state_a.registered_players.len(), state_b.registered_players.len());
        assert_eq!(state_a.total_prize_pool, state_b.total_prize_pool);
    }

    #[test]
    fn apply_rebuy_processed_delegates_to_state_fn() {
        let mut state = created_state();
        state.rebuy_config = Some(RebuyConfig {
            enabled: true,
            max_rebuys: 3,
            rebuy_level_cutoff: 10,
            stack_threshold: 5000,
            rebuy_cost: 100,
            rebuy_chips: 1000,
        });
        apply_player_enrolled(
            &mut state,
            TournamentPlayerEnrolled {
                player_root: vec![0xaa],
                reservation_id: vec![],
                fee_paid: 500,
                starting_stack: 10_000,
                registration_number: 1,
                enrolled_at: None,
            },
        );
        TournamentAggregate::apply_rebuy_processed(
            &mut state,
            RebuyProcessed {
                player_root: vec![0xaa],
                reservation_id: vec![],
                rebuy_cost: 100,
                chips_added: 1000,
                rebuy_count: 1,
                processed_at: None,
            },
        );
        assert_eq!(
            state
                .registered_players
                .get(&hex::encode(&[0xaau8]))
                .unwrap()
                .rebuys_used,
            1
        );
        assert_eq!(state.total_prize_pool, 600);
    }

    #[test]
    fn apply_rebuy_denied_delegates_to_state_fn() {
        // No-op applier; compare identical behavior.
        let mut state_a = created_state();
        let mut state_b = state_a.clone();
        let event = RebuyDenied {
            player_root: vec![0xaa],
            reservation_id: vec![],
            reason: "disabled".into(),
            denied_at: None,
        };
        TournamentAggregate::apply_rebuy_denied(&mut state_a, event.clone());
        apply_rebuy_denied(&mut state_b, event);
        assert_eq!(state_a.total_prize_pool, state_b.total_prize_pool);
    }

    #[test]
    fn apply_blind_advanced_delegates_to_state_fn() {
        let mut state = created_state();
        assert_eq!(state.current_level, 1);
        TournamentAggregate::apply_blind_advanced(
            &mut state,
            BlindLevelAdvanced {
                level: 5,
                small_blind: 100,
                big_blind: 200,
                ante: 25,
                advanced_at: None,
            },
        );
        assert_eq!(state.current_level, 5);
    }

    #[test]
    fn apply_player_eliminated_delegates_to_state_fn() {
        let mut state = created_state();
        apply_player_enrolled(
            &mut state,
            TournamentPlayerEnrolled {
                player_root: vec![0xaa],
                reservation_id: vec![],
                fee_paid: 500,
                starting_stack: 10_000,
                registration_number: 1,
                enrolled_at: None,
            },
        );
        apply_player_enrolled(
            &mut state,
            TournamentPlayerEnrolled {
                player_root: vec![0xbb],
                reservation_id: vec![],
                fee_paid: 500,
                starting_stack: 10_000,
                registration_number: 2,
                enrolled_at: None,
            },
        );
        assert_eq!(state.players_remaining, 2);
        TournamentAggregate::apply_player_eliminated(
            &mut state,
            PlayerEliminated {
                player_root: vec![0xaa],
                finish_position: 2,
                hand_root: vec![],
                payout: 0,
                eliminated_at: None,
            },
        );
        assert_eq!(state.players_remaining, 1);
        assert!(!state.registered_players.contains_key(&hex::encode(&[0xaau8])));
    }

    #[test]
    fn apply_paused_delegates_to_state_fn() {
        let mut state = created_state();
        state.status = TournamentStatus::TournamentRunning;
        TournamentAggregate::apply_paused(
            &mut state,
            TournamentPaused {
                reason: "break".into(),
                paused_at: None,
            },
        );
        assert_eq!(state.status, TournamentStatus::TournamentPaused);
    }

    #[test]
    fn apply_resumed_delegates_to_state_fn() {
        let mut state = created_state();
        state.status = TournamentStatus::TournamentPaused;
        TournamentAggregate::apply_resumed(&mut state, TournamentResumed { resumed_at: None });
        assert_eq!(state.status, TournamentStatus::TournamentRunning);
    }

    #[test]
    fn apply_completed_delegates_to_state_fn() {
        let mut state = created_state();
        state.status = TournamentStatus::TournamentRunning;
        TournamentAggregate::apply_completed(
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

#[cfg(test)]
mod handler_tests {
    //! Direct-call tests for each `#[handles]` method on `TournamentAggregate`.
    use super::*;
    use examples_proto::{BlindLevel, GameVariant, RebuyConfig, TournamentStatus};

    fn created_state() -> TournamentState {
        let mut s = TournamentState::default();
        apply_created(
            &mut s,
            TournamentCreated {
                name: "X".into(),
                game_variant: GameVariant::TexasHoldem as i32,
                buy_in: 500,
                starting_stack: 10_000,
                max_players: 4,
                min_players: 2,
                scheduled_start: None,
                rebuy_config: None,
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
        s
    }

    fn open_state() -> TournamentState {
        let mut s = created_state();
        apply_registration_opened(&mut s, RegistrationOpened { opened_at: None });
        s
    }

    #[test]
    fn on_create_delegates_to_handler() {
        let agg = TournamentAggregate;
        let state = TournamentState::default();
        let book = agg
            .on_create(
                CreateTournament {
                    name: "Test".into(),
                    game_variant: GameVariant::TexasHoldem as i32,
                    buy_in: 500,
                    starting_stack: 10_000,
                    max_players: 9,
                    min_players: 2,
                    scheduled_start: None,
                    rebuy_config: None,
                    addon_config: None,
                    blind_structure: vec![],
                },
                &state,
                0,
            )
            .expect("handler should succeed");
        assert_eq!(book.pages.len(), 1);
    }

    #[test]
    fn on_open_registration_delegates_to_handler() {
        let agg = TournamentAggregate;
        let state = created_state();
        let book = agg
            .on_open_registration(OpenRegistration {}, &state, 1)
            .expect("handler should succeed");
        assert_eq!(book.pages.len(), 1);
    }

    #[test]
    fn on_close_registration_delegates_to_handler() {
        let agg = TournamentAggregate;
        let state = open_state();
        let book = agg
            .on_close_registration(CloseRegistration {}, &state, 2)
            .expect("handler should succeed");
        assert_eq!(book.pages.len(), 1);
    }

    #[test]
    fn on_enroll_delegates_to_handler() {
        let agg = TournamentAggregate;
        let state = open_state();
        let book = agg
            .on_enroll(
                EnrollPlayer {
                    player_root: vec![1, 2],
                    reservation_id: vec![0x1a],
                },
                &state,
                3,
            )
            .expect("handler should succeed");
        assert_eq!(book.pages.len(), 1);
    }

    #[test]
    fn on_rebuy_delegates_to_handler() {
        let agg = TournamentAggregate;
        let mut state = created_state();
        state.rebuy_config = Some(RebuyConfig {
            enabled: true,
            max_rebuys: 3,
            rebuy_level_cutoff: 10,
            stack_threshold: 5000,
            rebuy_cost: 100,
            rebuy_chips: 1000,
        });
        state.status = TournamentStatus::TournamentRunning;
        apply_player_enrolled(
            &mut state,
            TournamentPlayerEnrolled {
                player_root: vec![0xaa],
                reservation_id: vec![],
                fee_paid: 500,
                starting_stack: 10_000,
                registration_number: 1,
                enrolled_at: None,
            },
        );
        let book = agg
            .on_rebuy(
                ProcessRebuy {
                    player_root: vec![0xaa],
                    reservation_id: vec![0x01],
                },
                &state,
                4,
            )
            .expect("handler should succeed");
        assert_eq!(book.pages.len(), 1);
    }

    #[test]
    fn on_advance_blind_delegates_to_handler() {
        let agg = TournamentAggregate;
        let mut state = created_state();
        state.status = TournamentStatus::TournamentRunning;
        let book = agg
            .on_advance_blind(AdvanceBlindLevel {}, &state, 5)
            .expect("handler should succeed");
        assert_eq!(book.pages.len(), 1);
    }

    #[test]
    fn on_eliminate_delegates_to_handler() {
        let agg = TournamentAggregate;
        let mut state = created_state();
        state.status = TournamentStatus::TournamentRunning;
        apply_player_enrolled(
            &mut state,
            TournamentPlayerEnrolled {
                player_root: vec![0xaa],
                reservation_id: vec![],
                fee_paid: 500,
                starting_stack: 10_000,
                registration_number: 1,
                enrolled_at: None,
            },
        );
        let book = agg
            .on_eliminate(
                EliminatePlayer {
                    player_root: vec![0xaa],
                    hand_root: vec![0x11],
                },
                &state,
                6,
            )
            .expect("handler should succeed");
        assert_eq!(book.pages.len(), 1);
    }

    #[test]
    fn on_pause_delegates_to_handler() {
        let agg = TournamentAggregate;
        let mut state = created_state();
        state.status = TournamentStatus::TournamentRunning;
        let book = agg
            .on_pause(
                PauseTournament {
                    reason: "break".into(),
                },
                &state,
                7,
            )
            .expect("handler should succeed");
        assert_eq!(book.pages.len(), 1);
    }

    #[test]
    fn on_resume_delegates_to_handler() {
        let agg = TournamentAggregate;
        let mut state = created_state();
        state.status = TournamentStatus::TournamentPaused;
        let book = agg
            .on_resume(ResumeTournament {}, &state, 8)
            .expect("handler should succeed");
        assert_eq!(book.pages.len(), 1);
    }
}
