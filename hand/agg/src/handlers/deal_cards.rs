//! DealCards command handler.

use rand::Rng;
use sha2::{Digest, Sha256};

use angzarr_client::proto::EventBook;
use angzarr_client::CommandResult;
use examples_proto::{Card, CardsDealt, DealCards, GameVariant, PlayerHoleCards, Rank, Suit};
use examples_utils::{event_page, pack_event, reject};

use crate::errors::{HandAlreadyDealt, NeedAtLeast2Players, NoPlayersInHand};
use crate::game_rules::get_rules;
use crate::state::HandState;

fn guard(state: &HandState) -> CommandResult<()> {
    if state.exists() {
        return Err(reject(HandAlreadyDealt));
    }
    Ok(())
}

fn validate(cmd: &DealCards) -> CommandResult<()> {
    if cmd.players.is_empty() {
        return Err(reject(NoPlayersInHand));
    }
    if cmd.players.len() < 2 {
        return Err(reject(NeedAtLeast2Players {
            got: cmd.players.len() as i32,
        }));
    }
    Ok(())
}

fn compute(cmd: &DealCards) -> CardsDealt {
    let mut deck = create_deck();
    let seed = if cmd.deck_seed.is_empty() {
        let mut rng = rand::thread_rng();
        let mut seed = [0u8; 32];
        rng.fill(&mut seed);
        seed.to_vec()
    } else {
        cmd.deck_seed.clone()
    };
    shuffle_deck(&mut deck, &seed);

    let variant = GameVariant::try_from(cmd.game_variant).unwrap_or(GameVariant::TexasHoldem);
    let rules = get_rules(variant);
    let cards_per_player = rules.hole_card_count();

    let mut player_cards = Vec::new();
    let total_cards_to_deal = cmd.players.len() * cards_per_player;

    for (i, player) in cmd.players.iter().enumerate() {
        let start = i * cards_per_player;
        let end = start + cards_per_player;
        let cards: Vec<Card> = deck[start..end].to_vec();
        player_cards.push(PlayerHoleCards {
            player_root: player.player_root.clone(),
            cards,
        });
    }

    let remaining_deck: Vec<Card> = deck[total_cards_to_deal..].to_vec();

    CardsDealt {
        table_root: cmd.table_root.clone(),
        hand_number: cmd.hand_number,
        game_variant: cmd.game_variant,
        player_cards,
        dealer_position: cmd.dealer_position,
        players: cmd.players.clone(),
        dealt_at: Some(angzarr_client::now()),
        remaining_deck,
        ..Default::default()
    }
}

pub fn handle_deal_cards(cmd: DealCards, state: &HandState, seq: u32) -> CommandResult<EventBook> {
    guard(state)?;
    validate(&cmd)?;

    let event = compute(&cmd);
    let event_any = pack_event(&event, "examples.CardsDealt");

    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}

/// Create a standard 52-card deck.
fn create_deck() -> Vec<Card> {
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

/// SplitMix64 — portable PRNG used so seeded shuffles produce byte-identical
/// decks across language implementations. Specified by the cucumber spec
/// (hand.feature EU-0004 asserts specific cards for a given seed); any
/// non-portable PRNG would silently break that assertion across languages.
/// Mirrors Python `_SplitMix64` in `hand/agg/handlers/game_rules.py`.
struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        z ^ (z >> 31)
    }
}

/// Shuffle the deck using a seed for determinism. Seed derivation:
/// SHA-256(seed_bytes) → first 8 bytes big-endian → u64 → SplitMix64 state.
/// Both languages must use this exact derivation for decks to match.
fn shuffle_deck(deck: &mut [Card], seed: &[u8]) {
    let hash = Sha256::digest(seed);
    let seed_int = u64::from_be_bytes(hash[..8].try_into().unwrap());
    let mut rng = SplitMix64::new(seed_int);
    let n = deck.len();
    for i in (1..n).rev() {
        let j = (rng.next_u64() % (i as u64 + 1)) as usize;
        deck.swap(i, j);
    }
}
