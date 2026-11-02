//! The buy-in, step by step (process_manager.feature, EU-1550 to EU-1564
//! and EU-1610 to EU-1612), and policy-table-wallet (policy.feature,
//! EU-1541 and EU-1542).
//!
//! Each step is the scenario's chain, link by link: the news an aggregate
//! recorded, the policy that reports it to the buy-in (its implementation
//! called directly, as its generated adapter calls it), the buy-in process
//! manager recording it on its own stream (angzarr-embedded's test
//! `Harness`, the kernel over an in-memory store, the stream's root the
//! conversation's correlation), and the policy that turns the buy-in's
//! event into what it asks of the table or the wallet. The whole flow, the
//! kernel delivering each link, is `tests/buy_in.rs`.

// The harness's `Then` carries angzarr-embedded's `Failure` unboxed: its
// size is not this crate's to choose.
#![allow(clippy::result_large_err)]

use angzarr_embedded::markup::{CommandContext, PolicyContext};
use angzarr_embedded::testing::{Harness as Kernel, Samples};
use angzarr_embedded::v1::{
    command_page, CommandBook, CommandPage, Cover, EventRef, Fact, FactOrigin, Uuid,
};
use angzarr_embedded::{pack, Failure, Handled, Payload, Registry, Rejected, Timestamp};
use examples_proto::v1::{self, buy_in_state::Phase, BuyInState};
use prost::{Message, Name};

use crate::adapters::{BuyInProcessManager, TableAggregate};
use crate::domain::{
    BuyInProcessManagerEvent, BuyInProcessManagerFact, BuyInProcessManagerOutput,
    BuyInTablePolicy as _, BuyInWalletPolicy as _, TableAggregateEvent, TableAggregateOutput,
    TableBuyInPolicy as _, TableWalletPolicy as _, WalletAggregateOutput, WalletBuyInPolicy as _,
};
use crate::{
    buy_in_table_policy_impl::BuyInTablePolicyImpl,
    buy_in_wallet_policy_impl::BuyInWalletPolicyImpl,
    table_buy_in_policy_impl::TableBuyInPolicyImpl,
    table_wallet_policy_impl::TableWalletPolicyImpl,
    wallet_buy_in_policy_impl::WalletBuyInPolicyImpl,
};

const ALICE: u8 = 0xa1;
const BOB: u8 = 0xb0;
const MAIN: u8 = 0x7a;
const B1: u8 = 0x01;
/// The conversation buy-in B1 started in: its process manager's root.
const C1: u8 = 0xc1;

fn bytes(b: u8) -> Vec<u8> {
    vec![b; 16]
}

fn root(b: u8) -> Uuid {
    Uuid::from_bytes([b; 16])
}

type Ev = BuyInProcessManagerEvent;
type Then = Result<Vec<Ev>, Failure<Rejected>>;

fn ev<M: Message + Name>(event: M) -> Ev {
    Ev::from_any(&pack(&event)).expect("a buy-in event")
}

/// The domain type of a wire message.
fn domain<W, D: TryFrom<W>>(wire: W) -> D
where
    D::Error: std::fmt::Debug,
{
    D::try_from(wire).expect("a domain value")
}

/// The context of an event of the `domain` stream at `stream`, recorded in
/// the conversation `correlation`.
fn context(domain: &str, stream: u8, correlation: u8) -> PolicyContext {
    PolicyContext::new(
        EventRef {
            stream: Some(Cover {
                domain: domain.to_string(),
                root: Some(root(stream)),
                edition: None,
            }),
            sequence: 3,
        },
        root(correlation),
        Timestamp {
            seconds: 1_000,
            nanos: 0,
        },
    )
}

/// The buy-in alone: what it records, its policies called by the tests.
fn registry() -> Registry {
    let mut registry = Registry::new();
    registry
        .process_manager::<BuyInProcessManager>(BuyInProcessManager::default)
        .expect("the buy-in registers");
    registry
}

