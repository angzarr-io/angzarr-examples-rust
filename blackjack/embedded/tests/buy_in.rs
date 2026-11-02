//! Buy-ins end to end (process_manager.feature): the table, the wallet,
//! the buy-in process manager and their policies, all in process on
//! angzarr-embedded-host's `App` over an in-memory store, the kernel
//! delivering every link. A client requests a seat; from there the buy-in
//! runs by the policies alone:
//!
//! - a buy-in that goes through: the seat held, the money held, the seat
//!   confirmed, the money spent (EU-1550 to EU-1553, EU-1561);
//! - a buy-in rolled back: the seat's hold is released meanwhile (another
//!   writer's SeatReleased lands on the table while the wallet holds the
//!   money), so the table cannot confirm the seat, and the held money is
//!   released (EU-1563, its undo a fact the wallet cannot refuse; EU-1612,
//!   the seat released again records nothing more);
//! - two buy-ins in different conversations (EU-1559);
//! - a settled round reaching each seat's wallet (EU-1541, EU-1542).

use std::sync::{Arc, Mutex};

use angzarr_embedded::testing::{block_on_ready, At, FaultyStore, MemoryStore};
use angzarr_embedded::v1::{
    command_page, CommandBook, CommandPage, Cover, EventMeta, EventPage, Fact, FactOrigin, Uuid,
};
use angzarr_embedded::{
    pack, type_name, Any, CommandBookExt, EventStore, Registry, Sampled, Stream, StreamRef,
    Timestamp, Write,
};
use angzarr_embedded_host::{App, OneStore};
use blackjack_embedded::adapters::{BuyInProcessManager, WalletAggregate};
use blackjack_embedded::register;
use examples_proto::v1::{self, buy_in_state::Phase};
use prost::{Message, Name};

const ALICE: u8 = 0xa1;
const BOB: u8 = 0xb0;
const MAIN: u8 = 0x7a;
const B1: u8 = 0x01;
const B2: u8 = 0x02;
const C1: u8 = 0xc1;
const C2: u8 = 0xc2;

type Store = FaultyStore<MemoryStore>;
type TheApp = App<OneStore<Store>>;

fn bytes(b: u8) -> Vec<u8> {
    vec![b; 16]
}

fn root(b: u8) -> Uuid {
    Uuid::from_bytes([b; 16])
}

fn cover(domain: &str, b: u8) -> Cover {
    Cover {
        domain: domain.to_string(),
        root: Some(root(b)),
        edition: None,
    }
}

async fn app_with(registry: Registry) -> TheApp {
    App::builder(
        OneStore::new(FaultyStore::new(MemoryStore::default())),
        registry,
    )
    .start()
    .await
}

/// The whole application, its registration the crate's.
async fn app() -> TheApp {
    let mut registry = Registry::new();
    register(&mut registry).expect("the application registers");
    app_with(registry).await
}

/// A client of the application: it numbers its inputs from 1, each's ids
/// and time following from its number.
struct Client<'a> {
    app: &'a TheApp,
    sent: u8,
}

impl<'a> Client<'a> {
    fn new(app: &'a TheApp) -> Self {
        Client { app, sent: 0 }
    }

    fn sampled(&mut self) -> (u8, Sampled) {
        self.sent += 1;
        let n = self.sent;
        let sampled = Sampled {
            now: Timestamp {
                seconds: i64::from(n),
                nanos: 0,
            },
            command_id: Uuid::from_bytes([0xe0, n, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]),
        };
        (n, sampled)
    }

    /// `command` to `to`, in the conversation `correlation`, which must be
    /// accepted along with everything it sets off.
    async fn send_in<M: Message + Name>(&mut self, to: Cover, command: M, correlation: Option<u8>) {
        let (n, sampled) = self.sampled();
        let mut book = CommandBook::command(to, pack(&command)).with_id(Uuid::from_bytes([
            0xd0, n, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ]));
        if let (Some(page), Some(c)) = (book.page.as_mut(), correlation) {
            page.correlation_id = Some(root(c));
        }
        if let Err(failure) = self.app.handle(book, &sampled).await {
            panic!("input {n} ({}) failed: {failure:?}", M::full_name());
        }
    }

    async fn send<M: Message + Name>(&mut self, to: Cover, command: M) {
        self.send_in(to, command, None).await
    }

