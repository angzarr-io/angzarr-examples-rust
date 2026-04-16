//! TransferFunds command handler.

use angzarr_client::proto::{CommandBook, EventBook};
use angzarr_client::{new_event_book, pack_event, CommandRejectedError, CommandResult, UnpackAny};
use examples_proto::{Currency, FundsTransferred, TransferFunds};
use prost_types::Any;

use crate::state::PlayerState;

fn transfer_funds_guard(state: &PlayerState) -> CommandResult<()> {
    if !state.exists() {
        return Err(CommandRejectedError::new("Player does not exist"));
    }
    Ok(())
}

fn transfer_funds_validate(cmd: &TransferFunds) -> CommandResult<i64> {
    let amount = cmd.amount.as_ref().map(|c| c.amount).unwrap_or(0);
    if amount == 0 {
        return Err(CommandRejectedError::invalid_argument(
            "amount must be non-zero",
        ));
    }
    Ok(amount)
}

fn transfer_funds_compute(
    cmd: &TransferFunds,
    state: &PlayerState,
    amount: i64,
) -> FundsTransferred {
    let new_balance = state.bankroll + amount;
    FundsTransferred {
        from_player_root: cmd.from_player_root.clone(),
        to_player_root: state.player_id.as_bytes().to_vec(),
        amount: cmd.amount.clone(),
        hand_root: cmd.hand_root.clone(),
        reason: cmd.reason.clone(),
        new_balance: Some(Currency {
            amount: new_balance,
            currency_code: "CHIPS".to_string(),
        }),
        transferred_at: Some(angzarr_client::now()),
    }
}

pub fn handle_transfer_funds(
    command_book: &CommandBook,
    command_any: &Any,
    state: &PlayerState,
    seq: u32,
) -> CommandResult<EventBook> {
    let cmd: TransferFunds = command_any
        .unpack()
        .map_err(|e| CommandRejectedError::new(format!("Failed to decode command: {}", e)))?;

    transfer_funds_guard(state)?;
    let amount = transfer_funds_validate(&cmd)?;

    let event = transfer_funds_compute(&cmd, state, amount);
    let event_any = pack_event(&event, "examples.FundsTransferred");

    Ok(new_event_book(command_book, seq, event_any))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transfer_increases_bankroll() {
        let state = PlayerState {
            player_id: "player_1".to_string(),
            bankroll: 1000,
            ..Default::default()
        };
        let cmd = TransferFunds {
            from_player_root: b"other_player".to_vec(),
            amount: Some(Currency {
                amount: 500,
                currency_code: "CHIPS".to_string(),
            }),
            hand_root: b"hand_1".to_vec(),
            reason: "pot_win".to_string(),
        };

        let event = transfer_funds_compute(&cmd, &state, 500);

        assert_eq!(event.new_balance.unwrap().amount, 1500);
        assert_eq!(event.reason, "pot_win");
        assert_eq!(event.from_player_root, b"other_player");
    }

    #[test]
    fn test_transfer_rejects_non_existent_player() {
        let state = PlayerState::default();

        let result = transfer_funds_guard(&state);

        assert!(result.is_err());
        assert!(result.unwrap_err().reason.contains("does not exist"));
    }

    #[test]
    fn test_transfer_rejects_zero_amount() {
        let cmd = TransferFunds {
            amount: Some(Currency {
                amount: 0,
                currency_code: "CHIPS".to_string(),
            }),
            ..Default::default()
        };

        let result = transfer_funds_validate(&cmd);

        assert!(result.is_err());
        assert!(result.unwrap_err().reason.contains("non-zero"));
    }
}
