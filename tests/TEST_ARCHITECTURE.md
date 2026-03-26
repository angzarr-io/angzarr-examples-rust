# Rust Test Architecture Plan

## Current State

The Rust examples already have a good BDD test structure using cucumber-rs. However, there's duplication of helper functions across test files.

## Current Structure

```
examples-rust/main/tests/
├── Cargo.toml
├── build.rs
├── src/
│   └── lib.rs                      # NEW: Shared test helpers (created)
├── tests/
│   ├── player.rs                   # BDD test runner
│   ├── table.rs
│   ├── hand.rs
│   ├── orchestration.rs
│   └── acceptance.rs
└── features/
    ├── unit/
    │   ├── player.feature
    │   ├── table.feature
    │   ├── hand.feature
    │   ├── orchestration.feature
    │   ├── process_manager.feature
    │   ├── saga.feature
    │   └── projector.feature
    └── acceptance/
        ├── poker_game.feature
        └── sync_modes.feature
```

## Proposed Changes

### 1. Expand src/lib.rs (Already Started)

```rust
// src/lib.rs - Shared test helpers

// Proto helpers (DONE)
pub fn uuid_for(seed: &str) -> Vec<u8>
pub fn pack_cmd<T: Message>(cmd: &T, type_name: &str) -> Any
pub fn command_book(root: &[u8], domain: &str) -> CommandBook
pub fn event_book(root: &[u8], domain: &str, events: &[Any]) -> EventBook
pub fn currency(amount: i64) -> Currency
pub fn parse_card(s: &str) -> Card
pub fn parse_cards(s: &str) -> Vec<Card>

// NEW: State builders
pub mod builders {
    pub struct PlayerStateBuilder { ... }
    pub struct TableStateBuilder { ... }
    pub struct HandStateBuilder { ... }
}

// NEW: Assertion helpers
pub mod assertions {
    pub fn assert_event_type(result: &EventBook, expected: &str)
    pub fn assert_event_field<T>(result: &EventBook, extractor: impl Fn(&T) -> bool)
    pub fn assert_command_rejected(result: &Result<EventBook, Error>, code: &str)
}

// NEW: Executor helpers
pub mod executors {
    pub fn execute_command<H, C, S>(handler: H, cmd: C, state: &S) -> Result<EventBook, Error>
    pub fn rebuild_state<S>(event_book: &EventBook) -> S
}
```

### 2. Refactor Test Files to Use Shared Helpers

Before (player.rs):
```rust
// Duplicated in each test file
fn uuid_for(seed: &str) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(seed.as_bytes());
    let result = hasher.finalize();
    result[..16].to_vec()
}
```

After (player.rs):
```rust
use poker_tests::{uuid_for, currency, command_book, PlayerStateBuilder};

#[given("a registered player with bankroll {int}")]
fn registered_player(world: &mut PlayerWorld, amount: i64) {
    world.state = PlayerStateBuilder::new()
        .registered()
        .with_bankroll(amount)
        .build();
}
```

### 3. Add Missing Test Scenarios

Current gaps to fill:
- Cascade/compensation scenarios in orchestration.feature
- Full saga coverage (currently 9 scenarios)
- Process manager timeout scenarios

### 4. Module Structure

```rust
// tests/src/lib.rs
pub mod proto;       // pack/unpack, event_book, command_book
pub mod builders;    // PlayerStateBuilder, TableStateBuilder, etc.
pub mod assertions;  // assert_event_type, assert_rejected
pub mod executors;   // execute_command, rebuild_state
pub mod cards;       // parse_card, format_card

// Re-export common items
pub use proto::*;
pub use builders::*;
pub use assertions::*;
```

## Implementation Steps

### Phase 1: Expand lib.rs (Partially Done)
- [x] uuid_for, pack_cmd, command_book, event_book
- [x] currency, parse_card, parse_cards
- [ ] PlayerStateBuilder, TableStateBuilder, HandStateBuilder
- [ ] Assertion helpers
- [ ] Executor helpers

### Phase 2: Refactor Existing Tests
- [ ] Update player.rs to use shared helpers
- [ ] Update table.rs to use shared helpers
- [ ] Update hand.rs to use shared helpers
- [ ] Update orchestration.rs to use shared helpers

### Phase 3: Add Missing Coverage
- [ ] Add cascade scenarios to orchestration.feature
- [ ] Add timeout scenarios to process_manager.feature
- [ ] Add edge case scenarios

## Test Count Targets

| Feature | Current | Target |
|---------|---------|--------|
| player.feature | 17 | 17 |
| table.feature | 21 | 21 |
| hand.feature | 48 | 48 |
| orchestration.feature | 18 | 25 |
| saga.feature | 9 | 12 |
| process_manager.feature | 21 | 25 |
| projector.feature | 31 | 31 |
| **Total Unit** | **165** | **179** |

## Benefits

1. **Single source of truth**: Helper functions defined once
2. **Easier maintenance**: Fix bugs in one place
3. **Consistent patterns**: Same API across all test files
4. **Faster test writing**: Builders reduce boilerplate
5. **Better assertions**: Semantic assertion helpers improve readability