/// The table alone.
fn table_registry() -> Registry {
    let mut registry = Registry::new();
    registry
        .aggregate::<TableAggregate>(TableAggregate::default)
        .expect("the table registers");
    registry
}

fn cover(domain: &str, b: u8) -> Cover {
    Cover {
        domain: domain.to_string(),
        root: Some(root(b)),
        edition: None,
    }
}

/// The buy-in of conversation `C1`, its history `events`.
struct BuyIn {
    kernel: Kernel<BuyInProcessManager>,
    facts: u32,
}

fn buy_in(events: Vec<Ev>) -> BuyIn {
    let mut kernel =
        Kernel::new(registry(), cover("buy-in", C1)).with_samples(Samples::starting_at(500));
    kernel.given(events);
    BuyIn { kernel, facts: 0 }
}

impl BuyIn {
    /// The book delivering `fact` to the buy-in, as a policy's arrives:
    /// in the Fact arm, under the test's next origin.
    fn book(&mut self, fact: BuyInProcessManagerOutput) -> CommandBook {
        self.facts += 1;
        CommandBook {
            cover: Some(cover("buy-in", C1)),
            page: Some(CommandPage {
                body: Some(command_page::Body::Fact(Fact {
                    payload: Some(output_any(&fact)),
                })),
                fact_origin: Some(FactOrigin {
                    external_id: format!("report-{}", self.facts),
                    description: "a policy's report".to_string(),
                }),
                // The conversation the buy-in is tracked under.
                correlation_id: Some(root(C1)),
                ..CommandPage::default()
            }),
            ..CommandBook::default()
        }
    }

    /// Deliver `fact`: the events the buy-in records.
    fn when(&mut self, fact: BuyInProcessManagerOutput) -> Then {
        let book = self.book(fact);
        self.again(&book)
    }

    /// Deliver `book` (again) exactly as it was.
    fn again(&mut self, book: &CommandBook) -> Then {
        self.kernel.when_book(book.clone()).result()
    }

    fn state(&self) -> BuyInState {
        self.kernel.state().0.state.clone()
    }
}

/// A buy-in input, packed as its wire message.
fn output_any(fact: &BuyInProcessManagerOutput) -> angzarr_embedded::Any {
    use BuyInProcessManagerOutput as O;
    match fact {
        O::StartBuyIn(f) => pack(&f.to_wire()),
        O::RecordFundsHeld(f) => pack(&f.to_wire()),
        O::RecordPlayerSeated(f) => pack(&f.to_wire()),
        O::RecordFundsCaptured(f) => pack(&f.to_wire()),
        O::SeatConfirmationRefused(f) => pack(&f.to_wire()),
    }
}

use angzarr_embedded::markup::DomainMessage as _;

// ---------------------------------------------------------------- the news

fn seat_held() -> v1::SeatHeld {
    v1::SeatHeld {
        buy_in_id: bytes(B1),
        player_root: bytes(ALICE),
        seat: 0,
        amount: 500,
    }
}

fn funds_held(hold: u8) -> v1::FundsHeld {
    v1::FundsHeld {
        hold_id: bytes(hold),
        table_root: bytes(MAIN),
        amount: 500,
    }
}

fn player_seated() -> v1::PlayerSeated {
    v1::PlayerSeated {
        buy_in_id: bytes(B1),
        player_root: bytes(ALICE),
        seat: 0,
        stack: 500,
    }
}

fn funds_captured() -> v1::FundsCaptured {
    v1::FundsCaptured {
        hold_id: bytes(B1),
        table_root: bytes(MAIN),
        amount: 500,
    }
}

// ---------------------------------------------------------------- the buy-in's history

fn started() -> Ev {
    ev(v1::BuyInStarted {
        buy_in_id: bytes(B1),
        player_root: bytes(ALICE),
        table_root: bytes(MAIN),
        seat: 0,
        amount: 500,
    })
}

