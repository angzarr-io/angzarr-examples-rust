//! ReleaseFunds command handler.

use angzarr_client::proto::EventBook;
use angzarr_client::{event_page, pack_event, CommandRejectedError, CommandResult};
use examples_proto::{Currency, FundsReleased, ReleaseFunds};

use crate::state::PlayerState;

fn release_funds_guard(state: &PlayerState) -> CommandResult<()> {
    if !state.exists() {
        return Err(CommandRejectedError::new("Player does not exist"));
    }
    Ok(())
}

fn release_funds_validate(cmd: &ReleaseFunds, state: &PlayerState) -> CommandResult<i64> {
    if cmd.table_root.is_empty() {
        return Err(CommandRejectedError::new("table_root is required"));
    }

    let table_key = hex::encode(&cmd.table_root);
    match state.table_reservations.get(&table_key) {
        Some(&amount) => Ok(amount),
        None => Err(CommandRejectedError::new(
            "No funds reserved for this table",
        )),
    }
}

fn release_funds_compute(cmd: &ReleaseFunds, state: &PlayerState, reserved: i64) -> FundsReleased {
    let new_reserved = state.reserved_funds - reserved;
    let new_available = state.bankroll - new_reserved;

    FundsReleased {
        amount: Some(Currency {
            amount: reserved,
            currency_code: "CHIPS".to_string(),
        }),
        table_root: cmd.table_root.clone(),
        new_available_balance: Some(Currency {
            amount: new_available,
            currency_code: "CHIPS".to_string(),
        }),
        new_reserved_balance: Some(Currency {
            amount: new_reserved,
            currency_code: "CHIPS".to_string(),
        }),
        released_at: Some(angzarr_client::now()),
    }
}

pub fn handle_release_funds(
    cmd: ReleaseFunds,
    state: &PlayerState,
    seq: u32,
) -> CommandResult<EventBook> {
    release_funds_guard(state)?;
    let reserved = release_funds_validate(&cmd, state)?;

    let event = release_funds_compute(&cmd, state, reserved);
    let event_any = pack_event(&event, "examples.FundsReleased");

    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{apply_registered, apply_reserved};
    use angzarr_client::proto::event_page::Payload;
    use examples_proto::{FundsReserved, PlayerRegistered, PlayerType};
    use prost::Message;

    fn currency(amount: i64) -> Currency {
        Currency {
            amount,
            currency_code: "CHIPS".into(),
        }
    }

    fn registered_with_reservation(table_root: &[u8], amount: i64) -> PlayerState {
        let mut state = PlayerState::default();
        apply_registered(
            &mut state,
            PlayerRegistered {
                display_name: "Alice".into(),
                email: "alice@example.com".into(),
                player_type: PlayerType::Human as i32,
                ai_model_id: String::new(),
                registered_at: None,
            },
        );
        state.bankroll = 1000;
        apply_reserved(
            &mut state,
            FundsReserved {
                amount: Some(currency(amount)),
                table_root: table_root.to_vec(),
                new_available_balance: Some(currency(1000 - amount)),
                new_reserved_balance: Some(currency(amount)),
                reserved_at: None,
            },
        );
        state
    }

    #[test]
    fn handle_release_funds_success_emits_funds_released() {
        let table_root = vec![0xab, 0xcd];
        let state = registered_with_reservation(&table_root, 300);
        let cmd = ReleaseFunds {
            table_root: table_root.clone(),
        };
        let book = handle_release_funds(cmd, &state, 9).expect("ok");
        assert_eq!(book.pages.len(), 1);
        let any = match book.pages[0].payload.as_ref() {
            Some(Payload::Event(any)) => any,
            _ => panic!("expected event payload"),
        };
        assert!(any.type_url.ends_with("examples.FundsReleased"));
        let decoded = FundsReleased::decode(any.value.as_slice()).expect("decode");
        assert_eq!(decoded.table_root, table_root);
        assert_eq!(decoded.amount.unwrap().amount, 300);
        assert_eq!(decoded.new_reserved_balance.unwrap().amount, 0);
    }

    #[test]
    fn handle_release_funds_rejects_when_no_player() {
        let state = PlayerState::default();
        let cmd = ReleaseFunds {
            table_root: vec![1],
        };
        let err = handle_release_funds(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("does not exist"));
    }

    #[test]
    fn handle_release_funds_rejects_empty_table_root() {
        let state = registered_with_reservation(&[1, 2, 3], 100);
        let cmd = ReleaseFunds {
            table_root: vec![],
        };
        let err = handle_release_funds(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("table_root"));
    }

    #[test]
    fn handle_release_funds_rejects_when_no_reservation_for_table() {
        let state = registered_with_reservation(&[1, 2, 3], 100);
        let cmd = ReleaseFunds {
            table_root: vec![9, 9, 9],
        };
        let err = handle_release_funds(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("No funds reserved"));
    }
}
