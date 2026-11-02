//! Given/when/then on the marked-up wallet (wallet.feature, EU-1400 to
//! EU-1439), through angzarr-embedded's test support
//! (`angzarr_embedded::testing::Harness`): its kernel over an in-memory
//! store, with the application's registration ([`crate::register`]), so the
//! wallet's upcaster is the one the application runs.
//!
//! - **Given** events are recorded as stored pages ([`given`]), or an older
//!   version's page as it was stored ([`Harness::given_stored`]).
//! - **When** a command is sent as a client sends it ([`Harness::when`]), a
//!   book sent again as it was ([`Harness::when_again`]), or a fact, as a
//!   policy's arrives ([`Harness::when_fact`]).
//! - **Then** the events of that one handling, or its refusal: the
//!   wallet's rejection key and args ([`rejected`]) or HoldFunds' typed
//!   `InsufficientFunds`.

// The harness's `Then` carries angzarr-embedded's `Failure` unboxed: its
// size is not this crate's to choose.
#![allow(clippy::result_large_err)]

use agg_wallet::reason;
use angzarr_diagnostic::{Arg, Diagnostic};
use angzarr_embedded::markup::Rejection;
use angzarr_embedded::testing::{Harness as Kernel, Samples};
use angzarr_embedded::v1::{
    command_page, CommandBook, CommandPage, Cover, EventMeta, EventPage, Fact, FactOrigin, Uuid,
};
use angzarr_embedded::{
    pack, CommandBookExt, Domain, EventStore, Failure, Handled, Payload, Registry, Rejected,
    StreamRef, Write,
};
use examples_proto::v1::{self, hold::Status, WalletState};
use prost::{Message, Name};

use crate::adapters::WalletAggregate;
use crate::domain::{InsufficientFunds, WalletAggregateEvent};

// ---------------------------------------------------------------- the events

type Ev = WalletAggregateEvent;

/// The wallet event a wire message is, as the store reads it back.
fn ev<M: Message + Name>(event: M) -> Ev {
    Ev::from_any(&pack(&event)).expect("a wallet event")
}

// ---------------------------------------------------------------- the harness

type Then = Result<Vec<Ev>, Failure<Rejected>>;

const ALICE: u8 = 0xa1;
const CAROL: u8 = 0xca;
const MAIN: u8 = 0x7a;
const B1: u8 = 0x01;

fn bytes(b: u8) -> Vec<u8> {
    vec![b; 16]
}

/// The lowercase-hex key `holds` is keyed by.
fn hex(b: u8) -> String {
    format!("{b:02x}").repeat(16)
}

fn registry() -> Registry {
    let mut registry = Registry::new();
    crate::register(&mut registry).expect("the application registers");
    registry
}

fn cover(player: u8) -> Cover {
    Cover {
        domain: "wallet".to_string(),
        root: Some(Uuid::from_bytes([player; 16])),
        edition: None,
    }
}

/// A distinct, well-formed id for the `n`th input of a test.
fn id(n: u32) -> Uuid {
    Uuid::from_bytes((0xc0de_0000_0000_0000_0000_0000_0000_0000_u128 + u128::from(n)).to_be_bytes())
}

/// The wallet of `player` with `events` already recorded.
struct Harness {
    kernel: Kernel<WalletAggregate>,
    player: u8,
    inputs: u32,
}

fn given_of(player: u8, events: Vec<Ev>) -> Harness {
    let mut kernel = Kernel::new(registry(), cover(player)).with_samples(Samples::starting_at(999));
    kernel.given(events);
    Harness {
        kernel,
        player,
        inputs: 0,
    }
}

/// Alice's wallet after `events`.
fn given(events: Vec<Ev>) -> Harness {
    given_of(ALICE, events)
}

impl Harness {
    /// Send `command` as a client does, under the test's next id.
    fn when<M: Message + Name>(&mut self, command: M) -> Then {
        let book = self.book(command);
        self.when_again(&book)
    }

    /// The test's next command book for `command`.
    fn book<M: Message + Name>(&mut self, command: M) -> CommandBook {
        self.inputs += 1;
        CommandBook::command(cover(self.player), pack(&command)).with_id(id(self.inputs))
    }

