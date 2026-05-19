//! Tournament aggregate library.

pub mod errors;
pub mod handlers;
pub mod state;

pub use state::TournamentState;

use angzarr_client::proto::EventBook;
use angzarr_client::{command_handler, CommandResult};
use examples_proto::{
    AbsentBlindAdvanced, AdvanceAbsentBlind, AdvanceBlindLevel, AwardBounty, BagAndTag,
    BagAndTagComplete, BlindLevelAdvanced, BountyAwarded, CloseRegistration, ColorUp,
    ColorUpCompleted, CompleteTournament, CreateTournament, DecrementPenalty, DetectNoShow,
    DisqualifyPlayer, EliminatePlayer, EnrollPlayer, EnterHandForHand, HandForHandEnded,
    HandForHandHandRecorded, HandForHandRoundComplete, HandForHandStarted, IssuePenalty,
    MixedGameVariantRotated, NewHandsHalted, NoShowDetected, OpenRegistration, PauseTournament,
    PenaltyIssued, PenaltyRoundsDecremented, PlayerDisqualified, PlayerEliminated,
    PlayerMovedBetweenTables, PlayerMovedTables, PlayerReEntered, ProcessRebuy, ReEntryPlayer,
    RebalanceTables, RebuyDenied, RebuyProcessed, RecordHandForHandHand,
    RecordHandForHandRoundComplete, RecordSimultaneousBusts, RecordTableHandComplete,
    RegistrationClosed, RegistrationOpened, ReseatAbsentPlayer, ResumeTournament,
    RotateMixedGameVariant, SeatRedrawTriggered, SimultaneousBustsRecorded, StartTournament,
    StopNewHands, TournamentCompleted, TournamentCreated, TournamentEnrollmentRejected,
    TournamentPaused, TournamentPlayerEnrolled, TournamentResumed, TournamentStarted,
    TriggerSeatRedraw,
};

use crate::state::{
    apply_bag_and_tag_complete, apply_blind_advanced, apply_bounty_awarded,
    apply_color_up_completed, apply_completed, apply_created, apply_enrollment_rejected,
    apply_hand_for_hand_ended, apply_hand_for_hand_round_complete, apply_hand_for_hand_started,
    apply_mixed_game_variant_rotated, apply_new_hands_halted, apply_no_show_detected, apply_paused,
    apply_penalty_issued, apply_penalty_rounds_decremented, apply_player_disqualified,
    apply_player_eliminated, apply_player_enrolled, apply_player_moved_tables,
    apply_player_re_entered, apply_rebuy_denied, apply_rebuy_processed, apply_registration_closed,
    apply_registration_opened, apply_resumed, apply_started,
};

pub struct TournamentAggregate;

