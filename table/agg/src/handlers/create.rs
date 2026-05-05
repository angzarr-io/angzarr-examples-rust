//! CreateTable command handler.

use angzarr_client::proto::EventBook;
use angzarr_client::CommandResult;
use examples_proto::{CreateTable, TableCreated};
use examples_utils::{event_page, pack_event, reject};

use crate::errors::{
    BigBlindMustExceedSmallBlind, MaxBuyInMustExceedMinBuyIn, MaxPlayersOutOfRange,
    MinBuyInMustBePositive, SmallBlindMustBePositive, TableAlreadyExists, TableNameRequired,
};
use crate::state::TableState;

fn guard(state: &TableState) -> CommandResult<()> {
    if state.exists() {
        return Err(reject(TableAlreadyExists));
    }
    Ok(())
}

fn validate(cmd: &CreateTable) -> CommandResult<()> {
    if cmd.table_name.is_empty() {
        return Err(reject(TableNameRequired));
    }
    if cmd.small_blind <= 0 {
        return Err(reject(SmallBlindMustBePositive {
            value: cmd.small_blind,
        }));
    }
    if cmd.big_blind <= 0 || cmd.big_blind < cmd.small_blind {
        return Err(reject(BigBlindMustExceedSmallBlind {
            lhs: cmd.big_blind,
            rhs: cmd.small_blind,
        }));
    }
    if cmd.min_buy_in <= 0 {
        return Err(reject(MinBuyInMustBePositive {
            value: cmd.min_buy_in,
        }));
    }
    if cmd.max_buy_in < cmd.min_buy_in {
        return Err(reject(MaxBuyInMustExceedMinBuyIn {
            lhs: cmd.max_buy_in,
            rhs: cmd.min_buy_in,
        }));
    }
    if cmd.max_players < 2 || cmd.max_players > 10 {
        return Err(reject(MaxPlayersOutOfRange {
            got: cmd.max_players,
        }));
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
