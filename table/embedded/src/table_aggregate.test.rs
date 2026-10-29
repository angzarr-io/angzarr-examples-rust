//! Given/when/then on the marked-up table, through angzarr-embedded's whole
//! in-memory pipeline (`Stream::execute`: check, identify, decode, decide,
//! continuations, commit), with no store.
//!
//! - **Given** events are replayed as stored pages through the registry's
//!   factory and the markup's appliers ([`given`]).
//! - **When** a command is sent as a client sends it, packed in a
//!   `CommandBook` ([`Harness::when`]).
//! - **Then** the events of that one decision, decoded back to the
//!   generated types, or the refusal: its `ErrorInfo.reason`
//!   ([`refused`]) or ConfirmSeat's typed `SeatNotConfirmable`
//!   ([`not_confirmable`]).
//!
//! The shoe is always seeded or written out, so every deal is
//! deterministic. "The shoe begins X" (round.feature) is [`shoe_beginning`]:
//! X, then the rest of a fresh deck with X taken out. Scenario ids are the
//! spec's (`features/example/blackjack/*.feature` in the angzarr-project
//! submodule). The shoe's golden vectors (EU-1520..1525) stay in agg-table's
//! `shoe.test.rs`; EU-1440's golden first twelve cards, and the seeds of
//! EU-1502/1503, are pinned again here at the aggregate.

use agg_table::shoe::{fresh_shoe, next_seed, shuffle};
use agg_table::test_support::{card, cards};
use angzarr_embedded::next::markup::{
    assert_step_idempotent, Context, Rejection, MARKUP_RULES_VERSION,
};
use angzarr_embedded::next::v1::event_meta::Causation;
use angzarr_embedded::next::v1::{
    command_page, Command, CommandBook, CommandPage, ConflictResolution, Cover, EventMeta,
    EventPage, Handling, Uuid,
};
use angzarr_embedded::next::{
    pack, type_name, Any, Executed, Failure, KernelError, MemoryDeadLetters, Payload, Registry,
    Sampled, Stream, Timestamp,
};
use examples_proto::v1::{self, seat_outcome::Outcome, table_state::Phase, Card, SeatHand};
use prost::{Message, Name};

use super::TableAggregate;
use crate::TableRefused;

// ---------------------------------------------------------------- the events

macro_rules! table_events {
    ($($name:ident),* $(,)?) => {
        /// A table event as the generated type it travels as.
        #[derive(Clone, Debug, PartialEq)]
        enum Ev {
            $($name(v1::$name),)*
        }

        impl Ev {
            fn to_any(&self) -> Any {
                match self {
                    $(Ev::$name(event) => pack(event),)*
                }
            }

            fn from_any(any: &Any) -> Ev {
                let name = type_name(&any.type_url);
                $(
                    if name == v1::$name::full_name() {
                        return Ev::$name(v1::$name::decode(&any.value[..]).expect("decodes"));
                    }
                )*
                panic!("not a table event: {}", any.type_url)
            }
        }
    };
}

table_events!(
    TableCreated,
    ShoeShuffled,
    SeatHeld,
    SeatReleased,
    PlayerSeated,
    PlayerCashedOut,
    BetPlaced,
    RoundDealt,
    TurnStarted,
    CardDealt,
    HandStood,
    HandDoubled,
    DealerPlayed,
    RoundSettled,
);

// ---------------------------------------------------------------- the harness

type Refusal = Failure<Rejection<TableRefused>>;
type Then = Result<Vec<Ev>, Refusal>;

fn registry() -> Registry {
    let mut registry = Registry::new();
    registry.aggregate::<TableAggregate>(TableAggregate::default);
    registry
}

fn cover() -> Cover {
    Cover {
        domain: "table".to_string(),
        root: Some(Uuid::from_bytes([0x7a; 16])),
        edition: None,
    }
}

/// A distinct, well-formed id for the `n`th command of a test.
fn id(n: u32) -> Uuid {
    Uuid::from_bytes((0xc0de_0000_0000_0000_0000_0000_0000_0000_u128 + u128::from(n)).to_be_bytes())
}

/// The table stream with `events` already recorded.
struct Harness {
    registry: Registry,
    stream: Stream<TableAggregate>,
    commands: u32,
}

/// The table after `events`, replayed as stored pages (recorded before any
/// command a test sends).
fn given(events: Vec<Ev>) -> Harness {
    let registry = registry();
    let pages: Vec<EventPage> = events
        .iter()
        .enumerate()
        .map(|(sequence, event)| EventPage {
            payload: Some(event.to_any()),
            meta: Some(EventMeta {
                causation: Some(Causation::CommandId(id(0))),
                correlation_id: Some(id(0)),
                recorded_at: Some(Timestamp {
                    seconds: sequence as i64,
                    nanos: 0,
                }),
                rules_version: MARKUP_RULES_VERSION.to_string(),
                ..EventMeta::default()
            }),
            sequence: sequence as u64,
        })
        .collect();
    let stream = Stream::replay(cover(), &pages, &registry).expect("the given events replay");
    Harness {
        registry,
        stream,
        commands: 0,
    }
}

// angzarr-embedded's `Failure` is what the pipeline returns, unboxed.
#[allow(clippy::result_large_err)]
impl Harness {
    /// Send `command` as a client does: the events of its one decision, or
    /// why it was refused.
    fn when<M: Message + Name>(&mut self, command: M) -> Then {
        self.when_handled(command, None)
    }

    /// As [`Harness::when`], sent with `handling`: an expected sequence and
    /// a conflict resolution.
    fn when_handled<M: Message + Name>(&mut self, command: M, handling: Option<Handling>) -> Then {
        self.commands += 1;
        let n = self.commands;
        let book = CommandBook {
            cover: Some(cover()),
            page: Some(CommandPage {
                body: Some(command_page::Body::Command(Command {
                    payload: Some(pack(&command)),
                })),
                command_id: Some(id(n)),
                ..CommandPage::default()
            }),
            handling,
        };
        let sampled = Sampled {
            now: Timestamp {
                seconds: 1_000 + i64::from(n),
                nanos: 0,
            },
            command_id: id(n),
        };
        let dead_letters = MemoryDeadLetters::new();
        match self
            .stream
            .execute(book, &self.registry, &sampled, &dead_letters, &mut [])?
        {
            Executed::Applied(events) => Ok(events
                .iter()
                .map(|e| Ev::from_any(&e.event().to_any()))
                .collect()),
            Executed::Duplicate(_) => panic!("command {n} is new"),
        }
    }

    fn state(&self) -> &v1::TableState {
        self.stream.state().state()
    }
}

/// The `ErrorInfo.reason` of a message-only refusal.
#[track_caller]
fn refused(then: Then) -> &'static str {
    match then {
        Err(Failure::Rejected(Rejection::Message(refused))) => refused.reason(),
        other => panic!("expected the table to refuse, got {other:?}"),
    }
}

/// The reason of ConfirmSeat's typed refusal.
#[track_caller]
fn not_confirmable(then: Then) -> String {
    match then {
        Err(Failure::Rejected(rejection @ Rejection::Typed(_))) => {
            let any = rejection.refusal().expect("a typed refusal is packed");
            assert_eq!(
                type_name(&any.type_url),
                v1::SeatNotConfirmable::full_name()
            );
            v1::SeatNotConfirmable::decode(&any.value[..])
                .expect("decodes")
                .reason
        }
        other => panic!("expected SeatNotConfirmable, got {other:?}"),
    }
}

/// Ledger rule L2: `sum(stacks) + sum(wagers) + house_result = chips_in -
/// chips_out`.
#[track_caller]
fn assert_ledger_balances(state: &v1::TableState) {
    let on_table: i64 = state.seated.values().map(|s| s.stack + s.wager).sum();
    assert_eq!(
        on_table + state.house_result,
        state.chips_in - state.chips_out,
        "L2 does not hold: {state:?}"
    );
}

// ---------------------------------------------------------------- the fixtures

const ALICE: u8 = 0xa1;
const BOB: u8 = 0xb0;
const B1: u8 = 0x01;
const B2: u8 = 0x02;

fn bytes(b: u8) -> Vec<u8> {
    vec![b; 16]
}

/// The lowercase-hex key `seat_holds` is keyed by.
fn hex(b: u8) -> String {
    format!("{b:02x}").repeat(16)
}

