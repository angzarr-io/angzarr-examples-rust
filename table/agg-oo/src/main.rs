//! Table aggregate using OO-style annotations.
//!
//! This demonstrates the OO pattern where:
//! - State and handlers are encapsulated in a struct
//! - `#[aggregate(domain, state)]` decorates the impl block
//! - `#[handles(CommandType)]` marks command handler methods
//! - `#[applies(EventType)]` marks state applier methods

use angzarr_client::proto::{CommandBook, EventBook};
use angzarr_client::{
    aggregate, new_event_book, pack_event, run_command_handler_server, CommandRejectedError,
    CommandResult,
};
use examples_proto::{CreateTable, TableCreated};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// Table state managed by the aggregate.
#[derive(Clone, Default)]
pub struct TableState {
    pub table_name: String,
    pub small_blind: i64,
    pub big_blind: i64,
    pub max_players: i32,
    pub player_count: i32,
    pub in_hand: bool,
}

/// Table aggregate using OO-style annotations.
pub struct TableAggregate;

// docs:start:oo_handlers
#[aggregate(domain = "table", state = TableState)]
impl TableAggregate {
    #[handles(CreateTable)]
    fn handle_create(
        &self,
        cmd_book: &CommandBook,
        create: CreateTable,
        state: &TableState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        // Guard: table must not exist
        if !state.table_name.is_empty() {
            return Err(CommandRejectedError::new("Table already exists"));
        }

        // Validate: required fields
        if create.table_name.is_empty() {
            return Err(CommandRejectedError::new("table_name is required"));
        }
        if create.small_blind <= 0 {
            return Err(CommandRejectedError::new("small_blind must be positive"));
        }

        // Compute: create the event
        let event = TableCreated {
            table_name: create.table_name.clone(),
            small_blind: create.small_blind,
            big_blind: create.big_blind,
            max_players: create.max_players,
            ..Default::default()
        };
        let event_any = pack_event(&event, "examples.TableCreated");

        Ok(new_event_book(cmd_book, seq, event_any))
    }

    #[applies(TableCreated)]
    fn apply_created(state: &mut TableState, event: TableCreated) {
        state.table_name = event.table_name;
        state.small_blind = event.small_blind;
        state.big_blind = event.big_blind;
        state.max_players = event.max_players;
    }
}
// docs:end:oo_handlers

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    println!("Starting Table aggregate (OO pattern)");

    let agg = TableAggregate;
    let router = agg.into_router();

    run_command_handler_server("table", 50002, router)
        .await
        .expect("Server failed");
}
