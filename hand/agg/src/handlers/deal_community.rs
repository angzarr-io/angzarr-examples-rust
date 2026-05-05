//! DealCommunityCards command handler.

use angzarr_client::proto::EventBook;
use angzarr_client::CommandResult;
use examples_proto::{BettingPhase, CommunityCardsDealt, DealCommunityCards};
use examples_utils::{event_page, pack_event, reject};

use crate::errors::{
    CannotDealMoreCommunityCards, CommunityCardsNotUsedInVariant, HandAlreadyComplete,
    HandNotDealt, MustDealAtLeast1Card, NotEnoughCardsInDeck, WrongCardCountForPhase,
};
use crate::game_rules;
use crate::state::HandState;

/// Validated deal parameters.
struct ValidatedDeal {
    new_phase: BettingPhase,
    cards_to_deal: usize,
}

fn guard(state: &HandState) -> CommandResult<()> {
    if !state.exists() {
        return Err(reject(HandNotDealt));
    }
    if state.is_complete() {
        return Err(reject(HandAlreadyComplete));
    }

    let rules = game_rules::get_rules(state.game_variant);
    if !rules.uses_community_cards() {
        return Err(reject(CommunityCardsNotUsedInVariant));
    }
    Ok(())
}

fn validate(cmd: &DealCommunityCards, state: &HandState) -> CommandResult<ValidatedDeal> {
    if cmd.count < 1 {
        return Err(reject(MustDealAtLeast1Card {
            got: cmd.count,
            bound: 1,
        }));
    }

    let (new_phase, cards_to_deal) = match state.current_phase {
        BettingPhase::Preflop => (BettingPhase::Flop, 3),
        BettingPhase::Flop => (BettingPhase::Turn, 1),
        BettingPhase::Turn => (BettingPhase::River, 1),
        _ => return Err(reject(CannotDealMoreCommunityCards)),
    };

    if cmd.count as usize != cards_to_deal {
        return Err(reject(WrongCardCountForPhase {
            expected: cards_to_deal as i32,
            got: cmd.count,
            phase: format!("{:?}", new_phase).to_uppercase(),
        }));
    }

    if state.remaining_deck.len() < cards_to_deal {
        return Err(reject(NotEnoughCardsInDeck {
            requested: cards_to_deal as i32,
            available: state.remaining_deck.len() as i32,
        }));
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
