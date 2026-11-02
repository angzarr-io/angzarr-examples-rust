# angzarr-examples-rust

Example implementations demonstrating Angzarr event sourcing patterns in Rust. See the [Angzarr documentation](https://angzarr.io/) for more information.

The poker example is parked, not deleted: it was removed for its complexity
(owner, 2026-10-27, "at least for now") and may come back. Its last version
is at 59bd1b8e (`git checkout 59bd1b8e`); it was removed in aabe5e3. Blackjack
replaces it as the example. This repository holds the Rust side of the
blackjack example: the aggregates, policies, process managers and
projections. The virtual players are Python. The spec (protos under
`proto/io/angzarr/examples/v1/`, Gherkin under `features/example/blackjack*/`)
lives in the [angzarr-project](https://github.com/angzarr-io/angzarr-project)
submodule; the submodule pin is the contract this repository implements.

## Crates

- `proto` (`examples-proto`) - the generated `io.angzarr.examples.v1`
  layer, from the submodule's protos (cards, table, player, policies) by
  angzarr-build (angzarr-cli) in `proto/build.rs` (needs `protoc`): the wire
  types (`v1`) and the markup domain layer over them (`domain`): every
  command, event and typed refusal, `TableRefused` (table.proto's Reason
  codes; keyed by its full name and reason, as angzarr-codegen's
  `refusal_key` gives, e.g. `io-angzarr-examples-v1-tablerefused-already-bet`,
  its set fields its args) and `SeatNotConfirmable` among them.
- `table/agg` (`agg-table`) - the table aggregate: `Table`, implementing
  the table's interface `domain::TableAggregate` (examples-proto's
  generated domain layer's, one method per command and event; `angzarr
  scaffold rust` writes only its implementation's stub) over the domain
  layer, its state the spec's `TableState`; the deterministic shoe
  (`cards.proto`) and the hand's value. Tests sit next to each module in
  `*.test.rs` and cite the spec's EU ids.
- `table/policies` (`table-player-policies`) - the two policies between
  the table and the Python player bot as plain Rust traits over the domain
  types (`TablePlayer`: TurnStarted to DecideMove; `PlayerTable`:
  MoveDecided to Hit, Stand or DoubleDown), no framework.
- `table/embedded` (`table-embedded`) - the table and both policies on
  angzarr-embedded (angzarr's in-app runtime). The table's markup and its
  `register()` are generated from table.proto into `OUT_DIR`
  (`table/embedded/build.rs`, angzarr-build's `adapters` and `register`),
  delegating to agg-table's `Table`. The policies are written in the markup
  (`#[policy]`) over table-player-policies; the player domain they name is
  a command set and an event set (`PlayerAggregate`), generated from
  player.proto (angzarr-build's `stand_ins`) and never registered. Its
  `register` registers the table and both policies.

  angzarr-embedded and angzarr-build are pinned by hash in the workspace
  `Cargo.toml`. `tests/remote_player.rs` plays a whole hand against the
  Python player bot (examples-python's `player`), hosted as a child
  process by angzarr-embedded-host from a host config of the shape
  `angzarr scaffold app` writes; it is ignored by default (it needs `uv`):

  ```bash
  ANGZARR_PLAYER_DIR=<examples-python>/player \
    cargo test -p table-embedded --test remote_player -- --ignored
  ```
- `wallet/agg` (`agg-wallet`) - the wallet aggregate (wallet.proto):
  `WalletAggregateImpl`, implementing the generated
  `domain::WalletAggregate` (`angzarr scaffold rust` wrote its stub), its
  state the spec's `WalletState`; its rejection keys (`reason`) and the
  upcasting rule for deposits older versions stored (`upcast`).
- `wallet/policies` (`wallet-policies`) - policy-table-wallet: a settled
  round as each seat's round result and loyalty points, facts for its
  wallet.
- `buy-in/pm` (`pm-buy-in`) - the buy-in process manager (buy_in.proto):
  `BuyInProcessManagerImpl`, implementing `domain::BuyInProcessManager`,
  tracked under the conversation (correlation) it started in.
- `buy-in/policies` (`buy-in-policies`) - the buy-in's four policies: the
  table's and the wallet's news to the buy-in, and the buy-in's steps to
  them (ConfirmSeat's refusal translated into SeatConfirmationRefused).
- `blackjack/embedded` (`blackjack-embedded`) - the table with wallets and
  buy-ins on angzarr-embedded: the generated adapters and `register()` of
  the table, the wallet, the buy-in and the five policies over their
  implementations, the wallet's upcaster added. `tests/buy_in.rs` runs
  buy-ins end to end on angzarr-embedded-host's `App` in process: one that
  goes through, and one rolled back when the seat is released meanwhile.
  table-embedded stays the bot table, seated by a driver with no buy-in.
- `tools/archlint` - the module-layering linter for `archlint.toml`.

## Development

### Setup

Install git hooks (requires [lefthook](https://github.com/evilmartians/lefthook)):

```bash
lefthook install
```

This configures a pre-commit hook that auto-formats Rust sources before each commit,
and a pre-push hook (`just push-check`) that runs every test of each pushed commit
in a tree of its own: red tests may be pushed to a branch, never to `main`.

The recipes run in the devcontainer, whose image (`.devcontainer/Containerfile`)
sets the toolchain, Rust 1.96.1 (angzarr-build needs 1.96); CI installs the same.

### Recipes

```bash
just -l              # List all available recipes
just build           # Build the workspace
just test            # Run tests
just fmt             # Check formatting
just fmt-fix         # Auto-format code
just lint            # Clippy, warnings denied
just archlint        # Module layering rules (archlint.toml)
just mutation-test   # cargo-mutants over the aggregate, policies and embedded host
just mutation-push   # by hand: the same, over what a push adds (stdin: git's pushed refs)
just push-check origin  # pre-push: every test of each pushed commit; red refused on main only
just up              # Kind cluster with infrastructure (postgres, rabbitmq)
```

## License

Licensed under either of Apache License 2.0 or BSD 3-Clause, at your option
([LICENSE-APACHE](LICENSE-APACHE), [LICENSE-BSD](LICENSE-BSD)).