/// Table "Main": 3 seats, one deck, bets 10 to 100, buy-ins 100 to 1000.
fn main_created() -> v1::TableCreated {
    v1::TableCreated {
        name: "Main".into(),
        min_bet: 10,
        max_bet: 100,
        min_buy_in: 100,
        max_buy_in: 1000,
        seats: 3,
        decks: 1,
    }
}

fn create_main(seed: u64) -> v1::CreateTable {
    v1::CreateTable {
        name: "Main".into(),
        min_bet: 10,
        max_bet: 100,
        min_buy_in: 100,
        max_buy_in: 1000,
        seats: 3,
        decks: 1,
        shoe_seed: seed,
    }
}

/// `text`'s cards first, then the rest of one fresh deck with those cards
/// taken out: "the shoe begins" (round.feature).
fn shoe_beginning(text: &str) -> Vec<Card> {
    let top = cards(text);
    let mut rest = fresh_shoe(1);
    for card in &top {
        let at = rest.iter().position(|c| c == card).expect("in the deck");
        rest.remove(at);
    }
    top.into_iter().chain(rest).collect()
}

fn shoe_one(cards: Vec<Card>) -> Ev {
    Ev::ShoeShuffled(v1::ShoeShuffled {
        shoe_number: 1,
        seed: 42,
        decks: 1,
        cards,
    })
}

fn held(buy_in: u8, player: u8, seat: i32, amount: i64) -> Ev {
    Ev::SeatHeld(v1::SeatHeld {
        buy_in_id: bytes(buy_in).into(),
        player_root: bytes(player).into(),
        seat,
        amount,
    })
}

fn seated(buy_in: u8, player: u8, seat: i32, stack: i64) -> Ev {
    Ev::PlayerSeated(v1::PlayerSeated {
        buy_in_id: bytes(buy_in).into(),
        player_root: bytes(player).into(),
        seat,
        stack,
    })
}

/// Table "Main" exists, its shoe shuffled.
fn main_table() -> Vec<Ev> {
    vec![Ev::TableCreated(main_created()), shoe_one(shuffle(42, 1))]
}

/// round.feature's background: Alice at seat 0 and Bob at seat 1, each
/// with `stacks`, and a shoe beginning `shoe`.
fn background_with(shoe: &str, stacks: (i64, i64)) -> Vec<Ev> {
    vec![
        Ev::TableCreated(main_created()),
        shoe_one(shoe_beginning(shoe)),
        held(B1, ALICE, 0, stacks.0),
        seated(B1, ALICE, 0, stacks.0),
        held(B2, BOB, 1, stacks.1),
        seated(B2, BOB, 1, stacks.1),
    ]
}

fn background(shoe: &str) -> Vec<Ev> {
    background_with(shoe, (500, 500))
}

fn player(seat: i32) -> u8 {
    match seat {
        0 => ALICE,
        1 => BOB,
        _ => panic!("no player at seat {seat}"),
    }
}

/// A bet of `amount` on round 1 by the background's player at `seat`.
fn bet(seat: i32, amount: i64, stack_after: i64) -> Ev {
    Ev::BetPlaced(v1::BetPlaced {
        round: 1,
        seat,
        player_root: bytes(player(seat)).into(),
        amount,
        stack_after,
    })
}

fn hand(seat: i32, text: &str, total: u32, soft: bool, blackjack: bool) -> SeatHand {
    SeatHand {
        seat,
        cards: cards(text),
        total,
        soft,
        blackjack,
    }
}

fn dealt(hands: Vec<SeatHand>, up: &str, hole: &str, turn: i32) -> Ev {
    Ev::RoundDealt(v1::RoundDealt {
        round: 1,
        hands,
        dealer_up: Some(card(up)),
        dealer_hole: Some(card(hole)),
        turn,
    })
}

/// Seat `seat`'s turn in round 1, holding `text` (worth `total`) against
/// the dealer's `up`.
#[allow(clippy::too_many_arguments)]
fn turn(
    seat: i32,
    text: &str,
    total: u32,
    soft: bool,
    up: &str,
    can_double: bool,
    stack: i64,
    wager: i64,
) -> Ev {
    Ev::TurnStarted(v1::TurnStarted {
        round: 1,
        seat,
        player_root: bytes(player(seat)).into(),
        hand: Some(hand(seat, text, total, soft, false)),
        dealer_up: Some(card(up)),
        can_double,
        stack,
        wager,
    })
}

fn card_dealt(seat: i32, text: &str, total: u32, soft: bool, busted: bool, next: i32) -> Ev {
    Ev::CardDealt(v1::CardDealt {
        seat,
        card: Some(card(text)),
        total,
        soft,
        busted,
        next_turn: next,
    })
}

fn stood(seat: i32, total: u32, next: i32) -> Ev {
    Ev::HandStood(v1::HandStood {
        seat,
        total,
        next_turn: next,
    })
}

/// The dealer's play: `hole` revealed, `drawn` drawn, ending on `total`.
fn dealer(hole: &str, drawn: &str, total: u32, soft: bool, busted: bool, bj: bool) -> Ev {
    Ev::DealerPlayed(v1::DealerPlayed {
        round: 1,
        hole: Some(card(hole)),
        drawn: cards(drawn),
        total,
        soft,
        busted,
        blackjack: bj,
    })
}

fn outcome(seat: i32, wager: i64, outcome: Outcome, returned: i64, after: i64) -> v1::SeatOutcome {
    v1::SeatOutcome {
        seat,
        player_root: bytes(player(seat)).into(),
        wager,
        outcome: outcome.into(),
        returned,
        net: returned - wager,
        stack_after: after,
    }
}

/// Round 1 settled with `outcomes`; the house had nothing before it.
fn settled(outcomes: Vec<v1::SeatOutcome>) -> Ev {
    let wagers: i64 = outcomes.iter().map(|o| o.wager).sum();
    let returned: i64 = outcomes.iter().map(|o| o.returned).sum();
    Ev::RoundSettled(v1::RoundSettled {
        round: 1,
        outcomes,
        house_delta: wagers - returned,
        house_result_after: wagers - returned,
    })
}

// ================================================================ CreateTable

#[test]
fn eu_1440_creating_a_table_shuffles_its_first_shoe_from_the_seed() {
    let mut table = given(vec![]);

    let events = table.when(create_main(42)).expect("created");

    assert_eq!(
        events,
        vec![
            Ev::TableCreated(main_created()),
            Ev::ShoeShuffled(v1::ShoeShuffled {
                shoe_number: 1,
                seed: 42,
                decks: 1,
                cards: shuffle(42, 1),
            }),
        ]
    );
    let state = table.state();
    assert_eq!(state.phase(), Phase::Idle);
    assert!(state.seated.is_empty());
    assert_eq!((state.shoe_number, state.shoe_seed), (1, 42));
    assert_eq!(
        state.shoe[..12],
        cards("7♣ 3♥ K♣ A♠ 2♠ A♣ A♥ K♦ 10♦ 5♣ 8♠ J♥")[..]
    );
}

#[test]
fn eu_1441_a_configuration_outside_the_house_rules_is_refused() {
    let mut table = given(vec![]);

    let eight_seats = v1::CreateTable {
        seats: 8,
        ..create_main(42)
    };

    assert_eq!(refused(table.when(eight_seats)), "INVALID_TABLE_CONFIG");
}

#[test]
fn eu_1442_a_table_cannot_be_created_twice() {
    let mut table = given(main_table());

    assert_eq!(refused(table.when(create_main(42))), "TABLE_ALREADY_EXISTS");
}

#[test]
fn a_command_to_a_table_that_does_not_exist_is_refused() {
    let mut table = given(vec![]);

    let request = v1::RequestSeat {
        player_root: bytes(ALICE).into(),
        seat: 0,
        amount: 500,
        request_id: bytes(B1).into(),
    };
    assert_eq!(refused(table.when(request)), "TABLE_NOT_FOUND");
    let bet = v1::PlaceBet {
        seat: 0,
        amount: 20,
    };
    assert_eq!(refused(table.when(bet)), "TABLE_NOT_FOUND");
    assert_eq!(refused(table.when(v1::DealRound {})), "TABLE_NOT_FOUND");
    assert_eq!(refused(table.when(v1::Hit { seat: 0 })), "TABLE_NOT_FOUND");
    assert_eq!(
        refused(table.when(v1::Stand { seat: 0 })),
        "TABLE_NOT_FOUND"
    );
}

