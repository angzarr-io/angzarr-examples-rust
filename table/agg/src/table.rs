//! [`Table`]: the table's decisions and folds, as the implementation of its
//! generated interface, [`TableAggregate`]. Commands and events are the
//! generated domain layer's; the state is the spec's `TableState`, and a
//! refusal the spec's `TableRefused`.
//!
//! A decision reads the state before its events. Where a later event is
//! decided from the state after an earlier one (table.proto's MANY
//! commands), the decision runs its steps on a working copy, each folding
//! what it decided before the next: DealRound decides the reshuffle (if
//! any), then [`deal`](Table::deal), then
//! [`after_action`](Table::after_action); Hit, Stand and DoubleDown decide
//! their event, then `after_action`. Each step is idempotent on its own
//! result: run again on the table after its events, it decides nothing.

use examples_proto::v1::seat_outcome::Outcome;
use examples_proto::v1::table_state::Phase;
use examples_proto::v1::{self, TableSeat, TableSeatHold, TableState};

use crate::cashout::cashout_id;
use crate::domain::*;
use crate::failure::{domain, refused};
use crate::hand::{value, Value};
use crate::shoe;
use crate::state::table_exists;
use crate::table_aggregate::TableAggregate;

/// A decision's result: its value, or its refusal or bug.
type Decided<T> = Result<T, Failed<TableRefused>>;

/// What a MANY decision, or one of its steps, records.
type Events = Vec<TableAggregateEvent>;

