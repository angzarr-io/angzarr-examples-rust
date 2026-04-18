//! ProcessRebuy command handler.

use angzarr_client::proto::EventBook;
use angzarr_client::{event_page, pack_event, CommandRejectedError, CommandResult};
use examples_proto::{ProcessRebuy, RebuyDenied, RebuyProcessed};

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
    cmd: ProcessRebuy,
    state: &TournamentState,
    seq: u32,
) -> CommandResult<EventBook> {
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
            Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
        }
        Err(reason) => {
            let event = RebuyDenied {
                player_root: cmd.player_root,
                reservation_id: cmd.reservation_id,
                reason,
                denied_at: Some(angzarr_client::now()),
            };
            let event_any = pack_event(&event, "examples.RebuyDenied");
            Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{apply_created, apply_player_enrolled};
    use angzarr_client::proto::event_page::Payload;
    use examples_proto::{
        GameVariant, RebuyConfig, TournamentCreated, TournamentPlayerEnrolled, TournamentStatus,
    };
    use prost::Message;

    fn running_state_with_rebuy(config: Option<RebuyConfig>) -> TournamentState {
        let mut s = TournamentState::default();
        apply_created(
            &mut s,
            TournamentCreated {
                name: "X".into(),
                game_variant: GameVariant::TexasHoldem as i32,
                buy_in: 500,
                starting_stack: 10_000,
                max_players: 4,
                min_players: 2,
                scheduled_start: None,
                rebuy_config: config,
                addon_config: None,
                blind_structure: vec![],
                created_at: None,
            },
        );
        s.status = TournamentStatus::TournamentRunning;
        apply_player_enrolled(
            &mut s,
            TournamentPlayerEnrolled {
                player_root: vec![0xaa],
                reservation_id: vec![],
                fee_paid: 500,
                starting_stack: 10_000,
                registration_number: 1,
                enrolled_at: None,
            },
        );
        s
    }

    fn config() -> RebuyConfig {
        RebuyConfig {
            enabled: true,
            max_rebuys: 3,
            rebuy_level_cutoff: 10,
            stack_threshold: 5000,
            rebuy_cost: 100,
            rebuy_chips: 1000,
        }
    }

    #[test]
    fn handle_process_rebuy_success_emits_rebuy_processed() {
        let state = running_state_with_rebuy(Some(config()));
        let cmd = ProcessRebuy {
            player_root: vec![0xaa],
            reservation_id: vec![0x01],
        };
        let book = handle_process_rebuy(cmd, &state, 1).expect("ok");
        let any = match book.pages[0].payload.as_ref() {
            Some(Payload::Event(a)) => a,
            _ => panic!(),
        };
        assert!(any.type_url.ends_with("examples.RebuyProcessed"));
        let decoded = RebuyProcessed::decode(any.value.as_slice()).unwrap();
        assert_eq!(decoded.rebuy_cost, 100);
        assert_eq!(decoded.chips_added, 1000);
        assert_eq!(decoded.rebuy_count, 1);
    }

    #[test]
    fn handle_process_rebuy_rejects_when_no_tournament() {
        let state = TournamentState::default();
        let cmd = ProcessRebuy {
            player_root: vec![0xaa],
            reservation_id: vec![],
        };
        let err = handle_process_rebuy(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("does not exist"));
    }

    #[test]
    fn handle_process_rebuy_emits_denied_when_rebuys_not_enabled() {
        let state = running_state_with_rebuy(None);
        let cmd = ProcessRebuy {
            player_root: vec![0xaa],
            reservation_id: vec![0x01],
        };
        let book = handle_process_rebuy(cmd, &state, 1).expect("ok");
        let any = match book.pages[0].payload.as_ref() {
            Some(Payload::Event(a)) => a,
            _ => panic!(),
        };
        assert!(any.type_url.ends_with("examples.RebuyDenied"));
        let decoded = RebuyDenied::decode(any.value.as_slice()).unwrap();
        assert!(decoded.reason.contains("not enabled"));
    }

    #[test]
    fn handle_process_rebuy_emits_denied_for_unregistered_player() {
        let state = running_state_with_rebuy(Some(config()));
        let cmd = ProcessRebuy {
            player_root: vec![0xff],
            reservation_id: vec![],
        };
        let book = handle_process_rebuy(cmd, &state, 1).expect("ok");
        let any = match book.pages[0].payload.as_ref() {
            Some(Payload::Event(a)) => a,
            _ => panic!(),
        };
        assert!(any.type_url.ends_with("examples.RebuyDenied"));
        let decoded = RebuyDenied::decode(any.value.as_slice()).unwrap();
        assert!(decoded.reason.contains("not registered"));
    }
}