// ================================================================ RequestSeat

fn request(player: u8, seat: i32, amount: i64, request: u8) -> v1::RequestSeat {
    v1::RequestSeat {
        player_root: bytes(player).into(),
        seat,
        amount,
        request_id: bytes(request).into(),
    }
}

#[test]
fn eu_1443_a_seat_request_holds_the_seat_for_the_buy_in() {
    let mut table = given(main_table());

    let events = table.when(request(ALICE, 0, 500, B1)).expect("held");

    assert_eq!(events, vec![held(B1, ALICE, 0, 500)]);
    let state = table.state();
    assert!(state.seated.is_empty(), "no one is seated");
    let hold = &state.seat_holds[&hex(B1)];
    assert_eq!(
        (&hold.player_root[..], hold.seat, hold.amount),
        (&bytes(ALICE)[..], 0, 500)
    );
}

#[test]
fn eu_1444_repeating_a_seat_request_changes_nothing() {
    let mut table = given(vec![
        Ev::TableCreated(main_created()),
        shoe_one(shuffle(42, 1)),
        held(B1, ALICE, 0, 500),
    ]);

    assert_eq!(table.when(request(ALICE, 0, 500, B1)).expect("ok"), vec![]);
}

#[test]
fn eu_1445_a_seat_that_is_held_cannot_be_requested() {
    let mut events = main_table();
    events.push(held(B2, BOB, 1, 500));
    let mut table = given(events);

    assert_eq!(
        refused(table.when(request(ALICE, 1, 500, 0x03))),
        "SEAT_TAKEN"
    );
}

#[test]
fn eu_1445_a_seat_that_is_taken_cannot_be_requested() {
    let mut events = main_table();
    events.extend([held(B2, BOB, 1, 500), seated(B2, BOB, 1, 500)]);
    let mut table = given(events);

    assert_eq!(
        refused(table.when(request(ALICE, 1, 500, 0x03))),
        "SEAT_TAKEN"
    );
}

#[test]
fn eu_1446_a_seat_outside_the_table_cannot_be_requested() {
    for seat in [-1, 3] {
        let mut table = given(main_table());

        assert_eq!(
            refused(table.when(request(ALICE, seat, 500, B1))),
            "SEAT_OUT_OF_RANGE",
            "seat {seat}"
        );
    }
}

#[test]
fn eu_1447_a_buy_in_outside_the_tables_range_is_refused() {
    for amount in [99, 1001] {
        let mut table = given(main_table());

        assert_eq!(
            refused(table.when(request(ALICE, 0, amount, B1))),
            "BUY_IN_OUT_OF_RANGE",
            "buy-in {amount}"
        );
    }
}

#[test]
fn eu_1448_a_seated_player_cannot_ask_for_a_second_seat() {
    let mut events = main_table();
    events.extend([held(B1, ALICE, 0, 500), seated(B1, ALICE, 0, 500)]);
    let mut table = given(events);

    assert_eq!(
        refused(table.when(request(ALICE, 2, 500, B2))),
        "PLAYER_ALREADY_SEATED"
    );
}

// ================================================================ ConfirmSeat

fn confirm(buy_in: u8) -> v1::ConfirmSeat {
    v1::ConfirmSeat {
        buy_in_id: bytes(buy_in).into(),
    }
}

#[test]
fn eu_1449_confirming_a_held_seat_seats_the_player_with_the_buy_in_as_stack() {
    let mut events = main_table();
    events.push(held(B1, ALICE, 0, 500));
    let mut table = given(events);

    let events = table.when(confirm(B1)).expect("seated");

    assert_eq!(events, vec![seated(B1, ALICE, 0, 500)]);
    let state = table.state();
    let seat = &state.seated[&0];
    assert_eq!(
        (&seat.player_root[..], seat.stack, seat.wager),
        (&bytes(ALICE)[..], 500, 0)
    );
    assert!(state.seat_holds.is_empty(), "the hold is spent");
    assert_eq!(state.chips_in, 500, "table Main has taken in 500 chips");
    assert_ledger_balances(state);
}

#[test]
fn eu_1450_confirming_the_same_buy_in_again_changes_nothing() {
    let mut events = main_table();
    events.extend([held(B1, ALICE, 0, 500), seated(B1, ALICE, 0, 500)]);
    let mut table = given(events);

    assert_eq!(table.when(confirm(B1)).expect("ok"), vec![]);
}

#[test]
fn confirming_a_buy_in_with_no_hold_is_refused_as_seat_not_confirmable() {
    let mut table = given(main_table());

    assert_eq!(
        not_confirmable(table.when(confirm(0x09))),
        "SEAT_HOLD_NOT_FOUND"
    );
}

#[test]
fn confirming_a_second_hold_once_the_player_is_seated_is_refused_as_seat_not_confirmable() {
    let mut events = main_table();
    events.extend([
        held(B1, ALICE, 0, 500),
        held(B2, ALICE, 2, 500),
        seated(B1, ALICE, 0, 500),
    ]);
    let mut table = given(events);

    assert_eq!(
        not_confirmable(table.when(confirm(B2))),
        "PLAYER_ALREADY_SEATED"
    );
}

// ================================================================ PlaceBet

fn place(seat: i32, amount: i64) -> v1::PlaceBet {
    v1::PlaceBet { seat, amount }
}

#[test]
fn eu_1470_a_bet_moves_chips_from_the_stack_to_the_wager() {
    let mut table = given(background("2♣"));

    let events = table.when(place(0, 20)).expect("bet");

    assert_eq!(events, vec![bet(0, 20, 480)]);
    let state = table.state();
    assert_eq!((state.seated[&0].wager, state.seated[&0].stack), (20, 480));
    assert_eq!(state.phase(), Phase::Betting);
    assert_ledger_balances(state);
}

#[test]
fn eu_1471_a_bet_must_be_within_the_table_limits() {
    for amount in [8, 102] {
        let mut table = given(background("2♣"));

        assert_eq!(
            refused(table.when(place(0, amount))),
            "BET_OUT_OF_RANGE",
            "bet {amount}"
        );
    }
}

#[test]
fn eu_1471_a_bet_must_be_even() {
    let mut table = given(background("2♣"));

    assert_eq!(refused(table.when(place(0, 15))), "BET_NOT_EVEN");
}

#[test]
fn eu_1472_a_seat_can_bet_only_once_per_round() {
    let mut events = background("2♣");
    events.push(bet(0, 20, 480));
    let mut table = given(events);

    assert_eq!(refused(table.when(place(0, 20))), "ALREADY_BET");
}

#[test]
fn eu_1473_a_bet_cannot_exceed_the_stack() {
    let mut table = given(background_with("2♣", (500, 30)));

    assert_eq!(refused(table.when(place(1, 40))), "INSUFFICIENT_STACK");
}

#[test]
fn eu_1474_an_empty_seat_cannot_bet() {
    let mut table = given(background("2♣"));

    assert_eq!(refused(table.when(place(2, 20))), "NOT_SEATED");
}

#[test]
fn eu_1475_no_bets_are_taken_once_the_cards_are_out() {
    let mut events = background("2♣ 9♦ 4♣ 7♠");
    events.extend([
        bet(0, 20, 480),
        dealt(vec![hand(0, "2♣ 4♣", 6, false, false)], "9♦", "7♠", 0),
        turn(0, "2♣ 4♣", 6, false, "9♦", true, 480, 20),
    ]);
    let mut table = given(events);

    assert_eq!(refused(table.when(place(1, 20))), "ROUND_IN_PROGRESS");
}

#[test]
fn eu_1501_the_next_round_is_numbered_after_the_last() {
    let mut events = background("10♣ 10♦ 9♥ 7♠");
    events.extend([
        bet(0, 20, 480),
        dealt(vec![hand(0, "10♣ 9♥", 19, false, false)], "10♦", "7♠", 0),
        turn(0, "10♣ 9♥", 19, false, "10♦", true, 480, 20),
        stood(0, 19, -1),
        dealer("7♠", "", 17, false, false, false),
        settled(vec![outcome(0, 20, Outcome::Win, 40, 520)]),
    ]);
    let mut table = given(events);

    let events = table.when(place(0, 20)).expect("bet");

    let [Ev::BetPlaced(placed)] = &events[..] else {
        panic!("expected BetPlaced, got {events:?}");
    };
    assert_eq!((placed.round, placed.stack_after), (2, 500));
}

