//! ProcessRebuy command handler.

use angzarr_client::proto::{CommandBook, EventBook};
use angzarr_client::{new_event_book, pack_event, CommandRejectedError, CommandResult, UnpackAny};
use examples_proto::{ProcessRebuy, RebuyDenied, RebuyProcessed};
use prost_types::Any;

use crate::state::TournamentState;

fn guard(state: &TournamentState) -> CommandResult<()> {
    if !state.exists() {
        return Err(CommandRejectedError::new("Tournament does not exist"));
    }
    Ok(())
}

fn validate(cmd: &ProcessRebuy, state: &TournamentState) -> Result<(), String> {
    if cmd.player_root.is_empty() {
        return Err("player_root is required".to_string());
    }

    let player_root_hex = hex::encode(&cmd.player_root);

    if !state.is_player_registered(&player_root_hex) {
        return Err("Player is not registered in this tournament".to_string());
    }

    if !state.can_rebuy(&player_root_hex) {
        // Determine the specific reason
        if !state.is_running() {
            return Err("Tournament is not running".to_string());
        }

        let rebuy_config = state.rebuy_config.as_ref();
        if rebuy_config.is_none() || !rebuy_config.unwrap().enabled {
            return Err("Rebuys are not enabled for this tournament".to_string());
        }

        let config = rebuy_config.unwrap();
        if config.rebuy_level_cutoff > 0 && state.current_level > config.rebuy_level_cutoff {
            return Err("Rebuy window has closed".to_string());
        }

        if let Some(registration) = state.registered_players.get(&player_root_hex) {
            if config.max_rebuys > 0 && registration.rebuys_used >= config.max_rebuys {
                return Err("Maximum rebuys reached".to_string());
            }
        }

        return Err("Rebuy not allowed".to_string());
    }

    Ok(())
}

pub fn handle_process_rebuy(
    command_book: &CommandBook,
    command_any: &Any,
    state: &TournamentState,
    seq: u32,
) -> CommandResult<EventBook> {
    let cmd: ProcessRebuy = command_any
        .unpack()
        .map_err(|e| CommandRejectedError::new(format!("Failed to decode command: {}", e)))?;

    guard(state)?;

    let player_root_hex = hex::encode(&cmd.player_root);

    match validate(&cmd, state) {
        Ok(()) => {
            let rebuy_config = state.rebuy_config.as_ref().unwrap();
            let current_rebuys = state
                .registered_players
                .get(&player_root_hex)
                .map(|r| r.rebuys_used)
                .unwrap_or(0);

            let event = RebuyProcessed {
                player_root: cmd.player_root,
                reservation_id: cmd.reservation_id,
                rebuy_cost: rebuy_config.rebuy_cost,
                chips_added: rebuy_config.rebuy_chips,
                rebuy_count: current_rebuys + 1,
                processed_at: Some(angzarr_client::now()),
            };
            let event_any = pack_event(&event, "examples.RebuyProcessed");
            Ok(new_event_book(command_book, seq, event_any))
        }
        Err(reason) => {
            let event = RebuyDenied {
                player_root: cmd.player_root,
                reservation_id: cmd.reservation_id,
                reason,
                denied_at: Some(angzarr_client::now()),
            };
            let event_any = pack_event(&event, "examples.RebuyDenied");
            Ok(new_event_book(command_book, seq, event_any))
        }
    }
}