    /// `book` sent (again) exactly as it was: the kernel's whole answer.
    fn when_again(&mut self, book: &CommandBook) -> Then {
        self.kernel.when_book(book.clone()).result()
    }

    /// `book` sent (again) exactly as it was: the kernel's answer, a
    /// duplicate included.
    fn handled(&mut self, book: &CommandBook) -> Result<Handled, Failure<Rejected>> {
        self.kernel.when_book(book.clone()).handled()
    }

    /// Deliver `fact` as its policy's arrives: in the Fact arm, its origin
    /// its own. The events it records, never a refusal.
    fn when_fact<M: Message + Name>(&mut self, fact: M) -> Then {
        self.inputs += 1;
        let book = CommandBook {
            cover: Some(cover(self.player)),
            page: Some(CommandPage {
                body: Some(command_page::Body::Fact(Fact {
                    payload: Some(pack(&fact)),
                })),
                fact_origin: Some(FactOrigin {
                    external_id: format!("fact-{}", self.inputs),
                    description: "the buy-in's report".to_string(),
                }),
                ..CommandPage::default()
            }),
            ..CommandBook::default()
        };
        self.kernel.when_book(book).result()
    }

    /// **Given** a page as an older version stored it: `payload` packed as
    /// is, whatever its type, after the events already recorded.
    fn given_stored<M: Message + Name>(&mut self, payload: M) {
        let store = self.kernel.store().inner();
        let stream = StreamRef::try_from(self.kernel.cover()).expect("a well-formed cover");
        let head = store
            .read(stream, 0, None)
            .ready_now()
            .expect("the in-memory store reads")
            .next_sequence;
        let page = EventPage {
            sequence: head,
            payload: Some(pack(&payload)),
            meta: Some(EventMeta {
                rules_version: <WalletAggregate as Domain>::RULES_VERSION.to_string(),
                ..EventMeta::default()
            }),
        };
        store
            .write(Write::append(stream, head, &[page]))
            .ready_now()
            .expect("the older page is written");
    }

    fn state(&self) -> WalletState {
        self.kernel.state().0.state.clone()
    }
}

/// An in-memory store's future, ready at once.
trait ReadyNow: std::future::Future + Sized {
    fn ready_now(self) -> Self::Output {
        angzarr_embedded::testing::block_on_ready(self)
    }
}

impl<F: std::future::Future> ReadyNow for F {}

/// The wallet's rejection, as the refused caller receives it: its key and
/// its args, by name.
#[track_caller]
fn rejected(then: Then) -> (String, Vec<(String, String)>) {
    match then {
        Err(failure @ Failure::Rejected(_)) => {
            let rejection = failure
                .rejection::<<WalletAggregate as Domain>::Rejection>()
                .unwrap_or_else(|| panic!("the wallet's own rejection: {failure:?}"));
            let args = rejection
                .args()
                .into_iter()
                .map(|(name, arg)| (name.to_string(), text(&arg)))
                .collect();
            (rejection.code().to_string(), args)
        }
        other => panic!("expected the wallet to refuse, got {other:?}"),
    }
}

fn text(arg: &Arg) -> String {
    match arg {
        Arg::Str(s) => s.clone(),
        other => format!("{other:?}"),
    }
}

/// The key of the wallet's rejection.
#[track_caller]
fn rejected_key(then: Then) -> String {
    rejected(then).0
}

/// HoldFunds' typed refusal, as the refused policy receives it.
#[track_caller]
fn insufficient(then: Then) -> InsufficientFunds {
    match then {
        Err(failure @ Failure::Rejected(_)) => failure
            .rejection::<<WalletAggregate as Domain>::Rejection>()
            .and_then(Rejection::typed)
            .and_then(|typed| typed.object::<InsufficientFunds>())
            .unwrap_or_else(|| panic!("an InsufficientFunds: {failure:?}")),
        other => panic!("expected the wallet to refuse, got {other:?}"),
    }
}

/// A handling that failed as a bug, never a business refusal.
#[track_caller]
fn assert_bug(then: Then, what: &str) {
    match then {
        Err(Failure::Rejected(rejected)) => panic!("{what} is a bug, not refused: {rejected:?}"),
        Err(_) => {}
        Ok(events) => panic!("{what} is a bug, not recorded: {events:?}"),
    }
}

