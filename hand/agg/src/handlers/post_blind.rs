//! PostBlind command handler.

use angzarr_client::proto::EventBook;
use angzarr_client::CommandResult;
use examples_utils::{event_page, invalid_arg, pack_event, rejected};
use examples_proto::{BlindPosted, PostBlind};

use crate::state::{HandState, PlayerHandState};

fn guard(state: &HandState) -> CommandResult<()> {
    if !state.exists() {
        return Err(rejected("Hand not dealt"));
    }
    if state.is_complete() {
        return Err(rejected("Hand already complete"));
    }
    Ok(())
}

fn validate<'a>(cmd: &PostBlind, state: &'a HandState) -> CommandResult<&'a PlayerHandState> {
    if cmd.player_root.is_empty() {
        return Err(rejected("player_root is required"));
    }

    let player = state
        .get_player(&cmd.player_root)
        .ok_or_else(|| rejected("Player not in hand"))?;

    if player.has_folded {
        return Err(rejected("Player has folded"));
    }

    if cmd.amount <= 0 {
        return Err(invalid_arg("Amount must be positive"));
    }

    Ok(player)
}

fn compute(cmd: &PostBlind, state: &HandState, player: &PlayerHandState) -> BlindPosted {
    let actual_amount = cmd.amount.min(player.stack);
    let new_stack = player.stack - actual_amount;
    let new_pot = state.total_pot() + actual_amount;

    BlindPosted {
        player_root: cmd.player_root.clone(),
        blind_type: cmd.blind_type.clone(),
        amount: actual_amount,
        player_stack: new_stack,
        pot_total: new_pot,
        posted_at: Some(angzarr_client::now()),
    }
}

pub fn handle_post_blind(cmd: PostBlind, state: &HandState, seq: u32) -> CommandResult<EventBook> {
    guard(state)?;
    let player = validate(&cmd, state)?;

    let event = compute(&cmd, state, player);
    let event_any = pack_event(&event, "examples.BlindPosted");

    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}