#[command_handler(domain = "tournament", state = TournamentState)]
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

    #[handles(StartTournament)]
    fn on_start(
        &self,
        cmd: StartTournament,
        state: &TournamentState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        handlers::handle_start_tournament(cmd, state, seq)
    }

    #[handles(CompleteTournament)]
    fn on_complete(
        &self,
        cmd: CompleteTournament,
        state: &TournamentState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        handlers::handle_complete_tournament(cmd, state, seq)
    }

    // --- Advanced (TDA/WSOP) command handlers ---

    #[handles(ColorUp)]
    fn on_color_up(
        &self,
        cmd: ColorUp,
        state: &TournamentState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        handlers::handle_color_up(cmd, state, seq)
    }

    #[handles(RebalanceTables)]
    fn on_rebalance_tables(
        &self,
        cmd: RebalanceTables,
        state: &TournamentState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        handlers::handle_rebalance_tables(cmd, state, seq)
    }

    #[handles(EnterHandForHand)]
    fn on_enter_hand_for_hand(
        &self,
        cmd: EnterHandForHand,
        state: &TournamentState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        handlers::handle_enter_hand_for_hand(cmd, state, seq)
    }

    #[handles(RecordTableHandComplete)]
    fn on_record_table_hand_complete(
        &self,
        cmd: RecordTableHandComplete,
        state: &TournamentState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        handlers::handle_record_table_hand_complete(cmd, state, seq)
    }

    #[handles(RecordHandForHandRoundComplete)]
    fn on_record_h4h_round_complete(
        &self,
        cmd: RecordHandForHandRoundComplete,
        state: &TournamentState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        handlers::handle_record_round_complete(cmd, state, seq)
    }

    #[handles(RecordHandForHandHand)]
    fn on_record_h4h_hand(
        &self,
        cmd: RecordHandForHandHand,
        state: &TournamentState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        handlers::handle_record_h4h_hand(cmd, state, seq)
    }

    #[handles(RecordSimultaneousBusts)]
    fn on_record_simultaneous_busts(
        &self,
        cmd: RecordSimultaneousBusts,
        state: &TournamentState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        handlers::handle_record_simultaneous_busts(cmd, state, seq)
    }

    #[handles(TriggerSeatRedraw)]
    fn on_trigger_seat_redraw(
        &self,
        cmd: TriggerSeatRedraw,
        state: &TournamentState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        handlers::handle_trigger_seat_redraw(cmd, state, seq)
    }

    #[handles(ReseatAbsentPlayer)]
    fn on_reseat_absent_player(
        &self,
        cmd: ReseatAbsentPlayer,
        state: &TournamentState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        handlers::handle_reseat_absent_player(cmd, state, seq)
    }

    #[handles(ReEntryPlayer)]
    fn on_re_entry_player(
        &self,
        cmd: ReEntryPlayer,
        state: &TournamentState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        handlers::handle_re_entry_player(cmd, state, seq)
    }

    #[handles(AdvanceAbsentBlind)]
    fn on_advance_absent_blind(
        &self,
        cmd: AdvanceAbsentBlind,
        state: &TournamentState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        handlers::handle_advance_absent_blind(cmd, state, seq)
    }

    #[handles(RotateMixedGameVariant)]
    fn on_rotate_mixed_game_variant(
        &self,
        cmd: RotateMixedGameVariant,
        state: &TournamentState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        handlers::handle_rotate_mixed_game_variant(cmd, state, seq)
    }

    #[handles(StopNewHands)]
    fn on_stop_new_hands(
        &self,
        cmd: StopNewHands,
        state: &TournamentState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        handlers::handle_stop_new_hands(cmd, state, seq)
    }

    #[handles(BagAndTag)]
    fn on_bag_and_tag(
        &self,
        cmd: BagAndTag,
        state: &TournamentState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        handlers::handle_bag_and_tag(cmd, state, seq)
    }

    #[handles(IssuePenalty)]
    fn on_issue_penalty(
        &self,
        cmd: IssuePenalty,
        state: &TournamentState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        handlers::handle_issue_penalty(cmd, state, seq)
    }

    #[handles(DecrementPenalty)]
    fn on_decrement_penalty(
        &self,
        cmd: DecrementPenalty,
        state: &TournamentState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        handlers::handle_decrement_penalty(cmd, state, seq)
    }

    #[handles(DisqualifyPlayer)]
    fn on_disqualify_player(
        &self,
        cmd: DisqualifyPlayer,
        state: &TournamentState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        handlers::handle_disqualify_player(cmd, state, seq)
    }

    #[handles(AwardBounty)]
    fn on_award_bounty(
        &self,
        cmd: AwardBounty,
        state: &TournamentState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        handlers::handle_award_bounty(cmd, state, seq)
    }

    #[handles(DetectNoShow)]
    fn on_detect_no_show(
        &self,
        cmd: DetectNoShow,
        state: &TournamentState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        handlers::handle_detect_no_show(cmd, state, seq)
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
    fn apply_enrollment_rejected(state: &mut TournamentState, event: TournamentEnrollmentRejected) {
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

    #[applies(TournamentStarted)]
    fn apply_started(state: &mut TournamentState, event: TournamentStarted) {
        apply_started(state, event);
    }

    // --- Advanced event appliers ---

    #[applies(ColorUpCompleted)]
    fn apply_color_up_completed(state: &mut TournamentState, event: ColorUpCompleted) {
        apply_color_up_completed(state, event);
    }

    #[applies(PlayerMovedBetweenTables)]
    fn apply_player_moved_between_tables(
        _state: &mut TournamentState,
        _event: PlayerMovedBetweenTables,
    ) {
        // No-op at the tournament aggregate; saga performs per-table
        // chip motion. Captured here so the event is replayable
        // without triggering "unknown event" warnings.
    }

    #[applies(HandForHandStarted)]
    fn apply_hand_for_hand_started(state: &mut TournamentState, event: HandForHandStarted) {
        apply_hand_for_hand_started(state, event);
    }

    #[applies(HandForHandRoundComplete)]
    fn apply_hand_for_hand_round_complete(
        state: &mut TournamentState,
        event: HandForHandRoundComplete,
    ) {
        apply_hand_for_hand_round_complete(state, event);
    }

    #[applies(HandForHandHandRecorded)]
    fn apply_hand_for_hand_hand_recorded(
        _state: &mut TournamentState,
        _event: HandForHandHandRecorded,
    ) {
        // No-op state change — the recorded event is consumed by the
        // tournament-clock projector, not the aggregate.
    }

    #[applies(HandForHandEnded)]
    fn apply_hand_for_hand_ended(state: &mut TournamentState, event: HandForHandEnded) {
        apply_hand_for_hand_ended(state, event);
    }

    #[applies(SimultaneousBustsRecorded)]
    fn apply_simultaneous_busts_recorded(
        _state: &mut TournamentState,
        _event: SimultaneousBustsRecorded,
    ) {
        // Bust group is consulted by CompleteTournament payout
        // splitting; no aggregate state mutation required.
    }

    #[applies(SeatRedrawTriggered)]
    fn apply_seat_redraw_triggered(_state: &mut TournamentState, _event: SeatRedrawTriggered) {
        // Saga signal; aggregate has no state to update.
    }

    #[applies(PlayerMovedTables)]
    fn apply_player_moved_tables(state: &mut TournamentState, event: PlayerMovedTables) {
        // Two-purpose applier:
        //   - Saga-side rebalance fan-out (no aggregate state to update).
        //   - Per-table progress receipt for `RecordTableHandComplete`
        //     while in hand-for-hand: discard the reported table from
        //     `hand_for_hand_pending_tables` so the next command's
        //     state replay sees the prior table's removal. Matches the
        //     cpp tournament `apply_player_moved_tables`.
        apply_player_moved_tables(state, event);
    }

    #[applies(PlayerReEntered)]
    fn apply_player_re_entered(state: &mut TournamentState, event: PlayerReEntered) {
        apply_player_re_entered(state, event);
    }

    #[applies(AbsentBlindAdvanced)]
    fn apply_absent_blind_advanced(_state: &mut TournamentState, _event: AbsentBlindAdvanced) {
        // Per-table chip motion; aggregate has no per-stack ledger.
    }

    #[applies(MixedGameVariantRotated)]
    fn apply_mixed_game_variant_rotated(
        state: &mut TournamentState,
        event: MixedGameVariantRotated,
    ) {
        apply_mixed_game_variant_rotated(state, event);
    }

    #[applies(NewHandsHalted)]
    fn apply_new_hands_halted(state: &mut TournamentState, event: NewHandsHalted) {
        apply_new_hands_halted(state, event);
    }

    #[applies(BagAndTagComplete)]
    fn apply_bag_and_tag_complete(state: &mut TournamentState, event: BagAndTagComplete) {
        apply_bag_and_tag_complete(state, event);
    }

    #[applies(PenaltyIssued)]
    fn apply_penalty_issued(state: &mut TournamentState, event: PenaltyIssued) {
        apply_penalty_issued(state, event);
    }

    #[applies(PenaltyRoundsDecremented)]
    fn apply_penalty_rounds_decremented(
        state: &mut TournamentState,
        event: PenaltyRoundsDecremented,
    ) {
        apply_penalty_rounds_decremented(state, event);
    }

    #[applies(PlayerDisqualified)]
    fn apply_player_disqualified(state: &mut TournamentState, event: PlayerDisqualified) {
        apply_player_disqualified(state, event);
    }

    #[applies(BountyAwarded)]
    fn apply_bounty_awarded(state: &mut TournamentState, event: BountyAwarded) {
        apply_bounty_awarded(state, event);
    }

    #[applies(NoShowDetected)]
    fn apply_no_show_detected(state: &mut TournamentState, event: NoShowDetected) {
        apply_no_show_detected(state, event);
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
                ..Default::default()
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
                ..Default::default()
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
        TournamentAggregate::apply_registration_closed(&mut state_a, event);
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
        assert_eq!(
            state_a.registered_players.len(),
            state_b.registered_players.len()
        );
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
                .get(&hex::encode([0xaau8]))
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
        assert!(!state
            .registered_players
            .contains_key(&hex::encode([0xaau8])));
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
                ..Default::default()
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
                    ..Default::default()
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
        // Add a second blind level so advancing from level 1 → 2 has a
        // defined target. AdvanceBlindLevel rejects with
        // BLIND_STRUCTURE_EXHAUSTED past the declared structure.
        state.blind_structure.push(BlindLevel {
            level: 2,
            small_blind: 50,
            big_blind: 100,
            ante: 10,
            duration_minutes: 20,
        });
        let book = agg
            .on_advance_blind(
                AdvanceBlindLevel {
                    ..Default::default()
                },
                &state,
                5,
            )
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
