//! ChangeSeats command handler.
//!
//! TDA Rule 33 / WSOP Rule 86 — a player voluntarily requests to change
//! seats between hands. Per PR #12 design decision 2, if the operator
//! wires the same seat into both `current_seat` and `requested_seat`,
//! the handler emits a structured `SEATS_IDENTICAL` rejection BEFORE any
//! seat-lookup or skip-blind logic. Otherwise the move is treated as a
//! single-blind skip and a `BlindDodgePenalty` is emitted with
//! `chips_forfeited = small_blind + big_blind` and
//! `missed_round_count = 1` per EU-1185.
//!
//! The actual seat-reassignment plumbing (re-keying `state.seats`) is
//! left for a separate `SeatsChanged` projection; this handler stays
//! focused on the penalty leg that the EU-1185 scenario pins.

use angzarr_client::proto::EventBook;
use angzarr_client::CommandResult;
use examples_proto::{BlindDodgePenalty, ChangeSeats};
use examples_utils::{event_page, pack_event, reject};

use crate::errors::SeatsIdentical;
use crate::state::TableState;

pub fn handle_change_seats(
    cmd: ChangeSeats,
    state: &TableState,
    seq: u32,
) -> CommandResult<EventBook> {
    // PR #12 decision 2 — fire BEFORE any seat-lookup or skip-blind
    // logic. The current_seat == requested_seat check is the very first
    // rule applied (precedence over all other validation).
    if cmd.current_seat == cmd.requested_seat {
        return Err(reject(SeatsIdentical {
            seat: cmd.current_seat,
        }));
    }

    // EU-1185 — single-blind skip charges SB + BB.
    let chips = state.small_blind + state.big_blind;

    // Best-effort root resolution: if the seat is currently occupied,
    // attach the player's root for downstream chip-charging. If empty,
    // emit an empty root (matches Python canonical: `current_root = b""`).
    let player_root = state
        .seats
        .get(&cmd.current_seat)
        .map(|s| s.player_root.clone())
        .unwrap_or_default();

    let event = BlindDodgePenalty {
        player_root,
        chips_forfeited: chips,
        missed_round_count: 1,
        assessed_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.BlindDodgePenalty");
    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{apply_player_joined, apply_table_created};
    use examples_proto::{GameVariant, PlayerJoined, TableCreated};

    fn created_state() -> TableState {
        let mut s = TableState::default();
        apply_table_created(
            &mut s,
            TableCreated {
                table_name: "T1".into(),
                game_variant: GameVariant::TexasHoldem as i32,
                small_blind: 25,
                big_blind: 50,
                min_buy_in: 100,
                max_buy_in: 1000,
                max_players: 6,
                action_timeout_seconds: 30,
                created_at: None,
            },
        );
        s
    }

    #[test]
    fn rejects_when_seats_identical() {
        let state = created_state();
        let err = handle_change_seats(
            ChangeSeats {
                player_root: vec![0xa1],
                current_seat: 3,
                requested_seat: 3,
            },
            &state,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "SEATS_IDENTICAL");
        assert_eq!(err.details.get("seat"), Some(&"3".to_string()));
    }

    #[test]
    fn seats_identical_precedence_fires_before_any_other_logic() {
        // PR #12 decision 2 — the reject must precede seat lookup, so
        // even with an EMPTY state (no seats, blinds zero), an identical
        // seat must still produce SEATS_IDENTICAL (not some downstream
        // "seat not found" error).
        let state = TableState::default();
        let err = handle_change_seats(
            ChangeSeats {
                player_root: vec![0xa1],
                current_seat: 0,
                requested_seat: 0,
            },
            &state,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "SEATS_IDENTICAL");
    }

    #[test]
    fn emits_blind_dodge_penalty_with_sb_plus_bb_and_count_one() {
        let mut state = created_state();
        apply_player_joined(
            &mut state,
            PlayerJoined {
                player_root: vec![0xa1],
                seat_position: 2,
                buy_in_amount: 500,
                stack: 500,
                joined_at: None,
            },
        );
        let book = handle_change_seats(
            ChangeSeats {
                player_root: vec![0xa1],
                current_seat: 2,
                requested_seat: 5,
            },
            &state,
            7,
        )
        .expect("ok");
        assert_eq!(book.pages.len(), 1);
        let any = match book.pages[0].payload.as_ref().unwrap() {
            angzarr_client::proto::event_page::Payload::Event(any) => any,
            _ => panic!(),
        };
        let evt: BlindDodgePenalty = prost::Message::decode(any.value.as_slice()).expect("decode");
        // SB (25) + BB (50) = 75
        assert_eq!(evt.chips_forfeited, 75);
        assert_eq!(evt.missed_round_count, 1);
        assert_eq!(evt.player_root, vec![0xa1]);
        assert!(evt.assessed_at.is_some());
    }

    #[test]
    fn emits_with_empty_root_when_current_seat_unoccupied() {
        let state = created_state();
        let book = handle_change_seats(
            ChangeSeats {
                player_root: vec![0xa1],
                current_seat: 2,
                requested_seat: 5,
            },
            &state,
            8,
        )
        .expect("ok");
        let any = match book.pages[0].payload.as_ref().unwrap() {
            angzarr_client::proto::event_page::Payload::Event(any) => any,
            _ => panic!(),
        };
        let evt: BlindDodgePenalty = prost::Message::decode(any.value.as_slice()).expect("decode");
        // Per Python canonical: empty current_root when seat not found.
        assert!(evt.player_root.is_empty());
    }
}
