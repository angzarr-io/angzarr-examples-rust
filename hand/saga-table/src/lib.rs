//! Saga: Hand -> Table (library).
//!
//! Exports the saga handler for testing.

use angzarr_client::proto::{command_page, CommandBook, CommandPage, Cover, EventBook, Uuid};
use angzarr_client::{
    CommandRejectedError, CommandResult, Destinations, SagaDomainHandler, SagaHandlerResponse,
    UnpackAny,
};
use examples_proto::{EndHand, HandComplete, PotResult};
use prost::Message;
use prost_types::Any;

/// Saga handler for Hand -> Table domain translation.
#[derive(Clone)]
pub struct HandTableSagaHandler;

impl SagaDomainHandler for HandTableSagaHandler {
    fn event_types(&self) -> Vec<String> {
        vec!["HandComplete".into()]
    }

    fn handle(
        &self,
        source: &EventBook,
        event: &Any,
        _destinations: &Destinations,
    ) -> CommandResult<SagaHandlerResponse> {
        if event.type_url.ends_with("HandComplete") {
            return Self::handle_hand_complete(source, event);
        }
        Ok(SagaHandlerResponse::default())
    }
}

impl HandTableSagaHandler {
    /// Translate HandComplete -> EndHand.
    ///
    /// Commands use deferred sequences - framework assigns on delivery.
    pub fn handle_hand_complete(
        source: &EventBook,
        event_any: &Any,
    ) -> CommandResult<SagaHandlerResponse> {
        let event: HandComplete = event_any.unpack().map_err(|e| {
            CommandRejectedError::new(format!("Failed to decode HandComplete: {}", e))
        })?;

        // Get hand_root from source cover
        let hand_root = source
            .cover
            .as_ref()
            .and_then(|c| c.root.as_ref())
            .map(|u| u.value.clone())
            .unwrap_or_default();

        // Convert PotWinner to PotResult
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

        // Build EndHand command
        let end_hand = EndHand { hand_root, results };

        let command_any = Any {
            type_url: "type.googleapis.com/examples.EndHand".to_string(),
            value: end_hand.encode_to_vec(),
        };

        Ok(SagaHandlerResponse {
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
