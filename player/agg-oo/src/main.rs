//! Player Aggregate using OO-style proc macros.
//!
//! This example demonstrates the OO pattern using:
//! - `#[aggregate(domain = "...")]` on impl blocks
//! - `#[handles(CommandType)]` on handler methods
//! - `#[applies(EventType)]` on event applier methods
//! - `#[rejected(domain = "...", command = "...")]` on rejection handlers
//!
//! # Dependency Injection via Higher-Order Functions (HOF)
//!
//! This example demonstrates IoC/Dependency Injection using:
//! - **Trait-based abstractions** (`AuditLogger` trait)
//! - **`into_router_factory()`** with closures that capture dependencies
//! - **Higher-order functions** for flexible handler construction
//!
//! ## Why Factories and HOF Matter
//!
//! The factory pattern (`into_router_factory`) is **essential** for:
//!
//! 1. **Dependency Injection**: Closures capture external dependencies (database
//!    pools, message queues, loggers) and inject them into handlers.
//!
//! 2. **Testability**: Swap production services for mocks without changing code:
//!    ```rust,ignore
//!    // Production
//!    let router = Aggregate::into_router_factory(|| Aggregate::new(prod_db.clone()));
//!
//!    // Test
//!    let router = Aggregate::into_router_factory(|| Aggregate::new(mock_db.clone()));
//!    ```
//!
//! 3. **Per-Request State**: Each handler invocation gets a fresh instance,
//!    enabling request-scoped dependencies or stateless handlers.
//!
//! 4. **Inversion of Control**: The aggregate doesn't create its dependencies -
//!    they're injected from outside, following SOLID principles.
//!
//! ## Pattern Comparison
//!
//! | Pattern | Use Case |
//! |---------|----------|
//! | `into_router()` | Simple aggregates without dependencies |
//! | `into_router_factory()` | Aggregates with injected dependencies |
//!
//! Always prefer `into_router_factory()` when you have external dependencies
//! to ensure testability and maintainability.

use std::collections::HashMap;
use std::sync::Arc;

