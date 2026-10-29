//! The table's logic behind one trait, [`TableLogic`]: one method per
//! command and per event, plain generated types in, events (or a
//! rejection) out, and no framework. A harness adapter (the
//! angzarr-embedded markup in `table-embedded`, or any other) delegates
//! each handler and applier to it, so the adapter can be regenerated when
//! the scaffold changes while this is written once.
//!
//! RED: [`Table`]'s methods are `todo!()` except those agg-table already
//! had (CreateTable and the TableCreated and ShoeShuffled appliers).

use examples_proto::v1::{
    BetPlaced, CardDealt, ConfirmSeat, CreateTable, DealRound, DealerPlayed, DoubleDown,
    HandDoubled, HandStood, Hit, PlaceBet, PlayerSeated, RequestSeat, RoundDealt, RoundSettled,
    SeatHeld, SeatNotConfirmable, ShoeShuffled, Stand, TableCreated, TableState, TurnStarted,
};

use crate::rejection::Rejection;
use crate::state::TableEvent;

/// What the table records once an action (or the deal) is applied: the
/// next decision point, or the dealer's play and the settlement once no
/// seat is left to act, or nothing (the step has already run).
#[derive(Debug, Clone, PartialEq)]
pub enum AfterAction {
    /// A seat is on turn: its TurnStarted (decision 14.38).
    Turn(TurnStarted),
    /// No seat is left to act: the dealer plays and the round settles.
    Settle(DealerPlayed, RoundSettled),
    /// Nothing more: re-running the step on its own result.
    Nothing,
}

/// The table's decisions and folds.
///
/// A decision reads the state before its events. Where a later event is
/// decided from the state after an earlier one, the decision is split:
/// DealRound decides [`deal_round`](TableLogic::deal_round) (the
/// reshuffle, if any), then [`deal`](TableLogic::deal) on the state after
/// it, then [`after_action`](TableLogic::after_action); Hit, Stand and
/// DoubleDown decide their event, then `after_action`. Each step is
/// idempotent on its own result: run again on the state after its events,
/// it decides nothing.
pub trait TableLogic {
    /// CreateTable: TableCreated, then ShoeShuffled for shoe 1.
    fn create_table(&self, cmd: &CreateTable) -> Result<(TableCreated, ShoeShuffled), Rejection>;
    /// RequestSeat: the hold, or nothing when this request already holds it.
    fn request_seat(&self, cmd: &RequestSeat) -> Result<Option<SeatHeld>, Rejection>;
    /// ConfirmSeat: the player seated, or nothing when this buy-in already
    /// seated them; refused with the typed SeatNotConfirmable.
    fn confirm_seat(&self, cmd: &ConfirmSeat) -> Result<Option<PlayerSeated>, SeatNotConfirmable>;
    /// PlaceBet: the bet.
    fn place_bet(&self, cmd: &PlaceBet) -> Result<BetPlaced, Rejection>;
    /// DealRound: whether it can be dealt, and the new shoe first when the
    /// round could run the old one out (AHR-4).
    fn deal_round(&self, cmd: &DealRound) -> Result<Option<ShoeShuffled>, Rejection>;
    /// DealRound's second step: the opening deal from the current shoe.
    fn deal(&self) -> Result<RoundDealt, Rejection>;
    /// Hit: the card drawn.
    fn hit(&self, cmd: &Hit) -> Result<CardDealt, Rejection>;
    /// Stand: the hand stood.
    fn stand(&self, cmd: &Stand) -> Result<HandStood, Rejection>;
    /// DoubleDown: the wager doubled and one card drawn.
    fn double_down(&self, cmd: &DoubleDown) -> Result<HandDoubled, Rejection>;
    /// The step after the deal and after every action.
    fn after_action(&self) -> AfterAction;

    fn apply_table_created(&mut self, event: &TableCreated);
    fn apply_shoe_shuffled(&mut self, event: &ShoeShuffled);
    fn apply_seat_held(&mut self, event: &SeatHeld);
    fn apply_player_seated(&mut self, event: &PlayerSeated);
    fn apply_bet_placed(&mut self, event: &BetPlaced);
    fn apply_round_dealt(&mut self, event: &RoundDealt);
    fn apply_turn_started(&mut self, event: &TurnStarted);
    fn apply_card_dealt(&mut self, event: &CardDealt);
    fn apply_hand_stood(&mut self, event: &HandStood);
    fn apply_hand_doubled(&mut self, event: &HandDoubled);
    fn apply_dealer_played(&mut self, event: &DealerPlayed);
    fn apply_round_settled(&mut self, event: &RoundSettled);
}

/// The table: the spec's `TableState`, and [`TableLogic`] over it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Table {
    state: TableState,
}

impl Table {
    /// The folded state.
    pub fn state(&self) -> &TableState {
        &self.state
    }
}

impl TableLogic for Table {
    fn create_table(&self, cmd: &CreateTable) -> Result<(TableCreated, ShoeShuffled), Rejection> {
        match crate::create_table(&self.state, cmd)?.as_slice() {
            [TableEvent::TableCreated(created), TableEvent::ShoeShuffled(shuffled)] => {
                Ok((created.clone(), shuffled.clone()))
            }
            other => unreachable!("create_table decides TableCreated, ShoeShuffled: {other:?}"),
        }
    }

    fn request_seat(&self, _cmd: &RequestSeat) -> Result<Option<SeatHeld>, Rejection> {
        todo!()
    }

    fn confirm_seat(&self, _cmd: &ConfirmSeat) -> Result<Option<PlayerSeated>, SeatNotConfirmable> {
        todo!()
    }

    fn place_bet(&self, _cmd: &PlaceBet) -> Result<BetPlaced, Rejection> {
        todo!()
    }

    fn deal_round(&self, _cmd: &DealRound) -> Result<Option<ShoeShuffled>, Rejection> {
        todo!()
    }

    fn deal(&self) -> Result<RoundDealt, Rejection> {
        todo!()
    }

    fn hit(&self, _cmd: &Hit) -> Result<CardDealt, Rejection> {
        todo!()
    }

    fn stand(&self, _cmd: &Stand) -> Result<HandStood, Rejection> {
        todo!()
    }

    fn double_down(&self, _cmd: &DoubleDown) -> Result<HandDoubled, Rejection> {
        todo!()
    }

    fn after_action(&self) -> AfterAction {
        todo!()
    }

    fn apply_table_created(&mut self, event: &TableCreated) {
        crate::apply_table_created(&mut self.state, event);
    }

    fn apply_shoe_shuffled(&mut self, event: &ShoeShuffled) {
        crate::apply_shoe_shuffled(&mut self.state, event);
    }

    fn apply_seat_held(&mut self, _event: &SeatHeld) {
        todo!()
    }

    fn apply_player_seated(&mut self, _event: &PlayerSeated) {
        todo!()
    }

    fn apply_bet_placed(&mut self, _event: &BetPlaced) {
        todo!()
    }

    fn apply_round_dealt(&mut self, _event: &RoundDealt) {
        todo!()
    }

    fn apply_turn_started(&mut self, _event: &TurnStarted) {
        todo!()
    }

    fn apply_card_dealt(&mut self, _event: &CardDealt) {
        todo!()
    }

    fn apply_hand_stood(&mut self, _event: &HandStood) {
        todo!()
    }

    fn apply_hand_doubled(&mut self, _event: &HandDoubled) {
        todo!()
    }

    fn apply_dealer_played(&mut self, _event: &DealerPlayed) {
        todo!()
    }

    fn apply_round_settled(&mut self, _event: &RoundSettled) {
        todo!()
    }
}