// ================================================================ DealRound

#[test]
fn eu_1476_a_round_cannot_be_dealt_without_bets() {
    let mut table = given(background("2♣"));

    assert_eq!(refused(table.when(v1::DealRound {})), "NO_BETS");
}

#[test]
fn eu_1477_cards_go_to_each_seat_in_order_then_the_dealer_twice() {
    let mut events = background("2♣ 3♣ 9♦ 4♣ 5♣ 7♠");
    events.extend([bet(0, 20, 480), bet(1, 20, 480)]);
    let mut table = given(events);

    let events = table.when(v1::DealRound {}).expect("dealt");

    assert_eq!(
        events,
        vec![
            dealt(
                vec![
                    hand(0, "2♣ 4♣", 6, false, false),
                    hand(1, "3♣ 5♣", 8, false, false),
                ],
                "9♦",
                "7♠",
                0,
            ),
            turn(0, "2♣ 4♣", 6, false, "9♦", true, 480, 20),
        ]
    );
    let state = table.state();
    assert_eq!(state.phase(), Phase::PlayerTurns);
    assert_eq!((state.round, state.turn), (1, 0));
    assert_eq!(state.shoe.len(), 52 - 6, "the dealt cards leave the shoe");
    assert_ledger_balances(state);
}

#[test]
fn eu_1478_a_seat_without_a_bet_is_dealt_no_cards() {
    let mut events = background("5♣ 9♦ 6♣ 7♠");
    events.push(bet(1, 20, 480));
    let mut table = given(events);

    let events = table.when(v1::DealRound {}).expect("dealt");

    assert_eq!(
        events,
        vec![
            dealt(vec![hand(1, "5♣ 6♣", 11, false, false)], "9♦", "7♠", 1),
            turn(1, "5♣ 6♣", 11, false, "9♦", true, 480, 20),
        ]
    );
    let alice = &table.state().seated[&0];
    assert!(
        alice.hand.as_ref().is_none_or(|h| h.cards.is_empty()),
        "seat 0 is dealt no cards"
    );
}

#[test]
fn eu_1481_a_player_blackjack_is_never_asked_to_act() {
    let mut events = background("A♠ 9♣ 8♦ K♠ 7♣ 9♥");
    events.extend([bet(0, 20, 480), bet(1, 20, 480)]);
    let mut table = given(events);

    let events = table.when(v1::DealRound {}).expect("dealt");

    assert_eq!(
        events,
        vec![
            dealt(
                vec![
                    hand(0, "A♠ K♠", 21, true, true),
                    hand(1, "9♣ 7♣", 16, false, false),
                ],
                "8♦",
                "9♥",
                1,
            ),
            turn(1, "9♣ 7♣", 16, false, "8♦", true, 480, 20),
        ]
    );
}

#[test]
fn eu_1494_a_dealer_showing_a_ten_without_blackjack_lets_play_go_on() {
    let mut events = background("10♣ 10♦ 8♥ 6♠");
    events.push(bet(0, 20, 480));
    let mut table = given(events);

    let events = table.when(v1::DealRound {}).expect("dealt");

    assert_eq!(
        events,
        vec![
            dealt(vec![hand(0, "10♣ 8♥", 18, false, false)], "10♦", "6♠", 0),
            turn(0, "10♣ 8♥", 18, false, "10♦", true, 480, 20),
        ]
    );
}

#[test]
fn eu_1493_a_dealer_blackjack_settles_the_round_at_the_deal() {
    let mut events = background("10♣ 9♣ A♦ 8♥ 9♥ K♠");
    events.extend([bet(0, 20, 480), bet(1, 20, 480)]);
    let mut table = given(events);

    let events = table.when(v1::DealRound {}).expect("dealt and settled");

    assert_eq!(
        events,
        vec![
            dealt(
                vec![
                    hand(0, "10♣ 8♥", 18, false, false),
                    hand(1, "9♣ 9♥", 18, false, false),
                ],
                "A♦",
                "K♠",
                -1,
            ),
            dealer("K♠", "", 21, true, false, true),
            settled(vec![
                outcome(0, 20, Outcome::Lose, 0, 480),
                outcome(1, 20, Outcome::Lose, 0, 480),
            ]),
        ]
    );
    assert_eq!(table.state().house_result, 40, "the house wins 40");
    assert_ledger_balances(table.state());
}

#[test]
fn eu_1495_a_player_blackjack_against_a_dealer_blackjack_is_a_push() {
    let mut events = background("A♣ A♦ K♣ Q♠");
    events.push(bet(0, 20, 480));
    let mut table = given(events);

    let events = table.when(v1::DealRound {}).expect("dealt and settled");

    assert_eq!(
        events,
        vec![
            dealt(vec![hand(0, "A♣ K♣", 21, true, true)], "A♦", "Q♠", -1),
            dealer("Q♠", "", 21, true, false, true),
            settled(vec![outcome(0, 20, Outcome::Push, 20, 500)]),
        ]
    );
}

#[test]
fn eu_1496_a_player_blackjack_is_paid_three_to_two_at_once() {
    let mut events = background("A♣ 9♦ K♣ 7♠");
    events.push(bet(0, 20, 480));
    let mut table = given(events);

    let events = table.when(v1::DealRound {}).expect("dealt and settled");

    assert_eq!(
        events,
        vec![
            dealt(vec![hand(0, "A♣ K♣", 21, true, true)], "9♦", "7♠", -1),
            dealer("7♠", "", 16, false, false, false),
            settled(vec![outcome(0, 20, Outcome::Blackjack, 50, 530)]),
        ]
    );
    assert_eq!(table.state().house_result, -30, "the house loses 30");
    assert_ledger_balances(table.state());
}

/// Table "Main" whose first shoe, from `seed`, has its last `left` cards
/// left, with a bet of 20 from each of the first `bettors` players.
fn shoe_running_out(seed: u64, left: usize, bettors: i32) -> Vec<Ev> {
    let full = shuffle(seed, 1);
    let mut events = vec![
        Ev::TableCreated(main_created()),
        Ev::ShoeShuffled(v1::ShoeShuffled {
            shoe_number: 1,
            seed,
            decks: 1,
            cards: full[full.len() - left..].to_vec(),
        }),
        held(B1, ALICE, 0, 500),
        seated(B1, ALICE, 0, 500),
        held(B2, BOB, 1, 500),
        seated(B2, BOB, 1, 500),
    ];
    events.extend((0..bettors).map(|seat| bet(seat, 20, 480)));
    events
}

#[test]
fn eu_1502_the_shoe_is_replaced_before_a_round_that_could_run_it_out() {
    let seed_2 = 13_679_457_532_755_275_413;
    assert_eq!(next_seed(42), seed_2, "the spec's seed for shoe 2");
    for (bettors, left, reshuffles) in
        [(1, 21, true), (1, 22, false), (2, 32, true), (2, 33, false)]
    {
        let mut table = given(shoe_running_out(42, left, bettors));

        let events = table.when(v1::DealRound {}).expect("dealt");

        let case = format!("{bettors} bettors, {left} cards left");
        if reshuffles {
            assert_eq!(
                events[0],
                Ev::ShoeShuffled(v1::ShoeShuffled {
                    shoe_number: 2,
                    seed: seed_2,
                    decks: 1,
                    cards: shuffle(seed_2, 1),
                }),
                "{case}"
            );
            assert!(matches!(events[1], Ev::RoundDealt(_)), "{case}: {events:?}");
            assert_eq!(table.state().shoe_number, 2, "{case}");
        } else {
            assert!(matches!(events[0], Ev::RoundDealt(_)), "{case}: {events:?}");
            assert_eq!(table.state().shoe_number, 1, "{case}");
        }
    }
}

