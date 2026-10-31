//! Seed 21's hand (one deck), the scenario every driver of the table plays
//! to settlement: the aggregate's harness sending the moves itself, and the
//! application whose remote player decides them.
//!
//! Table "Main" is created from seed 21; Alice takes seat 0 and Bob seat 1,
//! each buying in for 500 and betting 20; the round is dealt. Alice holds
//! 4♣ 6♠ and Bob A♣ 5♠ against the dealer's 9♦, Q♦ in the hole. Basic
//! strategy plays it ([`MOVES`]): Alice doubles her hard 10 and draws A♥
//! (21); Bob hits his soft 16 (K♣, a hard 16), hits again against the 9
//! (4♦, 20) and stands. The dealer stands on 19 and both win
//! ([`assert_settled`]).

use examples_proto::v1::{self, seat_outcome::Outcome, MoveAction};

use super::create_main;

/// The byte Alice's player root repeats.
pub const ALICE: u8 = 0xa1;
/// The byte Bob's player root repeats.
pub const BOB: u8 = 0xb0;

/// Table "Main" ([`create_main`]), its first shoe from seed 21.
pub fn create_table() -> v1::CreateTable {
    create_main(21)
}

/// Alice's request for seat 0 and Bob's for seat 1, each buying in for
/// 500, and each request's confirmation, in the order they are sent.
pub fn seats() -> [(v1::RequestSeat, v1::ConfirmSeat); 2] {
    [(ALICE, 0, 0x01), (BOB, 1, 0x02)].map(|(player, seat, buy_in)| {
        (
            v1::RequestSeat {
                player_root: vec![player; 16],
                seat,
                amount: 500,
                request_id: vec![buy_in; 16],
            },
            v1::ConfirmSeat {
                buy_in_id: vec![buy_in; 16],
            },
        )
    })
}

/// Each seat's bet of 20, seat 0 first.
pub fn bets() -> [v1::PlaceBet; 2] {
    [0, 1].map(|seat| v1::PlaceBet { seat, amount: 20 })
}

/// The moves basic strategy makes once the round is dealt, in the order
/// the table asks for them: `(player, seat, action)`.
pub const MOVES: [(u8, i32, MoveAction); 4] = [
    (ALICE, 0, MoveAction::DoubleDown),
    (BOB, 1, MoveAction::Hit),
    (BOB, 1, MoveAction::Hit),
    (BOB, 1, MoveAction::Stand),
];

/// The table's events from the deal to the settlement, by short type
/// name: each move comes back as the table's command, the dealer plays,
/// and the round settles.
pub const FROM_DEAL: [&str; 11] = [
    "RoundDealt",
    "TurnStarted",
    "HandDoubled",
    "TurnStarted",
    "CardDealt",
    "TurnStarted",
    "CardDealt",
    "TurnStarted",
    "HandStood",
    "DealerPlayed",
    "RoundSettled",
];

/// `player`'s moves of [`MOVES`], as `(seat, action)`.
pub fn moves_of(player: u8) -> Vec<(i32, MoveAction)> {
    MOVES
        .iter()
        .filter(|(p, _, _)| *p == player)
        .map(|&(_, seat, action)| (seat, action))
        .collect()
}

/// Asserts the round settled as seed 21's does: both seats won, Alice's
/// double paying 80 on 40 (stack 540), Bob 40 on 20 (stack 520), the
/// house down 60.
pub fn assert_settled(settled: &v1::RoundSettled) {
    let outcomes: Vec<(i32, i64, Outcome, i64, i64)> = settled
        .outcomes
        .iter()
        .map(|o| (o.seat, o.wager, o.outcome(), o.returned, o.stack_after))
        .collect();
    assert_eq!(
        outcomes,
        [
            (0, 40, Outcome::Win, 80, 540),
            (1, 20, Outcome::Win, 40, 520),
        ]
    );
    assert_eq!(settled.house_delta, -60);
}

#[cfg(test)]
#[path = "seed_21.test.rs"]
mod seed_21_tests;
