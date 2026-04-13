//! Saga: Hand -> Player (library).
//!
//! Exports the saga handler for testing.

use angzarr_client::proto::{command_page, CommandBook, CommandPage, Cover, EventBook, Uuid};
use angzarr_client::{
    CommandRejectedError, CommandResult, Destinations, SagaDomainHandler, SagaHandlerResponse,
    UnpackAny,
};
use examples_proto::{Currency, DepositFunds, PotAwarded};
use prost::Message;
use prost_types::Any;

/// Saga handler for Hand -> Player domain translation.
#[derive(Clone)]
pub struct HandPlayerSagaHandler;

impl SagaDomainHandler for HandPlayerSagaHandler {
    fn event_types(&self) -> Vec<String> {
        vec!["PotAwarded".into()]
    }

    fn handle(
        &self,
        source: &EventBook,
        event: &Any,
        _destinations: &Destinations,
    ) -> CommandResult<SagaHandlerResponse> {
        if event.type_url.ends_with("PotAwarded") {
            return Self::handle_pot_awarded(source, event);
        }
        Ok(SagaHandlerResponse::default())
    }
}

impl HandPlayerSagaHandler {
    /// Translate PotAwarded -> DepositFunds for each winner.
    ///
    /// Commands use deferred sequences - framework assigns on delivery.
    pub fn handle_pot_awarded(
        source: &EventBook,
        event_any: &Any,
    ) -> CommandResult<SagaHandlerResponse> {
        let event: PotAwarded = event_any.unpack().map_err(|e| {
            CommandRejectedError::new(format!("Failed to decode PotAwarded: {}", e))
        })?;

        // Get correlation ID from source
        let correlation_id = source
            .cover
            .as_ref()
            .map(|c| c.correlation_id.clone())
            .unwrap_or_default();

        // Create DepositFunds commands for each winner
        let commands: Vec<CommandBook> = event
            .winners
            .iter()
            .map(|winner| {
                let deposit_funds = DepositFunds {
                    amount: Some(Currency {
                        amount: winner.amount,
                        currency_code: String::new(),
                    }),
                };

                let command_any = Any {
                    type_url: "type.googleapis.com/examples.DepositFunds".to_string(),
                    value: deposit_funds.encode_to_vec(),
                };

                CommandBook {
                    cover: Some(Cover {
                        domain: "player".to_string(),
                        root: Some(Uuid {
                            value: winner.player_root.clone(),
                        }),
                        correlation_id: correlation_id.clone(),
                        ..Default::default()
                    }),
                    pages: vec![CommandPage {
                        payload: Some(command_page::Payload::Command(command_any)),
                        ..Default::default()
                    }],
                }
            })
            .collect();

        Ok(SagaHandlerResponse {
            commands,
            events: vec![],
        })
    }
}
