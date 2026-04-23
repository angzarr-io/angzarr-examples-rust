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
        return Err(CommandRejectedError::new("Hand not dealt"));
    }
    if state.is_complete() {
        return Err(CommandRejectedError::new("Hand already complete"));
    }

    let rules = game_rules::get_rules(state.game_variant);
    if !rules.uses_community_cards() {
        return Err(CommandRejectedError::new(
            "community cards not used in this variant",
        ));
    }
    Ok(())
}

fn validate(cmd: &DealCommunityCards, state: &HandState) -> CommandResult<ValidatedDeal> {
    if cmd.count < 1 {
        return Err(CommandRejectedError::invalid_argument(
            "count must be at least 1",
        ));
    }

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

    if cmd.count as usize != cards_to_deal {
        return Err(CommandRejectedError::new(format!(
            "Invalid card count for phase: Expected {}, got {}",
            cards_to_deal, cmd.count
        )));
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
