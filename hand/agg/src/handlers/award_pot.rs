//! AwardPot command handler.

use angzarr_client::proto::EventBook;
use angzarr_client::{CommandRejectedError, CommandResult};
use examples_utils::{event_page, pack_event, rejected};
use examples_proto::{AwardPot, HandComplete, PlayerStackSnapshot, PotAwarded, PotWinner};

use crate::state::HandState;

fn guard(state: &HandState) -> CommandResult<()> {
    if !state.exists() {
        return Err(rejected("Hand not dealt"));
    }
    if state.is_complete() {
        return Err(rejected("Hand already complete"));
    }
    Ok(())
}

fn validate(cmd: &AwardPot, state: &HandState) -> CommandResult<()> {
    if cmd.awards.is_empty() {
        return Err(rejected("No awards specified"));
    }

    let mut total_awarded = 0i64;
    for award in &cmd.awards {
        let player = state
            .get_player(&award.player_root)
            .ok_or_else(|| rejected("Award to player not in hand"))?;

        if player.has_folded {
            return Err(rejected("Folded player cannot win"));
        }
        total_awarded += award.amount;
    }

    if total_awarded > state.total_pot() {
        return Err(rejected("Awards exceed pot total"));
    }

    Ok(())
}

fn compute(cmd: &AwardPot, state: &HandState) -> (PotAwarded, HandComplete) {
    let pot_total = state.total_pot();
    let awarded_total: i64 = cmd.awards.iter().map(|a| a.amount).sum();
    let adjustment = pot_total - awarded_total;

    let winners: Vec<PotWinner> = cmd
        .awards
        .iter()
        .enumerate()
        .map(|(i, award)| {
            let amount = if i == 0 {
                award.amount + adjustment
            } else {
                award.amount
            };
            PotWinner {
                player_root: award.player_root.clone(),
                amount,
                pot_type: award.pot_type.clone(),
                winning_hand: None,
            }
        })
        .collect();

    let now = angzarr_client::now();

    let final_stacks: Vec<PlayerStackSnapshot> = state
        .players
        .values()
        .map(|player| {
            let mut final_stack = player.stack;
            for winner in &winners {
                if winner.player_root == player.player_root {
                    final_stack += winner.amount;
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

pub fn handle_award_pot(cmd: AwardPot, state: &HandState, seq: u32) -> CommandResult<EventBook> {
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