/// The bankroll minus the open holds.
fn available(state: &WalletState) -> i64 {
    state.bankroll - open_holds(state)
}

fn open_holds(state: &WalletState) -> i64 {
    state
        .holds
        .values()
        .filter(|h| h.status() == Status::Open)
        .map(|h| h.amount)
        .sum()
}

/// "X has a bankroll of B with A available".
#[track_caller]
fn assert_funds(state: &WalletState, bankroll: i64, available_: i64) {
    assert_eq!(
        (state.bankroll, available(state)),
        (bankroll, available_),
        "bankroll and available: {state:?}"
    );
}

/// Ledger rule L1: `bankroll >= sum(open holds) >= 0`, every open hold
/// positive, and `bankroll = deposited - withdrawn - to_tables +
/// from_tables`.
#[track_caller]
fn assert_balances(state: &WalletState) {
    let open = open_holds(state);
    assert!(state.bankroll >= open && open >= 0, "L1 holds: {state:?}");
    assert!(
        state
            .holds
            .values()
            .filter(|h| h.status() == Status::Open)
            .all(|h| h.amount > 0),
        "every open hold is positive: {state:?}"
    );
    assert_eq!(
        state.bankroll,
        state.total_deposited - state.total_withdrawn - state.total_to_tables
            + state.total_from_tables,
        "L1 does not hold: {state:?}"
    );
}

// ---------------------------------------------------------------- the fixtures

fn registered(name: &str) -> Ev {
    ev(v1::WalletRegistered {
        display_name: name.into(),
        email: format!("{}@example.com", name.to_lowercase()),
    })
}

fn deposited(amount: i64) -> Ev {
    ev(v1::FundsDeposited { amount })
}

fn held(hold: u8, amount: i64) -> Ev {
    ev(v1::FundsHeld {
        hold_id: bytes(hold),
        table_root: bytes(MAIN),
        amount,
    })
}

fn register(name: &str, email: &str) -> v1::RegisterWallet {
    v1::RegisterWallet {
        display_name: name.into(),
        email: email.into(),
    }
}

fn hold(hold: u8, amount: i64) -> v1::HoldFunds {
    v1::HoldFunds {
        hold_id: bytes(hold),
        table_root: bytes(MAIN),
        amount,
    }
}

fn capture(hold: u8) -> v1::CaptureFunds {
    v1::CaptureFunds {
        hold_id: bytes(hold),
    }
}

/// Alice registered with 1000 deposited.
fn alice_with_1000() -> Vec<Ev> {
    vec![registered("Alice"), deposited(1000)]
}

// ================================================================ identity

#[test]
fn eu_1400_registering_opens_an_empty_wallet() {
    let mut wallet = given(vec![]);

    let events = wallet
        .when(register("Alice", "alice@example.com"))
        .expect("registered");

    assert_eq!(events, vec![registered("Alice")]);
    let state = wallet.state();
    assert!(state.registered);
    assert_eq!(state.display_name, "Alice");
    assert_funds(&state, 0, 0);
}

#[test]
fn eu_1401_a_player_cannot_register_twice() {
    let mut wallet = given(vec![registered("Alice")]);

    let then = wallet.when(register("Alice", "alice@example.com"));

    assert_eq!(rejected_key(then), reason::WALLET_ALREADY_EXISTS);
}

#[test]
fn eu_1402_a_player_needs_a_display_name() {
    let mut wallet = given(vec![]);

    let then = wallet.when(register("", "alice@example.com"));

    assert_eq!(rejected_key(then), reason::DISPLAY_NAME_REQUIRED);
    assert!(!wallet.state().registered, "nothing is recorded");
}

#[test]
fn a_player_needs_an_email() {
    let mut wallet = given(vec![]);

    let then = wallet.when(register("Alice", ""));

    assert_eq!(rejected_key(then), reason::EMAIL_REQUIRED);
}

#[test]
fn eu_1403_a_player_can_change_their_display_name() {
    let mut wallet = given(vec![registered("Alice")]);

    let events = wallet
        .when(v1::UpdateProfile {
            display_name: "Ace".into(),
        })
        .expect("updated");

    assert_eq!(
        events,
        vec![ev(v1::ProfileUpdated {
            display_name: "Ace".into()
        })]
    );
    assert_eq!(wallet.state().display_name, "Ace");
}

