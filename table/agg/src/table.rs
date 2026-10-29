//! The table's logic behind one trait, [`TableLogic`]: one method per
//! command and per event, plain generated types in, events (or a
//! rejection) out, and no framework. A harness adapter (the
//! angzarr-embedded markup in `table-embedded`, or any other) delegates
//! each handler and applier to it, so the adapter can be regenerated when
//! the scaffold changes while this is written once.
//!
//! [`Table`] implements it over the spec's `TableState`.

use examples_proto::v1::seat_outcome::Outcome;
use examples_proto::v1::table_state::Phase;
use examples_proto::v1::{
    BetPlaced, Card, CardDealt, ConfirmSeat, CreateTable, DealRound, DealerPlayed, DoubleDown,
    HandDoubled, HandStood, Hit, LeaveTable, PlaceBet, PlayerCashedOut, PlayerSeated, ReleaseSeat,
    RequestSeat, RoundDealt, RoundSettled, SeatHand, SeatHeld, SeatNotConfirmable, SeatOutcome,
    SeatReleased, ShoeShuffled, Stand, TableCreated, TableSeat, TableSeatHold, TableState,
    TurnStarted,
};

use crate::cashout::cashout_id;
use crate::hand::{value, Value};
use crate::rejection::{Rejection, RejectionCode};
use crate::shoe;
use crate::state::{table_exists, TableEvent};

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

/// The shoe ran out while the dealer drew in round `round`: a bug, never a
/// business answer, since a round never runs the shoe out (AHR-4:
/// DealRound replaces a shoe that could).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShoeRanOut {
    pub round: u32,
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
    /// ReleaseSeat: the hold freed, or nothing when it no longer exists.
    fn release_seat(&self, cmd: &ReleaseSeat) -> Option<SeatReleased>;
    /// LeaveTable: the whole stack cashed out, its id derived from
    /// `table_root` (the table's stream root) and the event's position.
    fn leave_table(
        &self,
        cmd: &LeaveTable,
        table_root: &[u8; 16],
    ) -> Result<PlayerCashedOut, Rejection>;
    /// PlaceBet: the bet.
    fn place_bet(&self, cmd: &PlaceBet) -> Result<BetPlaced, Rejection>;
    /// DealRound: whether it can be dealt, and the new shoe first when the
    /// round could run the old one out (AHR-4).
    fn deal_round(&self, cmd: &DealRound) -> Result<Option<ShoeShuffled>, Rejection>;
    /// DealRound's second step: the opening deal from the current shoe,
    /// or nothing once the round is dealt (re-run on its own result).
    fn deal(&self) -> Result<Option<RoundDealt>, Rejection>;
    /// Hit: the card drawn.
    fn hit(&self, cmd: &Hit) -> Result<CardDealt, Rejection>;
    /// Stand: the hand stood.
    fn stand(&self, cmd: &Stand) -> Result<HandStood, Rejection>;
    /// DoubleDown: the wager doubled and one card drawn.
    fn double_down(&self, cmd: &DoubleDown) -> Result<HandDoubled, Rejection>;
    /// The step after the deal and after every action; the dealer's
    /// play running the shoe out is a bug, [`ShoeRanOut`].
    fn after_action(&self) -> Result<AfterAction, ShoeRanOut>;

    fn apply_table_created(&mut self, event: &TableCreated);
    fn apply_shoe_shuffled(&mut self, event: &ShoeShuffled);
    fn apply_seat_held(&mut self, event: &SeatHeld);
    fn apply_seat_released(&mut self, event: &SeatReleased);
    fn apply_player_seated(&mut self, event: &PlayerSeated);
    fn apply_player_cashed_out(&mut self, event: &PlayerCashedOut);
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
    /// Whether the current decision point's TurnStarted is recorded: what
    /// makes [`TableLogic::after_action`] idempotent. Not in `TableState`:
    /// it is folded from TurnStarted, cleared by every event that moves the
    /// turn.
    turn_announced: bool,
    /// The events folded so far: the sequence the next one is recorded at,
    /// which the cash-out id is derived from.
    events: u64,
}

