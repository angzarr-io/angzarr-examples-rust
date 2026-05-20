//! Poker game rules and hand evaluation.
//!
//! Polymorphic game rules supporting multiple poker variants.

use examples_proto::{BettingPhase, Card, GameVariant, HandRankType, Rank, Suit};
use itertools::Itertools;
use rand::prelude::*;
use sha2::{Digest, Sha256};
use std::collections::HashMap;

/// Hand ranking result with score for comparison.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandRank {
    pub rank_type: HandRankType,
    pub score: i32,
    pub kickers: Vec<Rank>,
}

impl HandRank {
    fn new(rank_type: HandRankType, score: i32, kickers: Vec<i32>) -> Self {
        Self {
            rank_type,
            score,
            kickers: kickers
                .into_iter()
                .filter_map(|r| Rank::try_from(r).ok())
                .collect(),
        }
    }
}

/// Phase transition result.
pub struct PhaseTransition {
    pub next_phase: BettingPhase,
    pub community_cards_to_deal: usize,
    pub is_showdown: bool,
}

/// Result of dealing hole cards.
pub struct DealResult {
    pub player_cards: HashMap<Vec<u8>, Vec<Card>>,
    pub remaining_deck: Vec<Card>,
}

/// Result of executing a Five Card Draw discard+draw.
pub struct DrawResult {
    pub new_hole_cards: Vec<Card>,
    pub cards_drawn: Vec<Card>,
    pub remaining_deck: Vec<Card>,
}

/// Game rules trait for variant-specific logic.
pub trait GameRules: Send + Sync {
    /// Game variant enum.
    fn variant(&self) -> GameVariant;

    /// Number of hole cards dealt to each player.
    fn hole_card_count(&self) -> usize;

    /// Ordered list of betting phases for this variant.
    fn phases(&self) -> Vec<BettingPhase>;

    /// Evaluate player's hand against community cards.
    fn evaluate_hand(&self, hole_cards: &[Card], community_cards: &[Card]) -> HandRank;

    /// Find the best 5-card hand from arbitrary card count. Returns HIGH_CARD/0
    /// when fewer than 5 cards are provided.
    fn find_best_hand(&self, cards: &[Card]) -> HandRank {
        find_best_hand_default(cards)
    }

    /// Get next phase transition.
    fn get_next_phase(&self, current_phase: BettingPhase) -> Option<PhaseTransition>;

    /// Check if variant uses community cards.
    fn uses_community_cards(&self) -> bool;

    /// Create and shuffle a 52-card deck, optionally seeded.
    fn create_deck(&self, seed: Option<&[u8]>) -> Vec<Card> {
        let mut deck = build_unshuffled_deck();
        match seed {
            Some(bytes) => {
                let hash = Sha256::digest(bytes);
                let seed_int = u64::from_be_bytes(hash[..8].try_into().unwrap());
                let mut rng = StdRng::seed_from_u64(seed_int);
                deck.shuffle(&mut rng);
            }
            None => {
                let mut rng = rand::thread_rng();
                deck.shuffle(&mut rng);
            }
        }
        deck
    }

    /// Deal hole cards to each player, drawing from the top (end) of the deck.
    fn deal_hole_cards(
        &self,
        deck: &[Card],
        players: &[Vec<u8>],
        seed: Option<&[u8]>,
    ) -> DealResult {
        let mut working: Vec<Card> = if seed.is_some() {
            self.create_deck(seed)
        } else if deck.is_empty() {
            self.create_deck(None)
        } else {
            deck.to_vec()
        };

        let mut player_cards: HashMap<Vec<u8>, Vec<Card>> = HashMap::new();
        for player_root in players {
            let mut cards = Vec::with_capacity(self.hole_card_count());
            for _ in 0..self.hole_card_count() {
                if let Some(c) = working.pop() {
                    cards.push(c);
                }
            }
            player_cards.insert(player_root.clone(), cards);
        }
        DealResult {
            player_cards,
            remaining_deck: working,
        }
    }

    /// Execute a Five Card Draw discard + draw. Default implementation returns
    /// the hand unchanged (variants that don't support draw).
    fn execute_draw(
        &self,
        deck: &[Card],
        hole_cards: &[Card],
        _discard_indices: &[usize],
    ) -> DrawResult {
        DrawResult {
            new_hole_cards: hole_cards.to_vec(),
            cards_drawn: Vec::new(),
            remaining_deck: deck.to_vec(),
        }
    }
}

