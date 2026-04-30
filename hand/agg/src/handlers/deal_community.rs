//! DealCommunityCards command handler.

use angzarr_client::proto::EventBook;
use angzarr_client::{CommandRejectedError, CommandResult};
use examples_utils::{event_page, invalid_arg, pack_event, rejected};
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
        return Err(rejected("Hand not dealt"));
    }
    if state.is_complete() {
        return Err(rejected("Hand already complete"));
    }

    let rules = game_rules::get_rules(state.game_variant);
    if !rules.uses_community_cards() {
        return Err(rejected(
            "community cards not used in this variant",
        ));
    }
    Ok(())
}

fn validate(cmd: &DealCommunityCards, state: &HandState) -> CommandResult<ValidatedDeal> {
    if cmd.count < 1 {
        return Err(invalid_arg("count must be at least 1"));
    }

    let (new_phase, cards_to_deal) = match state.current_phase {
        BettingPhase::Preflop => (BettingPhase::Flop, 3),
        BettingPhase::Flop => (BettingPhase::Turn, 1),
        BettingPhase::Turn => (BettingPhase::River, 1),
        _ => {
            return Err(rejected(
                "Cannot deal more community cards",
            ))
        }
    };

    if cmd.count as usize != cards_to_deal {
        return Err(CommandRejectedError::precondition_failed(
            "INVALID_COMMUNITY_CARD_COUNT",
            "Expected card count for phase",
            [
                ("expected", cards_to_deal.to_string()),
                ("got", cmd.count.to_string()),
            ],
        ));
    }

    if state.remaining_deck.len() < cards_to_deal {
        return Err(rejected("Not enough cards in deck"));
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
