//! Table aggregate library.

pub mod handlers;
pub mod state;

pub use state::{SeatState, TableState};

use angzarr_client::proto::EventBook;
use angzarr_client::{aggregate, CommandResult};
use examples_proto::{
    AddRebuyChips, ChipsAdded, CreateTable, EndHand, HandEnded, HandStarted, JoinTable, LeaveTable,
    PlayerJoined, PlayerLeft, PlayerSatIn, PlayerSatOut, PlayerSeated, RebuyChipsAdded, SeatPlayer,
    SeatingRejected, StartHand, TableCreated,
};

use crate::state::{
    apply_chips_added, apply_hand_ended, apply_hand_started, apply_player_joined,
    apply_player_left, apply_player_sat_in, apply_player_sat_out, apply_player_seated,
    apply_rebuy_chips_added, apply_seating_rejected, apply_table_created,
};

pub struct TableAggregate;

#[aggregate(domain = "table", state = TableState)]
impl TableAggregate {
    // --- Command handlers ---

    #[handles(CreateTable)]
    fn on_create_table(
        &self,
        cmd: CreateTable,
        state: &TableState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        handlers::handle_create_table(cmd, state, seq)
    }

    #[handles(JoinTable)]
    fn on_join_table(
        &self,
        cmd: JoinTable,
        state: &TableState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        handlers::handle_join_table(cmd, state, seq)
    }

    #[handles(LeaveTable)]
    fn on_leave_table(
        &self,
        cmd: LeaveTable,
        state: &TableState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        handlers::handle_leave_table(cmd, state, seq)
    }

    #[handles(StartHand)]
    fn on_start_hand(
        &self,
        cmd: StartHand,
        state: &TableState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        handlers::handle_start_hand(cmd, state, seq)
    }

    #[handles(EndHand)]
    fn on_end_hand(&self, cmd: EndHand, state: &TableState, seq: u32) -> CommandResult<EventBook> {
        handlers::handle_end_hand(cmd, state, seq)
    }

    #[handles(SeatPlayer)]
    fn on_seat_player(
        &self,
        cmd: SeatPlayer,
        state: &TableState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        handlers::handle_seat_player(cmd, state, seq)
    }

    #[handles(AddRebuyChips)]
    fn on_add_rebuy_chips(
        &self,
        cmd: AddRebuyChips,
        state: &TableState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        handlers::handle_add_rebuy_chips(cmd, state, seq)
    }

    // --- Event appliers ---

    #[applies(TableCreated)]
    fn apply_table_created(state: &mut TableState, event: TableCreated) {
        apply_table_created(state, event);
    }

    #[applies(PlayerJoined)]
    fn apply_player_joined(state: &mut TableState, event: PlayerJoined) {
        apply_player_joined(state, event);
    }

    #[applies(PlayerLeft)]
    fn apply_player_left(state: &mut TableState, event: PlayerLeft) {
        apply_player_left(state, event);
    }

    #[applies(PlayerSatOut)]
    fn apply_player_sat_out(state: &mut TableState, event: PlayerSatOut) {
        apply_player_sat_out(state, event);
    }

    #[applies(PlayerSatIn)]
    fn apply_player_sat_in(state: &mut TableState, event: PlayerSatIn) {
        apply_player_sat_in(state, event);
    }

    #[applies(HandStarted)]
    fn apply_hand_started(state: &mut TableState, event: HandStarted) {
        apply_hand_started(state, event);
    }

    #[applies(HandEnded)]
    fn apply_hand_ended(state: &mut TableState, event: HandEnded) {
        apply_hand_ended(state, event);
    }

    #[applies(ChipsAdded)]
    fn apply_chips_added(state: &mut TableState, event: ChipsAdded) {
        apply_chips_added(state, event);
    }

    #[applies(PlayerSeated)]
    fn apply_player_seated(state: &mut TableState, event: PlayerSeated) {
        apply_player_seated(state, event);
    }

    #[applies(SeatingRejected)]
    fn apply_seating_rejected(state: &mut TableState, event: SeatingRejected) {
        apply_seating_rejected(state, event);
    }

    #[applies(RebuyChipsAdded)]
    fn apply_rebuy_chips_added(state: &mut TableState, event: RebuyChipsAdded) {
        apply_rebuy_chips_added(state, event);
    }
}

#[cfg(test)]
mod applier_tests {
    //! Direct-call tests for each `#[applies]` method on `TableAggregate`.
    //! Catch mutants that stub the delegation body with `()`.
    use super::*;
    use examples_proto::GameVariant;