/// Get game rules for a variant.
pub fn get_rules(variant: GameVariant) -> Box<dyn GameRules> {
    match variant {
        GameVariant::TexasHoldem => Box::new(TexasHoldemRules),
        GameVariant::Omaha => Box::new(OmahaRules),
        GameVariant::FiveCardDraw => Box::new(FiveCardDrawRules),
        _ => Box::new(TexasHoldemRules), // Default
    }
}

/// Texas Hold'em rules.
pub struct TexasHoldemRules;

impl GameRules for TexasHoldemRules {
    fn variant(&self) -> GameVariant {
        GameVariant::TexasHoldem
    }

    fn hole_card_count(&self) -> usize {
        2
    }

    fn phases(&self) -> Vec<BettingPhase> {
        vec![
            BettingPhase::Preflop,
            BettingPhase::Flop,
            BettingPhase::Turn,
            BettingPhase::River,
            BettingPhase::Showdown,
        ]
    }

    fn evaluate_hand(&self, hole_cards: &[Card], community_cards: &[Card]) -> HandRank {
        let mut all_cards: Vec<Card> = Vec::with_capacity(hole_cards.len() + community_cards.len());
        all_cards.extend_from_slice(hole_cards);
        all_cards.extend_from_slice(community_cards);
        find_best_hand_default(&all_cards)
    }

    fn get_next_phase(&self, current_phase: BettingPhase) -> Option<PhaseTransition> {
        match current_phase {
            BettingPhase::Preflop => Some(PhaseTransition {
                next_phase: BettingPhase::Flop,
                community_cards_to_deal: 3,
                is_showdown: false,
            }),
            BettingPhase::Flop => Some(PhaseTransition {
                next_phase: BettingPhase::Turn,
                community_cards_to_deal: 1,
                is_showdown: false,
            }),
            BettingPhase::Turn => Some(PhaseTransition {
                next_phase: BettingPhase::River,
                community_cards_to_deal: 1,
                is_showdown: false,
            }),
            BettingPhase::River => Some(PhaseTransition {
                next_phase: BettingPhase::Showdown,
                community_cards_to_deal: 0,
                is_showdown: true,
            }),
            _ => None,
        }
    }

    fn uses_community_cards(&self) -> bool {
        true
    }
}

/// Omaha rules — must use exactly 2 hole cards + 3 community cards.
pub struct OmahaRules;

impl GameRules for OmahaRules {
    fn variant(&self) -> GameVariant {
        GameVariant::Omaha
    }

    fn hole_card_count(&self) -> usize {
        4
    }

    fn phases(&self) -> Vec<BettingPhase> {
        TexasHoldemRules.phases()
    }

    fn evaluate_hand(&self, hole_cards: &[Card], community_cards: &[Card]) -> HandRank {
        if hole_cards.len() < 2 || community_cards.len() < 3 {
            return HandRank::new(HandRankType::HighCard, 0, vec![]);
        }

        let mut best: HandRank = HandRank::new(HandRankType::HighCard, 0, vec![]);
        for hole_combo in hole_cards.iter().combinations(2) {
            for comm_combo in community_cards.iter().combinations(3) {
                let five: Vec<Card> = hole_combo
                    .iter()
                    .chain(comm_combo.iter())
                    .map(|c| **c)
                    .collect();
                let rank = evaluate_five(&five);
                if (rank.score, &rank.kickers) > (best.score, &best.kickers) {
                    best = rank;
                }
            }
        }
        best
    }

    fn get_next_phase(&self, current_phase: BettingPhase) -> Option<PhaseTransition> {
        TexasHoldemRules.get_next_phase(current_phase)
    }

    fn uses_community_cards(&self) -> bool {
        true
    }
}

/// Five Card Draw rules.
pub struct FiveCardDrawRules;

impl GameRules for FiveCardDrawRules {
    fn variant(&self) -> GameVariant {
        GameVariant::FiveCardDraw
    }

    fn hole_card_count(&self) -> usize {
        5
    }

