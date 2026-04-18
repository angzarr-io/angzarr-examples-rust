//! CreateTable command handler.

use angzarr_client::proto::EventBook;
use angzarr_client::{event_page, pack_event, CommandRejectedError, CommandResult};
use examples_proto::{CreateTable, TableCreated};

use crate::state::TableState;

fn guard(state: &TableState) -> CommandResult<()> {
    if state.exists() {
        return Err(CommandRejectedError::new("Table already exists"));
    }
    Ok(())
}

fn validate(cmd: &CreateTable) -> CommandResult<()> {
    if cmd.table_name.is_empty() {
        return Err(CommandRejectedError::new("table_name is required"));
    }
    if cmd.small_blind <= 0 {
        return Err(CommandRejectedError::invalid_argument(
            "small_blind must be positive",
        ));
    }
    if cmd.big_blind <= 0 || cmd.big_blind < cmd.small_blind {
        return Err(CommandRejectedError::new(
            "big_blind must be >= small_blind",
        ));
    }
    if cmd.min_buy_in <= 0 {
        return Err(CommandRejectedError::invalid_argument(
            "min_buy_in must be positive",
        ));
    }
    if cmd.max_buy_in < cmd.min_buy_in {
        return Err(CommandRejectedError::new(
            "max_buy_in must be >= min_buy_in",
        ));
    }
    if cmd.max_players < 2 || cmd.max_players > 10 {
        return Err(CommandRejectedError::new("max_players must be 2-10"));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::apply_table_created;
    use angzarr_client::proto::event_page::Payload;
    use examples_proto::GameVariant;
    use prost::Message;

    fn base_cmd() -> CreateTable {
        CreateTable {
            table_name: "HighStakes".into(),
            game_variant: GameVariant::TexasHoldem as i32,
            small_blind: 5,
            big_blind: 10,
            min_buy_in: 100,
            max_buy_in: 1000,
            max_players: 6,
            action_timeout_seconds: 30,
        }
    }

    #[test]
    fn handle_create_table_success_emits_table_created() {
        let state = TableState::default();
        let book = handle_create_table(base_cmd(), &state, 1).expect("ok");
        assert_eq!(book.pages.len(), 1);
        let any = match book.pages[0].payload.as_ref() {
            Some(Payload::Event(a)) => a,
            _ => panic!(),
        };
        assert!(any.type_url.ends_with("examples.TableCreated"));
        let decoded = TableCreated::decode(any.value.as_slice()).unwrap();
        assert_eq!(decoded.table_name, "HighStakes");
        assert_eq!(decoded.max_players, 6);
    }

    #[test]
    fn handle_create_table_rejects_duplicate_table() {
        let mut state = TableState::default();
        apply_table_created(
            &mut state,
            TableCreated {
                table_name: "Existing".into(),
                game_variant: GameVariant::TexasHoldem as i32,
                small_blind: 1,
                big_blind: 2,
                min_buy_in: 10,
                max_buy_in: 100,
                max_players: 4,
                action_timeout_seconds: 30,
                created_at: None,
            },
        );
        let err = handle_create_table(base_cmd(), &state, 1).unwrap_err();
        assert!(err.reason.contains("already exists"));
    }

    #[test]
    fn handle_create_table_rejects_empty_name() {
        let state = TableState::default();
        let mut cmd = base_cmd();
        cmd.table_name = String::new();
        let err = handle_create_table(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("table_name"));
    }

    #[test]
    fn handle_create_table_rejects_non_positive_small_blind() {
        let state = TableState::default();
        let mut cmd = base_cmd();
        cmd.small_blind = 0;
        let err = handle_create_table(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("small_blind"));
    }

    #[test]
    fn handle_create_table_rejects_big_blind_less_than_small() {
        let state = TableState::default();
        let mut cmd = base_cmd();
        cmd.big_blind = 2;
        cmd.small_blind = 5;
        let err = handle_create_table(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("big_blind"));
    }

    #[test]
    fn handle_create_table_rejects_invalid_min_buy_in() {
        let state = TableState::default();
        let mut cmd = base_cmd();
        cmd.min_buy_in = 0;
        let err = handle_create_table(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("min_buy_in"));
    }

    #[test]
    fn handle_create_table_rejects_max_buy_in_below_min() {
        let state = TableState::default();
        let mut cmd = base_cmd();
        cmd.min_buy_in = 500;
        cmd.max_buy_in = 100;
        let err = handle_create_table(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("max_buy_in"));
    }

    #[test]
    fn handle_create_table_rejects_invalid_max_players() {
        let state = TableState::default();
        let mut cmd = base_cmd();
        cmd.max_players = 1;
        let err = handle_create_table(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("max_players"));

        let mut cmd = base_cmd();
        cmd.max_players = 11;
        let err = handle_create_table(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("max_players"));
    }
}
