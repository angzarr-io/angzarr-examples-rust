//! RegisterPlayer command handler.

use angzarr_client::proto::EventBook;
use angzarr_client::{event_page, pack_event, CommandRejectedError, CommandResult};
use examples_proto::{PlayerRegistered, RegisterPlayer};

use crate::state::PlayerState;

fn register_player_guard(state: &PlayerState) -> CommandResult<()> {
    if state.exists() {
        return Err(CommandRejectedError::new("Player already exists"));
    }
    Ok(())
}

fn register_player_validate(cmd: &RegisterPlayer) -> CommandResult<()> {
    if cmd.display_name.is_empty() {
        return Err(CommandRejectedError::new("display_name is required"));
    }
    if cmd.email.is_empty() {
        return Err(CommandRejectedError::new("email is required"));
    }
    Ok(())
}

fn register_player_compute(cmd: &RegisterPlayer) -> PlayerRegistered {
    PlayerRegistered {
        display_name: cmd.display_name.clone(),
        email: cmd.email.clone(),
        player_type: cmd.player_type,
        ai_model_id: cmd.ai_model_id.clone(),
        registered_at: Some(angzarr_client::now()),
    }
}

pub fn handle_register_player(
    cmd: RegisterPlayer,
    state: &PlayerState,
    seq: u32,
) -> CommandResult<EventBook> {
    register_player_guard(state)?;
    register_player_validate(&cmd)?;

    let event = register_player_compute(&cmd);
    let event_any = pack_event(&event, "examples.PlayerRegistered");

    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}
