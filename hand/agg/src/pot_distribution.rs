//! Pot-distribution helpers — odd-chip allocation per TDA Rule 20.
//!
//! When a pot doesn't divide evenly among tied winners, real poker has
//! specific rules for allocating the odd chip(s):
//!
//! * **TDA Rule 20A** — Board games (Hold'em / Omaha / Draw): odd chip
//!   goes to the first seat clockwise of the dealer button.
//! * **TDA Rule 20B** — Stud / Razz / Stud-Hi-Lo: odd chip goes to the
//!   high card by suit in the player's 5-card winning hand.
//! * **TDA Rule 20C** — High/Low split: the odd chip in the total pot
//!   goes to the high side. Within each side, additional odd-chip
//!   distribution applies recursively.
//! * **Robert's §35-9** — When the high (or low) portion of an H/L
//!   split has multiple tied winners, the odd chip in that portion goes
//!   to the player with the high card by suit (low card by suit for the
//!   low half).
//!
//! These are pure functions — no state, no events, no commands. The Hand
//! aggregate's `AwardPot` handler calls into these helpers to compute
//! the awards list it then emits as a `PotAwarded` event.
//!
//! Mirrors `examples-python/main/hand/agg/pot_distribution.py`.

/// A tied winner and their seat position relative to the dealer button.
///
/// `seat` is the absolute seat number (0..max_seats-1). Distance from the
/// button is computed by the helper.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WinnerWithSeat {
    pub player_root: String,
    pub seat: i64,
}

/// A tied winner and their high (or low) card-by-suit ranking value.
///
/// `suit_rank` is a single integer where higher value = higher suit
/// rank by the standard ordering (Spades > Hearts > Diamonds > Clubs).
/// Convention: encode as `card_rank * 4 + suit_index` where suit_index
/// is 0=clubs, 1=diamonds, 2=hearts, 3=spades. Two players cannot
/// share a single rank-and-suit (it would be the same physical card),
/// so this ordering produces a strict total order on the tied set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WinnerWithSuit {
    pub player_root: String,
    pub suit_rank: i64,
}

/// A single pot allocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Award {
    pub player_root: String,
    pub amount: i64,
}

/// Result of splitting a pot between high and low sides.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HighLowSplit {
    pub high_share: i64,
    pub low_share: i64,
}

/// Split `pot` evenly among `winners`; odd chips go to the first seat
/// clockwise of the dealer button. Per TDA Rule 20A.
///
/// Returns awards in the same order as `winners`. Awards sum to exactly
/// `pot`.
///
/// # Errors
/// Returns `Err` if `winners` is empty, `pot` is non-positive, or
/// `max_seats` is too small to cover the seats in use.
pub fn split_pot_clockwise_from_button(
    pot: i64,
    winners: &[WinnerWithSeat],
    dealer_button_seat: i64,
    max_seats: i64,
) -> Result<Vec<Award>, String> {
    if pot <= 0 {
        return Err(format!("pot must be positive, got {}", pot));
    }
    if winners.is_empty() {
        return Err("winners must be non-empty".to_string());
    }
    let highest_seat = winners.iter().map(|w| w.seat).max().expect("non-empty");
    let min_required = highest_seat.max(dealer_button_seat) + 1;
    if max_seats < min_required {
        return Err(format!(
            "max_seats ({}) must be at least {} to cover button seat {} and winner seats",
            max_seats, min_required, dealer_button_seat
        ));
    }

    let n = winners.len() as i64;
    let base_share = pot / n;
    let odd_chips = pot - base_share * n;

    let distance_from_button = |seat: i64| -> i64 {
        // Clockwise distance from the seat immediately after the button.
        // `rem_euclid` is Python-style modulo: always returns a value in
        // [0, max_seats) even for negative dividends.
        (seat - dealer_button_seat - 1).rem_euclid(max_seats)
    };

    // Sort winners by clockwise distance.
    let mut indexed: Vec<(usize, i64)> = winners
        .iter()
        .enumerate()
        .map(|(i, w)| (i, distance_from_button(w.seat)))
        .collect();
    indexed.sort_by_key(|&(_, d)| d);

    let mut extra_recipients: std::collections::HashSet<usize> = std::collections::HashSet::new();
    for &(idx, _) in indexed.iter().take(odd_chips as usize) {
        extra_recipients.insert(idx);
    }

    Ok(winners
        .iter()
        .enumerate()
        .map(|(i, w)| Award {
            player_root: w.player_root.clone(),
            amount: base_share + if extra_recipients.contains(&i) { 1 } else { 0 },
        })
        .collect())
}

