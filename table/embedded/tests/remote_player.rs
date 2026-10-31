//! The table in process and the REMOTE Python player bot as a child
//! process, through angzarr-embedded-host (bot_play.feature, EU-1605 to
//! EU-1609): a driver seats two players and deals; from there the turn
//! crosses to the player and back by the two policies alone, until the
//! round settles.
//!
//! Ignored by default: the first run needs `uv` and the network (uv fetches
//! the player's dependencies, angzarr-client-python among them). Run it
//! with `ANGZARR_PLAYER_DIR=<examples-python>/player cargo test -p
//! table-embedded --test remote_player -- --ignored`: the player's project
//! is `$ANGZARR_PLAYER_DIR`, which must be set (there is no default).
//!
//! The hand is seed 21's ([`seed_21`]): the bot's simplified basic strategy
//! makes the moves the scenario names, and the round settles as it says.

use std::path::PathBuf;

use agg_table::test_support::seed_21::{self, ALICE, BOB};
use angzarr_embedded::v1::{command_page, Command, CommandBook, CommandPage, Cover, Uuid};
use angzarr_embedded::{pack, type_name, Any, Sampled, Timestamp};
use angzarr_embedded_host::{run_with, App, HostConfig, Languages};
use examples_proto::v1::{self, MoveAction};
use prost::{Message, Name};
use table_embedded::register;

const TABLE: u8 = 0x7a;

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

/// The player bot's project directory, `$ANGZARR_PLAYER_DIR`
/// ([`player_dir_from`]); panics, saying what to set, when it is unset.
fn player_dir() -> PathBuf {
    player_dir_from(std::env::var_os("ANGZARR_PLAYER_DIR"))
        .unwrap_or_else(|error| panic!("{error}"))
}

/// The player's project directory, from `ANGZARR_PLAYER_DIR`'s value
/// (`var`): the directory it names, or, unset or empty, why not.
fn player_dir_from(var: Option<std::ffi::OsString>) -> Result<PathBuf, String> {
    match var {
        Some(dir) if !dir.is_empty() => Ok(PathBuf::from(dir)),
        _ => Err(
            "ANGZARR_PLAYER_DIR is not set: set it to the player's project, \
                  the `player` directory of an examples-python checkout \
                  (ANGZARR_PLAYER_DIR=<examples-python>/player)"
                .to_string(),
        ),
    }
}

/// Where the player's project is, from `ANGZARR_PLAYER_DIR`'s value: set
/// and not empty, that directory; unset or empty, an error saying what to
/// set. There is no default: a guessed checkout goes stale, and a test run
/// against a stale player is a test of the wrong code.
#[test]
fn the_player_dir_is_angzarr_player_dir_and_unset_says_what_to_set() {
    assert_eq!(
        player_dir_from(Some("/checkouts/examples-python/player".into())),
        Ok(PathBuf::from("/checkouts/examples-python/player"))
    );
    for unset in [None, Some("".into())] {
        let error = player_dir_from(unset).unwrap_err();
        assert!(
            error.contains("ANGZARR_PLAYER_DIR") && error.contains("examples-python"),
            "{error}"
        );
    }
}

/// The application's host config, `angzarr.yaml` at `schema_version: 1`,
/// in angzarr-embedded-host's strict shape (what `angzarr scaffold app
/// --python player --in-memory` writes for the spec's table, player and
/// policies protos): every component listed with its kind, the table and
/// both policies (by their spec names, decision 14.50) in process, the
/// player a Python child; an in-memory store. Parsed from the YAML here,
/// so no file is written; [`run_with`] runs it.
fn config() -> HostConfig {
    let text = format!(
        r#"schema_version: 1
component:
  - name: table
    kind: domain
    language: rust
  - name: player
    kind: domain
    language: python
    entry: [angzarr-player]
    dir: {dir:?}
  - name: policy-table-player
    kind: policy
    language: rust
  - name: policy-player-table
    kind: policy
    language: rust
supervision:
  handshake_timeout_ms: 300000
store:
  in_memory: true
"#,
        dir = player_dir().display().to_string(),
    );
    HostConfig::from_yaml(&text, "remote_player.rs").expect("the host config loads")
}

/// A driver of the table in an application: it numbers its commands from
/// 1, and each command's ids and time follow from its number.
struct Driver<'a> {
    app: &'a App,
    sent: u8,
}

impl<'a> Driver<'a> {
    fn new(app: &'a App) -> Self {
        Driver { app, sent: 0 }
    }

    /// The driver's next command to the table, which must be accepted.
    async fn send<M: Message + Name>(&mut self, command: M) {
        self.sent += 1;
        let n = self.sent;
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
        if let Err(failure) = self.app.handle(book, &sampled).await {
            panic!("command {n} ({}) failed: {failure:?}", M::full_name());
        }
    }
}

/// Seed 21's table opened and its round dealt: created, both players
/// seated, both bets placed, then the deal.
async fn deal_seed_21(app: &App) {
    let mut table = Driver::new(app);
    table.send(seed_21::create_table()).await;
    for (request, confirm) in seed_21::seats() {
        table.send(request).await;
        table.send(confirm).await;
    }
    for bet in seed_21::bets() {
        table.send(bet).await;
    }
    table.send(v1::DealRound {}).await;
}

/// The payloads of the stream `cover` names.
async fn payloads(app: &App, cover: &Cover) -> Vec<Any> {
    app.events(cover)
        .await
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
async fn moves(app: &App, player: u8) -> Vec<(i32, MoveAction)> {
    decoded::<v1::MoveDecided>(&payloads(app, &cover("player", player)).await)
        .into_iter()
        .map(|m| (m.seat, MoveAction::try_from(m.action).expect("an action")))
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs uv, and the network the first time"]
async fn eu_1605_to_1609_the_remote_player_plays_seed_21_to_settlement() {
    let app = run_with(config(), Languages::builtin(), register)
        .await
        .expect("the application starts with the player child");
    // The deal sets off the whole loop: TurnStarted, DecideMove to the
    // player, MoveDecided, back to the table, until the round settles.
    deal_seed_21(&app).await;

    // Each seat's own player was asked, and decided by basic strategy
    // (EU-1606, EU-1608, EU-1609).
    for player in [ALICE, BOB] {
        assert_eq!(moves(&app, player).await, seed_21::moves_of(player));
    }

    // The table's history, from the deal: each move came back as the
    // table's command, and the round settled.
    let table = payloads(&app, &cover("table", TABLE)).await;
    let from_deal: Vec<&str> = names(&table)
        .into_iter()
        .skip_while(|name| *name != "RoundDealt")
        .collect();
    assert_eq!(from_deal, seed_21::FROM_DEAL);
    // Each move was asked for by a turn: seat 0 first, then seat 1
    // (EU-1609); a hand finished by a double is not asked again (EU-1608).
    let turns: Vec<i32> = decoded::<v1::TurnStarted>(&table)
        .iter()
        .map(|t| t.seat)
        .collect();
    assert_eq!(turns, seed_21::MOVES.map(|(_, seat, _)| seat));

    let [settled] = &decoded::<v1::RoundSettled>(&table)[..] else {
        panic!("one RoundSettled: {:?}", names(&table));
    };
    seed_21::assert_settled(settled);

    let exits = app.shutdown().await;
    assert_eq!(exits.len(), 1, "the player child stopped: {exits:?}");
}