/// The table: the spec's `TableState`, and [`TableAggregate`] over it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Table {
    state: TableState,
    /// Whether the current decision point's TurnStarted is recorded: what
    /// makes [`Table::after_action`] idempotent. Not in `TableState`: it is
    /// folded from TurnStarted, cleared by every event that moves the turn.
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

    /// Fold any table event.
    pub fn fold(&mut self, event: &TableAggregateEvent) {
        use TableAggregateEvent as E;
        match event {
            E::TableCreated(e) => self.apply_table_created(e),
            E::ShoeShuffled(e) => self.apply_shoe_shuffled(e),
            E::SeatHeld(e) => self.apply_seat_held(e),
            E::SeatReleased(e) => self.apply_seat_released(e),
            E::PlayerSeated(e) => self.apply_player_seated(e),
            E::PlayerCashedOut(e) => self.apply_player_cashed_out(e),
            E::BetPlaced(e) => self.apply_bet_placed(e),
            E::RoundDealt(e) => self.apply_round_dealt(e),
            E::TurnStarted(e) => self.apply_turn_started(e),
            E::CardDealt(e) => self.apply_card_dealt(e),
            E::HandStood(e) => self.apply_hand_stood(e),
            E::HandDoubled(e) => self.apply_hand_doubled(e),
            E::DealerPlayed(e) => self.apply_dealer_played(e),
            E::RoundSettled(e) => self.apply_round_settled(e),
        }
    }

    /// DealRound's second step: the opening deal from the current shoe, or
    /// nothing once the round is dealt (re-run on its own result).
    pub fn deal(&self) -> Decided<Events> {
        // Only bets wait to be dealt: once dealt the phase is PlayerTurns,
        // and once settled Idle.
        if self.state.phase() != Phase::Betting {
            return Ok(Vec::new());
        }
        // AHR-6: each wagered seat, the up-card, each seat again, the hole.
        let seats = self.wagered();
        let n = seats.len();
        let shoe = &self.state.shoe;
        let hands: Vec<v1::SeatHand> = seats
            .iter()
            .enumerate()
            .map(|(i, &seat)| seat_hand(seat, vec![shoe[i], shoe[n + 1 + i]]))
            .collect();
        let (up, hole) = (shoe[n], shoe[2 * n + 1]);
        let turn = match value(&[up, hole]).blackjack {
            true => -1,
            false => hands.iter().find(|h| !h.blackjack).map_or(-1, |h| h.seat),
        };
        let dealt: RoundDealt = domain(v1::RoundDealt {
            round: self.state.round + 1,
            hands,
            dealer_up: Some(up),
            dealer_hole: Some(hole),
            turn,
        })?;
        Ok(vec![dealt.into()])
    }

    /// The step after the deal and after every action: the next decision
    /// point's TurnStarted (decision 14.38), or, once no seat is left to
    /// act, the dealer's play and the settlement; nothing once recorded.
    /// The dealer's play running the shoe out is a bug, keyed
    /// `examples-table-shoe-ran-out` with its round: a round never runs the
    /// shoe out (AHR-4: DealRound replaces a shoe that could).
    pub fn after_action(&self) -> Decided<Events> {
        if self.state.phase() != Phase::PlayerTurns {
            return Ok(Vec::new());
        }
        if self.state.turn >= 0 {
            return Ok(match self.turn_announced {
                true => Vec::new(),
                false => vec![TurnStarted::into(domain(self.turn_started())?)],
            });
        }
        let (dealer, settled) = self
            .settle()
            .map_err(|round| Bug::new("examples-table-shoe-ran-out").arg("round", round))?;
        Ok(vec![
            DealerPlayed::into(domain(dealer)?),
            RoundSettled::into(domain(settled)?),
        ])
    }

    /// `first`, then each step's events, each step decided on the table
    /// after everything before it.
    fn and_then(&self, first: Events, steps: &[fn(&Table) -> Decided<Events>]) -> Decided<Events> {
        let mut working = self.clone();
        let mut events = Vec::new();
        let mut record = |working: &mut Table, decided: Events| {
            for event in decided {
                working.fold(&event);
                events.push(event);
            }
        };
        record(&mut working, first);
        for step in steps {
            let decided = step(&working)?;
            record(&mut working, decided);
        }
        Ok(events)
    }

    fn exists(&self) -> Result<(), TableRefused> {
        match table_exists(&self.state) {
            true => Ok(()),
            false => Err(refused(TableRefusedReason::TableNotFound)),
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
    fn on_turn(&self, seat: i32) -> Result<&TableSeat, TableRefused> {
        self.exists()?;
        if self.state.phase() != Phase::PlayerTurns || self.state.turn < 0 {
            return Err(refused(TableRefusedReason::NoRoundInProgress));
        }
        if self.state.turn != seat {
            return Err(TableRefused {
                seat: Some(seat),
                ..refused(TableRefusedReason::NotYourTurn)
            });
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
    fn top(&self) -> v1::Card {
        self.state.shoe[0]
    }

    fn turn_started(&self) -> v1::TurnStarted {
        let seat_number = self.state.turn;
        let seat = &self.state.seated[&seat_number];
        let hand = seat.hand.clone().unwrap_or_default();
        v1::TurnStarted {
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

    /// The dealer's play (AHR-7, AHR-11) and the settlement (AHR-12); the
    /// round, if the shoe runs out while the dealer draws.
    fn settle(&self) -> Result<(v1::DealerPlayed, v1::RoundSettled), u32> {
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
                let card = *shoe.next().ok_or(state.round)?;
                dealer.push(card);
                drawn.push(card);
            }
        }
        let dealt = value(&dealer);
        let outcomes: Vec<v1::SeatOutcome> = seats
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
                v1::SeatOutcome {
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
            v1::DealerPlayed {
                round: state.round,
                hole: state.dealer_cards.get(1).copied(),
                drawn,
                total: dealt.total,
                soft: dealt.soft,
                busted: dealt.busted(),
                blackjack: dealt.blackjack,
            },
            v1::RoundSettled {
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

/// The key `seat_holds` and `confirmed_buy_ins` use: lowercase hex.
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn seat_hand(seat: i32, cards: Vec<v1::Card>) -> v1::SeatHand {
    let v = value(&cards);
    v1::SeatHand {
        seat,
        cards,
        total: v.total,
        soft: v.soft,
        blackjack: v.blackjack,
    }
}

/// `card` as the state holds it.
fn wire(card: &Card) -> v1::Card {
    card.clone().into()
}

impl TableAggregate for Table {
    fn create_table(
        &self,
        cmd: &CreateTable,
        _ctx: &Context,
    ) -> Decided<(TableCreated, ShoeShuffled)> {
        crate::create_table(&self.state, cmd)
    }

    fn request_seat(&self, cmd: &RequestSeat, _ctx: &Context) -> Decided<Option<SeatHeld>> {
        self.exists()?;
        let key = hex(&cmd.request_id);
        if self.state.seat_holds.contains_key(&key) || self.state.confirmed_buy_ins.contains(&key) {
            return Ok(None);
        }
        let state = &self.state;
        if cmd.seat < 0 || cmd.seat >= state.seats {
            return Err(TableRefused {
                seat: Some(cmd.seat),
                ..refused(TableRefusedReason::SeatOutOfRange)
            }
            .into());
        }
        if cmd.amount < state.min_buy_in || cmd.amount > state.max_buy_in {
            return Err(TableRefused {
                min_buy_in: Some(state.min_buy_in),
                max_buy_in: Some(state.max_buy_in),
                ..refused(TableRefusedReason::BuyInOutOfRange)
            }
            .into());
        }
        if self.seat_of(&cmd.player_root).is_some() {
            return Err(refused(TableRefusedReason::PlayerAlreadySeated).into());
        }
        let held = state.seat_holds.values().any(|h| h.seat == cmd.seat);
        if held || state.seated.contains_key(&cmd.seat) {
            return Err(TableRefused {
                seat: Some(cmd.seat),
                ..refused(TableRefusedReason::SeatTaken)
            }
            .into());
        }
        Ok(Some(SeatHeld {
            buy_in_id: cmd.request_id.clone(),
            player_root: cmd.player_root.clone(),
            seat: cmd.seat,
            amount: cmd.amount,
        }))
    }

    fn leave_table(&self, cmd: &LeaveTable, ctx: &Context) -> Decided<PlayerCashedOut> {
        self.exists()?;
        // The cash-out id is derived from the table's root, the command's
        // stream's: the kernel only runs a handler for a well-formed root
        // (16 bytes).
        let table_root: [u8; 16] = ctx
            .command
            .cover
            .as_ref()
            .and_then(|cover| cover.root.as_ref())
            .and_then(|root| root.value[..].try_into().ok())
            .unwrap_or_default();
        let not_seated = || TableRefused {
            seat: Some(cmd.seat),
            ..refused(TableRefusedReason::NotSeated)
        };
        let seat = self.state.seated.get(&cmd.seat).ok_or_else(not_seated)?;
        if seat.wager > 0 {
            return Err(TableRefused {
                seat: Some(cmd.seat),
                ..refused(TableRefusedReason::WagerInPlay)
            }
            .into());
        }
        Ok(PlayerCashedOut {
            cashout_id: cashout_id(&table_root, self.events).to_vec(),
            player_root: seat.player_root.clone(),
            seat: cmd.seat,
            amount: seat.stack,
        })
    }

    fn place_bet(&self, cmd: &PlaceBet, ctx: &Context) -> Decided<BetPlaced> {
        // PlaceBet is sent with AGGREGATE_HANDLES (table.proto). The table
        // decides against its full history, the window the bet did not see
        // included, so a second bet on the seat is its own ALREADY_BET; it
        // acknowledges the window it was handed.
        if let Some(conflict) = &ctx.command.conflict {
            conflict.resolve_conflicts(conflict.window(), conflict.nonce());
        }
        self.exists()?;
        let state = &self.state;
        if state.phase() == Phase::PlayerTurns {
            return Err(refused(TableRefusedReason::RoundInProgress).into());
        }
        let not_seated = || TableRefused {
            seat: Some(cmd.seat),
            ..refused(TableRefusedReason::NotSeated)
        };
        let seat = state.seated.get(&cmd.seat).ok_or_else(not_seated)?;
        if cmd.amount < state.min_bet || cmd.amount > state.max_bet {
            return Err(TableRefused {
                min_bet: Some(state.min_bet),
                max_bet: Some(state.max_bet),
                ..refused(TableRefusedReason::BetOutOfRange)
            }
            .into());
        }
        if cmd.amount % 2 != 0 {
            return Err(refused(TableRefusedReason::BetNotEven).into());
        }
        if seat.wager > 0 {
            return Err(TableRefused {
                seat: Some(cmd.seat),
                ..refused(TableRefusedReason::AlreadyBet)
            }
            .into());
        }
        if cmd.amount > seat.stack {
            return Err(refused(TableRefusedReason::InsufficientStack).into());
        }
        Ok(BetPlaced {
            round: state.round + 1,
            seat: cmd.seat,
            player_root: seat.player_root.clone(),
            amount: cmd.amount,
            stack_after: seat.stack - cmd.amount,
        })
    }

    /// Whether the round can be dealt, and the new shoe first when it could
    /// run the old one out (AHR-4); then the deal and the step after it.
    fn deal_round(&self, _cmd: &DealRound, _ctx: &Context) -> Decided<Events> {
        self.exists()?;
        let state = &self.state;
        if state.phase() == Phase::PlayerTurns {
            return Err(refused(TableRefusedReason::RoundInProgress).into());
        }
        let wagered = self.wagered().len();
        if wagered == 0 {
            return Err(refused(TableRefusedReason::NoBets).into());
        }
        // AHR-4: 11 cards for each wagered seat and 11 for the dealer.
        let mut reshuffle = Vec::new();
        if state.shoe.len() < 11 * (wagered + 1) {
            let seed = shoe::next_seed(state.shoe_seed);
            let shuffled: ShoeShuffled = domain(v1::ShoeShuffled {
                shoe_number: state.shoe_number + 1,
                seed,
                decks: state.decks,
                cards: shoe::shuffle(seed, state.decks.unsigned_abs()),
            })?;
            reshuffle.push(shuffled.into());
        }
        self.and_then(reshuffle, &[Table::deal, Table::after_action])
    }

    fn hit(&self, cmd: &Hit, _ctx: &Context) -> Decided<Events> {
        let seat = self.on_turn(cmd.seat)?;
        let card = self.top();
        let mut cards = seat.hand.clone().unwrap_or_default().cards;
        cards.push(card);
        let v = value(&cards);
        let dealt = CardDealt {
            seat: cmd.seat,
            card: domain(card)?,
            total: v.total,
            soft: v.soft,
            busted: v.busted(),
            next_turn: match v.busted() {
                true => self.next_after(cmd.seat),
                false => cmd.seat,
            },
        };
        self.and_then(vec![dealt.into()], &[Table::after_action])
    }

    fn stand(&self, cmd: &Stand, _ctx: &Context) -> Decided<Events> {
        let seat = self.on_turn(cmd.seat)?;
        let stood = HandStood {
            seat: cmd.seat,
            total: seat.hand.as_ref().map_or(0, |h| h.total),
            next_turn: self.next_after(cmd.seat),
        };
        self.and_then(vec![stood.into()], &[Table::after_action])
    }

    fn double_down(&self, cmd: &DoubleDown, _ctx: &Context) -> Decided<Events> {
        let seat = self.on_turn(cmd.seat)?;
        let mut cards = seat.hand.clone().unwrap_or_default().cards;
        if cards.len() != 2 {
            return Err(refused(TableRefusedReason::DoubleNotAllowed).into());
        }
        if seat.stack < seat.wager {
            return Err(refused(TableRefusedReason::InsufficientStack).into());
        }
        let card = self.top();
        cards.push(card);
        let v = value(&cards);
        let doubled = HandDoubled {
            seat: cmd.seat,
            added: seat.wager,
            card: domain(card)?,
            total: v.total,
            busted: v.busted(),
            next_turn: self.next_after(cmd.seat),
            stack_after: seat.stack - seat.wager,
        };
        self.and_then(vec![doubled.into()], &[Table::after_action])
    }

    /// The player seated, or nothing when this buy-in already seated them;
    /// refused with the typed SeatNotConfirmable when the hold is gone or
    /// the player took another seat meanwhile.
    fn confirm_seat(
        &self,
        cmd: &ConfirmSeat,
        _ctx: &Context,
    ) -> Result<Option<PlayerSeated>, Failed<SeatNotConfirmable>> {
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
            return Err(not_confirmable("PLAYER_ALREADY_SEATED").into());
        }
        Ok(Some(PlayerSeated {
            buy_in_id: cmd.buy_in_id.clone(),
            player_root: hold.player_root.clone(),
            seat: hold.seat,
            stack: hold.amount,
        }))
    }

    /// The hold freed, or nothing when it no longer exists; never refused.
    fn release_seat(
        &self,
        cmd: &ReleaseSeat,
        _ctx: &Context,
    ) -> Result<Option<SeatReleased>, Failed<Rejection>> {
        Ok(self
            .state
            .seat_holds
            .get(&hex(&cmd.buy_in_id))
            .map(|hold| SeatReleased {
                buy_in_id: cmd.buy_in_id.clone(),
                player_root: hold.player_root.clone(),
                seat: hold.seat,
                reason: cmd.reason.clone(),
            }))
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

    fn apply_player_cashed_out(&mut self, event: &PlayerCashedOut) {
        self.events += 1;
        self.state.seated.remove(&event.seat);
        self.state.chips_out += event.amount;
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
                seat.hand = Some(hand.clone().into());
            }
        }
        state.dealer_cards = vec![wire(&event.dealer_up), wire(&event.dealer_hole)];
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
            let hand = seat.hand.get_or_insert_with(v1::SeatHand::default);
            hand.cards.push(wire(&event.card));
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
            let hand = seat.hand.get_or_insert_with(v1::SeatHand::default);
            hand.cards.push(wire(&event.card));
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
        state.dealer_cards.extend(event.drawn.iter().map(wire));
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
