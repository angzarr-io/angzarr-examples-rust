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
            deck_seed: vec![],
        };

        let command_any = Any {
            type_url: "type.googleapis.com/examples.DealCards".to_string(),
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
    fn emits_single_deal_cards_targeting_hand_domain() {
        let saga = TableHandSaga;
        let hand_root = vec![0xAB, 0xCD, 0xEF];
        let event = HandStarted {
            hand_root: hand_root.clone(),
            hand_number: 42,
            dealer_position: 3,
            small_blind_position: 4,
            big_blind_position: 5,
            active_players: vec![
                seat(3, vec![0x01; 8], 1_000),
                seat(4, vec![0x02; 8], 2_000),
                seat(5, vec![0x03; 8], 500),
            ],
            game_variant: 1, // TEXAS_HOLDEM
            small_blind: 10,
            big_blind: 20,
            started_at: None,
        };

        let response = saga
            .on_hand_started(event)
            .expect("handler succeeds");

        assert_eq!(response.commands.len(), 1);
        assert!(response.events.is_empty());

        let book = &response.commands[0];
        let cover = book.cover.as_ref().expect("cover set");
        assert_eq!(cover.domain, "hand");
        assert_eq!(cover.root.as_ref().expect("root set").value, hand_root);

        let cmd_any = extract_command_any(book);
        assert!(
            cmd_any.type_url.ends_with("examples.DealCards"),
            "type_url = {}",
            cmd_any.type_url
        );

        let deal = DealCards::decode(cmd_any.value.as_slice())
            .expect("decode DealCards");
        assert_eq!(deal.hand_number, 42);
        assert_eq!(deal.dealer_position, 3);
        assert_eq!(deal.small_blind, 10);
        assert_eq!(deal.big_blind, 20);
        assert_eq!(deal.game_variant, 1);
        // The saga copies `event.hand_root` into `table_root` on DealCards.
        assert_eq!(deal.table_root, hand_root);
        assert_eq!(deal.players.len(), 3);
        assert_eq!(deal.players[0].position, 3);
        assert_eq!(deal.players[0].player_root, vec![0x01_u8; 8]);
        assert_eq!(deal.players[0].stack, 1_000);
        assert_eq!(deal.players[2].player_root, vec![0x03_u8; 8]);
        assert_eq!(deal.players[2].stack, 500);
    }

    #[test]
    fn deals_with_no_active_players_still_emits_one_command() {
        let saga = TableHandSaga;
        let hand_root = vec![0x99];
        let response = saga
            .on_hand_started(HandStarted {
                hand_root: hand_root.clone(),
                hand_number: 1,
                dealer_position: 0,
                small_blind_position: 1,
                big_blind_position: 2,
                active_players: vec![],
                game_variant: 2,
                small_blind: 5,
                big_blind: 10,
                started_at: None,
            })
            .expect("handler succeeds");

        assert_eq!(response.commands.len(), 1);
        let book = &response.commands[0];
        assert_eq!(book.cover.as_ref().unwrap().domain, "hand");
        assert_eq!(
            book.cover.as_ref().unwrap().root.as_ref().unwrap().value,
            hand_root
        );

        let cmd_any = extract_command_any(book);
        assert!(cmd_any.type_url.ends_with("examples.DealCards"));
        let deal = DealCards::decode(cmd_any.value.as_slice()).unwrap();
        assert!(deal.players.is_empty());
        assert_eq!(deal.game_variant, 2);
    }

    #[test]
    fn command_has_no_deck_seed() {
        let saga = TableHandSaga;
        let response = saga
            .on_hand_started(HandStarted {
                hand_root: vec![0x55],
                hand_number: 8,
                dealer_position: 0,
                small_blind_position: 1,
                big_blind_position: 2,
                active_players: vec![seat(0, vec![0x77; 4], 100)],
                game_variant: 1,
                small_blind: 1,
                big_blind: 2,
                started_at: None,
            })
            .expect("handler succeeds");

        let cmd_any = extract_command_any(&response.commands[0]);
        let deal = DealCards::decode(cmd_any.value.as_slice()).unwrap();
        assert!(
            deal.deck_seed.is_empty(),
            "deck_seed should be empty for non-deterministic runs"
        );
    }
}