    fn phases(&self) -> Vec<BettingPhase> {
        vec![
            BettingPhase::Preflop,
            BettingPhase::Draw,
            BettingPhase::Showdown,
        ]
    }

    fn evaluate_hand(&self, hole_cards: &[Card], _community_cards: &[Card]) -> HandRank {
        if hole_cards.len() < 5 {
            return HandRank::new(HandRankType::HighCard, 0, vec![]);
        }
        evaluate_five(hole_cards)
    }

    fn get_next_phase(&self, current_phase: BettingPhase) -> Option<PhaseTransition> {
        match current_phase {
            BettingPhase::Preflop => Some(PhaseTransition {
                next_phase: BettingPhase::Draw,
                community_cards_to_deal: 0,
                is_showdown: false,
            }),
            BettingPhase::Draw => Some(PhaseTransition {
                next_phase: BettingPhase::Showdown,
                community_cards_to_deal: 0,
                is_showdown: true,
            }),
            _ => None,
        }
    }

    fn uses_community_cards(&self) -> bool {
        false
    }

    fn execute_draw(
        &self,
        deck: &[Card],
        hole_cards: &[Card],
        discard_indices: &[usize],
    ) -> DrawResult {
        let mut new_hole: Vec<Card> = hole_cards.to_vec();
        let mut remaining_deck: Vec<Card> = deck.to_vec();

        // Remove discards in reverse order.
        let mut idxs: Vec<usize> = discard_indices.to_vec();
        idxs.sort_unstable();
        for i in idxs.iter().rev() {
            if *i < new_hole.len() {
                new_hole.remove(*i);
            }
        }

        // Draw replacements from the end of the deck.
        let mut cards_drawn = Vec::new();
        for _ in 0..discard_indices.len() {
            if let Some(card) = remaining_deck.pop() {
                cards_drawn.push(card);
                new_hole.push(card);
            }
        }

        DrawResult {
            new_hole_cards: new_hole,
            cards_drawn,
            remaining_deck,
        }
    }
}

/// Find the best 5-card hand from arbitrary input.
pub fn find_best_hand_default(cards: &[Card]) -> HandRank {
    if cards.len() < 5 {
        return HandRank::new(HandRankType::HighCard, 0, vec![]);
    }
    let mut best = HandRank::new(HandRankType::HighCard, 0, vec![]);
    for combo in cards.iter().copied().combinations(5) {
        let rank = evaluate_five(&combo);
        if (rank.score, &rank.kickers) > (best.score, &best.kickers) {
            best = rank;
        }
    }
    best
}