impl Table {
    /// The folded state.
    pub fn state(&self) -> &TableState {
        &self.state
    }

    fn exists(&self) -> Result<(), Rejection> {
        match table_exists(&self.state) {
            true => Ok(()),
            false => Err(refuse(
                RejectionCode::TableNotFound,
                "the table does not exist",
            )),
        }
    }

    /// The seats with a wager this round, ascending.
    fn wagered(&self) -> Vec<i32> {
        let mut seats: Vec<i32> = self
            .state
            .seated
            .iter()
            .filter(|(_, seat)| seat.wager > 0)
            .map(|(&n, _)| n)
            .collect();
        seats.sort_unstable();
        seats
    }

    fn seat_of(&self, player_root: &[u8]) -> Option<i32> {
        self.state
            .seated
            .iter()
            .find(|(_, seat)| seat.player_root[..] == *player_root)
            .map(|(&n, _)| n)
    }

    /// The seat on turn, if it is `seat`: a turn action's precondition.
    fn on_turn(&self, seat: i32) -> Result<&TableSeat, Rejection> {
        self.exists()?;
        if self.state.phase() != Phase::PlayerTurns || self.state.turn < 0 {
            return Err(refuse(
                RejectionCode::NoRoundInProgress,
                "no round is in progress",
            ));
        }
        if self.state.turn != seat {
            return Err(refuse(
                RejectionCode::NotYourTurn,
                format!("it is not seat {seat}'s turn"),
            ));
        }
        Ok(&self.state.seated[&seat])
    }

    /// The next wagered seat after `seat` whose hand is not finished, or -1.
    fn next_after(&self, seat: i32) -> i32 {
        self.wagered()
            .into_iter()
            .find(|n| *n > seat && !self.state.seated[n].finished)
            .unwrap_or(-1)
    }

    /// The card on top of the shoe.
    fn top(&self) -> Card {
        self.state.shoe[0]
    }

    fn turn_started(&self) -> TurnStarted {
        let seat_number = self.state.turn;
        let seat = &self.state.seated[&seat_number];
        let hand = seat.hand.clone().unwrap_or_default();
        TurnStarted {
            round: self.state.round,
            seat: seat_number,
            player_root: seat.player_root.clone(),
            can_double: hand.cards.len() == 2 && seat.stack >= seat.wager,
            hand: Some(hand),
            dealer_up: self.state.dealer_cards.first().copied(),
            stack: seat.stack,
            wager: seat.wager,
        }
    }

    /// The dealer's play (AHR-7, AHR-11) and the settlement (AHR-12).
    fn settle(&self) -> Result<(DealerPlayed, RoundSettled), ShoeRanOut> {
        let state = &self.state;
        let seats = self.wagered();
        let hand_of = |n: &i32| value(&state.seated[n].hand.clone().unwrap_or_default().cards);
        let mut dealer = state.dealer_cards.clone();
        let opening = value(&dealer);
        let live = seats.iter().any(|n| {
            let v = hand_of(n);
            !v.busted() && !v.blackjack
        });
        let mut drawn = Vec::new();
        if live && !opening.blackjack {
            let mut shoe = state.shoe.iter();
            while value(&dealer).total < 17 {
                let card = *shoe.next().ok_or(ShoeRanOut { round: state.round })?;
                dealer.push(card);
                drawn.push(card);
            }
        }
        let dealt = value(&dealer);
        let outcomes: Vec<SeatOutcome> = seats
            .iter()
            .map(|n| {
                let seat = &state.seated[n];
                let outcome = outcome(hand_of(n), dealt);
                let wager = seat.wager;
                let returned = match outcome {
                    Outcome::Lose | Outcome::Unspecified => 0,
                    Outcome::Push => wager,
                    Outcome::Win => 2 * wager,
                    Outcome::Blackjack => wager + wager * 3 / 2,
                };
                SeatOutcome {
                    seat: *n,
                    player_root: seat.player_root.clone(),
                    wager,
                    outcome: outcome.into(),
                    returned,
                    net: returned - wager,
                    stack_after: seat.stack + returned,
                }
            })
            .collect();
        let house_delta: i64 = outcomes.iter().map(|o| o.wager - o.returned).sum();
        Ok((
            DealerPlayed {
                round: state.round,
                hole: state.dealer_cards.get(1).copied(),
                drawn,
                total: dealt.total,
                soft: dealt.soft,
                busted: dealt.busted(),
                blackjack: dealt.blackjack,
            },
            RoundSettled {
                round: state.round,
                outcomes,
                house_delta,
                house_result_after: state.house_result + house_delta,
            },
        ))
    }
}