use angzarr_client::proto::{
    event_page, page_header, CommandBook, EventBook, EventPage, Notification, PageHeader,
    RejectionNotification,
};
#[allow(unused_imports)]
use angzarr_client::{aggregate, applies, handles, rejected};
use angzarr_client::{
    event_page as make_event_page, now, pack_event, run_command_handler_server,
    CommandRejectedError, CommandResult, RejectionHandlerResponse, UnpackAny,
};
use examples_proto::{
    Currency, DepositFunds, FundsDeposited, FundsReleased, FundsReserved, FundsWithdrawn,
    PlayerRegistered, PlayerType, RegisterPlayer, ReleaseFunds, ReserveFunds, WithdrawFunds,
};
use prost_types::Any;
use tracing::{info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

// =============================================================================
// State
// =============================================================================

/// Player aggregate state rebuilt from events.
#[derive(Debug, Default, Clone)]
pub struct PlayerState {
    pub player_id: String,
    pub display_name: String,
    pub email: String,
    pub player_type: PlayerType,
    pub ai_model_id: String,
    pub bankroll: i64,
    pub reserved_funds: i64,
    pub table_reservations: HashMap<String, i64>, // table_root_hex -> amount
    pub status: String,
}

impl PlayerState {
    pub fn exists(&self) -> bool {
        !self.player_id.is_empty()
    }

    pub fn available_balance(&self) -> i64 {
        self.bankroll - self.reserved_funds
    }
}

// =============================================================================
// Dependency Injection - Traits
// =============================================================================

/// Audit logging service trait - abstraction for IoC.
pub trait AuditLogger: Send + Sync {
    fn log_command(&self, command_type: &str, player_id: &str);
    fn log_event(&self, event_type: &str, player_id: &str);
}

/// Production audit logger implementation.
pub struct ProductionAuditLogger;

impl AuditLogger for ProductionAuditLogger {
    fn log_command(&self, command_type: &str, player_id: &str) {
        info!(
            command = command_type,
            player = player_id,
            "Audit: command received"
        );
    }

    fn log_event(&self, event_type: &str, player_id: &str) {
        info!(
            event = event_type,
            player = player_id,
            "Audit: event emitted"
        );
    }
}

/// Test audit logger for unit testing.
#[allow(dead_code)]
pub struct TestAuditLogger {
    pub commands: std::sync::Mutex<Vec<String>>,
    pub events: std::sync::Mutex<Vec<String>>,
}

impl Default for TestAuditLogger {
    fn default() -> Self {
        Self {
            commands: std::sync::Mutex::new(Vec::new()),
            events: std::sync::Mutex::new(Vec::new()),
        }
    }
}

#[allow(dead_code)]
impl TestAuditLogger {
    pub fn new() -> Self {
        Self::default()
    }
}

impl AuditLogger for TestAuditLogger {
    fn log_command(&self, command_type: &str, player_id: &str) {
        self.commands
            .lock()
            .unwrap()
            .push(format!("{}:{}", command_type, player_id));
    }

    fn log_event(&self, event_type: &str, player_id: &str) {
        self.events
            .lock()
            .unwrap()
            .push(format!("{}:{}", event_type, player_id));
    }
}

// =============================================================================
// Aggregate with Injected Dependencies
// =============================================================================

/// Player aggregate using OO-style annotations with IoC.
///
/// Dependencies are injected via constructor, enabling:
/// - Testability (inject mocks in tests)
/// - Flexibility (swap implementations at runtime)
/// - Separation of concerns
///
/// Uses trait object (`dyn AuditLogger`) for runtime polymorphism.
pub struct PlayerAggregate {
    audit_logger: Arc<dyn AuditLogger>,
}

impl PlayerAggregate {
    /// Create aggregate with injected dependencies.
    pub fn new(audit_logger: Arc<dyn AuditLogger>) -> Self {
        Self { audit_logger }
    }

    /// Default constructor for production use.
    pub fn production() -> Self {
        Self::new(Arc::new(ProductionAuditLogger))
    }
}

#[aggregate(domain = "player", state = PlayerState)]
impl PlayerAggregate {
    // =========================================================================
    // Event Appliers
    // =========================================================================

    #[applies(PlayerRegistered)]
    fn apply_registered(state: &mut PlayerState, event: PlayerRegistered) {
        state.player_id = format!("player_{}", event.email);
        state.display_name = event.display_name;
        state.email = event.email;
        state.player_type = PlayerType::try_from(event.player_type).unwrap_or_default();
        state.ai_model_id = event.ai_model_id;
        state.status = "active".to_string();
        state.bankroll = 0;
        state.reserved_funds = 0;
    }

    #[applies(FundsDeposited)]
    fn apply_deposited(state: &mut PlayerState, event: FundsDeposited) {
        if let Some(balance) = event.new_balance {
            state.bankroll = balance.amount;
        }
    }

    #[applies(FundsWithdrawn)]
    fn apply_withdrawn(state: &mut PlayerState, event: FundsWithdrawn) {
        if let Some(balance) = event.new_balance {
            state.bankroll = balance.amount;
        }
    }

    #[applies(FundsReserved)]
    fn apply_reserved(state: &mut PlayerState, event: FundsReserved) {
        if let Some(balance) = event.new_reserved_balance {
            state.reserved_funds = balance.amount;
        }
        if let (Some(amount), table_root) = (event.amount, event.table_root) {
            let table_key = hex::encode(&table_root);
            state.table_reservations.insert(table_key, amount.amount);
        }
    }

    #[applies(FundsReleased)]
    fn apply_released(state: &mut PlayerState, event: FundsReleased) {
        if let Some(balance) = event.new_reserved_balance {
            state.reserved_funds = balance.amount;
        }
        let table_key = hex::encode(&event.table_root);
        state.table_reservations.remove(&table_key);
    }

    // =========================================================================
    // Command Handlers
    // =========================================================================

    #[handles(RegisterPlayer)]
    pub fn register(
        &self,
        cb: &CommandBook,
        cmd: RegisterPlayer,
        state: &PlayerState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        // Use injected dependency - audit the command
        self.audit_logger.log_command("RegisterPlayer", &cmd.email);

        // Guard
        if state.exists() {
            return Err(CommandRejectedError::new("Player already exists"));
        }

        // Validate
        if cmd.display_name.is_empty() {
            return Err(CommandRejectedError::new("display_name is required"));
        }
        if cmd.email.is_empty() {
            return Err(CommandRejectedError::new("email is required"));
        }

        // Compute
        let event = PlayerRegistered {
            display_name: cmd.display_name,
            email: cmd.email,
            player_type: cmd.player_type,
            ai_model_id: cmd.ai_model_id,
            registered_at: Some(now()),
        };

        Ok(new_event_book(cb, seq, &event, "examples.PlayerRegistered"))
    }

    #[handles(DepositFunds)]
    pub fn deposit(
        &self,
        cb: &CommandBook,
        cmd: DepositFunds,
        state: &PlayerState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        // Guard
        if !state.exists() {
            return Err(CommandRejectedError::new("Player does not exist"));
        }

        // Validate
        let amount = cmd.amount.as_ref().map(|c| c.amount).unwrap_or(0);
        if amount <= 0 {
            return Err(CommandRejectedError::new("amount must be positive"));
        }

        // Compute
        let new_balance = state.bankroll + amount;
        let event = FundsDeposited {
            amount: cmd.amount,
            new_balance: Some(Currency {
                amount: new_balance,
                currency_code: "CHIPS".to_string(),
            }),
            deposited_at: Some(now()),
        };

        Ok(new_event_book(cb, seq, &event, "examples.FundsDeposited"))
    }

    #[handles(WithdrawFunds)]
    pub fn withdraw(
        &self,
        cb: &CommandBook,
        cmd: WithdrawFunds,
        state: &PlayerState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        // Guard
        if !state.exists() {
            return Err(CommandRejectedError::new("Player does not exist"));
        }

        // Validate
        let amount = cmd.amount.as_ref().map(|c| c.amount).unwrap_or(0);
        if amount <= 0 {
            return Err(CommandRejectedError::new("amount must be positive"));
        }
        if amount > state.available_balance() {
            return Err(CommandRejectedError::new("insufficient available balance"));
        }

        // Compute
        let new_balance = state.bankroll - amount;
        let event = FundsWithdrawn {
            amount: cmd.amount,
            new_balance: Some(Currency {
                amount: new_balance,
                currency_code: "CHIPS".to_string(),
            }),
            withdrawn_at: Some(now()),
        };

        Ok(new_event_book(cb, seq, &event, "examples.FundsWithdrawn"))
    }

    #[handles(ReserveFunds)]
    pub fn reserve(
        &self,
        cb: &CommandBook,
        cmd: ReserveFunds,
        state: &PlayerState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        // Guard
        if !state.exists() {
            return Err(CommandRejectedError::new("Player does not exist"));
        }

        // Validate
        let amount = cmd.amount.as_ref().map(|c| c.amount).unwrap_or(0);
        if amount <= 0 {
            return Err(CommandRejectedError::new("amount must be positive"));
        }
        if amount > state.available_balance() {
            return Err(CommandRejectedError::new("Insufficient funds"));
        }
        let table_key = hex::encode(&cmd.table_root);
        if state.table_reservations.contains_key(&table_key) {
            return Err(CommandRejectedError::new(
                "Funds already reserved for this table",
            ));
        }

        // Compute
        let new_reserved = state.reserved_funds + amount;
        let new_available = state.bankroll - new_reserved;
        let event = FundsReserved {
            amount: cmd.amount,
            table_root: cmd.table_root,
            new_available_balance: Some(Currency {
                amount: new_available,
                currency_code: "CHIPS".to_string(),
            }),
            new_reserved_balance: Some(Currency {
                amount: new_reserved,
                currency_code: "CHIPS".to_string(),
            }),
            reserved_at: Some(now()),
        };

        Ok(new_event_book(cb, seq, &event, "examples.FundsReserved"))
    }

    #[handles(ReleaseFunds)]
    pub fn release(
        &self,
        cb: &CommandBook,
        cmd: ReleaseFunds,
        state: &PlayerState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        // Guard
        if !state.exists() {
            return Err(CommandRejectedError::new("Player does not exist"));
        }

        // Validate
        if cmd.table_root.is_empty() {
            return Err(CommandRejectedError::new("table_root is required"));
        }
        let table_key = hex::encode(&cmd.table_root);
        let reserved = state
            .table_reservations
            .get(&table_key)
            .copied()
            .ok_or_else(|| CommandRejectedError::new("No funds reserved for this table"))?;

        // Compute
        let new_reserved = state.reserved_funds - reserved;
        let new_available = state.bankroll - new_reserved;
        let event = FundsReleased {
            amount: Some(Currency {
                amount: reserved,
                currency_code: "CHIPS".to_string(),
            }),
            table_root: cmd.table_root,
            new_available_balance: Some(Currency {
                amount: new_available,
                currency_code: "CHIPS".to_string(),
            }),
            new_reserved_balance: Some(Currency {
                amount: new_reserved,
                currency_code: "CHIPS".to_string(),
            }),
            released_at: Some(now()),
        };

        Ok(new_event_book(cb, seq, &event, "examples.FundsReleased"))
    }

    // =========================================================================
    // Rejection Handlers
    // =========================================================================

    #[rejected(domain = "table", command = "JoinTable")]
    pub fn handle_join_rejected(
        &self,
        notification: &Notification,
        state: &PlayerState,
    ) -> CommandResult<RejectionHandlerResponse> {
        // Extract rejection details
        let rejection = notification
            .payload
            .as_ref()
            .and_then(|any| any.unpack::<RejectionNotification>().ok())
            .unwrap_or_default();

        warn!(
            rejection_reason = %rejection.rejection_reason,
            "Player compensation for JoinTable rejection"
        );

        // Extract table_root from rejected command
        let table_root = rejection
            .rejected_command
            .as_ref()
            .and_then(|cmd| cmd.cover.as_ref())
            .map(|cover| {
                cover
                    .root
                    .as_ref()
                    .map(|r| r.value.clone())
                    .unwrap_or_default()
            })
            .unwrap_or_default();

        // Release reserved funds for this table
        let table_key = hex::encode(&table_root);
        let reserved_amount = state
            .table_reservations
            .get(&table_key)
            .copied()
            .unwrap_or(0);
        let new_reserved = state.reserved_funds - reserved_amount;
        let new_available = state.bankroll - new_reserved;

        let event = FundsReleased {
            amount: Some(Currency {
                amount: reserved_amount,
                currency_code: "CHIPS".to_string(),
            }),
            table_root,
            new_available_balance: Some(Currency {
                amount: new_available,
                currency_code: "CHIPS".to_string(),
            }),
            new_reserved_balance: Some(Currency {
                amount: new_reserved,
                currency_code: "CHIPS".to_string(),
            }),
            released_at: Some(now()),
        };

        let event_any = pack_event(&event, "examples.FundsReleased");
        let event_book = EventBook {
            cover: notification.cover.clone(),
            pages: vec![make_event_page(0, event_any)],
            snapshot: None,
            next_sequence: 0,
        };

        Ok(RejectionHandlerResponse {
            events: Some(event_book),
            notification: None,
        })
    }
}

// =============================================================================
// Helpers
// =============================================================================

fn new_event_book<M: prost::Message>(
    cb: &CommandBook,
    seq: u32,
    event: &M,
    type_name: &str,
) -> EventBook {
    let event_any = Any {
        type_url: format!("type.googleapis.com/{}", type_name),
        value: event.encode_to_vec(),
    };

    EventBook {
        cover: cb.cover.clone(),
        pages: vec![EventPage {
            header: Some(PageHeader {
                sequence_type: Some(page_header::SequenceType::Sequence(seq)),
            }),
            payload: Some(event_page::Payload::Event(event_any)),
            created_at: Some(now()),
        }],
        snapshot: None,
        next_sequence: 0,
    }
}

// =============================================================================
// Main
// =============================================================================

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    // ==========================================================================
    // Dependency Injection via Factory/Higher-Order Function
    // ==========================================================================
    //
    // IMPORTANT: This pattern is essential for production systems!
    //
    // The factory closure "closes over" external variables, capturing them
    // for injection into handler instances. This enables:
    //
    // 1. IoC (Inversion of Control) - dependencies come from outside
    // 2. Testability - swap real services for mocks
    // 3. Configuration - inject environment-specific dependencies
    // 4. Lifecycle management - share connections/pools across handlers

    // Create shared dependencies
    let audit_logger: Arc<dyn AuditLogger> = Arc::new(ProductionAuditLogger);

    // Factory closure captures dependencies and injects them.
    // The `move` keyword transfers ownership of cloned Arcs into the closure.
    // Each handler invocation calls the factory to get a fresh instance.
    let router = PlayerAggregate::into_router_factory({
        let logger = audit_logger.clone();
        move || PlayerAggregate::new(logger.clone())
    });

    // Note: You could also inject database pools, message queues, etc:
    // let db_pool = Arc::new(create_pool());
    // let router = Aggregate::into_router_factory({
    //     let db = db_pool.clone();
    //     let logger = audit_logger.clone();
    //     move || Aggregate::new(db.clone(), logger.clone())
    // });

    info!("Starting Player aggregate (OO pattern with IoC)");
    info!("Domain: {}", router.domain());

    run_command_handler_server("player", 50001, router)
        .await
        .expect("Server failed");
}
