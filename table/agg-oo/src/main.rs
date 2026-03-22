//! Table aggregate using OO-style annotations.
//!
//! This demonstrates the OO pattern where:
//! - State and handlers are encapsulated in a struct
//! - `#[aggregate(name, domain, state)]` decorates the impl block
//! - `#[handles(CommandType)]` marks command handler methods
//! - `#[applies(EventType)]` marks state applier methods

use angzarr_client::proto::{CommandBook, EventBook};
use angzarr_client::{aggregate, applies, handles, run_command_handler_server};
use examples_proto::{
    CreateTable, EndHand, JoinTable, LeaveTable, StartHand, TableCreated,
};
use prost::Message;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// Table state managed by the aggregate.
#[derive(Clone, Default)]
pub struct TableState {
    pub table_name: String,
    pub small_blind: u64,
    pub big_blind: u64,
    pub max_players: u32,
    pub player_count: u32,
    pub in_hand: bool,
}

/// Table aggregate using OO-style annotations.
pub struct TableAggregate;

// docs:start:oo_handlers
#[aggregate(name = "table", domain = "table", state = TableState)]
impl TableAggregate {
    #[handles(CreateTable)]
    fn handle_create(
        &self,
        _cmd: &CommandBook,
        create: &CreateTable,
        state: &TableState,
        seq: u32,
    ) -> Result<EventBook, String> {
        // Guard: table must not exist
        if !state.table_name.is_empty() {
            return Err("Table already exists".into());
        }

        // Validate: required fields
        if create.table_name.is_empty() {
            return Err("table_name is required".into());
        }
        if create.small_blind == 0 {
            return Err("small_blind must be positive".into());
        }

        // Compute: create the event
        Ok(EventBook::single_event(
            "table",
            "TableCreated",
            seq,
            TableCreated {
                table_name: create.table_name.clone(),
                small_blind: create.small_blind,
                big_blind: create.big_blind,
                max_players: create.max_players,
                ..Default::default()
            }
            .encode_to_vec(),
        ))
    }

    #[applies(TableCreated)]
    fn apply_created(state: &mut TableState, event: &TableCreated) {
        state.table_name = event.table_name.clone();
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
