# angzarr-examples-rust

Example implementations demonstrating Angzarr event sourcing patterns in Rust. See the [Angzarr documentation](https://angzarr.io/) for more information.

> The poker example has been retired. A blackjack example is coming; its spec lives in [angzarr-project](https://github.com/angzarr-io/angzarr-project) under `proto/io/angzarr/examples/v1` and `features/example/blackjack*`.

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
just up              # Kind cluster with coordinators + infrastructure
```

## License

BSD-3-Clause
