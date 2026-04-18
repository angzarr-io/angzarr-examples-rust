//! EndHand command handler.

use std::collections::HashMap;

use angzarr_client::proto::EventBook;
use angzarr_client::{event_page, pack_event, CommandRejectedError, CommandResult};
use examples_proto::{EndHand, HandEnded};

use crate::state::TableState;

fn guard(state: &TableState) -> CommandResult<()> {
    if !state.exists() {
        return Err(CommandRejectedError::new("Table does not exist"));
    }
    if state.status != "in_hand" {
        return Err(CommandRejectedError::new("No hand in progress"));
    }
    Ok(())
}

fn validate(cmd: &EndHand, state: &TableState) -> CommandResult<()> {
    if hex::encode(&cmd.hand_root) != hex::encode(&state.current_hand_root) {
        return Err(CommandRejectedError::new("Hand root mismatch"));
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

pub fn handle_end_hand(
    cmd: EndHand,
    state: &TableState,
    seq: u32,
) -> CommandResult<EventBook> {
    guard(state)?;
    validate(&cmd, state)?;

    let event = compute(&cmd);
    let event_any = pack_event(&event, "examples.HandEnded");

    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{apply_hand_started, apply_player_joined, apply_table_created};
    use angzarr_client::proto::event_page::Payload;
    use examples_proto::{GameVariant, HandStarted, PlayerJoined, PotResult, TableCreated};
    use prost::Message;

    fn in_hand_state(hand_root: Vec<u8>) -> TableState {
        let mut s = TableState::default();
        apply_table_created(
            &mut s,
            TableCreated {
                table_name: "T".into(),
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
        apply_player_joined(
            &mut s,
            PlayerJoined {
                player_root: vec![0xa1],
                seat_position: 0,
                buy_in_amount: 50,
                stack: 50,
                joined_at: None,
            },
        );
        apply_hand_started(
            &mut s,
            HandStarted {
                hand_root: hand_root.clone(),
                hand_number: 1,
                dealer_position: 0,
                small_blind_position: 0,
                big_blind_position: 0,
                active_players: vec![],
                game_variant: GameVariant::TexasHoldem as i32,
                small_blind: 1,
                big_blind: 2,
                started_at: None,
            },
        );
        s
    }

    #[test]
    fn handle_end_hand_success_emits_hand_ended_with_stack_changes() {
        let hand_root = vec![0xbe, 0xef];
        let state = in_hand_state(hand_root.clone());
        let cmd = EndHand {
            hand_root: hand_root.clone(),
            results: vec![PotResult {
                winner_root: vec![0xa1],
                amount: 30,
                pot_type: "main".into(),
                winning_hand: None,
            }],
        };
        let book = handle_end_hand(cmd, &state, 1).expect("ok");
        let any = match book.pages[0].payload.as_ref() {
            Some(Payload::Event(a)) => a,
            _ => panic!(),
        };
        assert!(any.type_url.ends_with("examples.HandEnded"));
        let decoded = HandEnded::decode(any.value.as_slice()).unwrap();
        assert_eq!(decoded.hand_root, hand_root);
        assert_eq!(decoded.stack_changes.get(&hex::encode(&[0xa1u8])), Some(&30));
    }

    #[test]
    fn handle_end_hand_rejects_when_no_table() {
        let state = TableState::default();
        let cmd = EndHand {
            hand_root: vec![1],
            results: vec![],
        };
        let err = handle_end_hand(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("does not exist"));
    }

    #[test]
    fn handle_end_hand_rejects_when_not_in_hand() {
        let mut s = TableState::default();
        apply_table_created(
            &mut s,
            TableCreated {
                table_name: "X".into(),
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
        let cmd = EndHand {
            hand_root: vec![1],
            results: vec![],
        };
        let err = handle_end_hand(cmd, &s, 1).unwrap_err();
        assert!(err.reason.contains("No hand in progress"));
    }

    #[test]
    fn handle_end_hand_rejects_on_hand_root_mismatch() {
        let state = in_hand_state(vec![1, 2, 3]);
        let cmd = EndHand {
            hand_root: vec![9, 9, 9],
            results: vec![],
        };
        let err = handle_end_hand(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("mismatch"));
    }
}