    /// `fact` to `to`, as an outside writer records it.
    async fn tell<M: Message + Name>(&mut self, to: Cover, fact: M) {
        let (n, sampled) = self.sampled();
        let book = CommandBook {
            cover: Some(to),
            page: Some(CommandPage {
                body: Some(command_page::Body::Fact(Fact {
                    payload: Some(pack(&fact)),
                })),
                fact_origin: Some(FactOrigin {
                    external_id: format!("client-{n}"),
                    description: "an outside writer".into(),
                }),
                ..CommandPage::default()
            }),
            ..CommandBook::default()
        };
        if let Err(failure) = self.app.handle(book, &sampled).await {
            panic!("fact {n} ({}) failed: {failure:?}", M::full_name());
        }
    }
}

/// Table "Main": 3 seats, bets 10 to 100, buy-ins 100 to 1000, seed 21.
fn create_main() -> v1::CreateTable {
    v1::CreateTable {
        name: "Main".into(),
        seats: 3,
        decks: 1,
        min_bet: 10,
        max_bet: 100,
        min_buy_in: 100,
        max_buy_in: 1000,
        shoe_seed: 21,
    }
}

/// Table "Main" open, and `players` each registered with 1000 deposited.
async fn open<'a>(app: &'a TheApp, players: &[(u8, &str)]) -> Client<'a> {
    let mut client = Client::new(app);
    client.send(cover("table", MAIN), create_main()).await;
    for &(player, name) in players {
        let wallet = cover("wallet", player);
        let register = v1::RegisterWallet {
            display_name: name.into(),
            email: format!("{}@example.com", name.to_lowercase()),
        };
        client.send(wallet.clone(), register).await;
        client.send(wallet, v1::DepositFunds { amount: 1000 }).await;
    }
    client
}

fn request(player: u8, seat: i32, buy_in: u8) -> v1::RequestSeat {
    v1::RequestSeat {
        player_root: bytes(player),
        seat,
        amount: 500,
        request_id: bytes(buy_in),
    }
}

async fn pages(app: &TheApp, cover: &Cover) -> Vec<EventPage> {
    app.events(cover).await.expect("the stream reads")
}

fn payloads(pages: &[EventPage]) -> Vec<Any> {
    pages.iter().filter_map(|p| p.payload.clone()).collect()
}

/// Each payload's short type name, in order.
async fn names(app: &TheApp, cover: &Cover) -> Vec<String> {
    payloads(&pages(app, cover).await)
        .iter()
        .map(|any| {
            type_name(&any.type_url)
                .rsplit('.')
                .next()
                .unwrap_or_default()
                .to_string()
        })
        .collect()
}

fn decoded<M: Message + Name + Default>(payloads: &[Any]) -> Vec<M> {
    payloads
        .iter()
        .filter(|any| type_name(&any.type_url) == M::full_name())
        .map(|any| M::decode(&any.value[..]).expect("decodes"))
        .collect()
}

/// The stream `cover` of the domain `D`, replayed as the next process
/// would.
async fn replayed<D: angzarr_embedded::Domain>(app: &TheApp, cover: Cover) -> D::State {
    let pages = pages(app, &cover).await;
    Stream::<D>::replay(cover, &pages, app.registry())
        .expect("the stream replays")
        .state()
        .clone()
}

/// The buy-in of conversation `c`.
async fn buy_in(app: &TheApp, c: u8) -> v1::BuyInState {
    replayed::<BuyInProcessManager>(app, cover("buy-in", c))
        .await
        .0
        .state
}

/// The wallet of `player`.
async fn wallet(app: &TheApp, player: u8) -> v1::WalletState {
    replayed::<WalletAggregate>(app, cover("wallet", player))
        .await
        .0
        .state
}

/// The bankroll minus the open holds.
fn available(state: &v1::WalletState) -> i64 {
    let open: i64 = state
        .holds
        .values()
        .filter(|h| h.status() == v1::hold::Status::Open)
        .map(|h| h.amount)
        .sum();
    state.bankroll - open
}

// ================================================================ a buy-in that goes through

