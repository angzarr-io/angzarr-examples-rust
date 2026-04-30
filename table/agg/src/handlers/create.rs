//! CreateTable command handler.

use angzarr_client::proto::EventBook;
use angzarr_client::CommandResult;
use examples_utils::{event_page, invalid_arg, pack_event, rejected};
use examples_proto::{CreateTable, TableCreated};

use crate::state::TableState;

fn guard(state: &TableState) -> CommandResult<()> {
    if state.exists() {
        return Err(rejected("Table already exists"));
    }
    Ok(())
}

fn validate(cmd: &CreateTable) -> CommandResult<()> {
    if cmd.table_name.is_empty() {
        return Err(rejected("table_name is required"));
    }
    if cmd.small_blind <= 0 {
        return Err(invalid_arg("small_blind must be positive"));
    }
    if cmd.big_blind <= 0 || cmd.big_blind < cmd.small_blind {
        return Err(rejected("big_blind must be >= small_blind"));
    }
    if cmd.min_buy_in <= 0 {
        return Err(invalid_arg("min_buy_in must be positive"));
    }
    if cmd.max_buy_in < cmd.min_buy_in {
        return Err(rejected("max_buy_in must be >= min_buy_in"));
    }
    if cmd.max_players < 2 || cmd.max_players > 10 {
        return Err(rejected("max_players must be 2-10"));
    }
    Ok(())
}

fn compute(cmd: &CreateTable) -> TableCreated {
    TableCreated {
        table_name: cmd.table_name.clone(),
        game_variant: cmd.game_variant,
        small_blind: cmd.small_blind,
        big_blind: cmd.big_blind,
        min_buy_in: cmd.min_buy_in,
        max_buy_in: cmd.max_buy_in,
        max_players: cmd.max_players,
        action_timeout_seconds: cmd.action_timeout_seconds,
        created_at: Some(angzarr_client::now()),
    }
}

pub fn handle_create_table(
    cmd: CreateTable,
    state: &TableState,
    seq: u32,
) -> CommandResult<EventBook> {
    guard(state)?;
    validate(&cmd)?;

    let event = compute(&cmd);
    let event_any = pack_event(&event, "examples.TableCreated");

    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}