fn b1_funds_held() -> Ev {
    ev(v1::BuyInFundsHeld {
        buy_in_id: bytes(B1),
        table_root: bytes(MAIN),
    })
}

fn b1_seated() -> Ev {
    ev(v1::BuyInSeated {
        buy_in_id: bytes(B1),
        stack: 500,
        player_root: bytes(ALICE),
    })
}

// ---------------------------------------------------------------- the links

/// policy-table-buy-in on the table's `event`, recorded in conversation C1:
/// the one fact it sends, with its root.
fn table_reports(event: TableAggregateEvent) -> (Vec<u8>, BuyInProcessManagerOutput) {
    let ctx = context("table", MAIN, C1);
    let sent = match event {
        TableAggregateEvent::SeatHeld(e) => TableBuyInPolicyImpl.seat_held(&bytes(MAIN), &e, &ctx),
        TableAggregateEvent::PlayerSeated(e) => {
            TableBuyInPolicyImpl.player_seated(&bytes(MAIN), &e, &ctx)
        }
        other => panic!("policy-table-buy-in does not react to {other:?}"),
    };
    one(sent.expect("no bug"))
}

/// policy-wallet-buy-in on Alice's `event`, recorded in conversation C1.
fn wallet_reports<E>(event: E) -> (Vec<u8>, BuyInProcessManagerOutput)
where
    E: Into<WalletNews>,
{
    let ctx = context("wallet", ALICE, C1);
    let sent = match event.into() {
        WalletNews::Held(e) => WalletBuyInPolicyImpl.funds_held(&bytes(ALICE), &e, &ctx),
        WalletNews::Captured(e) => WalletBuyInPolicyImpl.funds_captured(&bytes(ALICE), &e, &ctx),
    };
    one(sent.expect("no bug"))
}

/// The wallet's news the buy-in hears of.
enum WalletNews {
    Held(crate::domain::FundsHeld),
    Captured(crate::domain::FundsCaptured),
}

impl From<v1::FundsHeld> for WalletNews {
    fn from(e: v1::FundsHeld) -> Self {
        WalletNews::Held(domain(e))
    }
}

impl From<v1::FundsCaptured> for WalletNews {
    fn from(e: v1::FundsCaptured) -> Self {
        WalletNews::Captured(domain(e))
    }
}

#[track_caller]
fn one<T: std::fmt::Debug>(mut sent: Vec<T>) -> T {
    assert_eq!(sent.len(), 1, "one output: {sent:?}");
    sent.remove(0)
}

/// What policy-buy-in-wallet asks of the wallet on the buy-in's `event`.
fn asks_wallet(event: &Ev) -> Vec<(Vec<u8>, WalletAggregateOutput)> {
    let ctx = context("buy-in", C1, C1);
    let source = bytes(C1);
    let sent = match event {
        Ev::BuyInStarted(e) => BuyInWalletPolicyImpl.buy_in_started(&source, e, &ctx),
        Ev::BuyInSeated(e) => BuyInWalletPolicyImpl.buy_in_seated(&source, e, &ctx),
        Ev::BuyInSeatLost(e) => BuyInWalletPolicyImpl.buy_in_seat_lost(&source, e, &ctx),
        _ => Ok(Vec::new()),
    };
    sent.expect("no bug")
}

/// What policy-buy-in-table asks of the table on the buy-in's `event`.
fn asks_table(event: &Ev) -> Vec<(Vec<u8>, TableAggregateOutput)> {
    let ctx = context("buy-in", C1, C1);
    let source = bytes(C1);
    let sent = match event {
        Ev::BuyInFundsHeld(e) => BuyInTablePolicyImpl.buy_in_funds_held(&source, e, &ctx),
        Ev::BuyInFailed(e) => BuyInTablePolicyImpl.buy_in_failed(&source, e, &ctx),
        _ => Ok(Vec::new()),
    };
    sent.expect("no bug")
}