#[test]
fn eu_1503_a_replacement_shoe_deals_in_the_order_its_seed_decides() {
    let seed_2 = 10_451_216_379_200_822_465;
    assert_eq!(next_seed(1), seed_2, "the spec's seed for shoe 2");
    let mut table = given(shoe_running_out(1, 10, 1));

    let events = table.when(v1::DealRound {}).expect("dealt");

    let shoe_2 = shuffle(seed_2, 1);
    let [Ev::ShoeShuffled(shuffled), Ev::RoundDealt(round), ..] = &events[..] else {
        panic!("expected ShoeShuffled then RoundDealt, got {events:?}");
    };
    assert_eq!((shuffled.shoe_number, shuffled.seed), (2, seed_2));
    assert_eq!(round.hands[0].cards, vec![shoe_2[0], shoe_2[2]]);
}

// ================================================================ Hit, Stand, DoubleDown

/// The background, with a bet of 20 from each of `seats`, dealt from a shoe
/// beginning `shoe` as `round`, and the first seat's TurnStarted.
fn dealt_round(shoe: &str, stacks: (i64, i64), seats: &[i32], round: Ev) -> Vec<Ev> {
    let mut events = background_with(shoe, stacks);
    let stack = |seat: i32| if seat == 0 { stacks.0 } else { stacks.1 };
    events.extend(seats.iter().map(|&seat| bet(seat, 20, stack(seat) - 20)));
    // The TurnStarted the deal records when its turn names a seat.
    let first = match &round {
        Ev::RoundDealt(r) if r.turn >= 0 => {
            let hand = r.hands.iter().find(|h| h.seat == r.turn).expect("dealt");
            let left = stack(r.turn) - 20;
            Some(Ev::TurnStarted(v1::TurnStarted {
                round: 1,
                seat: r.turn,
                player_root: bytes(player(r.turn)).into(),
                hand: Some(hand.clone()),
                dealer_up: r.dealer_up,
                can_double: hand.cards.len() == 2 && left >= 20,
                stack: left,
                wager: 20,
            }))
        }
        _ => None,
    };
    events.push(round);
    events.extend(first);
    events
}

/// Alice 2♣ 4♣ (6) and Bob 3♣ 5♣ (8) against 9♦ with 7♠ in the hole; then
/// `next` on the shoe. Seat 0's turn.
fn two_hands_low(next: &str) -> Vec<Ev> {
    dealt_round(
        &format!("2♣ 3♣ 9♦ 4♣ 5♣ 7♠ {next}"),
        (500, 500),
        &[0, 1],
        dealt(
            vec![
                hand(0, "2♣ 4♣", 6, false, false),
                hand(1, "3♣ 5♣", 8, false, false),
            ],
            "9♦",
            "7♠",
            0,
        ),
    )
}

#[test]
fn eu_1480_a_soft_hand_turns_hard_instead_of_busting() {
    let mut table = given(dealt_round(
        "A♣ 9♦ 6♣ 7♠ 10♥",
        (500, 500),
        &[0],
        dealt(vec![hand(0, "A♣ 6♣", 17, true, false)], "9♦", "7♠", 0),
    ));

    let events = table.when(v1::Hit { seat: 0 }).expect("hit");

    assert_eq!(
        events,
        vec![
            card_dealt(0, "10♥", 17, false, false, 0),
            turn(0, "A♣ 6♣ 10♥", 17, false, "9♦", false, 480, 20),
        ]
    );
}

#[test]
fn eu_1482_only_the_seat_on_turn_may_act() {
    let mut table = given(two_hands_low(""));

    assert_eq!(refused(table.when(v1::Hit { seat: 1 })), "NOT_YOUR_TURN");
    assert_eq!(refused(table.when(v1::Stand { seat: 1 })), "NOT_YOUR_TURN");
    assert_eq!(
        refused(table.when(v1::DoubleDown { seat: 1 })),
        "NOT_YOUR_TURN"
    );
}

#[test]
fn eu_1483_hitting_below_21_keeps_the_turn() {
    let mut table = given(two_hands_low("6♥"));

    let events = table.when(v1::Hit { seat: 0 }).expect("hit");

    assert_eq!(
        events,
        vec![
            card_dealt(0, "6♥", 12, false, false, 0),
            turn(0, "2♣ 4♣ 6♥", 12, false, "9♦", false, 480, 20),
        ]
    );
    let state = table.state();
    assert_eq!(state.turn, 0);
    assert_eq!(
        state.seated[&0].hand.as_ref().map(|h| h.cards.clone()),
        Some(cards("2♣ 4♣ 6♥"))
    );
}

#[test]
fn eu_1484_busting_ends_the_hand_and_passes_the_turn() {
    let mut table = given(dealt_round(
        "10♣ 3♣ 9♦ 6♣ 5♣ 7♠ K♥",
        (500, 500),
        &[0, 1],
        dealt(
            vec![
                hand(0, "10♣ 6♣", 16, false, false),
                hand(1, "3♣ 5♣", 8, false, false),
            ],
            "9♦",
            "7♠",
            0,
        ),
    ));

    let events = table.when(v1::Hit { seat: 0 }).expect("hit");

    assert_eq!(
        events,
        vec![
            card_dealt(0, "K♥", 26, false, true, 1),
            turn(1, "3♣ 5♣", 8, false, "9♦", true, 480, 20),
        ]
    );
    let state = table.state();
    assert_eq!(state.turn, 1);
    assert!(state.seated[&0].finished, "a bust finishes the hand");
}

#[test]
fn eu_1485_standing_passes_the_turn() {
    let mut table = given(two_hands_low(""));

    let events = table.when(v1::Stand { seat: 0 }).expect("stood");

    assert_eq!(
        events,
        vec![
            stood(0, 6, 1),
            turn(1, "3♣ 5♣", 8, false, "9♦", true, 480, 20),
        ]
    );
    assert_eq!(table.state().turn, 1);
}

#[test]
fn eu_1486_doubling_doubles_the_wager_deals_one_card_and_ends_the_hand() {
    let mut table = given(dealt_round(
        "6♣ 3♣ 9♦ 5♣ 5♦ 7♠ K♥",
        (500, 500),
        &[0, 1],
        dealt(
            vec![
                hand(0, "6♣ 5♣", 11, false, false),
                hand(1, "3♣ 5♦", 8, false, false),
            ],
            "9♦",
            "7♠",
            0,
        ),
    ));

    let events = table.when(v1::DoubleDown { seat: 0 }).expect("doubled");

    assert_eq!(
        events,
        vec![
            Ev::HandDoubled(v1::HandDoubled {
                seat: 0,
                added: 20,
                card: Some(card("K♥")),
                total: 21,
                busted: false,
                next_turn: 1,
                stack_after: 460,
            }),
            turn(1, "3♣ 5♦", 8, false, "9♦", true, 480, 20),
        ]
    );
    let state = table.state();
    assert_eq!((state.seated[&0].wager, state.seated[&0].stack), (40, 460));
    assert_eq!(state.turn, 1);
    assert_ledger_balances(state);
}

#[test]
fn eu_1487_doubling_is_only_allowed_on_the_first_two_cards() {
    let mut events = two_hands_low("6♥");
    events.extend([
        card_dealt(0, "6♥", 12, false, false, 0),
        turn(0, "2♣ 4♣ 6♥", 12, false, "9♦", false, 480, 20),
    ]);
    let mut table = given(events);

    assert_eq!(
        refused(table.when(v1::DoubleDown { seat: 0 })),
        "DOUBLE_NOT_ALLOWED"
    );
}

#[test]
fn eu_1488_doubling_needs_a_stack_that_covers_the_wager() {
    let mut table = given(dealt_round(
        "5♣ 9♦ 6♣ 7♠",
        (500, 30),
        &[1],
        dealt(vec![hand(1, "5♣ 6♣", 11, false, false)], "9♦", "7♠", 1),
    ));

    assert_eq!(
        refused(table.when(v1::DoubleDown { seat: 1 })),
        "INSUFFICIENT_STACK"
    );
}

#[test]
fn eu_1489_nobody_can_act_between_rounds() {
    let mut table = given(background("2♣"));

    assert_eq!(
        refused(table.when(v1::Hit { seat: 0 })),
        "NO_ROUND_IN_PROGRESS"
    );
    assert_eq!(
        refused(table.when(v1::Stand { seat: 0 })),
        "NO_ROUND_IN_PROGRESS"
    );
    assert_eq!(
        refused(table.when(v1::DoubleDown { seat: 0 })),
        "NO_ROUND_IN_PROGRESS"
    );
}

