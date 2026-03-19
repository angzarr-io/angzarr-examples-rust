# Spec vs Angzarr Framework: Divergence Analysis

## Executive Summary

The spec-draft.md describes a sophisticated DDD/event-sourcing architecture for poker that aligns **conceptually** with angzarr but diverges in **several critical implementation patterns**. The most significant gaps involve:

1. **Sync cascade pattern** - spec's direct saga calls don't exist in angzarr
2. **Fact injection semantics** - terminology overlap but different mechanics
3. **Process manager scope** - HandController PM is unusually fine-grained
4. **Saga statefulness** - spec implies saga state that angzarr doesn't support

---

## 1. Terminology Mapping

| Spec Term | Angzarr Equivalent | Alignment | Notes |
|-----------|-------------------|-----------|-------|
| **Command** | Command | ✅ Exact | Both use commands as aggregate inputs |
| **Event** | Event | ✅ Exact | Aggregate-produced outputs |
| **Fact** | Event (via HandleEvent RPC) | ⚠️ Partial | Angzarr calls this "fact injection" but mechanics differ |
| **Aggregate** | CommandHandler | ✅ Exact | Business logic, Guard/Validate/Compute |
| **EventBook** | EventBook | ✅ Exact | Storage container for events + snapshot |
| **Command Book** | CommandBook | ⚠️ Partial | Angzarr CommandBook is a request envelope, not a queue |
| **Snapshot** | Snapshot | ✅ Exact | Materialized state at a sequence |
| **Coordinator** | Aggregate Coordinator | ✅ Exact | Serializes sequence numbers |
| **Saga** | Saga | ⚠️ Divergent | See section 3 |
| **Process Manager** | ProcessManager | ⚠️ Divergent | See section 4 |

### Key Terminology Issues

**Command Book as Queue vs Envelope:**
- Spec: "where commands arrive for an aggregate. Input queue."
- Angzarr: CommandBook is a single request containing Cover + pages, not a persistent queue

This is mostly semantic - angzarr's message bus + coordinator handle the "queue" semantics. However, the spec's mental model of "writing to a command book" maps better to angzarr's `HandleCommand` RPC.

---

## 2. Sync Cascade Pattern - MAJOR DIVERGENCE

### Spec Pattern
```
Aggregate → calls saga directly → saga writes command to foreign aggregate →
foreign aggregate responds → saga calls back → original aggregate continues
```

Example from spec (Buy-In flow):
```
PlayerAccount evaluates InitiateBuyIn
  → calls BuyInSeat saga directly (no bus)
    → saga WRITES COMMAND: SeatPlayer to Table
      → Table evaluates, produces PlayerSeated or SeatingRejected
      → Table calls BuyInConfirm saga directly
        → saga WRITES COMMAND: ConfirmBuyIn to PlayerAccount
```

### Angzarr Reality

**Angzarr does NOT support synchronous saga invocation from within aggregate evaluation.**

The angzarr aggregate handler returns a `BusinessResponse` (events OR revocation). It cannot:
- Call external services during evaluation
- Wait for cross-aggregate responses
- Chain synchronous operations

### Workarounds

**Option A: Saga with CASCADE sync mode**
```
Client sends InitiateBuyIn
  → Aggregate produces BuyInReserved event
  → Event published to bus (CASCADE mode)
  → Saga observes, sends SeatPlayer command to Table
  → Table produces PlayerSeated/SeatingRejected
  → Second saga observes Table event, sends ConfirmBuyIn back
  → PlayerAccount produces BuyInConfirmed
  → CASCADE completes, client gets response
```
- ✅ Achieves synchronous client experience
- ⚠️ Still eventually consistent internally
- ❌ Complex failure handling (what if step 3 fails after step 1 committed?)

**Option B: Process Manager orchestration**
```
Client sends InitiateBuyIn to BuyInOrchestrator PM
  → PM Prepare phase declares destinations: [PlayerAccount, Table]
  → PM Handle phase:
    1. Commands PlayerAccount to reserve
    2. Commands Table to seat
    3. Commands PlayerAccount to confirm/release based on Table response
  → PM produces BuyInCompleted event
```
- ✅ Single orchestration point
- ✅ Clear failure/compensation semantics
- ⚠️ Adds PM infrastructure overhead
- ⚠️ Changes command entry point (PM, not PlayerAccount)