/// What the buy-in asks of the table, and of the wallet.
type Asks = (
    Vec<(Vec<u8>, TableAggregateOutput)>,
    Vec<(Vec<u8>, WalletAggregateOutput)>,
);

/// Every event's asks, of the table and of the wallet.
fn asks(events: &[Ev]) -> Asks {
    (
        events.iter().flat_map(asks_table).collect(),
        events.iter().flat_map(asks_wallet).collect(),
    )
}

// ================================================================ the happy path

#[test]
fn eu_1550_a_held_seat_starts_a_buy_in_and_asks_the_wallet_to_hold_the_money() {
    let mut buy_in = buy_in(vec![]);

    let (to, start) = table_reports(domain::<_, crate::domain::SeatHeld>(seat_held()).into());
    assert_eq!(to, bytes(C1), "the buy-in of the event's conversation");
    let events = buy_in.when(start).expect("started");

    assert_eq!(events, vec![started()]);
    let state = buy_in.state();
    assert_eq!(state.phase(), Phase::AwaitingHold, "waiting for funds");
    // The wallet is asked, by a command it may refuse: the request is
    // decided (by the wallet) before the buy-in moves on.
    assert_eq!(
        asks(&events),
        (
            vec![],
            vec![(
                bytes(ALICE),
                WalletAggregateOutput::HoldFunds(domain(v1::HoldFunds {
                    hold_id: bytes(B1),
                    table_root: bytes(MAIN),
                    amount: 500,
                }))
            )]
        )
    );
}

#[test]
fn eu_1551_held_money_leads_to_confirming_the_seat() {
    let mut buy_in = buy_in(vec![started()]);

    let (to, held) = wallet_reports(funds_held(B1));
    assert_eq!(to, bytes(C1));
    let events = buy_in.when(held).expect("recorded");

    assert_eq!(events, vec![b1_funds_held()]);
    assert_eq!(buy_in.state().phase(), Phase::AwaitingSeat);
    assert_eq!(
        asks(&events),
        (
            vec![(
                bytes(MAIN),
                TableAggregateOutput::ConfirmSeat(domain(v1::ConfirmSeat {
                    buy_in_id: bytes(B1)
                }))
            )],
            vec![]
        )
    );
}

#[test]
fn eu_1552_a_confirmed_seat_leads_to_spending_the_money() {
    let mut buy_in = buy_in(vec![started(), b1_funds_held()]);

    let (to, seated) =
        table_reports(domain::<_, crate::domain::PlayerSeated>(player_seated()).into());
    assert_eq!(to, bytes(C1));
    let events = buy_in.when(seated).expect("recorded");

    assert_eq!(events, vec![b1_seated()]);
    assert_eq!(buy_in.state().phase(), Phase::AwaitingCapture);
    assert_eq!(
        asks(&events),
        (
            vec![],
            vec![(
                bytes(ALICE),
                WalletAggregateOutput::CaptureFunds(domain(v1::CaptureFunds {
                    hold_id: bytes(B1)
                }))
            )]
        )
    );
}

#[test]
fn eu_1553_spent_money_completes_the_buy_in() {
    let mut buy_in = buy_in(vec![started(), b1_funds_held(), b1_seated()]);

    let (to, captured) = wallet_reports(funds_captured());
    assert_eq!(to, bytes(C1));
    let events = buy_in.when(captured).expect("recorded");

    assert_eq!(
        events,
        vec![ev(v1::BuyInCompleted {
            buy_in_id: bytes(B1)
        })]
    );
    assert_eq!(buy_in.state().phase(), Phase::Completed);
    assert_eq!(asks(&events), (vec![], vec![]), "nothing more is asked");
}

// ================================================================ a refusal

