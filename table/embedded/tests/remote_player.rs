//! The table in process and the REMOTE Python player bot as a child
//! process, through angzarr-embedded-host (bot_play.feature, EU-1605 to
//! EU-1609): a driver seats two players and deals; from there the turn
//! crosses to the player and back by the two policies alone, until the
//! round settles.
//!
//! Ignored by default: the first run needs `uv` and the network (uv fetches
//! the player's dependencies, angzarr-client-python among them). Run it
//! with `cargo test -p table-embedded --test remote_player -- --ignored`.
//! The player's project is `$ANGZARR_PLAYER_DIR`, by default
//! `~/workspace/worktrees/examples-python--player-bot/player`.
//!
//! The hand is seed 21's (one deck): Alice 4♣ 6♠ and Bob A♣ 5♠ against
//! the dealer's 9♦, Q♦ in the hole. The bot plays a simplified basic
//! strategy: Alice doubles her hard 10 and draws A♥ (21); Bob hits his soft
//! 16 (K♣, a hard 16), hits again against the 9 (4♦, 20) and stands. The
//! dealer stands on 19; both win.

use std::path::PathBuf;

use angzarr_embedded::next::v1::{command_page, Command, CommandBook, CommandPage, Cover, Uuid};
use angzarr_embedded::next::{pack, type_name, Any, Registry, Sampled, Timestamp};
use angzarr_embedded_host::{run, App};
use examples_proto::v1::{self, seat_outcome::Outcome, MoveAction};
use prost::{Message, Name};
use table_embedded::{PlayerTablePolicy, TableAggregate, TablePlayerPolicy};

const TABLE: u8 = 0x7a;
const ALICE: u8 = 0xa1;
const BOB: u8 = 0xb0;

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

/// The player bot's project directory.
fn player_dir() -> PathBuf {
    match std::env::var_os("ANGZARR_PLAYER_DIR") {
        Some(dir) => PathBuf::from(dir),
        None => {
            let home = std::env::var_os("HOME").expect("HOME is set");
            PathBuf::from(home).join("workspace/worktrees/examples-python--player-bot/player")
        }
    }
}

/// The application's angzarr.toml: the table in process, the player a
/// Python child, an in-memory store. Written beside the test's other
/// scratch files, in the target directory.
fn config() -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let path = dir.join(format!("remote-player-{}.toml", std::process::id()));
    let text = format!(
        r#"[[domain]]
name = "table"
language = "rust"

[[domain]]
name = "player"
language = "python"
entry = ["angzarr-player"]
dir = {dir:?}

[supervision]
handshake_timeout_ms = 300000

[store]
in_memory = true
"#,
        dir = player_dir().display().to_string(),
    );
    std::fs::write(&path, text).expect("the config is written");
    path
}

/// The table and both policies, registered in process beside the remote
/// player. The player domain is the child's: nothing registers the
/// contract-only `PlayerAggregate`.
fn register(registry: &mut Registry) {
    registry
        .aggregate::<TableAggregate>(TableAggregate::default)
        .policy(TablePlayerPolicy)
        .policy(PlayerTablePolicy);
}

/// A driver's command `n` to the table.
fn to_table<M: Message + Name>(app: &App, n: u8, command: M) {
    let book = CommandBook {
        cover: Some(cover("table", TABLE)),
        page: Some(CommandPage {
            body: Some(command_page::Body::Command(Command {
                payload: Some(pack(&command)),
            })),
            command_id: Some(Uuid::from_bytes([0xd0 | n; 16])),
            ..CommandPage::default()
        }),
        handling: None,
    };
    let sampled = Sampled {
        now: Timestamp {
            seconds: i64::from(n),
            nanos: 0,
        },
        command_id: Uuid::from_bytes([0xe0 | n; 16]),
    };
    if let Err(failure) = app.handle(book, &sampled) {
        panic!("command {n} ({}) failed: {failure:?}", M::full_name());
    }
}

/// The payloads of the stream `cover` names.
fn payloads(app: &App, cover: &Cover) -> Vec<Any> {
    app.events(cover)
        .expect("the stream reads")
        .into_iter()
        .filter_map(|page| page.payload)
        .collect()
}

fn decoded<M: Message + Name + Default>(payloads: &[Any]) -> Vec<M> {
    payloads
        .iter()
        .filter(|any| type_name(&any.type_url) == M::full_name())
        .map(|any| M::decode(&any.value[..]).expect("decodes"))
        .collect()
}

