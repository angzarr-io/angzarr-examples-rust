> **⚠️ Notice:** This repository was recently extracted from the [angzarr monorepo](https://github.com/angzarr-io/angzarr) and has not yet been validated as a standalone project. Expect rough edges. See the [Angzarr documentation](https://angzarr.io/) for more information.

# angzarr-examples-rust

Example implementations demonstrating Angzarr event sourcing patterns in Rust.

## Architecture

### Aggregates
Command handlers that emit events. Single domain, source of truth.
- `player/agg` - Player registration, bankroll management
- `table/agg` - Table management, seating
- `hand/agg` - Poker hand logic
- `tournament/agg` - Tournament management

### Sagas
Stateless domain translators (events → commands).
- `table/saga-hand` - HandStarted → DealCards
- `hand/saga-table` - HandComplete → EndHand
- `hand/saga-player` - PotAwarded → DepositFunds
- `table/saga-player` - HandEnded → ReleaseFunds

### Process Managers
Stateful multi-domain orchestrators.
- `pmg-buy-in` - Player ↔ Table buy-in coordination
- `pmg-registration` - Player ↔ Tournament registration
- `pmg-rebuy` - Player ↔ Tournament ↔ Table rebuy flow
- `pmg-hand-flow` - Hand lifecycle orchestration

## PM Design Philosophy

**PMs are coordinators, NOT decision makers.**

### Output Options

| Output | When to Use | Aggregate Response |
|--------|-------------|-------------------|
| **Commands** (preferred) | Normal flow - aggregate validates | Accept/Reject |
| **Facts** | Inject external data aggregate can't derive | Always accepted |

### Key Principles

1. **Don't rebuild destination state** - PMs receive sequences, not EventBooks
2. **Let aggregates decide** - Business logic in aggregates, not coordinators
3. **Prefer commands with sync mode** - Use `SyncMode::Simple` for immediate feedback
4. **Use facts sparingly** - Only for external data injection

```rust
// PM sends command, aggregate decides
fn handle_buy_in_requested(&self, ..., destinations: &Destinations) {
    let cmd = SeatPlayer { player_root, seat, amount };
    let seq = destinations.sequence_for("table")?;
    // Table aggregate validates seat availability, buy-in range, etc.
    // PM handles rejection via on_rejected()
}
```

## Prerequisites

- Rust build tools
- Buf CLI for proto generation
- Kind (for Kubernetes deployment)

## Building

```bash
cargo build
cargo test
```

## Running

### Standalone Mode

Run with standalone runtime configuration.

### Kubernetes Mode

```bash
skaffold run
```

## License

BSD-3-Clause