**Option C: Client-side orchestration**
```
Client:
  1. Call PlayerAccount.InitiateBuyIn → BuyInReserved
  2. Call Table.SeatPlayer → PlayerSeated/SeatingRejected
  3. Call PlayerAccount.ConfirmBuyIn or ReleaseBuyIn
```
- ✅ Simple, no framework changes
- ❌ Client must handle failures and retries
- ❌ Exposes internal flow to clients
- ❌ Multi-step client protocol

### Recommendation

**Use Process Manager for buy-in/registration/rebuy flows.** The spec's sync cascade is architecturally clean but doesn't map to angzarr. A BuyInOrchestrator PM is the closest equivalent.

---

## 3. Saga Divergences

### Spec's Saga Types

The spec defines two saga categories:

**A. Async (bus-based)**
```
HandSettlement: Observes HandCompleted → Creates fact HandSettled in Table
Payout: Observes PayoutTriggered → Creates fact TournamentPrizeAwarded in PlayerAccount
```

**B. Sync Cascade (direct call)**
```
BuyInSeat: Called by PlayerAccount → Writes command SeatPlayer to Table
CashOutCredit: Called by Table → Creates fact CashOutCredited in PlayerAccount
```

### Angzarr Saga Reality

Angzarr sagas are **strictly async and stateless**:
- Subscribe to events from a source domain
- Emit commands to target domains
- No state between invocations
- No direct calling from aggregates

### Mapping Issues

| Spec Saga | Type | Angzarr Support | Issue |
|-----------|------|-----------------|-------|
| HandSettlement | Async fact | ⚠️ Partial | Can emit fact via HandleEvent RPC, but "fact" is just an event with external_id |
| Payout | Async fact | ⚠️ Partial | Same as above |
| BuyInSeat | Sync cascade | ❌ Not supported | No sync saga invocation |
| CashOutCredit | Sync cascade | ❌ Not supported | No sync saga invocation |
| HandControllerForcedBets | PM→Aggregate bridge | ⚠️ Complex | See section 4 |

### Fact vs Event Semantics

**Spec definition:**
> "Fact: a special event written into an aggregate's EventBook that the aggregate didn't produce itself. May come from a saga, a process manager, or an external actor."

**Angzarr implementation:**
- `HandleEvent` RPC accepts events with `external_id` for idempotency
- These events bypass command validation
- The aggregate still processes them and may produce follow-on events

**The semantic gap:** Angzarr treats "facts" as externally-injected events. The spec treats facts as a distinct conceptual category. In practice, this works fine - the distinction is documentation/modeling, not runtime behavior.

### Saga Creates Fact vs Command

Spec heuristic:
> "Saga creates fact when outcome is deterministic. Writes command when target has invariants to enforce."

Angzarr sagas always emit commands via `SagaResponse.commands`. To "create a fact," the saga would emit a command that the coordinator routes to `HandleEvent` instead of `HandleCommand`.

**Implementation:** Add a flag or convention (e.g., command type suffix `*Fact`) that the coordinator interprets as fact injection.

---

## 4. Process Manager Divergences

### Spec's Process Managers

**TournamentOrchestrator:**
- Consumes: Tournament, Table events
- Produces: Events to own EventBook + commands/facts to Table, Tournament

**HandController:**
- Consumes: Hand events
- Produces: Events to own EventBook + commands to Hand
- One instance per hand

**TableManager:**
- Consumes: Table events (all cash tables)
- Produces: Commands to create/close Tables

### Angzarr PM Model

Angzarr PMs use a **two-phase protocol**:

1. **Prepare**: Declare which destination aggregates are needed
2. **Handle**: Receive trigger event + destination states, emit events/commands

### Divergence Analysis

**TournamentOrchestrator:** ✅ Good fit
- Multi-domain consumption (Tournament, Table)
- Stateful (tracks pending moves, table sizes)
- Commands to foreign aggregates
- Angzarr PM handles this well

**TableManager:** ✅ Good fit
- Multi-table observation
- Policy decisions (open/close tables)
- Commands to Table aggregates
- Standard PM pattern

**HandController:** ⚠️ Unusual fit
- Single-domain consumption (Hand only)
- One PM instance per hand (fine-grained)
- Heavy event production to own EventBook
- Bridging sagas to translate PM events → Hand commands

The HandController is essentially a **state machine for hand sequencing** implemented as a PM. This works in angzarr but is heavyweight:

1. PM must declare Hand as destination in Prepare
2. PM Handle receives Hand state, decides next step
3. PM emits event (e.g., `DealRequested`)
4. Separate saga observes PM event, writes `DealHoleCards` command to Hand
5. Hand processes command, produces events
6. Events published, PM observes, loop continues

### Alternative: Embedded State Machine

Consider embedding the hand sequencing logic **inside the Hand aggregate**:

```rust
impl HandAggregate {
    fn compute(&self, cmd: Command, state: &State) -> Vec<Event> {
        match cmd {
            PostForcedBets => {
                let events = self.validate_and_post_blinds(state);
                // State machine: after ForcedBetsPosted, auto-advance
                if self.straddle_window_enabled(state) {
                    events.push(StraddleWindowOpened { ... });
                } else {
                    events.push(ReadyToDeal { ... });
                }
                events
            }
            // ...
        }
    }
}
```

**Pros:**
- Eliminates PM/saga overhead for hand sequencing
- Single aggregate controls hand flow
- Simpler debugging (all state in one EventBook)

**Cons:**
- Hand aggregate becomes complex (sequencing + rules)
- Player timeouts need external timer integration
- Timer events still require saga/external injection

### Recommendation

**Keep HandController as PM** but simplify the bridging saga layer. Instead of:
```
PM emits DealRequested → Saga observes → Saga writes DealHoleCards command
```

Consider:
```
PM emits DealHoleCards command directly (no intermediate saga)
```

Angzarr PMs can emit commands in their `Handle` response. The extra saga layer adds latency and failure points.

---

## 5. External Actor Fact Injection

### Spec Pattern

External actors create facts directly:
```
RNG service → DeckPrepared fact in Hand EventBook
Payment processor → FundsDeposited fact in PlayerAccount EventBook
Floor manager → ChipsAdjusted fact in Table EventBook
```

### Angzarr Support

✅ **Fully supported** via `HandleEvent` RPC with `external_id`.

The coordinator's `HandleEvent(EventRequest)` accepts:
- Cover (domain, root, correlation_id)
- EventPages with sequence type `ExternalDeferredSequence`
- `external_id` for idempotency

The aggregate processes the event and may produce follow-on events.

### Implementation Notes

1. **RNG Service Integration:**
   - DeckPreparation saga requests shuffle from RNG
   - RNG service calls `HandleEvent` with DeckPrepared
   - Hand aggregate processes, produces ReadyToDeal

2. **Payment Processor:**
   - External webhook → payment service
   - Payment service calls `HandleEvent` with FundsDeposited
   - PlayerAccount processes, updates balance