/// Evaluate exactly 5 cards.
fn evaluate_five(cards: &[Card]) -> HandRank {
    if cards.len() != 5 {
        return HandRank::new(HandRankType::HighCard, 0, vec![]);
    }

    let mut suits = Vec::with_capacity(5);
    let mut ranks: Vec<i32> = Vec::with_capacity(5);
    let mut rank_counts: HashMap<i32, i32> = HashMap::new();

    for card in cards {
        suits.push(card.suit);
        ranks.push(card.rank);
        *rank_counts.entry(card.rank).or_insert(0) += 1;
    }
    ranks.sort_by_key(|&r| std::cmp::Reverse(r)); // descending

    let is_flush = suits.iter().all(|&s| s == suits[0]);
    let is_straight = is_straight(&ranks);

    // Sort rank keys by (count desc, rank desc).
    let mut sorted_by_count: Vec<(i32, i32)> = rank_counts.iter().map(|(&r, &c)| (r, c)).collect();
    sorted_by_count
        .sort_by_key(|&(rank, count)| (std::cmp::Reverse(count), std::cmp::Reverse(rank)));
    let counts: Vec<i32> = sorted_by_count.iter().map(|(_, c)| *c).collect();

    if is_straight && is_flush {
        if ranks == [Rank::Ace as i32, 13, 12, 11, 10] {
            return HandRank::new(HandRankType::RoyalFlush, 10_000_000, vec![]);
        }
        // Wheel straight flush: ranks [14,5,4,3,2] — high is 5.
        let high = if ranks == [Rank::Ace as i32, 5, 4, 3, 2] {
            5
        } else {
            ranks[0]
        };
        return HandRank::new(HandRankType::StraightFlush, 9_000_000 + high, vec![]);
    }
    if counts == [4, 1] {
        let quad_rank = sorted_by_count[0].0;
        let kicker = sorted_by_count[1].0;
        return HandRank::new(
            HandRankType::FourOfAKind,
            8_000_000 + quad_rank * 100,
            vec![kicker],
        );
    }
    if counts == [3, 2] {
        let trips_rank = sorted_by_count[0].0;
        let pair_rank = sorted_by_count[1].0;
        return HandRank::new(
            HandRankType::FullHouse,
            7_000_000 + trips_rank * 100 + pair_rank,
            vec![],
        );
    }
    if is_flush {
        return HandRank::new(
            HandRankType::Flush,
            6_000_000 + rank_score(&ranks),
            ranks.clone(),
        );
    }
    if is_straight {
        let high = if ranks == [Rank::Ace as i32, 5, 4, 3, 2] {
            5
        } else {
            ranks[0]
        };
        return HandRank::new(HandRankType::Straight, 5_000_000 + high, vec![]);
    }
    if counts == [3, 1, 1] {
        let trips_rank = sorted_by_count[0].0;
        let kickers: Vec<i32> = sorted_by_count[1..].iter().map(|(r, _)| *r).collect();
        return HandRank::new(
            HandRankType::ThreeOfAKind,
            4_000_000 + trips_rank * 1000,
            kickers,
        );
    }
    if counts == [2, 2, 1] {
        let pairs: Vec<i32> = sorted_by_count
            .iter()
            .filter(|(_, c)| *c == 2)
            .map(|(r, _)| *r)
            .collect();
        let kicker: Vec<i32> = sorted_by_count
            .iter()
            .filter(|(_, c)| *c == 1)
            .map(|(r, _)| *r)
            .collect();
        let high = *pairs.iter().max().unwrap();
        let low = *pairs.iter().min().unwrap();
        return HandRank::new(HandRankType::TwoPair, 3_000_000 + high * 100 + low, kicker);
    }
    if counts == [2, 1, 1, 1] {
        let pair_rank = sorted_by_count
            .iter()
            .find(|(_, c)| *c == 2)
            .map(|(r, _)| *r)
            .unwrap();
        let kickers: Vec<i32> = sorted_by_count
            .iter()
            .filter(|(_, c)| *c == 1)
            .map(|(r, _)| *r)
            .collect();
        return HandRank::new(HandRankType::Pair, 2_000_000 + pair_rank * 1000, kickers);
    }

    HandRank::new(
        HandRankType::HighCard,
        1_000_000 + rank_score(&ranks),
        ranks,
    )
}

/// Check if descending-sorted ranks form a straight (including wheel).
fn is_straight(ranks: &[i32]) -> bool {
    if ranks.len() != 5 {
        return false;
    }
    // Wheel: [14,5,4,3,2]
    if ranks == [Rank::Ace as i32, 5, 4, 3, 2] {
        return true;
    }
    for i in 0..4 {
        if ranks[i] - ranks[i + 1] != 1 {
            return false;
        }
    }
    true
}

/// Python-style rank score: sum of rank * 15^(4-i).
fn rank_score(ranks: &[i32]) -> i32 {
    let mut score = 0i32;
    let weights = [50625i32, 3375, 225, 15, 1]; // 15^4, 15^3, 15^2, 15, 1
    for (i, &r) in ranks.iter().take(5).enumerate() {
        score += r * weights[i];
    }
    score
}

/// Build an ordered 52-card deck (clubs, diamonds, hearts, spades × 2..Ace).
fn build_unshuffled_deck() -> Vec<Card> {
    let suits = [Suit::Clubs, Suit::Diamonds, Suit::Hearts, Suit::Spades];
    let ranks = [
        Rank::Two,
        Rank::Three,
        Rank::Four,
        Rank::Five,
        Rank::Six,
        Rank::Seven,
        Rank::Eight,
        Rank::Nine,
        Rank::Ten,
        Rank::Jack,
        Rank::Queen,
        Rank::King,
        Rank::Ace,
    ];
    let mut deck = Vec::with_capacity(52);
    for suit in suits {
        for rank in ranks {
            deck.push(Card {
                suit: suit as i32,
                rank: rank as i32,
            });
        }
    }
    deck
}
