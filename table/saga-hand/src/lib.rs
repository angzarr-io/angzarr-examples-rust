//! Saga: Table -> Hand (library).

use angzarr_client::proto::{command_page, CommandBook, CommandPage, Cover, SagaResponse, Uuid};
use angzarr_client::{saga, CommandResult};
use examples_proto::{DealCards, HandStarted, PlayerInHand};
use prost::Message;
use prost_types::Any;

/// Translate `table.HandStarted` into a `DealCards` command for `hand`.
pub struct TableHandSaga;

#[saga(name = "saga-table-hand", source = "table", target = "hand")]
impl TableHandSaga {
    #[handles(HandStarted)]
    pub fn on_hand_started(&self, event: HandStarted) -> CommandResult<SagaResponse> {
        let players: Vec<PlayerInHand> = event
            .active_players
            .iter()
            .map(|seat| PlayerInHand {
                player_root: seat.player_root.clone(),
                position: seat.position,
                stack: seat.stack,
                ..Default::default()
            })
            .collect();

        let deal_cards = DealCards {
            table_root: event.hand_root.clone(),
            hand_number: event.hand_number,
            game_variant: event.game_variant,
            players,
            dealer_position: event.dealer_position,
            small_blind: event.small_blind,
            big_blind: event.big_blind,
            // Propagate hand_root as deck_seed so the shuffle is reproducible
            // across runs — required for acceptance tests asserting specific
            // cards. hand_root = sha256(table_id, hand_n) is deterministic
            // per-hand.
            deck_seed: event.hand_root.clone(),
            ..Default::default()
        };

        let command_any = Any {
            type_url: angzarr_client::full_type_url::<DealCards>(),
            value: deal_cards.encode_to_vec(),
        };

        Ok(SagaResponse {
            commands: vec![CommandBook {
                cover: Some(Cover {
                    domain: "hand".to_string(),
                    root: Some(Uuid {
                        value: event.hand_root,
                    }),
                    ..Default::default()
                }),
                pages: vec![CommandPage {
                    payload: Some(command_page::Payload::Command(command_any)),
                    ..Default::default()
                }],
            }],
            events: vec![],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use angzarr_client::proto::command_page::Payload;
    use examples_proto::SeatSnapshot;

    fn seat(position: i32, player_root: Vec<u8>, stack: i64) -> SeatSnapshot {
        SeatSnapshot {
            position,
            player_root,
            stack,
        }
    }

    fn extract_command_any(book: &CommandBook) -> &Any {
        let page = book.pages.first().expect("command book has one page");
        match page.payload.as_ref().expect("payload set") {
            Payload::Command(any) => any,
            _ => panic!("expected inline Command payload"),
        }
    }

    #[test]
    fn command_propagates_hand_root_as_deck_seed() {
        let saga = TableHandSaga;
        let hand_root = vec![0x55];
        let response = saga
            .on_hand_started(HandStarted {
                hand_root: hand_root.clone(),
                hand_number: 8,
                dealer_position: 0,
                small_blind_position: 1,
                big_blind_position: 2,
                active_players: vec![seat(0, vec![0x77; 4], 100)],
                game_variant: 1,
                small_blind: 1,
                big_blind: 2,
                started_at: None,
                ..Default::default()
            })
            .expect("handler succeeds");

        let cmd_any = extract_command_any(&response.commands[0]);
        let deal = DealCards::decode(cmd_any.value.as_slice()).unwrap();
        assert_eq!(
            deal.deck_seed, hand_root,
            "deck_seed must equal hand_root for deterministic shuffles"
        );
    }
}
