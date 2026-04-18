//! StartHand command handler.

use angzarr_client::proto::EventBook;
use angzarr_client::{event_page, pack_event, CommandRejectedError, CommandResult};
use examples_proto::{HandStarted, SeatSnapshot, StartHand};

use crate::state::TableState;

fn guard(state: &TableState) -> CommandResult<()> {
    if !state.exists() {
        return Err(CommandRejectedError::new("Table does not exist"));
    }
    if state.status == "in_hand" {
        return Err(CommandRejectedError::new("Hand already in progress"));
    }
    if state.active_player_count() < 2 {
        return Err(CommandRejectedError::new(
            "Not enough players to start hand",
        ));
    }
    Ok(())
}

fn compute(state: &TableState) -> HandStarted {
    let hand_number = state.hand_count + 1;
    let hand_root = generate_hand_root(&state.table_id, hand_number);

    let dealer_position = advance_to_next_active(state.dealer_position, state);
    let small_blind_position = advance_to_next_active(dealer_position, state);
    let big_blind_position = advance_to_next_active(small_blind_position, state);

    let active_players: Vec<SeatSnapshot> = state
        .seats
        .values()
        .filter(|seat| !seat.is_sitting_out)
        .map(|seat| SeatSnapshot {
            position: seat.position,
            player_root: seat.player_root.clone(),
            stack: seat.stack,
        })
        .collect();

    HandStarted {
        hand_root,
        hand_number,
        dealer_position,
        small_blind_position,
        big_blind_position,
        active_players,
        game_variant: state.game_variant as i32,
        small_blind: state.small_blind,
        big_blind: state.big_blind,
        started_at: Some(angzarr_client::now()),
    }
}

pub fn handle_start_hand(
    _cmd: StartHand,
    state: &TableState,
    seq: u32,
) -> CommandResult<EventBook> {
    guard(state)?;

    let event = compute(state);
    let event_any = pack_event(&event, "examples.HandStarted");

    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}

/// Generate deterministic 16-byte hand root from the table id + hand number.
///
/// Mirrors the Python reference: `sha256("angzarr.poker.hand.{table_id}.{n}")[:16]`.
fn generate_hand_root(table_id: &str, hand_number: i64) -> Vec<u8> {
    use sha2::{Digest, Sha256};
    let input = format!("angzarr.poker.hand.{}.{}", table_id, hand_number);
    let hash = Sha256::digest(input.as_bytes());
    hash[..16].to_vec()
}

/// Find the next active (non-sitting-out) player position.
fn advance_to_next_active(current_pos: i32, state: &TableState) -> i32 {
    let max_players = state.max_players;
    for i in 1..=max_players {
        let next_pos = (current_pos + i) % max_players;
        if let Some(seat) = state.seats.get(&next_pos) {
            if !seat.is_sitting_out {
                return next_pos;
            }
        }
    }
    current_pos // Shouldn't happen if we have active players
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{apply_player_joined, apply_table_created};
    use angzarr_client::proto::event_page::Payload;
    use examples_proto::{GameVariant, PlayerJoined, TableCreated};
    use prost::Message;

    fn created_state() -> TableState {
        let mut s = TableState::default();
        apply_table_created(
            &mut s,
            TableCreated {
                table_name: "T".into(),
                game_variant: GameVariant::TexasHoldem as i32,
                small_blind: 5,
                big_blind: 10,
                min_buy_in: 100,
                max_buy_in: 1000,
                max_players: 4,
                action_timeout_seconds: 30,
                created_at: None,
            },
        );
        s
    }

    fn add_player(state: &mut TableState, player_root: Vec<u8>, seat: i32) {
        apply_player_joined(
            state,
            PlayerJoined {
                player_root,
                seat_position: seat,
                buy_in_amount: 500,
                stack: 500,
                joined_at: None,
            },
        );
    }

    #[test]
    fn handle_start_hand_success_emits_hand_started_with_deterministic_root() {
        let mut state = created_state();
        add_player(&mut state, vec![1], 0);
        add_player(&mut state, vec![2], 1);

        let book = handle_start_hand(StartHand {}, &state, 1).expect("ok");
        let any = match book.pages[0].payload.as_ref() {
            Some(Payload::Event(a)) => a,
            _ => panic!(),
        };
        assert!(any.type_url.ends_with("examples.HandStarted"));
        let decoded = HandStarted::decode(any.value.as_slice()).unwrap();
        assert_eq!(decoded.hand_number, 1);
        assert_eq!(decoded.active_players.len(), 2);
        // Verify deterministic hand_root derived from table_id + hand_number
        let expected = {
            use sha2::{Digest, Sha256};
            let input = format!("angzarr.poker.hand.{}.{}", state.table_id, 1);
            let hash = Sha256::digest(input.as_bytes());
            hash[..16].to_vec()
        };
        assert_eq!(decoded.hand_root, expected);
    }

    #[test]
    fn handle_start_hand_rejects_when_no_table() {
        let state = TableState::default();
        let err = handle_start_hand(StartHand {}, &state, 1).unwrap_err();
        assert!(err.reason.contains("does not exist"));
    }

    #[test]
    fn handle_start_hand_rejects_when_already_in_hand() {
        let mut state = created_state();
        add_player(&mut state, vec![1], 0);
        add_player(&mut state, vec![2], 1);
        state.status = "in_hand".into();
        let err = handle_start_hand(StartHand {}, &state, 1).unwrap_err();
        assert!(err.reason.contains("already in progress"));
    }

    #[test]
    fn handle_start_hand_rejects_when_insufficient_active_players() {
        let mut state = created_state();
        add_player(&mut state, vec![1], 0);
        let err = handle_start_hand(StartHand {}, &state, 1).unwrap_err();
        assert!(err.reason.contains("Not enough players"));
    }
}
