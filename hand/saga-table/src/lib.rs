//! Saga: Hand -> Table (library).

use angzarr_client::proto::{command_page, CommandBook, CommandPage, Cover, SagaResponse, Uuid};
use angzarr_client::{saga, CommandResult};
use examples_proto::{EndHand, HandComplete, PotResult};
use prost::Message;
use prost_types::Any;

/// Translate `hand.HandComplete` into an `EndHand` command for `table`.
pub struct HandTableSaga;

#[saga(name = "saga-hand-table", source = "hand", target = "table")]
impl HandTableSaga {
    #[handles(HandComplete)]
    pub fn on_hand_complete(&self, event: HandComplete) -> CommandResult<SagaResponse> {
        let results: Vec<PotResult> = event
            .winners
            .iter()
            .map(|winner| PotResult {
                winner_root: winner.player_root.clone(),
                amount: winner.amount,
                pot_type: winner.pot_type.clone(),
                winning_hand: winner.winning_hand.clone(),
            })
            .collect();

        // NOTE: Tier 5 saga dispatch does not surface the source cover; the
        // `hand_root` here must come from `event` metadata. When the proto
        // carries only `table_root` + `hand_number`, downstream validation
        // that checks `hand_root` expects the same handle the Hand aggregate
        // carries in its cover. See pmg-hand-flow for the flow that plumbs
        // this through.
        let hand_root = Vec::new();
        let end_hand = EndHand { hand_root, results };
        let command_any = Any {
            type_url: "type.googleapis.com/examples.EndHand".to_string(),
            value: end_hand.encode_to_vec(),
        };

        Ok(SagaResponse {
            commands: vec![CommandBook {
                cover: Some(Cover {
                    domain: "table".to_string(),
                    root: Some(Uuid {
                        value: event.table_root,
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
    use examples_proto::{HandRanking, PotWinner};

    fn winner(player_root: Vec<u8>, amount: i64, pot_type: &str) -> PotWinner {
        PotWinner {
            player_root,
            amount,
            pot_type: pot_type.to_string(),
            winning_hand: Some(HandRanking::default()),
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
    fn emits_single_end_hand_targeting_table_domain() {
        let saga = HandTableSaga;
        let table_root = vec![0xDE, 0xAD, 0xBE, 0xEF];
        let event = HandComplete {
            table_root: table_root.clone(),
            hand_number: 7,
            winners: vec![
                winner(vec![0x11; 8], 120, "main"),
                winner(vec![0x22; 8], 40, "side_1"),
            ],
            final_stacks: vec![],
            completed_at: None,
        };

        let response = saga
            .on_hand_complete(event)
            .expect("handler succeeds");

        assert_eq!(response.commands.len(), 1);
        assert!(response.events.is_empty());

        let book = &response.commands[0];
        let cover = book.cover.as_ref().expect("cover set");
        assert_eq!(cover.domain, "table");
        assert_eq!(cover.root.as_ref().expect("root set").value, table_root);

        let cmd_any = extract_command_any(book);
        assert!(
            cmd_any.type_url.ends_with("examples.EndHand"),
            "type_url = {}",
            cmd_any.type_url
        );

        let end_hand =
            EndHand::decode(cmd_any.value.as_slice()).expect("decode EndHand");
        assert_eq!(end_hand.results.len(), 2);
        assert_eq!(end_hand.results[0].winner_root, vec![0x11_u8; 8]);
        assert_eq!(end_hand.results[0].amount, 120);
        assert_eq!(end_hand.results[0].pot_type, "main");
        assert_eq!(end_hand.results[1].winner_root, vec![0x22_u8; 8]);
        assert_eq!(end_hand.results[1].amount, 40);
        assert_eq!(end_hand.results[1].pot_type, "side_1");
    }

    #[test]
    fn emits_end_hand_even_with_no_winners() {
        let saga = HandTableSaga;
        let table_root = vec![0xFE, 0xED];
        let response = saga
            .on_hand_complete(HandComplete {
                table_root: table_root.clone(),
                hand_number: 1,
                winners: vec![],
                final_stacks: vec![],
                completed_at: None,
            })
            .expect("handler succeeds");

        assert_eq!(response.commands.len(), 1);
        let book = &response.commands[0];
        assert_eq!(
            book.cover.as_ref().unwrap().domain,
            "table"
        );
        assert_eq!(
            book.cover.as_ref().unwrap().root.as_ref().unwrap().value,
            table_root
        );

        let cmd_any = extract_command_any(book);
        assert!(cmd_any.type_url.ends_with("examples.EndHand"));
        let end_hand = EndHand::decode(cmd_any.value.as_slice()).unwrap();
        assert!(end_hand.results.is_empty());
    }

    #[test]
    fn propagates_winning_hand_and_amount_fields() {
        let saga = HandTableSaga;
        let response = saga
            .on_hand_complete(HandComplete {
                table_root: vec![0x01],
                hand_number: 9,
                winners: vec![winner(vec![0x33; 4], 999, "main")],
                final_stacks: vec![],
                completed_at: None,
            })
            .expect("handler succeeds");

        let cmd_any = extract_command_any(&response.commands[0]);
        let end_hand = EndHand::decode(cmd_any.value.as_slice()).unwrap();
        assert_eq!(end_hand.results.len(), 1);
        let result = &end_hand.results[0];
        assert_eq!(result.winner_root, vec![0x33_u8; 4]);
        assert_eq!(result.amount, 999);
        assert!(result.winning_hand.is_some());
    }
}