#[test]
fn eu_1404_an_unregistered_player_cannot_change_their_profile() {
    let mut wallet = given(vec![]);

    let then = wallet.when(v1::UpdateProfile {
        display_name: "Ace".into(),
    });

    assert_eq!(rejected_key(then), reason::WALLET_NOT_FOUND);
}

#[test]
fn a_display_name_cannot_be_changed_to_nothing() {
    let mut wallet = given(vec![registered("Alice")]);

    let then = wallet.when(v1::UpdateProfile {
        display_name: String::new(),
    });

    assert_eq!(rejected_key(then), reason::DISPLAY_NAME_REQUIRED);
}

fn import(name: &str, email: &str, legacy: &str) -> v1::ImportWallet {
    v1::ImportWallet {
        display_name: name.into(),
        email: email.into(),
        legacy_id: legacy.into(),
    }
}

#[test]
fn eu_1405_a_player_migrated_from_the_old_system_keeps_their_identity_but_no_money() {
    let mut wallet = given_of(CAROL, vec![]);

    let events = wallet
        .when(import("Carol", "carol@example.com", "L-77"))
        .expect("imported");

    assert_eq!(
        events,
        vec![ev(v1::WalletImported {
            display_name: "Carol".into(),
            email: "carol@example.com".into(),
            legacy_id: "L-77".into(),
        })]
    );
    let state = wallet.state();
    assert!(state.registered);
    assert_eq!(
        (state.display_name.as_str(), state.legacy_id.as_str()),
        ("Carol", "L-77")
    );
    assert_funds(&state, 0, 0);
}

#[test]
fn eu_1406_an_existing_player_cannot_be_imported_over() {
    let mut wallet = given(vec![registered("Alice")]);

    let then = wallet.when(import("Alice", "alice@example.com", "L-1"));

    assert_eq!(rejected_key(then), reason::WALLET_ALREADY_EXISTS);
}

#[test]
fn an_import_needs_a_display_name_and_an_email() {
    let mut wallet = given_of(CAROL, vec![]);
    let then = wallet.when(import("", "carol@example.com", "L-77"));
    assert_eq!(rejected_key(then), reason::DISPLAY_NAME_REQUIRED);
    let then = wallet.when(import("Carol", "", "L-77"));
    assert_eq!(rejected_key(then), reason::EMAIL_REQUIRED);
}

// ================================================================ deposits and withdrawals

#[test]
fn eu_1407_deposits_increase_the_bankroll() {
    let mut wallet = given(vec![registered("Alice")]);

    let events = wallet
        .when(v1::DepositFunds { amount: 1000 })
        .expect("deposited");

    assert_eq!(events, vec![deposited(1000)]);
    let state = wallet.state();
    assert_funds(&state, 1000, 1000);
    assert_balances(&state);
}

#[test]
fn eu_1408_a_deposit_must_be_positive() {
    for amount in [0, -50] {
        let mut wallet = given(vec![registered("Alice")]);

        let then = wallet.when(v1::DepositFunds { amount });

        assert_eq!(
            rejected(then),
            (
                reason::AMOUNT_NOT_POSITIVE.to_string(),
                vec![("amount".to_string(), amount.to_string())]
            ),
            "a deposit of {amount}"
        );
    }
}

#[test]
fn eu_1409_an_unregistered_player_cannot_deposit() {
    let mut wallet = given(vec![]);

    let then = wallet.when(v1::DepositFunds { amount: 1000 });

    assert_eq!(rejected_key(then), reason::WALLET_NOT_FOUND);
}

#[test]
fn eu_1410_withdrawals_decrease_the_bankroll() {
    let mut wallet = given(alice_with_1000());

    let events = wallet
        .when(v1::WithdrawFunds { amount: 400 })
        .expect("withdrawn");

    assert_eq!(events, vec![ev(v1::FundsWithdrawn { amount: 400 })]);
    let state = wallet.state();
    assert_funds(&state, 600, 600);
    assert_balances(&state);
}

