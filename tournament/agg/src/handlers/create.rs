//! CreateTournament command handler.

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
        return Err(CommandRejectedError::invalid_argument(
            "buy_in must be positive",
        ));
    }
    if cmd.starting_stack <= 0 {
        return Err(CommandRejectedError::invalid_argument(
            "starting_stack must be positive",
        ));
    }
    if cmd.max_players < 2 {
        return Err(CommandRejectedError::invalid_argument(
            "max_players must be at least 2",
        ));
    }
    if cmd.min_players < 2 {
        return Err(CommandRejectedError::invalid_argument(
            "min_players must be at least 2",
        ));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::apply_created;
    use angzarr_client::proto::event_page::Payload;
    use examples_proto::GameVariant;
    use prost::Message;

    fn base_cmd() -> CreateTournament {
        CreateTournament {
            name: "Spring Classic".into(),
            game_variant: GameVariant::TexasHoldem as i32,
            buy_in: 500,
            starting_stack: 10_000,
            max_players: 9,
            min_players: 2,
            scheduled_start: None,
            rebuy_config: None,
            addon_config: None,
            blind_structure: vec![],
        }
    }

    #[test]
    fn handle_create_tournament_success_emits_tournament_created() {
        let state = TournamentState::default();
        let book = handle_create_tournament(base_cmd(), &state, 1).expect("ok");
        let any = match book.pages[0].payload.as_ref() {
            Some(Payload::Event(a)) => a,
            _ => panic!(),
        };
        assert!(any.type_url.ends_with("examples.TournamentCreated"));
        let decoded = TournamentCreated::decode(any.value.as_slice()).unwrap();
        assert_eq!(decoded.name, "Spring Classic");
        assert_eq!(decoded.buy_in, 500);
    }

    #[test]
    fn handle_create_tournament_rejects_duplicate() {
        let mut state = TournamentState::default();
        apply_created(
            &mut state,
            TournamentCreated {
                name: "Existing".into(),
                game_variant: GameVariant::TexasHoldem as i32,
                buy_in: 100,
                starting_stack: 1000,
                max_players: 2,
                min_players: 2,
                scheduled_start: None,
                rebuy_config: None,
                addon_config: None,
                blind_structure: vec![],
                created_at: None,
            },
        );
        let err = handle_create_tournament(base_cmd(), &state, 1).unwrap_err();
        assert!(err.reason.contains("already exists"));
    }

    #[test]
    fn handle_create_tournament_rejects_empty_name() {
        let state = TournamentState::default();
        let mut cmd = base_cmd();
        cmd.name = String::new();
        let err = handle_create_tournament(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("name"));
    }

    #[test]
    fn handle_create_tournament_rejects_invalid_buy_in() {
        let state = TournamentState::default();
        let mut cmd = base_cmd();
        cmd.buy_in = 0;
        let err = handle_create_tournament(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("buy_in"));
    }

    #[test]
    fn handle_create_tournament_rejects_min_greater_than_max() {
        let state = TournamentState::default();
        let mut cmd = base_cmd();
        cmd.min_players = 10;
        cmd.max_players = 5;
        let err = handle_create_tournament(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("min_players"));
    }
}
