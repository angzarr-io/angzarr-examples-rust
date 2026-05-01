//! EndHand command handler.

use std::collections::HashMap;

use angzarr_client::proto::EventBook;
use angzarr_client::CommandResult;
use examples_proto::{EndHand, HandEnded};
use examples_utils::{event_page, pack_event, rejected};

use crate::state::TableState;

fn guard(state: &TableState) -> CommandResult<()> {
    if !state.exists() {
        return Err(rejected("Table does not exist"));
    }
    if state.status != "in_hand" {
        return Err(rejected("No hand in progress"));
    }
    Ok(())
}

fn validate(cmd: &EndHand, state: &TableState) -> CommandResult<()> {
    if hex::encode(&cmd.hand_root) != hex::encode(&state.current_hand_root) {
        return Err(rejected("Hand root mismatch"));
    }
    Ok(())
}

fn compute(cmd: &EndHand) -> HandEnded {
    let mut stack_changes: HashMap<String, i64> = HashMap::new();
    for result in &cmd.results {
        let winner_hex = hex::encode(&result.winner_root);
        *stack_changes.entry(winner_hex).or_insert(0) += result.amount;
    }

    HandEnded {
        hand_root: cmd.hand_root.clone(),
        results: cmd.results.clone(),
        stack_changes,
        ended_at: Some(angzarr_client::now()),
    }
}

pub fn handle_end_hand(cmd: EndHand, state: &TableState, seq: u32) -> CommandResult<EventBook> {
    guard(state)?;
    validate(&cmd, state)?;

    let event = compute(&cmd);
    let event_any = pack_event(&event, "examples.HandEnded");

    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}
