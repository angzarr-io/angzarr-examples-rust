# angzarr-examples-rust

Example implementations demonstrating Angzarr event sourcing patterns in Rust. See the [Angzarr documentation](https://angzarr.io/) for more information.

The poker example is parked, not deleted: it was removed for its complexity
(owner, 2026-10-27, "at least for now") and may come back. Its last version
is at 59bd1b8e (`git checkout 59bd1b8e`); it was removed in aabe5e3. Blackjack
replaces it as the example. This repository holds the Rust side of the
blackjack example: the aggregates, sagas, process managers and
projections. The virtual players are Python. The spec (protos under
`proto/io/angzarr/examples/v1/`, Gherkin under `features/example/blackjack*/`)
lives in the [angzarr-project](https://github.com/angzarr-io/angzarr-project)
submodule; the submodule pin is the contract this repository implements.

## Crates

- `proto` (`examples-proto`) - the generated `io.angzarr.examples.v1` types
  (cards, table), read straight from the submodule's protos.
- `table/agg` (`agg-table`) - the table aggregate's domain logic as pure
  functions with no framework dependency: the deterministic shoe
  (`cards.proto`), the CreateTable decision (TableCreated + ShoeShuffled) and
  the appliers that fold those events into `TableState`. Tests sit next to
  each module in `*.test.rs` and cite the spec's EU ids.
- `tools/archlint` - the module-layering linter for `archlint.toml`.

## Development

### Setup

Install git hooks (requires [lefthook](https://github.com/evilmartians/lefthook)):

```bash
lefthook install
```

This configures a pre-commit hook that auto-formats Rust sources before each commit.

### Recipes

```bash
just -l              # List all available recipes
just build           # Build the workspace
just test            # Run tests
just fmt             # Check formatting
just fmt-fix         # Auto-format code
just lint            # Clippy, warnings denied
just archlint        # Module layering rules (archlint.toml)
just mutation-test   # cargo-mutants over the aggregates
just up              # Kind cluster with coordinators + infrastructure
```

## License

BSD-3-Clause