#[test]
fn eu_1563_a_table_that_cannot_confirm_the_seat_fails_the_buy_in_and_frees_the_money() {
    let mut buy_in = buy_in(vec![started(), b1_funds_held()]);

    // The table refuses ConfirmSeat with its typed refusal, and records
    // nothing; policy-buy-in-table translates it into the buy-in's fact.
    let confirm = domain(v1::ConfirmSeat {
        buy_in_id: bytes(B1),
    });
    let refusal = domain(v1::SeatNotConfirmable {
        reason: "SEAT_HOLD_NOT_FOUND".into(),
    });
    let translated = BuyInTablePolicyImpl
        .confirm_seat_refused(&confirm, Some(refusal), &CommandContext::default())
        .expect("no bug")
        .expect("translated, never passed through");
    let BuyInProcessManagerFact::SeatConfirmationRefused(fact) = translated else {
        panic!("SeatConfirmationRefused, not {translated:?}");
    };
    assert_eq!(
        fact.to_wire(),
        v1::SeatConfirmationRefused {
            buy_in_id: bytes(B1),
            reason: "SEAT_HOLD_NOT_FOUND".into(),
        }
    );
    let events = buy_in
        .when(BuyInProcessManagerOutput::SeatConfirmationRefused(fact))
        .expect("recorded");

    assert_eq!(
        events,
        vec![ev(v1::BuyInSeatLost {
            buy_in_id: bytes(B1),
            player_root: bytes(ALICE),
            reason: "SEAT_HOLD_NOT_FOUND".into(),
        })]
    );
    let state = buy_in.state();
    assert_eq!(state.phase(), Phase::Failed);
    assert_eq!(state.failure_reason, "SEAT_HOLD_NOT_FOUND");
    assert_eq!(
        asks(&events),
        (
            vec![],
            vec![(
                bytes(ALICE),
                WalletAggregateOutput::ReleaseHold(domain(v1::ReleaseHold {
                    hold_id: bytes(B1),
                    reason: "SEAT_HOLD_NOT_FOUND".into(),
                }))
            )]
        )
    );
}

/// The wallet's refusal of HoldFunds is passed through: the buy-in has no
/// fact to translate it into (EU-1562 waits on one, buy_in.proto).
#[test]
fn a_refused_hold_is_passed_through_to_the_buy_in() {
    let hold = domain(v1::HoldFunds {
        hold_id: bytes(B1),
        table_root: bytes(MAIN),
        amount: 500,
    });
    let refusal = domain(v1::InsufficientFunds {
        available: 300,
        requested: 500,
    });
    let translated = BuyInWalletPolicyImpl
        .hold_funds_refused(&hold, Some(refusal), &CommandContext::default())
        .expect("no bug");
    assert_eq!(translated, None);
}

// ================================================================ robustness

#[test]
fn eu_1558_the_same_news_delivered_twice_moves_the_buy_in_once() {
    let mut buy_in = buy_in(vec![started()]);
    let (_, held) = wallet_reports(funds_held(B1));
    let news = buy_in.book(held);
    buy_in.again(&news).expect("recorded");
    assert_eq!(buy_in.state().phase(), Phase::AwaitingSeat);

    let again = buy_in.kernel.when_book(news).handled().expect("ok");

    assert!(matches!(again, Handled::Duplicate(..)), "a duplicate");
    assert_eq!(
        buy_in.state().phase(),
        Phase::AwaitingSeat,
        "still waiting for the seat"
    );
    assert_eq!(
        buy_in.kernel.pages().len(),
        2,
        "nothing more recorded: the table is not asked again"
    );
}

#[test]
fn eu_1560_the_buy_in_remembers_its_progress_from_its_own_history() {
    let buy_in = buy_in(vec![started(), b1_funds_held(), b1_seated()]);

    let state = buy_in.state();

    assert_eq!(state.phase(), Phase::AwaitingCapture);
    assert_eq!(
        (
            &state.buy_in_id[..],
            &state.player_root[..],
            state.seat,
            &state.table_root[..],
            state.amount
        ),
        (&bytes(B1)[..], &bytes(ALICE)[..], 0, &bytes(MAIN)[..], 500)
    );
}

