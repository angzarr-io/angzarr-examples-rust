//! AwardPot command handler.

use angzarr_client::proto::EventBook;
use angzarr_client::{event_page, pack_event, CommandRejectedError, CommandResult};
use examples_proto::{AwardPot, HandComplete, PlayerStackSnapshot, PotAwarded, PotWinner};

use crate::state::HandState;

fn guard(state: &HandState) -> CommandResult<()> {
    if !state.exists() {
        return Err(CommandRejectedError::new("Hand does not exist"));
    }
    if state.is_complete() {
        return Err(CommandRejectedError::new("Hand already complete"));
    }
    Ok(())
}

fn validate(cmd: &AwardPot, state: &HandState) -> CommandResult<()> {
    if cmd.awards.is_empty() {
        return Err(CommandRejectedError::new("No awards specified"));
    }

    let mut total_awarded = 0i64;
    for award in &cmd.awards {
        let player = state
            .get_player(&award.player_root)
            .ok_or_else(|| CommandRejectedError::new("Award to player not in hand"))?;

        if player.has_folded {
            return Err(CommandRejectedError::new("Cannot award to folded player"));
        }
        total_awarded += award.amount;
    }

    if total_awarded > state.total_pot() {
        return Err(CommandRejectedError::new("Awards exceed pot total"));
    }

    Ok(())
}

fn compute(cmd: &AwardPot, state: &HandState) -> (PotAwarded, HandComplete) {
    let winners: Vec<PotWinner> = cmd
        .awards
        .iter()
        .map(|award| PotWinner {
            player_root: award.player_root.clone(),
            amount: award.amount,
            pot_type: award.pot_type.clone(),
            winning_hand: None,
        })
        .collect();

    let now = angzarr_client::now();

    let final_stacks: Vec<PlayerStackSnapshot> = state
        .players
        .values()
        .map(|player| {
            let mut final_stack = player.stack;
            for award in &cmd.awards {
                if award.player_root == player.player_root {
                    final_stack += award.amount;
                }
            }
            PlayerStackSnapshot {
                player_root: player.player_root.clone(),
                stack: final_stack,
                is_all_in: player.is_all_in,
                has_folded: player.has_folded,
            }
        })
        .collect();

    let pot_awarded = PotAwarded {
        winners: winners.clone(),
        awarded_at: Some(now),
    };

    let hand_complete = HandComplete {
        table_root: state.table_root.clone(),
        hand_number: state.hand_number,
        winners,
        final_stacks,
        completed_at: Some(now),
    };

    (pot_awarded, hand_complete)
}

pub fn handle_award_pot(
    cmd: AwardPot,
    state: &HandState,
    seq: u32,
) -> CommandResult<EventBook> {
    guard(state)?;
    validate(&cmd, state)?;

    let (pot_awarded, hand_complete) = compute(&cmd, state);
    let events = [
        pack_event(&pot_awarded, "examples.PotAwarded"),
        pack_event(&hand_complete, "examples.HandComplete"),
    ];

    Ok(EventBook {
        pages: events
            .into_iter()
            .enumerate()
            .map(|(i, ev)| event_page(seq + i as u32, ev))
            .collect(),
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{apply_blind_posted, apply_cards_dealt, new_hand_state};
    use angzarr_client::proto::event_page::Payload;
    use examples_proto::{BlindPosted, CardsDealt, GameVariant, PlayerInHand};
    use prost::Message;

    fn funded_pot_state() -> HandState {
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
        // seed the pot with 100 chips via a blind
        apply_blind_posted(
            &mut state,
            BlindPosted {
                player_root: vec![1],
                blind_type: "small".into(),
                amount: 100,
                player_stack: 400,
                pot_total: 100,
                posted_at: None,
            },
        );
        state
    }

    #[test]
    fn handle_award_pot_success_emits_pot_awarded_and_hand_complete() {
        let state = funded_pot_state();
        let cmd = AwardPot {
            awards: vec![examples_proto::PotAward {
                player_root: vec![1],
                amount: 100,
                pot_type: "main".into(),
            }],
        };
        let book = handle_award_pot(cmd, &state, 5).expect("ok");
        assert_eq!(book.pages.len(), 2);
        let first = match book.pages[0].payload.as_ref() {
            Some(Payload::Event(a)) => a,
            _ => panic!(),
        };
        let second = match book.pages[1].payload.as_ref() {
            Some(Payload::Event(a)) => a,
            _ => panic!(),
        };
        assert!(first.type_url.ends_with("examples.PotAwarded"));
        assert!(second.type_url.ends_with("examples.HandComplete"));
        let pa = PotAwarded::decode(first.value.as_slice()).unwrap();
        assert_eq!(pa.winners.len(), 1);
        assert_eq!(pa.winners[0].amount, 100);
        let hc = HandComplete::decode(second.value.as_slice()).unwrap();
        assert_eq!(hc.hand_number, 1);
    }

    #[test]
    fn handle_award_pot_rejects_empty_awards() {
        let state = funded_pot_state();
        let cmd = AwardPot { awards: vec![] };
        let err = handle_award_pot(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("No awards"));
    }

    #[test]
    fn handle_award_pot_rejects_when_hand_does_not_exist() {
        let state = new_hand_state();
        let cmd = AwardPot {
            awards: vec![examples_proto::PotAward {
                player_root: vec![1],
                amount: 100,
                pot_type: "main".into(),
            }],
        };
        let err = handle_award_pot(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("does not exist"));
    }
}