#[test]
fn eu_1411_a_withdrawal_cannot_touch_held_funds() {
    let mut events = alice_with_1000();
    events.push(held(B1, 700));
    let mut wallet = given(events);

    let then = wallet.when(v1::WithdrawFunds { amount: 400 });

    assert_eq!(
        rejected(then),
        (
            reason::INSUFFICIENT_AVAILABLE_FUNDS.to_string(),
            vec![
                ("available".to_string(), "300".to_string()),
                ("requested".to_string(), "400".to_string()),
            ]
        )
    );
    assert_funds(&wallet.state(), 1000, 300);
}

#[test]
fn a_withdrawal_of_all_that_is_available_is_taken() {
    let mut events = alice_with_1000();
    events.push(held(B1, 700));
    let mut wallet = given(events);

    wallet
        .when(v1::WithdrawFunds { amount: 300 })
        .expect("withdrawn");

    assert_funds(&wallet.state(), 700, 0);
}

#[test]
fn eu_1412_a_withdrawal_must_be_positive() {
    let mut wallet = given(alice_with_1000());

    let then = wallet.when(v1::WithdrawFunds { amount: 0 });

    assert_eq!(rejected_key(then), reason::AMOUNT_NOT_POSITIVE);
}

#[test]
fn an_unregistered_player_cannot_withdraw() {
    let mut wallet = given(vec![]);

    let then = wallet.when(v1::WithdrawFunds { amount: 10 });

    assert_eq!(rejected_key(then), reason::WALLET_NOT_FOUND);
}

// ================================================================ buy-in holds

#[test]
fn eu_1413_holding_funds_reduces_what_is_available_but_not_the_bankroll() {
    let mut wallet = given(alice_with_1000());

    let events = wallet.when(hold(B1, 500)).expect("held");

    assert_eq!(events, vec![held(B1, 500)]);
    let state = wallet.state();
    assert_funds(&state, 1000, 500);
    let hold = &state.holds[&hex(B1)];
    assert_eq!(
        (hold.status(), hold.amount, &hold.table_root[..]),
        (Status::Open, 500, &bytes(MAIN)[..])
    );
    assert_balances(&state);
}

#[test]
fn eu_1415_repeating_an_identical_hold_changes_nothing() {
    // The buy-in process may deliver the same request twice: the same
    // HoldFunds, under the same id.
    let mut wallet = given(alice_with_1000());
    let request = wallet.book(hold(B1, 500));
    wallet.when_again(&request).expect("held");

    let again = wallet.handled(&request).expect("the request succeeds");

    assert!(matches!(again, Handled::Duplicate(..)), "a duplicate");
    assert_funds(&wallet.state(), 1000, 500);
}

#[test]
fn a_hold_the_wallet_cannot_fund_is_refused_with_what_is_available() {
    let mut events = alice_with_1000();
    events.push(held(B1, 700));
    let mut wallet = given(events);

    let then = wallet.when(hold(0x02, 500));

    assert_eq!(
        insufficient(then),
        InsufficientFunds {
            available: 300,
            requested: 500
        }
    );
    assert_funds(&wallet.state(), 1000, 300);
}

#[test]
fn a_hold_of_all_that_is_available_is_taken() {
    let mut events = alice_with_1000();
    events.push(held(B1, 700));
    let mut wallet = given(events);

    wallet.when(hold(0x02, 300)).expect("held");

    assert_funds(&wallet.state(), 1000, 0);
}

#[test]
fn eu_1417_capturing_a_hold_spends_it() {
    let mut events = alice_with_1000();
    events.push(held(B1, 500));
    let mut wallet = given(events);

    let events = wallet.when_fact(capture(B1)).expect("captured");

    assert_eq!(
        events,
        vec![ev(v1::FundsCaptured {
            hold_id: bytes(B1),
            table_root: bytes(MAIN),
            amount: 500,
        })]
    );
    let state = wallet.state();
    assert_funds(&state, 500, 500);
    assert_ne!(state.holds[&hex(B1)].status(), Status::Open, "no open hold");
    assert_eq!(state.total_to_tables, 500, "sent 500 to tables in total");
    assert_balances(&state);
}