// ================================================================ dealer play and settlement

/// Alice alone, dealt `alice` against `up` with `hole` in the hole.
fn alice_alone(shoe: &str, alice: SeatHand, up: &str, hole: &str) -> Harness {
    given(dealt_round(
        shoe,
        (500, 500),
        &[0],
        dealt(vec![alice], up, hole, 0),
    ))
}

#[test]
fn eu_1490_the_dealer_stands_on_a_soft_17() {
    let mut table = alice_alone(
        "10♣ A♦ 9♥ 6♠",
        hand(0, "10♣ 9♥", 19, false, false),
        "A♦",
        "6♠",
    );

    let events = table.when(v1::Stand { seat: 0 }).expect("stood");

    assert_eq!(
        events,
        vec![
            stood(0, 19, -1),
            dealer("6♠", "", 17, true, false, false),
            settled(vec![outcome(0, 20, Outcome::Win, 40, 520)]),
        ]
    );
}

#[test]
fn eu_1491_the_dealer_draws_while_below_17() {
    let mut table = alice_alone(
        "10♣ 10♦ 8♥ 6♠ 5♥",
        hand(0, "10♣ 8♥", 18, false, false),
        "10♦",
        "6♠",
    );

    let events = table.when(v1::Stand { seat: 0 }).expect("stood");

    assert_eq!(
        events,
        vec![
            stood(0, 18, -1),
            dealer("6♠", "5♥", 21, false, false, false),
            settled(vec![outcome(0, 20, Outcome::Lose, 0, 480)]),
        ]
    );
}

#[test]
fn eu_1492_the_dealer_does_not_draw_when_no_hand_is_left_to_beat() {
    let mut table = given(dealt_round(
        "A♠ 10♣ 9♦ K♠ 6♣ 7♠ K♥",
        (500, 500),
        &[0, 1],
        dealt(
            vec![
                hand(0, "A♠ K♠", 21, true, true),
                hand(1, "10♣ 6♣", 16, false, false),
            ],
            "9♦",
            "7♠",
            1,
        ),
    ));

    let events = table.when(v1::Hit { seat: 1 }).expect("hit");

    assert_eq!(
        events,
        vec![
            card_dealt(1, "K♥", 26, false, true, -1),
            dealer("7♠", "", 16, false, false, false),
            settled(vec![
                outcome(0, 20, Outcome::Blackjack, 50, 530),
                outcome(1, 20, Outcome::Lose, 0, 480),
            ]),
        ]
    );
    assert_ledger_balances(table.state());
}

#[test]
fn eu_1497_standing_hands_are_paid_by_comparing_with_the_dealer() {
    // (shoe, Alice, dealer's play, outcome, returned, stack after)
    let cases = [
        (
            "10♣ 10♦ 9♥ 7♠",
            19,
            ("7♠", "", 17, false),
            Outcome::Win,
            40,
            520,
        ),
        (
            "10♣ 10♦ 8♥ 8♠",
            18,
            ("8♠", "", 18, false),
            Outcome::Push,
            20,
            500,
        ),
        (
            "10♣ 10♦ 7♥ 8♠",
            17,
            ("8♠", "", 18, false),
            Outcome::Lose,
            0,
            480,
        ),
        (
            "10♣ 10♦ 2♥ 6♠ K♥",
            12,
            ("6♠", "K♥", 26, true),
            Outcome::Win,
            40,
            520,
        ),
    ];
    for (shoe, total, (hole, drawn, dealer_total, busted), result, returned, after) in cases {
        let top = cards(shoe);
        let alice = SeatHand {
            seat: 0,
            cards: vec![top[0], top[2]],
            total,
            soft: false,
            blackjack: false,
        };
        let mut table = given(dealt_round(
            shoe,
            (500, 500),
            &[0],
            Ev::RoundDealt(v1::RoundDealt {
                round: 1,
                hands: vec![alice],
                dealer_up: Some(top[1]),
                dealer_hole: Some(top[3]),
                turn: 0,
            }),
        ));

        let events = table.when(v1::Stand { seat: 0 }).expect("stood");

        assert_eq!(
            events,
            vec![
                stood(0, total, -1),
                dealer(hole, drawn, dealer_total, false, busted, false),
                settled(vec![outcome(0, 20, result, returned, after)]),
            ],
            "shoe {shoe}"
        );
        assert_ledger_balances(table.state());
    }
}

#[test]
fn eu_1498_a_busted_hand_loses_even_when_the_dealer_busts_too() {
    let mut events = dealt_round(
        "10♣ 10♥ 10♦ 6♥ 2♥ 6♠ K♣ K♥",
        (500, 500),
        &[0, 1],
        dealt(
            vec![
                hand(0, "10♣ 6♥", 16, false, false),
                hand(1, "10♥ 2♥", 12, false, false),
            ],
            "10♦",
            "6♠",
            0,
        ),
    );
    events.extend([
        card_dealt(0, "K♣", 26, false, true, 1),
        turn(1, "10♥ 2♥", 12, false, "10♦", true, 480, 20),
    ]);
    let mut table = given(events);

    let events = table.when(v1::Stand { seat: 1 }).expect("stood");

    assert_eq!(
        events,
        vec![
            stood(1, 12, -1),
            dealer("6♠", "K♥", 26, false, true, false),
            settled(vec![
                outcome(0, 20, Outcome::Lose, 0, 480),
                outcome(1, 20, Outcome::Win, 40, 520),
            ]),
        ]
    );
    assert_eq!(table.state().house_result, 0, "the house breaks even");
}

#[test]
fn eu_1499_a_doubled_hand_is_paid_on_the_doubled_wager() {
    let mut events = dealt_round(
        "6♣ 3♣ 9♦ 5♣ 5♦ 7♠ K♥ 2♥",
        (500, 500),
        &[0, 1],
        dealt(
            vec![
                hand(0, "6♣ 5♣", 11, false, false),
                hand(1, "3♣ 5♦", 8, false, false),
            ],
            "9♦",
            "7♠",
            0,
        ),
    );
    events.push(Ev::HandDoubled(v1::HandDoubled {
        seat: 0,
        added: 20,
        card: Some(card("K♥")),
        total: 21,
        busted: false,
        next_turn: 1,
        stack_after: 460,
    }));
    events.push(turn(1, "3♣ 5♦", 8, false, "9♦", true, 480, 20));
    let mut table = given(events);

    let events = table.when(v1::Stand { seat: 1 }).expect("stood");

    assert_eq!(
        events,
        vec![
            stood(1, 8, -1),
            dealer("7♠", "2♥", 18, false, false, false),
            settled(vec![
                outcome(0, 40, Outcome::Win, 80, 540),
                outcome(1, 20, Outcome::Lose, 0, 480),
            ]),
        ]
    );
    assert_eq!(table.state().house_result, -20, "the house loses 20");
    assert_ledger_balances(table.state());
}

#[test]
fn eu_1500_the_last_action_plays_the_dealer_and_settles_in_the_same_step() {
    let mut table = alice_alone(
        "10♣ 10♦ 9♥ 7♠",
        hand(0, "10♣ 9♥", 19, false, false),
        "10♦",
        "7♠",
    );

    // One command, one decision: all three events come back from it.
    let events = table.when(v1::Stand { seat: 0 }).expect("stood");

    assert_eq!(
        events,
        vec![
            stood(0, 19, -1),
            dealer("7♠", "", 17, false, false, false),
            settled(vec![outcome(0, 20, Outcome::Win, 40, 520)]),
        ]
    );
    let state = table.state();
    assert_eq!(
        state.phase(),
        Phase::Idle,
        "ready for the next round's bets"
    );
    assert_eq!(state.round, 1);
    assert!(state.seated.values().all(|s| s.wager == 0), "no wager left");
    assert_ledger_balances(state);
}

// ================================================================ decision points (TurnStarted)