3. **Floor Decisions:**
   - Admin UI sends ChipsAdjusted via `HandleEvent`
   - No command validation (it's a fact, not a request)

---

## 6. Compound Event Production

### Spec Pattern

> "All events in a compound set are produced in one evaluation pass. The aggregate does not consume its own events."

Example:
```
PlaceAction may produce: ActionPlaced + PlayerWentAllIn + PotsRecalculated + SidePotCreated + StreetClosed
```

### Angzarr Support

✅ **Fully supported.** Aggregate `compute()` returns `Vec<Event>`. All events in the vector are:
- Assigned sequential sequence numbers
- Persisted atomically
- Published to bus together

No issues here.

---

## 7. Timer Integration

### Spec Pattern

HandController manages player action timers:
```
EVENT: TimerStarted { seat: 4, duration: 30s }
... time passes ...
EVENT: TimerExpired { seat: 4 }
→ Saga writes ExpireActionTimer command to Hand
```

### Angzarr Considerations

Angzarr doesn't provide built-in timer management. Options:

**Option A: External Timer Service**
- Separate service receives `TimerStarted` events
- Schedules callbacks
- On expiry, calls saga or PM trigger endpoint

**Option B: PM Internal Timers**
- HandController PM maintains timer state
- Uses Tokio timers internally
- On expiry, emits `TimerExpired` event

**Option C: Temporal.io Integration**
- Timer workflows in Temporal
- Temporal calls angzarr on expiry

### Recommendation

**Option B (PM internal timers)** for lowest latency. The HandController PM already tracks hand state; adding timer management is natural.

Implementation sketch:
```rust
struct HandControllerState {
    active_timers: HashMap<Seat, Instant>,
    // ...
}

impl ProcessManager for HandController {
    fn handle(&mut self, trigger: Event, destinations: &[EventBook]) -> PmResponse {
        // Check for expired timers
        let now = Instant::now();
        let expired: Vec<_> = self.active_timers
            .iter()
            .filter(|(_, deadline)| **deadline <= now)
            .map(|(seat, _)| *seat)
            .collect();

        let mut events = vec![];
        for seat in expired {
            events.push(TimerExpired { seat });
            self.active_timers.remove(&seat);
        }

        // Process trigger event...
    }
}
```

---

## 8. Replay / Speculative Execution

### Spec Implicit Need

For tournament features like chip race calculation, speculative execution is needed:
```
What would happen if we ran this chip race configuration?
```

### Angzarr Support

✅ **Fully supported** via speculative clients:
- `HandleSyncSpeculative` for command handlers
- `SpeculateSagaRequest` for sagas
- `SpeculateProjectorRequest` for projectors

---

## 9. Multi-Variant Rule Engine

### Spec Pattern

Hand aggregate loads variant rules from storage:
> "Variant rules are loaded from rules data storage on startup and injected via IoC."

Variants include: NL Hold'em, 7-Card Stud, 5-Card Draw, Omaha, etc.

### Angzarr Considerations

Angzarr aggregates are stateless gRPC services. "Injected via IoC" means:
- Variant rules loaded at service startup
- Passed to handler via constructor/factory
- No runtime variant switching per-request

**Issue:** Mixed games rotate variants. The Hand aggregate needs different rule configurations for different hands.

### Solutions

**Option A: Variant in HandInitiated fact**
```
HandInitiated { variant: SevenCardStud, rules: {...} }
```
The fact carries all variant-specific configuration. Aggregate is truly stateless.

**Option B: Variant service lookup**
```rust
fn compute(&self, cmd: Command, state: &State) -> Vec<Event> {
    let variant = state.variant; // from HandInitiated
    let rules = self.rule_service.get(variant); // external call
    // ...
}
```

**Option C: Compile-time variant handlers**
Separate handler implementations per variant, routed by type URL suffix.

### Recommendation

**Option A** is cleanest. The HandInitiated fact already carries variant info; extend it to include all necessary rule parameters. This keeps the aggregate stateless and the variant rules explicit in the event stream.

---

## 10. Kill Pot State Crossing

### Spec Pattern

Kill state persists on Table, crosses to Hand via HandInitiated:
```
Table processes HandSettled → produces KillActivated
HandInitiation saga reads Table state → includes killState in HandInitiated
Hand processes HandInitiated with kill rules active
```

### Angzarr Fit

✅ **Good fit.** This is the standard saga pattern:
1. Saga observes trigger event
2. Saga queries source aggregate state (Table)
3. Saga includes state in command/fact payload (HandInitiated)
4. Target aggregate (Hand) uses the payload

The saga needs to call `QueryClient` to fetch Table state before emitting HandInitiated.

---

## 11. Wait List Boundary

### Spec Note

> "Wait list is currently modeled on Table aggregate (per-table list). In larger operations, wait lists may be cross-table... Flag as potential refactor."

### Angzarr Consideration

If wait lists become cross-table:
- New WaitList aggregate per game type/stakes
- WaitList PM coordinates with Tables
- PromoteFromWaitList becomes saga (WaitList → Table)

No framework limitation here, just domain modeling.

---

## Summary: Critical Divergences

| Area | Severity | Issue | Workaround |
|------|----------|-------|------------|
| Sync cascade | 🔴 High | Not supported | Use PM orchestration or CASCADE sync |
| Saga direct call | 🔴 High | Not supported | Async saga + PM |
| HandController PM | 🟡 Medium | Heavyweight | Consider embedded state machine |
| Fact semantics | 🟢 Low | Terminology | HandleEvent with external_id |
| Timer integration | 🟡 Medium | Not built-in | PM internal timers |
| Compound events | 🟢 Low | Fully supported | N/A |
| External facts | 🟢 Low | Fully supported | HandleEvent RPC |

---

## Next Steps

1. **Redesign buy-in/registration flows** as PM-orchestrated (not sync cascade)
2. **Decide on HandController architecture** (keep PM vs embed in aggregate)
3. **Implement timer service** for player action timeouts
4. **Define fact injection convention** (command suffix or type annotation)
5. **Prototype hand flow** with simplified PM → aggregate pattern (no bridging sagas)
