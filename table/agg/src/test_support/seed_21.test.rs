use examples_proto::v1::{self, seat_outcome::Outcome, MoveAction};

use super::*;

#[test]
fn the_table_is_main_from_seed_21() {
    let created = create_table();
    assert_eq!((created.name.as_str(), created.shoe_seed), ("Main", 21));
    assert_eq!(created, create_main(21));
}

/// Each request names its player, seat and buy-in, and its confirmation
/// confirms that request's buy-in.
#[test]
fn alice_takes_seat_0_and_bob_seat_1_each_buying_in_for_500() {
    let seated: Vec<(u8, i32, i64, bool)> = seats()
        .iter()
        .map(|(request, confirm)| {
            (
                request.player_root[0],
                request.seat,
                request.amount,
                confirm.buy_in_id == request.request_id,
            )
        })
        .collect();
    assert_eq!(seated, [(ALICE, 0, 500, true), (BOB, 1, 500, true)]);
}

#[test]
fn each_seat_bets_20() {
    let bets: Vec<(i32, i64)> = bets().iter().map(|b| (b.seat, b.amount)).collect();
    assert_eq!(bets, [(0, 20), (1, 20)]);
}

#[test]
fn alice_doubles_and_bob_hits_twice_then_stands() {
    assert_eq!(moves_of(ALICE), [(0, MoveAction::DoubleDown)]);
    assert_eq!(
        moves_of(BOB),
        [
            (1, MoveAction::Hit),
            (1, MoveAction::Hit),
            (1, MoveAction::Stand)
        ]
    );
}

fn outcome(seat: i32, wager: i64, outcome: Outcome, returned: i64, after: i64) -> v1::SeatOutcome {
    v1::SeatOutcome {
        seat,
        wager,
        outcome: outcome.into(),
        returned,
        stack_after: after,
        ..v1::SeatOutcome::default()
    }
}

fn settled(house_delta: i64) -> v1::RoundSettled {
    v1::RoundSettled {
        outcomes: vec![
            outcome(0, 40, Outcome::Win, 80, 540),
            outcome(1, 20, Outcome::Win, 40, 520),
        ],
        house_delta,
        ..v1::RoundSettled::default()
    }
}

#[test]
fn seed_21_settles_alice_540_bob_520_house_minus_60() {
    assert_settled(&settled(-60));
}

#[test]
#[should_panic(expected = "left: 60")]
fn any_other_settlement_is_not_seed_21s() {
    assert_settled(&settled(60));
}