/// Split `pot` evenly among `winners`; odd chip goes to the highest (or
/// lowest) card by suit. Per Robert's Rules §35-9.
///
/// `high_wins=true` means the highest `suit_rank` gets the odd chip
/// (high stud, high half of H/L). `high_wins=false` means the lowest
/// gets it (razz, low half of H/L).
///
/// # Errors
/// Returns `Err` if `winners` is empty or `pot` is non-positive.
pub fn split_pot_by_suit(
    pot: i64,
    winners: &[WinnerWithSuit],
    high_wins: bool,
) -> Result<Vec<Award>, String> {
    if pot <= 0 {
        return Err(format!("pot must be positive, got {}", pot));
    }
    if winners.is_empty() {
        return Err("winners must be non-empty".to_string());
    }
    let n = winners.len() as i64;
    let base_share = pot / n;
    let odd_chips = pot - base_share * n;

    // Sort indices by suit_rank: descending for high, ascending for low.
    let mut indexed: Vec<(usize, i64)> = winners
        .iter()
        .enumerate()
        .map(|(i, w)| (i, w.suit_rank))
        .collect();
    if high_wins {
        indexed.sort_by(|a, b| b.1.cmp(&a.1));
    } else {
        indexed.sort_by(|a, b| a.1.cmp(&b.1));
    }

    let mut extra_recipients: std::collections::HashSet<usize> = std::collections::HashSet::new();
    for &(idx, _) in indexed.iter().take(odd_chips as usize) {
        extra_recipients.insert(idx);
    }

    Ok(winners
        .iter()
        .enumerate()
        .map(|(i, w)| Award {
            player_root: w.player_root.clone(),
            amount: base_share + if extra_recipients.contains(&i) { 1 } else { 0 },
        })
        .collect())
}