    fn created_state() -> TableState {
        let mut s = TableState::default();
        apply_table_created(
            &mut s,
            TableCreated {
                table_name: "T1".into(),
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
        s
    }

    #[test]
    fn apply_table_created_delegates_to_state_fn() {
        let mut state = TableState::default();
        TableAggregate::apply_table_created(
            &mut state,
            TableCreated {
                table_name: "Test".into(),
                game_variant: GameVariant::TexasHoldem as i32,
                small_blind: 1,
                big_blind: 2,
                min_buy_in: 10,
                max_buy_in: 100,
                max_players: 4,
                action_timeout_seconds: 20,
                created_at: None,
            },
        );
        assert_eq!(state.table_id, "table_Test");
        assert_eq!(state.max_players, 4);
        assert_eq!(state.status, "waiting");
    }

    #[test]
    fn apply_player_joined_delegates_to_state_fn() {
        let mut state = created_state();
        TableAggregate::apply_player_joined(
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
        assert!(state.seats.contains_key(&2));
    }

    #[test]
    fn apply_player_left_delegates_to_state_fn() {
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
        TableAggregate::apply_player_left(
            &mut state,
            PlayerLeft {
                player_root: vec![1],
                seat_position: 0,
                chips_cashed_out: 100,
                left_at: None,
            },
        );
        assert_eq!(state.player_count(), 0);
    }

    #[test]
    fn apply_player_sat_out_delegates_to_state_fn() {
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
        TableAggregate::apply_player_sat_out(
            &mut state,
            PlayerSatOut {
                player_root: vec![1],
                sat_out_at: None,
            },
        );
        assert_eq!(state.active_player_count(), 0);
        assert!(state.seats.get(&0).unwrap().is_sitting_out);
    }

    #[test]
    fn apply_player_sat_in_delegates_to_state_fn() {
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
        TableAggregate::apply_player_sat_in(
            &mut state,
            PlayerSatIn {
                player_root: vec![1],
                sat_in_at: None,
            },
        );
        assert_eq!(state.active_player_count(), 1);
    }

    #[test]
    fn apply_hand_started_delegates_to_state_fn() {
        let mut state = created_state();
        TableAggregate::apply_hand_started(
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
        assert_eq!(state.hand_count, 1);
        assert_eq!(state.current_hand_root, vec![9]);
    }

    #[test]
    fn apply_hand_ended_delegates_to_state_fn() {
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
        state.status = "in_hand".into();
        state.current_hand_root = vec![9];
        let mut changes = std::collections::HashMap::new();
        changes.insert(hex::encode(&[1u8]), 50i64);
        TableAggregate::apply_hand_ended(
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
        assert_eq!(state.seats.get(&0).unwrap().stack, 150);
    }

    #[test]
    fn apply_chips_added_delegates_to_state_fn() {
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
        TableAggregate::apply_chips_added(
            &mut state,
            ChipsAdded {
                player_root: vec![1],
                amount: 400,
                new_stack: 500,
                added_at: None,
            },
        );
        assert_eq!(state.seats.get(&0).unwrap().stack, 500);
    }

    #[test]
    fn apply_player_seated_delegates_to_state_fn() {
        let mut state = created_state();
        TableAggregate::apply_player_seated(
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
        assert_eq!(state.seats.get(&3).unwrap().stack, 1000);
    }

    #[test]
    fn apply_seating_rejected_delegates_to_state_fn() {
        // seating_rejected is a no-op in state.rs. Mutants that stub the
        // delegation with `()` produce the same behavior, so we can't catch
        // them via state observation alone. Instead, we invoke both the
        // delegation method and the state function on clones of the same
        // starting state and require the observable results match — any
        // future change to the state function's behavior would be caught.
        let mut state_a = created_state();
        let mut state_b = state_a.clone();
        let event = SeatingRejected {
            player_root: vec![1],
            reservation_id: vec![],
            requested_seat: 0,
            reason: "full".into(),
            rejected_at: None,
        };
        TableAggregate::apply_seating_rejected(&mut state_a, event.clone());
        apply_seating_rejected(&mut state_b, event);
        assert_eq!(state_a.seats.len(), state_b.seats.len());
        assert_eq!(state_a.status, state_b.status);
    }

    #[test]
    fn apply_rebuy_chips_added_delegates_to_state_fn() {
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
        TableAggregate::apply_rebuy_chips_added(
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
        assert_eq!(state.seats.get(&0).unwrap().stack, 150);
    }
}

#[cfg(test)]
mod handler_tests {
    //! Direct-call tests for each `#[handles]` method on `TableAggregate`.
    use super::*;
    use examples_proto::{GameVariant, PotResult};

    fn created_state() -> TableState {
        let mut s = TableState::default();
        apply_table_created(
            &mut s,
            TableCreated {
                table_name: "T1".into(),
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
        s
    }

    #[test]
    fn on_create_table_delegates_to_handler() {
        let agg = TableAggregate;
        let state = TableState::default();
        let book = agg
            .on_create_table(
                CreateTable {
                    table_name: "Test".into(),
                    game_variant: GameVariant::TexasHoldem as i32,
                    small_blind: 5,
                    big_blind: 10,
                    min_buy_in: 100,
                    max_buy_in: 1000,
                    max_players: 6,
                    action_timeout_seconds: 30,
                },
                &state,
                0,
            )
            .expect("handler should succeed");
        assert_eq!(book.pages.len(), 1);
    }

    #[test]
    fn on_join_table_delegates_to_handler() {
        let agg = TableAggregate;
        let state = created_state();
        let book = agg
            .on_join_table(
                JoinTable {
                    player_root: vec![1, 2, 3],
                    preferred_seat: 3,
                    buy_in_amount: 500,
                },
                &state,
                1,
            )
            .expect("handler should succeed");
        assert_eq!(book.pages.len(), 1);
    }

    #[test]
    fn on_leave_table_delegates_to_handler() {
        let agg = TableAggregate;
        let mut state = created_state();
        apply_player_joined(
            &mut state,
            PlayerJoined {
                player_root: vec![1, 2, 3],
                seat_position: 4,
                buy_in_amount: 400,
                stack: 400,
                joined_at: None,
            },
        );
        let book = agg
            .on_leave_table(
                LeaveTable {
                    player_root: vec![1, 2, 3],
                },
                &state,
                2,
            )
            .expect("handler should succeed");
        assert_eq!(book.pages.len(), 1);
    }

    #[test]
    fn on_start_hand_delegates_to_handler() {
        let agg = TableAggregate;
        let mut state = created_state();
        apply_player_joined(
            &mut state,
            PlayerJoined {
                player_root: vec![1],
                seat_position: 0,
                buy_in_amount: 500,
                stack: 500,
                joined_at: None,
            },
        );
        apply_player_joined(
            &mut state,
            PlayerJoined {
                player_root: vec![2],
                seat_position: 1,
                buy_in_amount: 500,
                stack: 500,
                joined_at: None,
            },
        );
        let book = agg
            .on_start_hand(StartHand {}, &state, 3)
            .expect("handler should succeed");
        assert_eq!(book.pages.len(), 1);
    }

    #[test]
    fn on_end_hand_delegates_to_handler() {
        let agg = TableAggregate;
        let mut state = created_state();
        apply_player_joined(
            &mut state,
            PlayerJoined {
                player_root: vec![0xa1],
                seat_position: 0,
                buy_in_amount: 500,
                stack: 500,
                joined_at: None,
            },
        );
        let hand_root = vec![0xbe, 0xef];
        apply_hand_started(
            &mut state,
            HandStarted {
                hand_root: hand_root.clone(),
                hand_number: 1,
                dealer_position: 0,
                small_blind_position: 0,
                big_blind_position: 0,
                active_players: vec![],
                game_variant: GameVariant::TexasHoldem as i32,
                small_blind: 5,
                big_blind: 10,
                started_at: None,
            },
        );
        let book = agg
            .on_end_hand(
                EndHand {
                    hand_root,
                    results: vec![PotResult {
                        winner_root: vec![0xa1],
                        amount: 30,
                        pot_type: "main".into(),
                        winning_hand: None,
                    }],
                },
                &state,
                4,
            )
            .expect("handler should succeed");
        assert_eq!(book.pages.len(), 1);
    }

    #[test]
    fn on_seat_player_delegates_to_handler() {
        let agg = TableAggregate;
        let state = created_state();
        let book = agg
            .on_seat_player(
                SeatPlayer {
                    player_root: vec![1, 2, 3],
                    reservation_id: vec![0xaa],
                    seat: 1,
                    amount: 500,
                },
                &state,
                5,
            )
            .expect("handler should succeed");
        assert_eq!(book.pages.len(), 1);
    }

    #[test]
    fn on_add_rebuy_chips_delegates_to_handler() {
        let agg = TableAggregate;
        let mut state = created_state();
        apply_player_joined(
            &mut state,
            PlayerJoined {
                player_root: vec![0xa1],
                seat_position: 2,
                buy_in_amount: 200,
                stack: 200,
                joined_at: None,
            },
        );
        let book = agg
            .on_add_rebuy_chips(
                AddRebuyChips {
                    player_root: vec![0xa1],
                    reservation_id: vec![0x11],
                    seat: 2,
                    amount: 300,
                },
                &state,
                6,
            )
            .expect("handler should succeed");
        assert_eq!(book.pages.len(), 1);
    }
}