#[tokio::test(flavor = "multi_thread")]
async fn eu_1550_to_1553_a_buy_in_holds_the_seat_then_the_money_then_seats_and_spends() {
    let app = app().await;
    let mut client = open(&app, &[(ALICE, "Alice")]).await;

    client
        .send_in(cover("table", MAIN), request(ALICE, 0, B1), Some(C1))
        .await;

    // The buy-in, tracked under the conversation it started in, moved
    // through every step, each decided before the next.
    assert_eq!(
        names(&app, &cover("buy-in", C1)).await,
        [
            "BuyInStarted",
            "BuyInFundsHeld",
            "BuyInSeated",
            "BuyInCompleted"
        ]
    );
    assert_eq!(buy_in(&app, C1).await.phase(), Phase::Completed);
    // The wallet held the money, then spent it on the table.
    assert_eq!(
        names(&app, &cover("wallet", ALICE)).await,
        [
            "WalletRegistered",
            "FundsDeposited",
            "FundsHeld",
            "FundsCaptured"
        ]
    );
    let alice = wallet(&app, ALICE).await;
    assert_eq!((alice.bankroll, available(&alice)), (500, 500));
    assert_eq!(alice.total_to_tables, 500);
    // The table held the seat, then seated Alice with the buy-in.
    let table = payloads(&pages(&app, &cover("table", MAIN)).await);
    let [seated] = &decoded::<v1::PlayerSeated>(&table)[..] else {
        panic!("one PlayerSeated");
    };
    assert_eq!(
        (&seated.player_root[..], seated.seat, seated.stack),
        (&bytes(ALICE)[..], 0, 500)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn eu_1561_the_buy_ins_requests_carry_the_conversation_they_belong_to() {
    let app = app().await;
    let mut client = open(&app, &[(ALICE, "Alice")]).await;

    client
        .send_in(cover("table", MAIN), request(ALICE, 0, B1), Some(C1))
        .await;

    // ConfirmSeat's effect at the table, and the money held and spent at
    // the wallet, are each recorded under the buy-in's conversation.
    let correlation_of = |pages: Vec<EventPage>, name: &str| {
        pages
            .into_iter()
            .find(|p| {
                p.payload
                    .as_ref()
                    .is_some_and(|a| a.type_url.ends_with(name))
            })
            .and_then(|p| p.meta)
            .and_then(|m: EventMeta| m.correlation_id)
    };
    let table = pages(&app, &cover("table", MAIN)).await;
    assert_eq!(correlation_of(table, ".PlayerSeated"), Some(root(C1)));
    let alice = pages(&app, &cover("wallet", ALICE)).await;
    assert_eq!(correlation_of(alice.clone(), ".FundsHeld"), Some(root(C1)));
    assert_eq!(correlation_of(alice, ".FundsCaptured"), Some(root(C1)));
}

// ================================================================ a buy-in rolled back

/// `event`, appended to `cover`'s stream in `store` as another writer
/// commits it.
fn append(store: &MemoryStore, cover: &Cover, event: &impl Name) {
    let stream = StreamRef::try_from(cover).expect("a well-formed cover");
    let head = block_on_ready(store.read(stream, 0, None))
        .expect("the store reads")
        .next_sequence;
    let page = EventPage {
        sequence: head,
        payload: Some(pack(event)),
        meta: Some(EventMeta {
            rules_version: env!("CARGO_PKG_VERSION").to_string(),
            ..EventMeta::default()
        }),
    };
    block_on_ready(store.write(Write::append(stream, head, &[page])))
        .expect("the other writer's event lands");
}

#[tokio::test(flavor = "multi_thread")]
async fn eu_1563_a_buy_in_whose_seat_is_released_meanwhile_rolls_back_and_frees_the_money() {
    let app = app().await;
    let mut client = open(&app, &[(ALICE, "Alice")]).await;
    // While the wallet holds the money (the buy-in's third write: the
    // seat's hold, the buy-in's start, the hold), another writer releases
    // the seat's hold.
    let released = Arc::new(Mutex::new(false));
    let done = Arc::clone(&released);
    app.store().before_nth(At::Write, 3, move |store| {
        let release = v1::SeatReleased {
            buy_in_id: bytes(B1),
            player_root: bytes(ALICE),
            seat: 0,
            reason: "released meanwhile".into(),
        };
        append(store, &cover("table", MAIN), &release);
        *done.lock().unwrap() = true;
    });

    client
        .send_in(cover("table", MAIN), request(ALICE, 0, B1), Some(C1))
        .await;

    assert!(*released.lock().unwrap(), "the seat was released meanwhile");
    // The table refused ConfirmSeat and recorded nothing for it: no one is
    // seated.
    assert_eq!(
        names(&app, &cover("table", MAIN)).await,
        ["TableCreated", "ShoeShuffled", "SeatHeld", "SeatReleased"]
    );
    // The buy-in failed because the seat could not be confirmed ...
    assert_eq!(
        names(&app, &cover("buy-in", C1)).await,
        ["BuyInStarted", "BuyInFundsHeld", "BuyInSeatLost"]
    );
    let state = buy_in(&app, C1).await;
    assert_eq!(
        (state.phase(), state.failure_reason.as_str()),
        (Phase::Failed, "SEAT_HOLD_NOT_FOUND")
    );
    // ... and the wallet was told to release the hold, which it could not
    // refuse: the money is back.
    assert_eq!(
        names(&app, &cover("wallet", ALICE)).await,
        [
            "WalletRegistered",
            "FundsDeposited",
            "FundsHeld",
            "HoldReleased"
        ]
    );
    let alice = wallet(&app, ALICE).await;
    assert_eq!((alice.bankroll, available(&alice)), (1000, 1000));

    // EU-1612: the seat released again records nothing more.
    client
        .tell(
            cover("table", MAIN),
            v1::ReleaseSeat {
                buy_in_id: bytes(B1),
                reason: "SEAT_HOLD_NOT_FOUND".into(),
            },
        )
        .await;
    assert_eq!(names(&app, &cover("table", MAIN)).await.len(), 4);
}

// ================================================================ two conversations

#[tokio::test(flavor = "multi_thread")]
async fn eu_1559_two_buy_ins_in_different_conversations_do_not_interfere() {
    // The buy-in process manager alone: what it records for each input,
    // the policies' outputs not delivered.
    let mut registry = Registry::new();
    registry
        .process_manager::<BuyInProcessManager>(BuyInProcessManager::default)
        .expect("registers");
    let app = app_with(registry).await;
    let mut client = Client::new(&app);
    let start = |buy_in: u8, player: u8, seat: i32| v1::StartBuyIn {
        buy_in_id: bytes(buy_in),
        player_root: bytes(player),
        table_root: bytes(MAIN),
        seat,
        amount: 500,
    };
    client.tell(cover("buy-in", C1), start(B1, ALICE, 0)).await;
    client.tell(cover("buy-in", C2), start(B2, BOB, 1)).await;

    let held = v1::RecordFundsHeld {
        hold_id: bytes(B2),
        table_root: bytes(MAIN),
        amount: 500,
    };
    client.tell(cover("buy-in", C2), held).await;

    assert_eq!(buy_in(&app, C2).await.phase(), Phase::AwaitingSeat);
    assert_eq!(buy_in(&app, C1).await.phase(), Phase::AwaitingHold);
}

// ================================================================ a settled round reaches the wallets

#[tokio::test(flavor = "multi_thread")]
async fn eu_1541_eu_1542_a_settled_round_is_each_seats_round_result_and_loyalty_points() {
    let app = app().await;
    let mut client = open(&app, &[(ALICE, "Alice"), (BOB, "Bob")]).await;
    client
        .send(cover("wallet", ALICE), v1::EnrollLoyalty {})
        .await;
    client
        .send_in(cover("table", MAIN), request(ALICE, 0, B1), Some(C1))
        .await;
    client
        .send_in(cover("table", MAIN), request(BOB, 1, B2), Some(C2))
        .await;
    for (seat, amount) in [(0, 40), (1, 20)] {
        client
            .send(cover("table", MAIN), v1::PlaceBet { seat, amount })
            .await;
    }
    client.send(cover("table", MAIN), v1::DealRound {}).await;
    // A driver plays: each seat on turn stands, until the round settles.
    for _ in 0..2 {
        let table = payloads(&pages(&app, &cover("table", MAIN)).await);
        if !decoded::<v1::RoundSettled>(&table).is_empty() {
            break;
        }
        let turn = decoded::<v1::TurnStarted>(&table)
            .last()
            .expect("a turn")
            .seat;
        client
            .send(cover("table", MAIN), v1::Stand { seat: turn })
            .await;
    }

    let table = payloads(&pages(&app, &cover("table", MAIN)).await);
    let [settled] = &decoded::<v1::RoundSettled>(&table)[..] else {
        panic!("one RoundSettled");
    };
    for outcome in &settled.outcomes {
        let player = outcome.player_root[0];
        let events = payloads(&pages(&app, &cover("wallet", player)).await);
        let results = decoded::<v1::RoundResultRecorded>(&events);
        assert_eq!(
            results,
            vec![v1::RoundResultRecorded {
                table_root: bytes(MAIN),
                round: settled.round,
                wager: outcome.wager,
                net: outcome.net,
            }],
            "the round result of seat {}",
            outcome.seat
        );
        let awards = decoded::<v1::LoyaltyPointsAwarded>(&events);
        assert_eq!(
            awards,
            vec![v1::LoyaltyPointsAwarded {
                table_root: bytes(MAIN),
                round: settled.round,
                points: outcome.wager,
            }]
        );
    }
    // Alice is a member: her points count; Bob's award is recorded only.
    assert_eq!(wallet(&app, ALICE).await.loyalty_points, 40);
    assert_eq!(wallet(&app, BOB).await.loyalty_points, 0);
}