#[test]
fn eu_1505_the_deal_records_the_first_seats_turn_with_its_hand_and_the_up_card() {
    let mut events = background("2♣ 3♣ 9♦ 4♣ 5♣ 7♠");
    events.extend([bet(0, 20, 480), bet(1, 20, 480)]);
    let mut table = given(events);

    let events = table.when(v1::DealRound {}).expect("dealt");

    // Right after RoundDealt, in the same decision; may double, stack 480,
    // wager 20, and Alice named as the seat's player.
    assert_eq!(
        events.last(),
        Some(&turn(0, "2♣ 4♣", 6, false, "9♦", true, 480, 20))
    );
    assert!(matches!(events[events.len() - 2], Ev::RoundDealt(_)));
}

#[test]
fn eu_1506_standing_starts_the_next_seats_turn_with_that_seats_own_hand() {
    let mut table = given(two_hands_low(""));

    let events = table.when(v1::Stand { seat: 0 }).expect("stood");

    assert_eq!(
        events,
        vec![
            stood(0, 6, 1),
            turn(1, "3♣ 5♣", 8, false, "9♦", true, 480, 20),
        ]
    );
}

#[test]
fn eu_1507_a_turn_never_shows_the_dealers_hole_card_or_the_shoe() {
    let mut events = background("2♣ 3♣ 9♦ 4♣ 5♣ 7♠");
    events.extend([bet(0, 20, 480), bet(1, 20, 480)]);
    let mut table = given(events);

    let events = table.when(v1::DealRound {}).expect("dealt");

    let turns: Vec<&v1::TurnStarted> = events
        .iter()
        .filter_map(|e| match e {
            Ev::TurnStarted(t) => Some(t),
            _ => None,
        })
        .collect();
    assert!(!turns.is_empty(), "a turn started: {events:?}");
    let undealt = &table.state().shoe;
    for turn in turns {
        let shown: Vec<Card> = turn
            .hand
            .iter()
            .flat_map(|h| h.cards.iter().copied())
            .chain(turn.dealer_up)
            .collect();
        assert!(!shown.contains(&card("7♠")), "the hole card: {turn:?}");
        assert!(
            shown.iter().all(|c| !undealt.contains(c)),
            "an undealt card: {turn:?}"
        );
    }
}

#[test]
fn eu_1508_a_hit_that_keeps_the_turn_starts_it_again_and_can_no_longer_double() {
    let mut table = given(two_hands_low("6♥"));

    let events = table.when(v1::Hit { seat: 0 }).expect("hit");

    assert_eq!(
        events.last(),
        Some(&turn(0, "2♣ 4♣ 6♥", 12, false, "9♦", false, 480, 20))
    );
}

#[test]
fn eu_1509_a_stack_smaller_than_the_wager_cannot_double() {
    let mut events = background_with("2♣ 3♣ 9♦ 4♣ 5♣ 7♠", (30, 500));
    events.extend([bet(0, 20, 10), bet(1, 20, 480)]);
    let mut table = given(events);

    let events = table.when(v1::DealRound {}).expect("dealt");

    assert_eq!(
        events.last(),
        Some(&turn(0, "2♣ 4♣", 6, false, "9♦", false, 10, 20))
    );
}

#[test]
fn eu_1510_the_action_that_settles_the_round_starts_no_turn() {
    let mut events = two_hands_low("10♥");
    events.extend([
        stood(0, 6, 1),
        turn(1, "3♣ 5♣", 8, false, "9♦", true, 480, 20),
    ]);
    let mut table = given(events);

    let events = table.when(v1::Stand { seat: 1 }).expect("stood");

    // 9♦ 7♠ draws 10♥ and busts: both standing hands win.
    assert_eq!(
        events,
        vec![
            stood(1, 8, -1),
            dealer("7♠", "10♥", 26, false, true, false),
            settled(vec![
                outcome(0, 20, Outcome::Win, 40, 520),
                outcome(1, 20, Outcome::Win, 40, 520),
            ]),
        ]
    );
}

// ================================================================ whole hands, by command only

#[test]
fn a_minimal_hand_plays_from_create_table_to_settlement_by_command() {
    // Seed 42's shoe begins 7♣ 3♥ K♣ A♠ 2♠ A♣ (EU-1440): Alice 7♣ K♣ (17)
    // against 3♥ and A♠; the dealer draws 2♠ and A♣ to a soft 17: a push.
    let mut table = given(vec![]);
    table.when(create_main(42)).expect("created");
    table.when(request(ALICE, 0, 500, B1)).expect("held");
    table.when(confirm(B1)).expect("seated");
    table.when(place(0, 20)).expect("bet");

    let dealing = table.when(v1::DealRound {}).expect("dealt");
    assert_eq!(
        dealing,
        vec![
            dealt(vec![hand(0, "7♣ K♣", 17, false, false)], "3♥", "A♠", 0),
            turn(0, "7♣ K♣", 17, false, "3♥", true, 480, 20),
        ]
    );

    let standing = table.when(v1::Stand { seat: 0 }).expect("stood");
    assert_eq!(
        standing,
        vec![
            stood(0, 17, -1),
            dealer("A♠", "2♠ A♣", 17, true, false, false),
            settled(vec![outcome(0, 20, Outcome::Push, 20, 500)]),
        ]
    );
    assert_ledger_balances(table.state());
}

#[test]
fn eu_1504_every_round_keeps_the_tables_chips_in_balance() {
    for seed in [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 42] {
        let mut table = given(vec![]);
        let step = |table: &mut Harness, events: Then| {
            let events = events.unwrap_or_else(|e| panic!("seed {seed}: refused: {e:?}"));
            assert_ledger_balances(table.state());
            events
        };
        let created = table.when(create_main(seed));
        step(&mut table, created);
        for (player, seat, buy_in, wager) in [(ALICE, 0, B1, 20), (BOB, 1, B2, 30)] {
            let held = table.when(request(player, seat, 500, buy_in));
            step(&mut table, held);
            let seated = table.when(confirm(buy_in));
            step(&mut table, seated);
            let placed = table.when(place(seat, wager));
            step(&mut table, placed);
        }

        let dealt = table.when(v1::DealRound {});
        let mut last = step(&mut table, dealt);
        // Every player stands as soon as it is their turn.
        for _ in 0..2 {
            if table.state().phase() != Phase::PlayerTurns {
                break;
            }
            let seat = table.state().turn;
            let stood = table.when(v1::Stand { seat });
            last = step(&mut table, stood);
        }

        assert!(
            matches!(last.last(), Some(Ev::RoundSettled(s)) if s.round == 1),
            "seed {seed}: round 1 is settled, got {last:?}"
        );
        assert_eq!(table.state().phase(), Phase::Idle, "seed {seed}");
    }
}

// ================================================================ continuation steps

fn step_context() -> Context {
    Context::new(
        Timestamp {
            seconds: 1_000,
            nanos: 0,
        },
        cover(),
        id(99),
        id(99),
    )
}

/// The background with Alice and Bob's bets, from a shoe beginning `shoe`.
fn bets_on(shoe: &str) -> TableAggregate {
    let mut events = background(shoe);
    events.extend([bet(0, 20, 480), bet(1, 20, 480)]);
    given(events).stream.state().clone()
}

#[test]
fn the_deal_step_does_nothing_when_re_run_on_its_own_result() {
    let dealt = assert_step_idempotent(
        bets_on("2♣ 3♣ 9♦ 4♣ 5♣ 7♠"),
        TableAggregate::deal,
        &step_context(),
    );
    assert_eq!(dealt.state().turn, 0, "the first run dealt");
}

#[test]
fn the_deal_step_does_nothing_when_re_run_after_a_dealer_blackjack_settled() {
    let settled = assert_step_idempotent(
        bets_on("10♣ 9♣ A♦ 8♥ 9♥ K♠"),
        TableAggregate::deal,
        &step_context(),
    );
    assert_eq!(
        settled.state().phase(),
        Phase::Idle,
        "the first run settled"
    );
}

#[test]
fn the_after_action_step_does_nothing_when_re_run_on_its_own_result() {
    // A turn to announce, then a round to settle.
    let on_turn = given(two_hands_low("")).stream.state().clone();
    assert_step_idempotent(on_turn, TableAggregate::after_action, &step_context());

    let mut events = two_hands_low("10♥");
    events.extend([
        stood(0, 6, 1),
        turn(1, "3♣ 5♣", 8, false, "9♦", true, 480, 20),
        stood(1, 8, -1),
    ]);
    let settled = assert_step_idempotent(
        given(events).stream.state().clone(),
        TableAggregate::after_action,
        &step_context(),
    );
    assert_eq!(settled.state().phase(), Phase::Idle);
}