#[test]
fn eu_1420_releasing_a_hold_restores_what_is_available() {
    let mut events = alice_with_1000();
    events.push(held(B1, 500));
    let mut wallet = given(events);

    let events = wallet
        .when_fact(v1::ReleaseHold {
            hold_id: bytes(B1),
            reason: "the seat was refused".into(),
        })
        .expect("released");

    assert_eq!(
        events,
        vec![ev(v1::HoldReleased {
            hold_id: bytes(B1),
            amount: 500,
            reason: "the seat was refused".into(),
        })]
    );
    let state = wallet.state();
    assert_funds(&state, 1000, 1000);
    assert_eq!(state.holds[&hex(B1)].status(), Status::Released);
    assert_balances(&state);
}

#[test]
fn eu_1422_a_captured_hold_cannot_be_held_again() {
    // A late duplicate of the original request must not earmark the money
    // a second time.
    let mut wallet = given(alice_with_1000());
    let request = wallet.book(hold(B1, 500));
    wallet.when_again(&request).expect("held");
    wallet.when_fact(capture(B1)).expect("captured");

    let again = wallet.handled(&request).expect("the request succeeds");

    assert!(matches!(again, Handled::Duplicate(..)), "a duplicate");
    assert_funds(&wallet.state(), 500, 500);
}

#[test]
fn a_hold_needs_a_registered_wallet_and_a_positive_amount() {
    let mut wallet = given(vec![]);
    assert_bug(wallet.when(hold(B1, 500)), "a hold for no wallet");
    let mut wallet = given(alice_with_1000());
    assert_bug(wallet.when(hold(B1, 0)), "a hold of nothing");
}

// ================================================================ loyalty and round history

#[test]
fn eu_1431_a_player_can_join_the_loyalty_programme() {
    let mut wallet = given(vec![registered("Alice")]);

    let events = wallet.when(v1::EnrollLoyalty {}).expect("enrolled");

    assert_eq!(events, vec![ev(v1::LoyaltyEnrolled {})]);
    let state = wallet.state();
    assert!(state.loyalty_enrolled);
    assert_eq!(state.loyalty_points, 0);
}

#[test]
fn an_unregistered_player_cannot_join_the_loyalty_programme() {
    let mut wallet = given(vec![]);

    let then = wallet.when(v1::EnrollLoyalty {});

    assert_eq!(rejected_key(then), reason::WALLET_NOT_FOUND);
}

fn award(round: u32, points: i64) -> v1::AwardLoyaltyPoints {
    v1::AwardLoyaltyPoints {
        table_root: bytes(MAIN),
        round,
        points,
    }
}

#[test]
fn eu_1432_loyalty_points_are_awarded_to_members() {
    let mut wallet = given(vec![registered("Alice"), ev(v1::LoyaltyEnrolled {})]);

    let events = wallet.when_fact(award(1, 20)).expect("awarded");

    assert_eq!(
        events,
        vec![ev(v1::LoyaltyPointsAwarded {
            table_root: bytes(MAIN),
            round: 1,
            points: 20,
        })]
    );
    let state = wallet.state();
    assert!(state.loyalty_enrolled);
    assert_eq!(state.loyalty_points, 20);
}

#[test]
fn a_round_result_is_recorded_by_its_table_and_round() {
    let mut wallet = given(vec![registered("Alice")]);

    let result = v1::RecordRoundResult {
        table_root: bytes(MAIN),
        round: 1,
        wager: 20,
        net: 20,
    };
    let events = wallet.when_fact(result).expect("recorded");

    assert_eq!(
        events,
        vec![ev(v1::RoundResultRecorded {
            table_root: bytes(MAIN),
            round: 1,
            wager: 20,
            net: 20,
        })]
    );
    let state = wallet.state();
    let recorded = &state.round_results[&format!("{}/1", hex(MAIN))];
    assert_eq!((recorded.wager, recorded.net), (20, 20));
}

// ================================================================ history written by older versions

#[test]
fn eu_1437_a_deposit_recorded_by_an_older_version_still_counts() {
    let mut wallet = given(vec![registered("Alice")]);
    wallet.given_stored(v1::FundsDepositedV1 { amount_chips: 300 });

    wallet
        .when(v1::DepositFunds { amount: 200 })
        .expect("deposited");

    let state = wallet.state();
    assert_funds(&state, 500, 500);
    assert_balances(&state);
}
