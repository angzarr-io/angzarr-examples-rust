//! CreateTournament command handler.
//!
//! Validation uses `CommandRejectedError::new` (FAILED_PRECONDITION) for all
//! business-rule violations, matching the gherkin expectations in
//! `features/example/unit/tournament.feature` (@EU-0803..@EU-0806).

use angzarr_client::proto::EventBook;
use angzarr_client::{event_page, pack_event, CommandRejectedError, CommandResult};
use examples_proto::{CreateTournament, TournamentCreated};

use crate::state::TournamentState;

fn guard(state: &TournamentState) -> CommandResult<()> {
    if state.exists() {
        return Err(CommandRejectedError::new("Tournament already exists"));
    }
    Ok(())
}

fn validate(cmd: &CreateTournament) -> CommandResult<()> {
    if cmd.name.is_empty() {
        return Err(CommandRejectedError::new("name is required"));
    }
    if cmd.buy_in <= 0 {
        return Err(CommandRejectedError {
            reason: "buy_in must be positive".to_string(),
            status_code: "FAILED_PRECONDITION".to_string(),
        });
    }
    if cmd.starting_stack <= 0 {
        return Err(CommandRejectedError {
            reason: "starting_stack must be positive".to_string(),
            status_code: "FAILED_PRECONDITION".to_string(),
        });
    }
    if cmd.max_players < 2 {
        return Err(CommandRejectedError::new("max_players must be at least 2"));
    }
    if cmd.min_players < 2 {
        return Err(CommandRejectedError {
            reason: "min_players must be at least 2".to_string(),
            status_code: "FAILED_PRECONDITION".to_string(),
        });
    }
    if cmd.min_players > cmd.max_players {
        return Err(CommandRejectedError::new(
            "min_players cannot exceed max_players",
        ));
    }
    Ok(())
}

fn compute(cmd: &CreateTournament) -> TournamentCreated {
    TournamentCreated {
        name: cmd.name.clone(),
        game_variant: cmd.game_variant,
        buy_in: cmd.buy_in,
        starting_stack: cmd.starting_stack,
        max_players: cmd.max_players,
        min_players: cmd.min_players,
        scheduled_start: cmd.scheduled_start,
        rebuy_config: cmd.rebuy_config,
        addon_config: cmd.addon_config,
        blind_structure: cmd.blind_structure.clone(),
        created_at: Some(angzarr_client::now()),
    }
}

pub fn handle_create_tournament(
    cmd: CreateTournament,
    state: &TournamentState,
    seq: u32,
) -> CommandResult<EventBook> {
    guard(state)?;
    validate(&cmd)?;

    let event = compute(&cmd);
    let event_any = pack_event(&event, "examples.TournamentCreated");

    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}