// ================================================================ ReleaseSeat

#[test]
fn eu_1453_releasing_a_held_seat_frees_it() {
    let mut events = main_table();
    events.push(held(B1, ALICE, 0, 500));
    let mut table = given(events);

    let release = v1::ReleaseSeat {
        buy_in_id: bytes(B1).into(),
        reason: "funds were refused".into(),
    };
    let events = table.when(release).expect("released");

    assert_eq!(
        events,
        vec![Ev::SeatReleased(v1::SeatReleased {
            buy_in_id: bytes(B1).into(),
            player_root: bytes(ALICE).into(),
            seat: 0,
            reason: "funds were refused".into(),
        })]
    );
    assert!(table.state().seat_holds.is_empty(), "seat 0 is free");
    // Free: Bob can now ask for it.
    table
        .when(request(BOB, 0, 500, B2))
        .expect("seat 0 can be requested");
}

#[test]
fn eu_1454_releasing_a_seat_that_is_no_longer_held_changes_nothing() {
    let mut table = given(main_table());

    let release = v1::ReleaseSeat {
        buy_in_id: bytes(0x09).into(),
        reason: "funds were refused".into(),
    };
    assert_eq!(table.when(release).expect("ok"), vec![]);
}

// ================================================================ LeaveTable

/// Alice seated at seat 0 with `stack`, after table "Main" (event 3).
fn alice_seated(stack: i64) -> Vec<Ev> {
    let mut events = main_table();
    events.extend([held(B1, ALICE, 0, stack), seated(B1, ALICE, 0, stack)]);
    events
}

/// UUIDv5(namespace = the table root, [0x7a; 16], read as a UUID,
/// name = "cashout/<sequence>"), from Python's `uuid.uuid5`.
fn cashout_id(sequence: u64) -> Vec<u8> {
    match sequence {
        4 => vec![
            34, 253, 80, 2, 216, 234, 83, 209, 160, 246, 57, 63, 200, 211, 187, 223,
        ],
        5 => vec![
            176, 120, 52, 182, 203, 213, 82, 247, 184, 245, 7, 64, 141, 142, 43, 119,
        ],
        _ => panic!("no vector for sequence {sequence}"),
    }
}

#[test]
fn eu_1460_leaving_cashes_out_the_whole_stack_and_frees_the_seat() {
    let mut table = given(alice_seated(640));

    let events = table.when(v1::LeaveTable { seat: 0 }).expect("left");

    assert_eq!(
        events,
        vec![Ev::PlayerCashedOut(v1::PlayerCashedOut {
            cashout_id: cashout_id(4).into(),
            player_root: bytes(ALICE).into(),
            seat: 0,
            amount: 640,
        })]
    );
    let state = table.state();
    assert!(state.seated.is_empty(), "seat 0 is free");
    assert_eq!(state.chips_out, 640, "table Main has paid out 640 chips");
    assert_ledger_balances(state);
}

#[test]
fn eu_1461_a_cash_out_is_identified_by_the_table_and_its_position_in_the_history() {
    // One event more before it: the cash-out is event 5, not 4.
    let mut events = alice_seated(640);
    events.push(held(B2, BOB, 1, 500));
    let mut table = given(events);

    let events = table.when(v1::LeaveTable { seat: 0 }).expect("left");

    let [Ev::PlayerCashedOut(cashed)] = &events[..] else {
        panic!("expected PlayerCashedOut, got {events:?}");
    };
    assert_eq!(cashed.cashout_id[..], cashout_id(5)[..]);
}

#[test]
fn eu_1462_a_player_with_a_bet_in_play_cannot_leave() {
    let mut events = alice_seated(500);
    events.push(bet(0, 20, 480));
    let mut table = given(events);

    assert_eq!(
        refused(table.when(v1::LeaveTable { seat: 0 })),
        "WAGER_IN_PLAY"
    );
}

#[test]
fn eu_1463_nobody_can_leave_an_empty_seat() {
    let mut table = given(main_table());

    assert_eq!(
        refused(table.when(v1::LeaveTable { seat: 2 })),
        "NOT_SEATED"
    );
}

#[test]
fn leaving_a_table_that_does_not_exist_is_refused() {
    let mut table = given(vec![]);

    assert_eq!(
        refused(table.when(v1::LeaveTable { seat: 0 })),
        "TABLE_NOT_FOUND"
    );
}

// ================================================================ two callers at once
//
// cluster.feature's conflict scenarios (EA-0022..0024), forced in process:
// both commands are prepared from the same head (the same expected
// sequence) and sent one after the other, so the second always meets the
// first's events. table.proto: PlaceBet is sent with AGGREGATE_HANDLES,
// the turn actions with STRICT.
//
// The AGGREGATE_HANDLES pair is ignored, not deleted: a marked-up
// aggregate's handler cannot reach the kernel's `Tx`, and the expansion
// never calls `resolve_conflicts`, so angzarr-embedded (b3e3f20) refuses
// even the first bet as Unresolved and dead-letters it. The table decides
// against its full history already; un-ignore them once the markup
// acknowledges the window.

/// Sent as prepared from the table at `head`, under `resolution`.
fn prepared(head: u64, resolution: ConflictResolution) -> Option<Handling> {
    Some(Handling {
        expected_sequence: Some(head),
        conflict_resolution: resolution.into(),
    })
}

fn head(table: &Harness) -> u64 {
    table.stream.events().len() as u64
}

#[test]
#[ignore = "angzarr-embedded gap: its markup never acknowledges an AGGREGATE_HANDLES window (Tx::resolve_conflicts), so every such command fails Unresolved"]
fn ea_0022_a_second_bet_for_the_same_seat_is_refused_by_the_table_itself() {
    let mut table = given(background("2♣"));
    let at = head(&table);
    let handles = || prepared(at, ConflictResolution::AggregateHandles);

    let first = table.when_handled(place(0, 20), handles());
    let second = table.when_handled(place(0, 20), handles());

    assert_eq!(first.expect("placed"), vec![bet(0, 20, 480)]);
    assert_eq!(refused(second), "ALREADY_BET");
    assert_eq!(table.state().seated[&0].wager, 20);
}

#[test]
#[ignore = "angzarr-embedded gap: its markup never acknowledges an AGGREGATE_HANDLES window (Tx::resolve_conflicts), so every such command fails Unresolved"]
fn ea_0023_bets_for_different_seats_prepared_at_the_same_moment_both_land() {
    let mut table = given(background("2♣"));
    let at = head(&table);
    let handles = || prepared(at, ConflictResolution::AggregateHandles);

    table.when_handled(place(0, 20), handles()).expect("placed");
    table.when_handled(place(1, 30), handles()).expect("placed");

    let state = table.state();
    assert_eq!((state.seated[&0].wager, state.seated[&1].wager), (20, 30));
}

#[test]
fn ea_0024_two_hits_for_one_hand_at_the_same_moment_deal_exactly_one_card() {
    // Seed 1 deals Alice 7♦ 4♦ against a dealer showing 9♣.
    let mut table = given(vec![]);
    table.when(create_main(1)).expect("created");
    table.when(request(ALICE, 0, 500, B1)).expect("held");
    table.when(confirm(B1)).expect("seated");
    table.when(place(0, 20)).expect("bet");
    let dealt = table.when(v1::DealRound {}).expect("dealt");
    let Ev::RoundDealt(round) = &dealt[0] else {
        panic!("expected RoundDealt, got {dealt:?}");
    };
    assert_eq!(
        (round.hands[0].cards.clone(), round.dealer_up),
        (cards("7♦ 4♦"), Some(card("9♣")))
    );
    let at = head(&table);
    let strict = || prepared(at, ConflictResolution::Strict);

    let first = table.when_handled(v1::Hit { seat: 0 }, strict());
    let second = table.when_handled(v1::Hit { seat: 0 }, strict());

    let first = first.expect("one hit lands");
    assert!(
        matches!(&first[0], Ev::CardDealt(c) if c.card == Some(card("8♦"))),
        "the 8♦: {first:?}"
    );
    // Out of date, and retryable.
    assert!(
        matches!(second, Err(Failure::Kernel(KernelError::Stale { .. }))),
        "refused as stale: {second:?}"
    );
}
