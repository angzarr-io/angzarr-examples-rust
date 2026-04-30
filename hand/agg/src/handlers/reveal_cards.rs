//! RevealCards command handler.

use angzarr_client::proto::EventBook;
use angzarr_client::CommandResult;
use examples_utils::{event_page, pack_event, rejected};
use examples_proto::{CardsMucked, CardsRevealed, HandRanking, RevealCards};

use crate::game_rules::get_rules;
use crate::state::{HandState, PlayerHandState};

fn guard(state: &HandState) -> CommandResult<()> {
    if !state.exists() {
        return Err(rejected("Hand not dealt"));
    }
    if state.is_complete() {
        return Err(rejected("Hand already complete"));
    }
    if state.status != "showdown" {
        return Err(rejected("Not in showdown phase"));
    }
    Ok(())
}

fn validate<'a>(cmd: &RevealCards, state: &'a HandState) -> CommandResult<&'a PlayerHandState> {
    if cmd.player_root.is_empty() {
        return Err(rejected("player_root is required"));
    }

    let player = state
        .get_player(&cmd.player_root)
        .ok_or_else(|| rejected("Player not in hand"))?;

    if player.has_folded {
        return Err(rejected("Player has folded"));
    }

    Ok(player)
}

fn compute_muck(cmd: &RevealCards) -> CardsMucked {
    CardsMucked {
        player_root: cmd.player_root.clone(),
        mucked_at: Some(angzarr_client::now()),
    }
}

fn compute_reveal(cmd: &RevealCards, state: &HandState, player: &PlayerHandState) -> CardsRevealed {
    let rules = get_rules(state.game_variant);
    let hand_rank = rules.evaluate_hand(&player.hole_cards, &state.community_cards);

    let ranking = HandRanking {
        rank_type: hand_rank.rank_type as i32,
        kickers: hand_rank.kickers.into_iter().map(|r| r as i32).collect(),
        score: hand_rank.score,
    };

    CardsRevealed {
        player_root: cmd.player_root.clone(),
        cards: player.hole_cards.clone(),
        ranking: Some(ranking),
        revealed_at: Some(angzarr_client::now()),
    }
}

pub fn handle_reveal_cards(
    cmd: RevealCards,
    state: &HandState,
    seq: u32,
) -> CommandResult<EventBook> {
    guard(state)?;
    let player = validate(&cmd, state)?;

    let event_any = if cmd.muck {
        let event = compute_muck(&cmd);
        pack_event(&event, "examples.CardsMucked")
    } else {
        let event = compute_reveal(&cmd, state, player);
        pack_event(&event, "examples.CardsRevealed")
    };

    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}
