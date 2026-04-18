//! RequestDraw command handler (Five Card Draw specific).

use angzarr_client::proto::EventBook;
use angzarr_client::{event_page, pack_event, CommandRejectedError, CommandResult};
use examples_proto::{BettingPhase, DrawCompleted, GameVariant, RequestDraw};

use crate::state::{HandState, PlayerHandState};

/// Validated draw parameters.
struct ValidatedDraw {
    indices: Vec<i32>,
}

fn guard(state: &HandState) -> CommandResult<()> {
    if !state.exists() {
        return Err(CommandRejectedError::new("Hand does not exist"));
    }
    if state.is_complete() {
        return Err(CommandRejectedError::new("Hand already complete"));
    }
    if state.game_variant != GameVariant::FiveCardDraw {
        return Err(CommandRejectedError::invalid_argument(
            "Draw not supported in this game variant",
        ));
    }
    if state.current_phase != BettingPhase::Draw {
        return Err(CommandRejectedError::new("Not in draw phase"));
    }
    Ok(())
}

fn validate<'a>(
    cmd: &RequestDraw,
    state: &'a HandState,
) -> CommandResult<(&'a PlayerHandState, ValidatedDraw)> {
    let player = state
        .get_player(&cmd.player_root)
        .ok_or_else(|| CommandRejectedError::new("Player not in hand"))?;

    if player.has_folded {
        return Err(CommandRejectedError::new("Player has folded"));
    }

    let mut indices: Vec<i32> = cmd.card_indices.clone();
    indices.sort();
    indices.dedup();

    if indices.len() != cmd.card_indices.len() {
        return Err(CommandRejectedError::new("Duplicate card indices"));
    }

    for &idx in &indices {
        if !(0..5).contains(&idx) {
            return Err(CommandRejectedError::new("Card index out of range (0-4)"));
        }
    }

    if indices.len() > state.remaining_deck.len() {
        return Err(CommandRejectedError::new("Not enough cards in deck"));
    }

    Ok((player, ValidatedDraw { indices }))
}

fn compute(
    cmd: &RequestDraw,
    state: &HandState,
    player: &PlayerHandState,
    validated: &ValidatedDraw,
) -> DrawCompleted {
    let cards_to_draw = validated.indices.len();
    let cards_drawn = state.remaining_deck[..cards_to_draw].to_vec();

    let mut new_cards = player.hole_cards.clone();
    for (i, &idx) in validated.indices.iter().enumerate() {
        new_cards[idx as usize] = cards_drawn[i];
    }

    DrawCompleted {
        player_root: cmd.player_root.clone(),
        cards_discarded: cards_to_draw as i32,
        cards_drawn: cards_to_draw as i32,
        new_cards,
        drawn_at: Some(angzarr_client::now()),
    }
}

pub fn handle_request_draw(
    cmd: RequestDraw,
    state: &HandState,
    seq: u32,
) -> CommandResult<EventBook> {
        guard(state)?;
    let (player, validated) = validate(&cmd, state)?;

    let event = compute(&cmd, state, player, &validated);
    let event_any = pack_event(&event, "examples.DrawCompleted");

    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{apply_cards_dealt, new_hand_state, PlayerHandState};
    use angzarr_client::proto::event_page::Payload;
    use examples_proto::{Card, CardsDealt, PlayerHoleCards, PlayerInHand, Rank, Suit};
    use prost::Message;

    fn card(suit: Suit, rank: Rank) -> Card {
        Card {
            suit: suit as i32,
            rank: rank as i32,
        }
    }

    fn draw_state() -> HandState {
        let mut state = new_hand_state();
        let hole_p1 = vec![
            card(Suit::Clubs, Rank::Two),
            card(Suit::Diamonds, Rank::Three),
            card(Suit::Hearts, Rank::Four),
            card(Suit::Spades, Rank::Five),
            card(Suit::Clubs, Rank::Six),
        ];
        let deck: Vec<Card> = (0..10)
            .map(|i| Card {
                suit: Suit::Hearts as i32,
                rank: (Rank::Seven as i32 + (i as i32 % 5)).min(Rank::Ace as i32),
            })
            .collect();
        apply_cards_dealt(
            &mut state,
            CardsDealt {
                table_root: vec![0xab],
                hand_number: 1,
                game_variant: GameVariant::FiveCardDraw as i32,
                player_cards: vec![PlayerHoleCards {
                    player_root: vec![1],
                    cards: hole_p1,
                }],
                dealer_position: 0,
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
                dealt_at: None,
                remaining_deck: deck,
            },
        );
        // apply_cards_dealt sets phase to Preflop. Manually transition to Draw.
        state.current_phase = BettingPhase::Draw;
        state
    }

    #[test]
    fn handle_request_draw_success_emits_draw_completed() {
        let state = draw_state();
        let cmd = RequestDraw {
            player_root: vec![1],
            card_indices: vec![0, 1],
        };
        let book = handle_request_draw(cmd, &state, 1).expect("ok");
        let any = match book.pages[0].payload.as_ref() {
            Some(Payload::Event(a)) => a,
            _ => panic!(),
        };
        assert!(any.type_url.ends_with("examples.DrawCompleted"));
        let decoded = DrawCompleted::decode(any.value.as_slice()).unwrap();
        assert_eq!(decoded.cards_drawn, 2);
        assert_eq!(decoded.cards_discarded, 2);
        assert_eq!(decoded.new_cards.len(), 5);
    }

    #[test]
    fn handle_request_draw_rejects_non_draw_variant() {
        let mut state = new_hand_state();
        apply_cards_dealt(
            &mut state,
            CardsDealt {
                table_root: vec![0xab],
                hand_number: 1,
                game_variant: GameVariant::TexasHoldem as i32,
                player_cards: vec![],
                dealer_position: 0,
                players: vec![PlayerInHand {
                    player_root: vec![1],
                    position: 0,
                    stack: 500,
                }],
                dealt_at: None,
                remaining_deck: vec![],
            },
        );
        state.current_phase = BettingPhase::Draw;
        let cmd = RequestDraw {
            player_root: vec![1],
            card_indices: vec![0],
        };
        let err = handle_request_draw(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("Draw not supported"));
    }

    #[test]
    fn handle_request_draw_rejects_duplicate_indices() {
        let state = draw_state();
        let cmd = RequestDraw {
            player_root: vec![1],
            card_indices: vec![0, 0, 1],
        };
        let err = handle_request_draw(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("Duplicate"));
    }

    // Unused import noise suppressor.
    #[allow(dead_code)]
    fn _touch(_p: PlayerHandState) {}
}
