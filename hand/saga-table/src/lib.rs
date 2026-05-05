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
    pub fn on_hand_complete(
        &self,
        event: HandComplete,
        source_cover: Option<Cover>,
    ) -> CommandResult<SagaResponse> {
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

        // `hand_root` is the originating Hand aggregate's UUID — extracted
        // from the source EventBook's cover.root by the saga macro. Mirrors
        // Python's `hand_root = source_cover.proto().root.value` pattern.
        let hand_root = source_cover
            .as_ref()
            .and_then(|c| c.root.as_ref())
            .map(|r| r.value.clone())
            .unwrap_or_default();
        let end_hand = EndHand { hand_root, results };
        let command_any = Any {
            type_url: angzarr_client::full_type_url::<EndHand>(),
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