/// Split a total pot between the high and low halves. Per TDA Rule 20C:
/// the odd chip in the total pot goes to the high side.
///
/// # Errors
/// Returns `Err` if `pot` is non-positive.
pub fn split_high_low_total(pot: i64) -> Result<HighLowSplit, String> {
    if pot <= 0 {
        return Err(format!("pot must be positive, got {}", pot));
    }
    let low_share = pot / 2;
    let high_share = pot - low_share; // high gets any odd chip
    Ok(HighLowSplit {
        high_share,
        low_share,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn winner(name: &str, seat: i64) -> WinnerWithSeat {
        WinnerWithSeat {
            player_root: name.to_string(),
            seat,
        }
    }

    fn winner_suit(name: &str, suit_rank: i64) -> WinnerWithSuit {
        WinnerWithSuit {
            player_root: name.to_string(),
            suit_rank,
        }
    }

    fn award(name: &str, amount: i64) -> Award {
        Award {
            player_root: name.to_string(),
            amount,
        }
    }

    fn by_player(awards: &[Award]) -> std::collections::HashMap<String, i64> {
        awards
            .iter()
            .map(|a| (a.player_root.clone(), a.amount))
            .collect()
    }

    // === split_pot_clockwise_from_button (TDA Rule 20A) ===

    #[test]
    fn eu_1170_odd_chip_to_first_seat_clockwise_of_button() {
        let winners = vec![winner("Alice", 1), winner("Bob", 2)];
        let awards = split_pot_clockwise_from_button(101, &winners, 0, 9).unwrap();
        let bp = by_player(&awards);
        assert_eq!(bp.get("Alice"), Some(&51));
        assert_eq!(bp.get("Bob"), Some(&50));
        assert_eq!(awards.iter().map(|a| a.amount).sum::<i64>(), 101);
    }

    #[test]
    fn even_pot_distributes_equally() {
        let winners = vec![winner("Alice", 1), winner("Bob", 2)];
        let awards = split_pot_clockwise_from_button(100, &winners, 0, 9).unwrap();
        assert!(awards.iter().all(|a| a.amount == 50));
    }

    #[test]
    fn three_way_tie_with_one_odd_chip_goes_clockwise() {
        let winners = vec![winner("Alice", 1), winner("Bob", 2), winner("Carol", 3)];
        let awards = split_pot_clockwise_from_button(100, &winners, 0, 9).unwrap();
        let bp = by_player(&awards);
        assert_eq!(bp.get("Alice"), Some(&34));
        assert_eq!(bp.get("Bob"), Some(&33));
        assert_eq!(bp.get("Carol"), Some(&33));
    }

    #[test]
    fn three_way_tie_with_two_odd_chips_continues_clockwise() {
        let winners = vec![winner("Alice", 1), winner("Bob", 2), winner("Carol", 3)];
        let awards = split_pot_clockwise_from_button(101, &winners, 0, 9).unwrap();
        let bp = by_player(&awards);
        assert_eq!(bp.get("Alice"), Some(&34));
        assert_eq!(bp.get("Bob"), Some(&34));
        assert_eq!(bp.get("Carol"), Some(&33));
    }

    #[test]
    fn button_at_higher_seat_wraps_clockwise() {
        let winners = vec![winner("Alice", 1), winner("Bob", 8)];
        let awards = split_pot_clockwise_from_button(101, &winners, 5, 9).unwrap();
        let bp = by_player(&awards);
        assert_eq!(bp.get("Alice"), Some(&50));
        assert_eq!(bp.get("Bob"), Some(&51));
    }

    #[test]
    fn rejects_empty_winners() {
        let err = split_pot_clockwise_from_button(100, &[], 0, 9).unwrap_err();
        assert_eq!(err, "winners must be non-empty");
    }

    #[test]
    fn rejects_non_positive_pot() {
        let winners = vec![winner("Alice", 1)];
        assert_eq!(
            split_pot_clockwise_from_button(0, &winners, 0, 9).unwrap_err(),
            "pot must be positive, got 0"
        );
        assert_eq!(
            split_pot_clockwise_from_button(-1, &winners, 0, 9).unwrap_err(),
            "pot must be positive, got -1"
        );
    }

    #[test]
    fn rejects_max_seats_too_small() {
        let winners = vec![winner("Alice", 8)];
        let err = split_pot_clockwise_from_button(100, &winners, 2, 5).unwrap_err();
        assert!(err.contains("max_seats (5)"));
        assert!(err.contains("at least 9"));
    }

    #[test]
    fn pot_of_exactly_one_chip_succeeds() {
        let winners = vec![winner("Alice", 1)];
        let awards = split_pot_clockwise_from_button(1, &winners, 0, 9).unwrap();
        assert_eq!(awards, vec![award("Alice", 1)]);
    }

    #[test]
    fn button_beyond_winner_seats() {
        let winners = vec![winner("Alice", 0), winner("Bob", 5)];
        let awards = split_pot_clockwise_from_button(101, &winners, 10, 11).unwrap();
        let bp = by_player(&awards);
        assert_eq!(bp.get("Alice"), Some(&51));
        assert_eq!(bp.get("Bob"), Some(&50));
    }

    #[test]
    fn winner_at_button_seat_is_furthest_clockwise() {
        // The button's own seat is the LAST seat clockwise — distance
        // = max_seats - 1. The seat immediately before the button (the
        // "cutoff") is the second-to-last; the seat immediately after
        // the button (the SB) is the first.
        //
        // With button=0 and max_seats=11, a winner at seat 0 (the button
        // itself) has distance 10. A winner at seat 10 has distance 9.
        // Seat 10 wins the odd chip, NOT seat 0.
        //
        // Catches `-1` → `/1` (effectively dropping the -1) which would
        // map seat=button to distance 0 (looking like the immediate
        // next-clockwise seat) — flipping the result.
        let winners = vec![winner("Alice", 10), winner("Bob", 0)];
        let awards = split_pot_clockwise_from_button(101, &winners, 0, 11).unwrap();
        let bp = by_player(&awards);
        assert_eq!(bp.get("Alice"), Some(&51));
        assert_eq!(bp.get("Bob"), Some(&50));
    }

    #[test]
    fn seat_just_before_button_is_furthest() {
        // btn=0, max_seats=9. Seat 7 distance=6, Seat 8 distance=7.
        // Catches `-1` → `+1` mutation that would wrap seat 8 to 0.
        let winners = vec![winner("Alice", 7), winner("Bob", 8)];
        let awards = split_pot_clockwise_from_button(101, &winners, 0, 9).unwrap();
        let bp = by_player(&awards);
        assert_eq!(bp.get("Alice"), Some(&51));
        assert_eq!(bp.get("Bob"), Some(&50));
    }

    #[test]
    fn distance_is_modular_via_max_seats() {
        let winners = vec![winner("Alice", 0), winner("Bob", 4)];
        let bp_9 = by_player(&split_pot_clockwise_from_button(101, &winners, 8, 9).unwrap());
        assert_eq!(bp_9.get("Alice"), Some(&51));
        let bp_10 = by_player(&split_pot_clockwise_from_button(101, &winners, 8, 10).unwrap());
        assert_eq!(bp_10.get("Alice"), Some(&51));
    }

    #[test]
    fn single_winner_takes_full_pot() {
        let winners = vec![winner("Alice", 3)];
        let awards = split_pot_clockwise_from_button(101, &winners, 0, 9).unwrap();
        assert_eq!(awards, vec![award("Alice", 101)]);
    }

    // === split_pot_by_suit (Robert's §35-9) ===

    #[test]
    fn eu_1172_multi_way_high_chip_goes_to_high_card_by_suit() {
        // Ah=12*4+2=50, Kh=11*4+2=46.
        let winners = vec![winner_suit("Alice", 50), winner_suit("Bob", 46)];
        let awards = split_pot_by_suit(51, &winners, true).unwrap();
        let bp = by_player(&awards);
        assert_eq!(bp.get("Alice"), Some(&26));
        assert_eq!(bp.get("Bob"), Some(&25));
    }

    #[test]
    fn split_by_suit_low_wins_gives_chip_to_lowest() {
        let winners = vec![winner_suit("Alice", 0), winner_suit("Bob", 3)];
        let awards = split_pot_by_suit(51, &winners, false).unwrap();
        let bp = by_player(&awards);
        assert_eq!(bp.get("Alice"), Some(&26));
        assert_eq!(bp.get("Bob"), Some(&25));
    }

    #[test]
    fn split_by_suit_even_pot_distributes_equally() {
        let winners = vec![winner_suit("Alice", 50), winner_suit("Bob", 46)];
        let awards = split_pot_by_suit(100, &winners, true).unwrap();
        assert!(awards.iter().all(|a| a.amount == 50));
    }

    #[test]
    fn split_by_suit_three_way_with_two_odd_chips() {
        let winners = vec![
            winner_suit("Alice", 50),
            winner_suit("Bob", 46),
            winner_suit("Carol", 42),
        ];
        let awards = split_pot_by_suit(101, &winners, true).unwrap();
        let bp = by_player(&awards);
        assert_eq!(bp.get("Alice"), Some(&34));
        assert_eq!(bp.get("Bob"), Some(&34));
        assert_eq!(bp.get("Carol"), Some(&33));
    }

    #[test]
    fn split_by_suit_rejects_empty_or_non_positive() {
        assert_eq!(
            split_pot_by_suit(100, &[], true).unwrap_err(),
            "winners must be non-empty"
        );
        let winners = vec![winner_suit("Alice", 10)];
        assert_eq!(
            split_pot_by_suit(0, &winners, true).unwrap_err(),
            "pot must be positive, got 0"
        );
    }

    #[test]
    fn split_by_suit_with_pot_of_one_chip() {
        let winners = vec![winner_suit("Alice", 50)];
        let awards = split_pot_by_suit(1, &winners, true).unwrap();
        assert_eq!(awards, vec![award("Alice", 1)]);
    }

    // === split_high_low_total (TDA Rule 20C) ===

    #[test]
    fn eu_1171_odd_chip_in_total_pot_goes_to_high_side() {
        let result = split_high_low_total(101).unwrap();
        assert_eq!(
            result,
            HighLowSplit {
                high_share: 51,
                low_share: 50
            }
        );
    }

    #[test]
    fn high_low_even_split() {
        let result = split_high_low_total(100).unwrap();
        assert_eq!(
            result,
            HighLowSplit {
                high_share: 50,
                low_share: 50
            }
        );
    }

    #[test]
    fn high_low_rejects_non_positive_pot() {
        assert_eq!(
            split_high_low_total(0).unwrap_err(),
            "pot must be positive, got 0"
        );
        assert_eq!(
            split_high_low_total(-5).unwrap_err(),
            "pot must be positive, got -5"
        );
    }

    #[test]
    fn high_low_minimum_pot_of_one_chip_goes_entirely_to_high() {
        let result = split_high_low_total(1).unwrap();
        assert_eq!(
            result,
            HighLowSplit {
                high_share: 1,
                low_share: 0
            }
        );
    }
}
