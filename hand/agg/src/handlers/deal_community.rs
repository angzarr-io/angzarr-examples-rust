//! DealCommunityCards command handler.

use angzarr_client::proto::EventBook;
use angzarr_client::{event_page, pack_event, CommandRejectedError, CommandResult};
use examples_proto::{BettingPhase, CommunityCardsDealt, DealCommunityCards};

use crate::game_rules;
use crate::state::HandState;

/// Validated deal parameters.
struct ValidatedDeal {
    new_phase: BettingPhase,
    cards_to_deal: usize,
}

fn guard(state: &HandState) -> CommandResult<()> {
    if !state.exists() {
        return Err(CommandRejectedError::new("Hand does not exist"));
    }
    if state.is_complete() {
        return Err(CommandRejectedError::new("Hand already complete"));
    }

    let rules = game_rules::get_rules(state.game_variant);
    if !rules.uses_community_cards() {
        return Err(CommandRejectedError::invalid_argument(
            "Community cards not used in this variant",
        ));
    }
    Ok(())
}

fn validate(cmd: &DealCommunityCards, state: &HandState) -> CommandResult<ValidatedDeal> {
    let (new_phase, cards_to_deal) = match state.current_phase {
        BettingPhase::Preflop => (BettingPhase::Flop, 3),
        BettingPhase::Flop => (BettingPhase::Turn, 1),
        BettingPhase::Turn => (BettingPhase::River, 1),
        _ => {
            return Err(CommandRejectedError::new(
                "Cannot deal more community cards",
            ))
        }
    };

    if cmd.count > 0 && cmd.count as usize != cards_to_deal {
        return Err(CommandRejectedError::new("Invalid card count for phase"));
    }

    if state.remaining_deck.len() < cards_to_deal {
        return Err(CommandRejectedError::new("Not enough cards in deck"));
    }

    Ok(ValidatedDeal {
        new_phase,
        cards_to_deal,
    })
}

fn compute(state: &HandState, validated: &ValidatedDeal) -> CommunityCardsDealt {
    let new_cards: Vec<_> = state.remaining_deck[..validated.cards_to_deal].to_vec();
    let mut all_community = state.community_cards.clone();
    all_community.extend(new_cards.clone());

    CommunityCardsDealt {
        cards: new_cards,
        phase: validated.new_phase as i32,
        all_community_cards: all_community,
        dealt_at: Some(angzarr_client::now()),
    }
}

pub fn handle_deal_community_cards(
    cmd: DealCommunityCards,
    state: &HandState,
    seq: u32,
) -> CommandResult<EventBook> {
        guard(state)?;
    let validated = validate(&cmd, state)?;

    let event = compute(state, &validated);
    let event_any = pack_event(&event, "examples.CommunityCardsDealt");

    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{apply_cards_dealt, new_hand_state};
    use angzarr_client::proto::event_page::Payload;
    use examples_proto::{Card, CardsDealt, GameVariant, PlayerInHand, Rank, Suit};
    use prost::Message;

    fn dealt_state_with_deck(cards: usize) -> HandState {
        let mut state = new_hand_state();
        let deck: Vec<Card> = (0..cards)
            .map(|i| Card {
                suit: Suit::Clubs as i32,
                rank: (Rank::Two as i32).max(i as i32 % 14),
            })
            .collect();
        apply_cards_dealt(
            &mut state,
            CardsDealt {
                table_root: vec![0xab],
                hand_number: 1,
                game_variant: GameVariant::TexasHoldem as i32,
                player_cards: vec![],
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
        state
    }

    #[test]
    fn handle_deal_community_flop_success_emits_three_cards() {
        let state = dealt_state_with_deck(48);
        let cmd = DealCommunityCards { count: 3 };
        let book = handle_deal_community_cards(cmd, &state, 1).expect("ok");
        let any = match book.pages[0].payload.as_ref() {
            Some(Payload::Event(a)) => a,
            _ => panic!(),
        };
        assert!(any.type_url.ends_with("examples.CommunityCardsDealt"));
        let decoded = CommunityCardsDealt::decode(any.value.as_slice()).unwrap();
        assert_eq!(decoded.cards.len(), 3);
        assert_eq!(decoded.phase, BettingPhase::Flop as i32);
    }

    #[test]
    fn handle_deal_community_rejects_when_hand_does_not_exist() {
        let state = new_hand_state();
        let cmd = DealCommunityCards { count: 3 };
        let err = handle_deal_community_cards(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("does not exist"));
    }

    #[test]
    fn handle_deal_community_rejects_invalid_count_for_phase() {
        let state = dealt_state_with_deck(48);
        let cmd = DealCommunityCards { count: 5 };
        let err = handle_deal_community_cards(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("Invalid card count"));
    }
}
