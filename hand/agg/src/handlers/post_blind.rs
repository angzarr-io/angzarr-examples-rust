//! PostBlind command handler.

use angzarr_client::proto::EventBook;
use angzarr_client::{event_page, pack_event, CommandRejectedError, CommandResult};
use examples_proto::{BlindPosted, PostBlind};

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

fn validate<'a>(cmd: &PostBlind, state: &'a HandState) -> CommandResult<&'a PlayerHandState> {
    let player = state
        .get_player(&cmd.player_root)
        .ok_or_else(|| CommandRejectedError::new("Player not in hand"))?;

    if cmd.amount <= 0 {
        return Err(CommandRejectedError::invalid_argument(
            "Amount must be positive",
        ));
    }

    Ok(player)
}

fn compute(cmd: &PostBlind, state: &HandState, player: &PlayerHandState) -> BlindPosted {
    let actual_amount = cmd.amount.min(player.stack);
    let new_stack = player.stack - actual_amount;
    let new_pot = state.total_pot() + actual_amount;

    BlindPosted {
        player_root: cmd.player_root.clone(),
        blind_type: cmd.blind_type.clone(),
        amount: actual_amount,
        player_stack: new_stack,
        pot_total: new_pot,
        posted_at: Some(angzarr_client::now()),
    }
}

pub fn handle_post_blind(
    cmd: PostBlind,
    state: &HandState,
    seq: u32,
) -> CommandResult<EventBook> {
        guard(state)?;
    let player = validate(&cmd, state)?;

    let event = compute(&cmd, state, player);
    let event_any = pack_event(&event, "examples.BlindPosted");

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
    use examples_proto::{CardsDealt, GameVariant, PlayerInHand};
    use prost::Message;

    fn dealt_state() -> HandState {
        let mut state = new_hand_state();
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
                remaining_deck: vec![],
            },
        );
        state
    }

    #[test]
    fn handle_post_blind_success_emits_blind_posted() {
        let state = dealt_state();
        let cmd = PostBlind {
            player_root: vec![1],
            blind_type: "small".into(),
            amount: 5,
        };
        let book = handle_post_blind(cmd, &state, 1).expect("ok");
        let any = match book.pages[0].payload.as_ref() {
            Some(Payload::Event(a)) => a,
            _ => panic!(),
        };
        assert!(any.type_url.ends_with("examples.BlindPosted"));
        let decoded = BlindPosted::decode(any.value.as_slice()).unwrap();
        assert_eq!(decoded.amount, 5);
        assert_eq!(decoded.player_stack, 495);
        assert_eq!(decoded.pot_total, 5);
    }

    #[test]
    fn handle_post_blind_rejects_when_hand_does_not_exist() {
        let state = new_hand_state();
        let cmd = PostBlind {
            player_root: vec![1],
            blind_type: "small".into(),
            amount: 5,
        };
        let err = handle_post_blind(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("does not exist"));
    }

    #[test]
    fn handle_post_blind_rejects_unknown_player() {
        let state = dealt_state();
        let cmd = PostBlind {
            player_root: vec![0xff],
            blind_type: "small".into(),
            amount: 5,
        };
        let err = handle_post_blind(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("Player not in hand"));
    }
}
