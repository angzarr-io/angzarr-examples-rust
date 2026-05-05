//! RequestDraw command handler (Five Card Draw specific).

use angzarr_client::proto::EventBook;
use angzarr_client::CommandResult;
use examples_proto::{BettingPhase, DrawCompleted, GameVariant, RequestDraw};
use examples_utils::{event_page, pack_event, reject};

use crate::errors::{
    DrawNotSupportedInVariant, DuplicateCardIndices, HandAlreadyComplete, HandNotDealt,
    InvalidCardIndex, NotEnoughCardsInDeck, NotInDrawPhase, PlayerHasFolded, PlayerNotInHand,
    TooManyDiscards,
};
use crate::state::{HandState, PlayerHandState};

/// Validated draw parameters.
struct ValidatedDraw {
    indices: Vec<i32>,
}

fn guard(state: &HandState) -> CommandResult<()> {
    if !state.exists() {
        return Err(reject(HandNotDealt));
    }
    if state.is_complete() {
        return Err(reject(HandAlreadyComplete));
    }
    if state.game_variant != GameVariant::FiveCardDraw {
        return Err(reject(DrawNotSupportedInVariant));
    }
    if state.current_phase != BettingPhase::Draw {
        return Err(reject(NotInDrawPhase));
    }
    Ok(())
}

fn validate<'a>(
    cmd: &RequestDraw,
    state: &'a HandState,
) -> CommandResult<(&'a PlayerHandState, ValidatedDraw)> {
    let player = state
        .get_player(&cmd.player_root)
        .ok_or_else(|| reject(PlayerNotInHand))?;

    if player.has_folded {
        return Err(reject(PlayerHasFolded));
    }

    if cmd.card_indices.len() > 5 {
        return Err(reject(TooManyDiscards {
            got: cmd.card_indices.len() as i32,
            bound: 5,
        }));
    }

    let mut indices: Vec<i32> = cmd.card_indices.clone();
    indices.sort();
    indices.dedup();

    if indices.len() != cmd.card_indices.len() {
        return Err(reject(DuplicateCardIndices));
    }

    for &idx in &indices {
        if !(0..5).contains(&idx) {
            return Err(reject(InvalidCardIndex { got: idx }));
        }
    }

    if indices.len() > state.remaining_deck.len() {
        return Err(reject(NotEnoughCardsInDeck {
            requested: indices.len() as i32,
            available: state.remaining_deck.len() as i32,
        }));
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
