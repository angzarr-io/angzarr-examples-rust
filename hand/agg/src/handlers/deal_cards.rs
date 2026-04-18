//! DealCards command handler.

use rand::prelude::*;
use sha2::{Digest, Sha256};

use angzarr_client::proto::EventBook;
use angzarr_client::{event_page, pack_event, CommandRejectedError, CommandResult};
use examples_proto::{Card, CardsDealt, DealCards, GameVariant, PlayerHoleCards, Rank, Suit};

use crate::game_rules::get_rules;
use crate::state::HandState;

fn guard(state: &HandState) -> CommandResult<()> {
    if state.exists() {
        return Err(CommandRejectedError::new("Hand already dealt"));
    }
    Ok(())
}

fn validate(cmd: &DealCards) -> CommandResult<()> {
    if cmd.players.len() < 2 {
        return Err(CommandRejectedError::invalid_argument(
            "Need at least 2 players",
        ));
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
    }
}

pub fn handle_deal_cards(
    cmd: DealCards,
    state: &HandState,
    seq: u32,
) -> CommandResult<EventBook> {
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

/// Shuffle the deck using a seed for determinism.
fn shuffle_deck(deck: &mut [Card], seed: &[u8]) {
    let hash = Sha256::digest(seed);
    let seed_int = u64::from_be_bytes(hash[..8].try_into().unwrap());
    let mut rng = StdRng::seed_from_u64(seed_int);
    deck.shuffle(&mut rng);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{apply_cards_dealt, new_hand_state};
    use angzarr_client::proto::event_page::Payload;
    use examples_proto::PlayerInHand;
    use prost::Message;

    #[test]
    fn handle_deal_cards_success_emits_cards_dealt_with_hole_cards() {
        let state = new_hand_state();
        let cmd = DealCards {
            table_root: vec![0xab, 0xcd],
            hand_number: 1,
            game_variant: GameVariant::TexasHoldem as i32,
            players: vec![
                PlayerInHand {
                    player_root: vec![1],
                    position: 0,
                    stack: 500,
                },
                PlayerInHand {
                    player_root: vec![2],
                    position: 1,
                    stack: 500,
                },
            ],
            dealer_position: 0,
            small_blind: 5,
            big_blind: 10,
            deck_seed: vec![0u8; 32],
        };
        let book = handle_deal_cards(cmd, &state, 1).expect("ok");
        let any = match book.pages[0].payload.as_ref() {
            Some(Payload::Event(a)) => a,
            _ => panic!(),
        };
        assert!(any.type_url.ends_with("examples.CardsDealt"));
        let decoded = CardsDealt::decode(any.value.as_slice()).unwrap();
        assert_eq!(decoded.player_cards.len(), 2);
        assert_eq!(decoded.player_cards[0].cards.len(), 2); // Texas Hold'em = 2 hole cards
        assert_eq!(decoded.remaining_deck.len(), 52 - 4);
    }

    #[test]
    fn handle_deal_cards_rejects_when_already_dealt() {
        let mut state = new_hand_state();
        // Apply a CardsDealt to make the hand exist
        apply_cards_dealt(
            &mut state,
            CardsDealt {
                table_root: vec![0xab],
                hand_number: 1,
                game_variant: GameVariant::TexasHoldem as i32,
                player_cards: vec![],
                dealer_position: 0,
                players: vec![],
                dealt_at: None,
                remaining_deck: vec![],
            },
        );
        let cmd = DealCards {
            table_root: vec![0xab],
            hand_number: 2,
            game_variant: GameVariant::TexasHoldem as i32,
            players: vec![
                PlayerInHand {
                    player_root: vec![1],
                    position: 0,
                    stack: 500,
                },
                PlayerInHand {
                    player_root: vec![2],
                    position: 1,
                    stack: 500,
                },
            ],
            dealer_position: 0,
            small_blind: 5,
            big_blind: 10,
            deck_seed: vec![0u8; 32],
        };
        let err = handle_deal_cards(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("already dealt"));
    }

    #[test]
    fn handle_deal_cards_rejects_less_than_two_players() {
        let state = new_hand_state();
        let cmd = DealCards {
            table_root: vec![0xab],
            hand_number: 1,
            game_variant: GameVariant::TexasHoldem as i32,
            players: vec![PlayerInHand {
                player_root: vec![1],
                position: 0,
                stack: 500,
            }],
            dealer_position: 0,
            small_blind: 5,
            big_blind: 10,
            deck_seed: vec![0u8; 32],
        };
        let err = handle_deal_cards(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("at least 2"));
    }
}
