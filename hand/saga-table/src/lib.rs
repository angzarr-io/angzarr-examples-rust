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