/// Each payload's short type name, in order.
fn names(payloads: &[Any]) -> Vec<&str> {
    payloads
        .iter()
        .map(|any| {
            type_name(&any.type_url)
                .rsplit('.')
                .next()
                .unwrap_or_default()
        })
        .collect()
}

/// (seat, action) of each MoveDecided in a player's stream.
fn moves(app: &App, player: u8) -> Vec<(i32, MoveAction)> {
    decoded::<v1::MoveDecided>(&payloads(app, &cover("player", player)))
        .into_iter()
        .map(|m| (m.seat, MoveAction::try_from(m.action).expect("an action")))
        .collect()
}

#[test]
#[ignore = "needs uv, and the network the first time"]
fn eu_1605_to_1609_the_remote_player_plays_seed_21_to_settlement() {
    let app = run(config(), register).expect("the application starts with the player child");

    to_table(
        &app,
        1,
        v1::CreateTable {
            name: "Main".into(),
            min_bet: 10,
            max_bet: 100,
            min_buy_in: 100,
            max_buy_in: 1000,
            seats: 3,
            decks: 1,
            shoe_seed: 21,
        },
    );
    for (n, player, seat) in [(2, ALICE, 0), (4, BOB, 1)] {
        to_table(
            &app,
            n,
            v1::RequestSeat {
                player_root: vec![player; 16].into(),
                seat,
                amount: 500,
                request_id: vec![n; 16].into(),
            },
        );
        to_table(
            &app,
            n + 1,
            v1::ConfirmSeat {
                buy_in_id: vec![n; 16].into(),
            },
        );
    }
    to_table(
        &app,
        6,
        v1::PlaceBet {
            seat: 0,
            amount: 20,
        },
    );
    to_table(
        &app,
        7,
        v1::PlaceBet {
            seat: 1,
            amount: 20,
        },
    );
    // The deal sets off the whole loop: TurnStarted, DecideMove to the
    // player, MoveDecided, back to the table, until the round settles.
    to_table(&app, 8, v1::DealRound {});

    // Each seat's own player was asked, and decided by basic strategy
    // (EU-1606, EU-1608, EU-1609).
    assert_eq!(moves(&app, ALICE), [(0, MoveAction::DoubleDown)]);
    assert_eq!(
        moves(&app, BOB),
        [
            (1, MoveAction::Hit),
            (1, MoveAction::Hit),
            (1, MoveAction::Stand)
        ]
    );

    // The table's history, from the deal: each move came back as the
    // table's command, and the round settled.
    let table = payloads(&app, &cover("table", TABLE));
    let from_deal: Vec<&str> = names(&table)
        .into_iter()
        .skip_while(|name| *name != "RoundDealt")
        .collect();
    assert_eq!(
        from_deal,
        [
            "RoundDealt",
            "TurnStarted",
            "HandDoubled",
            "TurnStarted",
            "CardDealt",
            "TurnStarted",
            "CardDealt",
            "TurnStarted",
            "HandStood",
            "DealerPlayed",
            "RoundSettled",
        ]
    );
    // Seat 0 is asked first, then seat 1 (EU-1609); a hand finished by a
    // double is not asked again (EU-1608).
    let turns: Vec<i32> = decoded::<v1::TurnStarted>(&table)
        .iter()
        .map(|t| t.seat)
        .collect();
    assert_eq!(turns, [0, 1, 1, 1]);

    let [settled] = &decoded::<v1::RoundSettled>(&table)[..] else {
        panic!("one RoundSettled: {:?}", names(&table));
    };
    let outcomes: Vec<(i32, i64, Outcome, i64, i64)> = settled
        .outcomes
        .iter()
        .map(|o| {
            (
                o.seat,
                o.wager,
                Outcome::try_from(o.outcome).expect("an outcome"),
                o.returned,
                o.stack_after,
            )
        })
        .collect();
    assert_eq!(
        outcomes,
        [
            (0, 40, Outcome::Win, 80, 540),
            (1, 20, Outcome::Win, 40, 520),
        ]
    );
    assert_eq!(settled.house_delta, -60);

    let exits = app.shutdown();
    assert_eq!(exits.len(), 1, "the player child stopped: {exits:?}");
}
