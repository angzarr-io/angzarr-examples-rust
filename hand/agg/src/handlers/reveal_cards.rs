//! RevealCards command handler.

use angzarr_client::proto::EventBook;
use angzarr_client::{event_page, pack_event, CommandRejectedError, CommandResult};
use examples_proto::{CardsMucked, CardsRevealed, HandRanking, RevealCards};

use crate::game_rules::get_rules;
use crate::state::{HandState, PlayerHandState};

fn guard(state: &HandState) -> CommandResult<()> {
    if !state.exists() {
        return Err(CommandRejectedError::new("Hand does not exist"));
    }
    if state.is_complete() {
        return Err(CommandRejectedError::new("Hand already complete"));
    }
    Ok(())
}

fn validate<'a>(cmd: &RevealCards, state: &'a HandState) -> CommandResult<&'a PlayerHandState> {
    let player = state
        .get_player(&cmd.player_root)
        .ok_or_else(|| CommandRejectedError::new("Player not in hand"))?;

    if player.has_folded {
        return Err(CommandRejectedError::new("Player has folded"));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{apply_cards_dealt, new_hand_state};
    use angzarr_client::proto::event_page::Payload;
    use examples_proto::{Card, CardsDealt, GameVariant, PlayerHoleCards, PlayerInHand, Rank, Suit};
    use prost::Message;

    fn dealt_state() -> HandState {
        let mut state = new_hand_state();
        let hole = vec![
            Card {
                suit: Suit::Clubs as i32,
                rank: Rank::Ace as i32,
            },
            Card {
                suit: Suit::Diamonds as i32,
                rank: Rank::King as i32,
            },
        ];
        apply_cards_dealt(
            &mut state,
            CardsDealt {
                table_root: vec![0xab],
                hand_number: 1,
                game_variant: GameVariant::TexasHoldem as i32,
                player_cards: vec![PlayerHoleCards {
                    player_root: vec![1],
                    cards: hole,
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
                remaining_deck: vec![],
            },
        );
        state
    }

    #[test]
    fn handle_reveal_cards_success_emits_cards_revealed() {
        let state = dealt_state();
        let cmd = RevealCards {
            player_root: vec![1],
            muck: false,
        };
        let book = handle_reveal_cards(cmd, &state, 1).expect("ok");
        let any = match book.pages[0].payload.as_ref() {
            Some(Payload::Event(a)) => a,
            _ => panic!(),
        };
        assert!(any.type_url.ends_with("examples.CardsRevealed"));
        let decoded = CardsRevealed::decode(any.value.as_slice()).unwrap();
        assert_eq!(decoded.cards.len(), 2);
        assert!(decoded.ranking.is_some());
    }

    #[test]
    fn handle_reveal_cards_muck_emits_cards_mucked() {
        let state = dealt_state();
        let cmd = RevealCards {
            player_root: vec![1],
            muck: true,
        };
        let book = handle_reveal_cards(cmd, &state, 1).expect("ok");
        let any = match book.pages[0].payload.as_ref() {
            Some(Payload::Event(a)) => a,
            _ => panic!(),
        };
        assert!(any.type_url.ends_with("examples.CardsMucked"));
    }

    #[test]
    fn handle_reveal_cards_rejects_when_hand_does_not_exist() {
        let state = new_hand_state();
        let cmd = RevealCards {
            player_root: vec![1],
            muck: false,
        };
        let err = handle_reveal_cards(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("does not exist"));
    }
}