/// One seat's outcome against the dealer's final hand.
fn outcome(player: Value, dealer: Value) -> Outcome {
    if dealer.blackjack {
        return match player.blackjack {
            true => Outcome::Push,
            false => Outcome::Lose,
        };
    }
    if player.blackjack {
        Outcome::Blackjack
    } else if player.busted() {
        Outcome::Lose
    } else if dealer.busted() || player.total > dealer.total {
        Outcome::Win
    } else if player.total == dealer.total {
        Outcome::Push
    } else {
        Outcome::Lose
    }
}

fn refuse(code: RejectionCode, message: impl Into<String>) -> Rejection {
    Rejection::new(code, message)
}

/// The key `seat_holds` and `confirmed_buy_ins` use: lowercase hex.
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn seat_hand(seat: i32, cards: Vec<Card>) -> SeatHand {
    let v = value(&cards);
    SeatHand {
        seat,
        cards,
        total: v.total,
        soft: v.soft,
        blackjack: v.blackjack,
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

    fn request_seat(&self, cmd: &RequestSeat) -> Result<Option<SeatHeld>, Rejection> {
        self.exists()?;
        let key = hex(&cmd.request_id);
        if self.state.seat_holds.contains_key(&key) || self.state.confirmed_buy_ins.contains(&key) {
            return Ok(None);
        }
        let state = &self.state;
        if cmd.seat < 0 || cmd.seat >= state.seats {
            return Err(refuse(
                RejectionCode::SeatOutOfRange,
                format!("seat {} does not exist", cmd.seat),
            ));
        }
        if cmd.amount < state.min_buy_in || cmd.amount > state.max_buy_in {
            return Err(refuse(
                RejectionCode::BuyInOutOfRange,
                format!(
                    "the buy-in is outside {} to {}",
                    state.min_buy_in, state.max_buy_in
                ),
            ));
        }
        if self.seat_of(&cmd.player_root).is_some() {
            return Err(refuse(
                RejectionCode::PlayerAlreadySeated,
                "the player is already seated",
            ));
        }
        let held = state.seat_holds.values().any(|h| h.seat == cmd.seat);
        if held || state.seated.contains_key(&cmd.seat) {
            return Err(refuse(
                RejectionCode::SeatTaken,
                format!("seat {} is taken", cmd.seat),
            ));
        }
        Ok(Some(SeatHeld {
            buy_in_id: cmd.request_id.clone(),
            player_root: cmd.player_root.clone(),
            seat: cmd.seat,
            amount: cmd.amount,
        }))
    }

    fn confirm_seat(&self, cmd: &ConfirmSeat) -> Result<Option<PlayerSeated>, SeatNotConfirmable> {
        let key = hex(&cmd.buy_in_id);
        if self.state.confirmed_buy_ins.contains(&key) {
            return Ok(None);
        }
        let not_confirmable = |reason: &str| SeatNotConfirmable {
            reason: reason.to_string(),
        };
        let hold = self
            .state
            .seat_holds
            .get(&key)
            .ok_or_else(|| not_confirmable("SEAT_HOLD_NOT_FOUND"))?;
        if self.seat_of(&hold.player_root).is_some() {
            return Err(not_confirmable("PLAYER_ALREADY_SEATED"));
        }
        Ok(Some(PlayerSeated {
            buy_in_id: cmd.buy_in_id.clone(),
            player_root: hold.player_root.clone(),
            seat: hold.seat,
            stack: hold.amount,
        }))
    }

    fn release_seat(&self, cmd: &ReleaseSeat) -> Option<SeatReleased> {
        let hold = self.state.seat_holds.get(&hex(&cmd.buy_in_id))?;
        Some(SeatReleased {
            buy_in_id: cmd.buy_in_id.clone(),
            player_root: hold.player_root.clone(),
            seat: hold.seat,
            reason: cmd.reason.clone(),
        })
    }

    fn leave_table(
        &self,
        cmd: &LeaveTable,
        table_root: &[u8; 16],
    ) -> Result<PlayerCashedOut, Rejection> {
        self.exists()?;
        let seat = self.state.seated.get(&cmd.seat).ok_or_else(|| {
            refuse(
                RejectionCode::NotSeated,
                format!("seat {} is not occupied", cmd.seat),
            )
        })?;
        if seat.wager > 0 {
            return Err(refuse(
                RejectionCode::WagerInPlay,
                format!("seat {} has a wager in play", cmd.seat),
            ));
        }
        Ok(PlayerCashedOut {
            cashout_id: cashout_id(table_root, self.events).to_vec().into(),
            player_root: seat.player_root.clone(),
            seat: cmd.seat,
            amount: seat.stack,
        })
    }

    fn place_bet(&self, cmd: &PlaceBet) -> Result<BetPlaced, Rejection> {
        self.exists()?;
        let state = &self.state;
        if state.phase() == Phase::PlayerTurns {
            return Err(refuse(
                RejectionCode::RoundInProgress,
                "a round is in progress",
            ));
        }
        let seat = state.seated.get(&cmd.seat).ok_or_else(|| {
            refuse(
                RejectionCode::NotSeated,
                format!("seat {} is not occupied", cmd.seat),
            )
        })?;
        if cmd.amount < state.min_bet || cmd.amount > state.max_bet {
            return Err(refuse(
                RejectionCode::BetOutOfRange,
                format!("the bet is outside {} to {}", state.min_bet, state.max_bet),
            ));
        }
        if cmd.amount % 2 != 0 {
            return Err(refuse(RejectionCode::BetNotEven, "the bet is not even"));
        }
        if seat.wager > 0 {
            return Err(refuse(
                RejectionCode::AlreadyBet,
                format!("seat {} has already bet this round", cmd.seat),
            ));
        }
        if cmd.amount > seat.stack {
            return Err(refuse(
                RejectionCode::InsufficientStack,
                "the stack is too small",
            ));
        }
        Ok(BetPlaced {
            round: state.round + 1,
            seat: cmd.seat,
            player_root: seat.player_root.clone(),
            amount: cmd.amount,
            stack_after: seat.stack - cmd.amount,
        })
    }

    fn deal_round(&self, _cmd: &DealRound) -> Result<Option<ShoeShuffled>, Rejection> {
        self.exists()?;
        let state = &self.state;
        if state.phase() == Phase::PlayerTurns {
            return Err(refuse(
                RejectionCode::RoundInProgress,
                "a round is in progress",
            ));
        }
        let wagered = self.wagered().len();
        if wagered == 0 {
            return Err(refuse(RejectionCode::NoBets, "nobody has bet"));
        }
        // AHR-4: 11 cards for each wagered seat and 11 for the dealer.
        if state.shoe.len() >= 11 * (wagered + 1) {
            return Ok(None);
        }
        let seed = shoe::next_seed(state.shoe_seed);
        Ok(Some(ShoeShuffled {
            shoe_number: state.shoe_number + 1,
            seed,
            decks: state.decks,
            cards: shoe::shuffle(seed, state.decks.unsigned_abs()),
        }))
    }

    fn deal(&self) -> Result<Option<RoundDealt>, Rejection> {
        // Only bets wait to be dealt: once dealt the phase is PlayerTurns,
        // and once settled Idle.
        if self.state.phase() != Phase::Betting {
            return Ok(None);
        }
        // AHR-6: each wagered seat, the up-card, each seat again, the hole.
        let seats = self.wagered();
        let n = seats.len();
        let shoe = &self.state.shoe;
        let hands: Vec<SeatHand> = seats
            .iter()
            .enumerate()
            .map(|(i, &seat)| seat_hand(seat, vec![shoe[i], shoe[n + 1 + i]]))
            .collect();
        let (up, hole) = (shoe[n], shoe[2 * n + 1]);
        let turn = match value(&[up, hole]).blackjack {
            true => -1,
            false => hands.iter().find(|h| !h.blackjack).map_or(-1, |h| h.seat),
        };
        Ok(Some(RoundDealt {
            round: self.state.round + 1,
            hands,
            dealer_up: Some(up),
            dealer_hole: Some(hole),
            turn,
        }))
    }

    fn hit(&self, cmd: &Hit) -> Result<CardDealt, Rejection> {
        let seat = self.on_turn(cmd.seat)?;
        let card = self.top();
        let mut cards = seat.hand.clone().unwrap_or_default().cards;
        cards.push(card);
        let v = value(&cards);
        Ok(CardDealt {
            seat: cmd.seat,
            card: Some(card),
            total: v.total,
            soft: v.soft,
            busted: v.busted(),
            next_turn: match v.busted() {
                true => self.next_after(cmd.seat),
                false => cmd.seat,
            },
        })
    }

    fn stand(&self, cmd: &Stand) -> Result<HandStood, Rejection> {
        let seat = self.on_turn(cmd.seat)?;
        Ok(HandStood {
            seat: cmd.seat,
            total: seat.hand.as_ref().map_or(0, |h| h.total),
            next_turn: self.next_after(cmd.seat),
        })
    }

    fn double_down(&self, cmd: &DoubleDown) -> Result<HandDoubled, Rejection> {
        let seat = self.on_turn(cmd.seat)?;
        let mut cards = seat.hand.clone().unwrap_or_default().cards;
        if cards.len() != 2 {
            return Err(refuse(
                RejectionCode::DoubleNotAllowed,
                "doubling is only allowed on the first two cards",
            ));
        }
        if seat.stack < seat.wager {
            return Err(refuse(
                RejectionCode::InsufficientStack,
                "the stack is too small",
            ));
        }
        let card = self.top();
        cards.push(card);
        let v = value(&cards);
        Ok(HandDoubled {
            seat: cmd.seat,
            added: seat.wager,
            card: Some(card),
            total: v.total,
            busted: v.busted(),
            next_turn: self.next_after(cmd.seat),
            stack_after: seat.stack - seat.wager,
        })
    }

    fn after_action(&self) -> Result<AfterAction, ShoeRanOut> {
        if self.state.phase() != Phase::PlayerTurns {
            return Ok(AfterAction::Nothing);
        }
        if self.state.turn >= 0 {
            return Ok(match self.turn_announced {
                true => AfterAction::Nothing,
                false => AfterAction::Turn(self.turn_started()),
            });
        }
        let (dealer, settled) = self.settle()?;
        Ok(AfterAction::Settle(dealer, settled))
    }

    fn apply_table_created(&mut self, event: &TableCreated) {
        self.events += 1;
        crate::apply_table_created(&mut self.state, event);
        self.state.turn = -1;
    }

    fn apply_shoe_shuffled(&mut self, event: &ShoeShuffled) {
        self.events += 1;
        crate::apply_shoe_shuffled(&mut self.state, event);
    }

    fn apply_seat_held(&mut self, event: &SeatHeld) {
        self.events += 1;
        self.state.seat_holds.insert(
            hex(&event.buy_in_id),
            TableSeatHold {
                player_root: event.player_root.clone(),
                seat: event.seat,
                amount: event.amount,
            },
        );
    }

    fn apply_seat_released(&mut self, event: &SeatReleased) {
        self.events += 1;
        self.state.seat_holds.remove(&hex(&event.buy_in_id));
    }

    fn apply_player_cashed_out(&mut self, event: &PlayerCashedOut) {
        self.events += 1;
        self.state.seated.remove(&event.seat);
        self.state.chips_out += event.amount;
    }

    fn apply_player_seated(&mut self, event: &PlayerSeated) {
        self.events += 1;
        let key = hex(&event.buy_in_id);
        self.state.seat_holds.remove(&key);
        self.state.confirmed_buy_ins.push(key);
        self.state.seated.insert(
            event.seat,
            TableSeat {
                player_root: event.player_root.clone(),
                stack: event.stack,
                ..TableSeat::default()
            },
        );
        self.state.chips_in += event.stack;
    }

    fn apply_bet_placed(&mut self, event: &BetPlaced) {
        self.events += 1;
        if let Some(seat) = self.state.seated.get_mut(&event.seat) {
            seat.wager = event.amount;
            seat.stack = event.stack_after;
        }
        self.state.set_phase(Phase::Betting);
    }

    fn apply_round_dealt(&mut self, event: &RoundDealt) {
        self.events += 1;
        let state = &mut self.state;
        state.round = event.round;
        state.shoe.drain(..2 * event.hands.len() + 2);
        for hand in &event.hands {
            if let Some(seat) = state.seated.get_mut(&hand.seat) {
                seat.finished = hand.blackjack;
                seat.hand = Some(hand.clone());
            }
        }
        state.dealer_cards = event
            .dealer_up
            .into_iter()
            .chain(event.dealer_hole)
            .collect();
        state.turn = event.turn;
        state.set_phase(Phase::PlayerTurns);
        self.turn_announced = false;
    }

    fn apply_turn_started(&mut self, _event: &TurnStarted) {
        self.events += 1;
        self.turn_announced = true;
    }

    fn apply_card_dealt(&mut self, event: &CardDealt) {
        self.events += 1;
        let state = &mut self.state;
        state.shoe.remove(0);
        if let Some(seat) = state.seated.get_mut(&event.seat) {
            let hand = seat.hand.get_or_insert_with(SeatHand::default);
            hand.cards.extend(event.card);
            hand.total = event.total;
            hand.soft = event.soft;
            hand.blackjack = false;
            seat.finished = event.busted;
        }
        state.turn = event.next_turn;
        self.turn_announced = false;
    }

    fn apply_hand_stood(&mut self, event: &HandStood) {
        self.events += 1;
        if let Some(seat) = self.state.seated.get_mut(&event.seat) {
            seat.finished = true;
        }
        self.state.turn = event.next_turn;
        self.turn_announced = false;
    }

    fn apply_hand_doubled(&mut self, event: &HandDoubled) {
        self.events += 1;
        let state = &mut self.state;
        state.shoe.remove(0);
        if let Some(seat) = state.seated.get_mut(&event.seat) {
            seat.wager += event.added;
            seat.stack = event.stack_after;
            let hand = seat.hand.get_or_insert_with(SeatHand::default);
            hand.cards.extend(event.card);
            let v = value(&hand.cards);
            hand.total = v.total;
            hand.soft = v.soft;
            hand.blackjack = false;
            seat.finished = true;
        }
        state.turn = event.next_turn;
        self.turn_announced = false;
    }

    fn apply_dealer_played(&mut self, event: &DealerPlayed) {
        self.events += 1;
        let state = &mut self.state;
        state.shoe.drain(..event.drawn.len());
        state.dealer_cards.extend(event.drawn.iter().copied());
    }

    fn apply_round_settled(&mut self, event: &RoundSettled) {
        self.events += 1;
        let state = &mut self.state;
        for outcome in &event.outcomes {
            if let Some(seat) = state.seated.get_mut(&outcome.seat) {
                seat.stack = outcome.stack_after;
                seat.wager = 0;
                seat.hand = None;
                seat.finished = false;
            }
        }
        state.house_result = event.house_result_after;
        state.dealer_cards.clear();
        state.turn = -1;
        state.set_phase(Phase::Idle);
        self.turn_announced = false;
    }
}
