//! StartHand command handler.

use angzarr_client::proto::EventBook;
use angzarr_client::CommandResult;
use examples_utils::{event_page, pack_event, rejected};
use examples_proto::{HandStarted, SeatSnapshot, StartHand};

use crate::state::TableState;

fn guard(state: &TableState) -> CommandResult<()> {
    if !state.exists() {
        return Err(rejected("Table does not exist"));
    }
    if state.status == "in_hand" {
        return Err(rejected("Hand already in progress"));
    }
    if state.active_player_count() < 2 {
        return Err(rejected(
            "Not enough players to start hand",
        ));
    }
    Ok(())
}

fn compute(state: &TableState) -> HandStarted {
    let hand_number = state.hand_count + 1;
    let hand_root = generate_hand_root(&state.table_id, hand_number);

    let dealer_position = advance_to_next_active(state.dealer_position, state);
    // Heads-up rule: with exactly two active players the dealer posts the
    // small blind (and the other player posts the big blind). With 3+ players
    // the SB sits to the left of the dealer.
    let (small_blind_position, big_blind_position) = if state.active_player_count() == 2 {
        (
            dealer_position,
            advance_to_next_active(dealer_position, state),
        )
    } else {
        let sb = advance_to_next_active(dealer_position, state);
        let bb = advance_to_next_active(sb, state);
        (sb, bb)
    };

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