#[test]
fn every_report_names_the_buy_in_of_its_events_conversation() {
    let ctx = context("wallet", ALICE, 0xc2);
    let sent = WalletBuyInPolicyImpl
        .funds_held(&bytes(ALICE), &domain(funds_held(B1)), &ctx)
        .expect("no bug");
    assert_eq!(sent[0].0, bytes(0xc2));
    let sent = WalletBuyInPolicyImpl
        .funds_captured(&bytes(ALICE), &domain(funds_captured()), &ctx)
        .expect("no bug");
    assert_eq!(sent[0].0, bytes(0xc2));
}

// ================================================================ EU-1612, at the table

#[test]
fn eu_1612_a_seat_released_twice_is_released_once() {
    let created = v1::TableCreated {
        name: "Main".into(),
        min_bet: 10,
        max_bet: 100,
        min_buy_in: 100,
        max_buy_in: 1000,
        seats: 3,
        decks: 1,
    };
    let table_ev = |m: &dyn Fn() -> angzarr_embedded::Any| {
        TableAggregateEvent::from_any(&m()).expect("a table event")
    };
    let mut table = Kernel::<TableAggregate>::new(table_registry(), cover("table", MAIN));
    table.given([
        table_ev(&|| pack(&created)),
        table_ev(&|| pack(&seat_held())),
        table_ev(&|| {
            pack(&v1::SeatReleased {
                buy_in_id: bytes(B1),
                player_root: bytes(ALICE),
                seat: 0,
                reason: "SEAT_HOLD_NOT_FOUND".into(),
            })
        }),
    ]);
    let release = CommandBook {
        cover: Some(cover("table", MAIN)),
        page: Some(CommandPage {
            body: Some(command_page::Body::Fact(Fact {
                payload: Some(pack(&v1::ReleaseSeat {
                    buy_in_id: bytes(B1),
                    reason: "SEAT_HOLD_NOT_FOUND".into(),
                })),
            })),
            fact_origin: Some(FactOrigin {
                external_id: "release-again".into(),
                description: "the buy-in's undo, again".into(),
            }),
            ..CommandPage::default()
        }),
        ..CommandBook::default()
    };

    let events = table.when_book(release).result().expect("recorded");

    assert_eq!(events, vec![], "the table records nothing more");
}

// ================================================================ policy-table-wallet

fn outcome(seat: i32, player: u8, wager: i64, won: bool, returned: i64) -> v1::SeatOutcome {
    use v1::seat_outcome::Outcome;
    v1::SeatOutcome {
        seat,
        player_root: bytes(player),
        wager,
        outcome: if won { Outcome::Win } else { Outcome::Lose }.into(),
        returned,
        net: returned - wager,
        stack_after: 500 - wager + returned,
    }
}

/// Round 1 at table "Main": Alice wagered 20 and won 40 back, Bob wagered
/// 30 and lost.
fn round_1_settled() -> crate::domain::RoundSettled {
    domain(v1::RoundSettled {
        round: 1,
        outcomes: vec![
            outcome(0, ALICE, 20, true, 40),
            outcome(1, BOB, 30, false, 0),
        ],
        house_delta: -10,
        house_result_after: -10,
    })
}

fn settlement_news() -> Vec<(Vec<u8>, WalletAggregateOutput)> {
    TableWalletPolicyImpl
        .round_settled(
            &bytes(MAIN),
            &round_1_settled(),
            &context("table", MAIN, C1),
        )
        .expect("no bug")
}

#[test]
fn eu_1541_a_settled_round_becomes_a_round_result_for_each_seat() {
    let results: Vec<_> = settlement_news()
        .into_iter()
        .filter_map(|(to, out)| match out {
            WalletAggregateOutput::RecordRoundResult(r) => Some((to, r.to_wire())),
            _ => None,
        })
        .collect();

    let result = |player, wager, net| {
        (
            bytes(player),
            v1::RecordRoundResult {
                table_root: bytes(MAIN),
                round: 1,
                wager,
                net,
            },
        )
    };
    assert_eq!(results, vec![result(ALICE, 20, 20), result(BOB, 30, -30)]);
}

#[test]
fn eu_1542_a_settled_round_awards_loyalty_points_for_each_seat() {
    let awards: Vec<_> = settlement_news()
        .into_iter()
        .filter_map(|(to, out)| match out {
            WalletAggregateOutput::AwardLoyaltyPoints(a) => Some((to, a.to_wire())),
            _ => None,
        })
        .collect();

    let award = |player, points| {
        (
            bytes(player),
            v1::AwardLoyaltyPoints {
                table_root: bytes(MAIN),
                round: 1,
                points,
            },
        )
    };
    assert_eq!(awards, vec![award(ALICE, 20), award(BOB, 30)]);
}

#[test]
fn each_seat_is_told_its_result_then_its_points_in_seat_order() {
    let kinds: Vec<_> = settlement_news()
        .into_iter()
        .map(|(to, out)| {
            let kind = match out {
                WalletAggregateOutput::RecordRoundResult(_) => "result",
                WalletAggregateOutput::AwardLoyaltyPoints(_) => "points",
                other => panic!("only news: {other:?}"),
            };
            (to[0], kind)
        })
        .collect();
    assert_eq!(
        kinds,
        vec![
            (ALICE, "result"),
            (ALICE, "points"),
            (BOB, "result"),
            (BOB, "points")
        ]
    );
}

// ================================================================ what the folds keep

#[test]
fn a_fact_of_another_buy_in_leaves_this_one_where_it_was() {
    // buy_in.proto declares each fact to record one event, so it is
    // recorded; the buy-in does not move (EU-1557's "still waiting").
    let mut buy_in = buy_in(vec![started()]);

    let (_, held) = wallet_reports(funds_held(0x07));
    buy_in.when(held).expect("recorded");

    assert_eq!(buy_in.state().phase(), Phase::AwaitingHold);
}

#[test]
fn a_fact_out_of_turn_leaves_the_buy_in_where_it_was() {
    let mut buy_in = buy_in(vec![started()]);

    let (_, captured) = wallet_reports(funds_captured());
    buy_in.when(captured).expect("recorded");

    assert_eq!(buy_in.state().phase(), Phase::AwaitingHold);
}

#[test]
fn a_started_buy_in_keeps_its_seat() {
    let buy_in = buy_in(vec![ev(v1::BuyInStarted {
        buy_in_id: bytes(B1),
        player_root: bytes(ALICE),
        table_root: bytes(MAIN),
        seat: 2,
        amount: 500,
    })]);

    assert_eq!(buy_in.state().seat, 2);
}

/// The wallet's refusal of the hold (pass-through, buy_in.proto): the
/// buy-in fails, and the held seat is released.
#[test]
fn a_buy_in_failed_for_its_funds_releases_its_seat() {
    let failed = v1::BuyInFailed {
        buy_in_id: bytes(B1),
        table_root: bytes(MAIN),
        reason: "INSUFFICIENT_AVAILABLE_FUNDS".into(),
        available: 300,
        requested: 500,
    };
    let buy_in = buy_in(vec![started(), ev(failed.clone())]);

    let state = buy_in.state();
    assert_eq!(
        (state.phase(), state.failure_reason.as_str()),
        (Phase::Failed, "INSUFFICIENT_AVAILABLE_FUNDS")
    );
    assert_eq!(
        asks(&[ev(failed)]),
        (
            vec![(
                bytes(MAIN),
                TableAggregateOutput::ReleaseSeat(domain(v1::ReleaseSeat {
                    buy_in_id: bytes(B1),
                    reason: "INSUFFICIENT_AVAILABLE_FUNDS".into(),
                }))
            )],
            vec![]
        )
    );
}
