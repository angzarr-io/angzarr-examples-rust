//! Saga: Table -> Hand (library).
//!
//! Exports the saga handler for testing.

use angzarr_client::proto::{command_page, CommandBook, CommandPage, Cover, EventBook, Uuid};
use angzarr_client::{
    CommandRejectedError, CommandResult, Destinations, SagaDomainHandler, SagaHandlerResponse,
    UnpackAny,
};
use examples_proto::{DealCards, HandStarted, PlayerInHand};
use prost::Message;
use prost_types::Any;

/// Saga handler for Table -> Hand domain translation.
#[derive(Clone)]
pub struct TableHandSagaHandler;

impl SagaDomainHandler for TableHandSagaHandler {
    fn event_types(&self) -> Vec<String> {
        vec!["HandStarted".into()]
    }

    fn handle(
        &self,
        source: &EventBook,
        event: &Any,
        _destinations: &Destinations,
    ) -> CommandResult<SagaHandlerResponse> {
        if event.type_url.ends_with("HandStarted") {
            return Self::handle_hand_started(source, event);
        }
        Ok(SagaHandlerResponse::default())
    }
}

impl TableHandSagaHandler {
    /// Translate HandStarted -> DealCards.
    ///
    /// Commands use deferred sequences - framework assigns on delivery.
    pub fn handle_hand_started(
        _source: &EventBook,
        event_any: &Any,
    ) -> CommandResult<SagaHandlerResponse> {
        let event: HandStarted = event_any.unpack().map_err(|e| {
            CommandRejectedError::new(format!("Failed to decode HandStarted: {}", e))
        })?;

        // Convert SeatSnapshot to PlayerInHand
        let players: Vec<PlayerInHand> = event
            .active_players
            .iter()
            .map(|seat| PlayerInHand {
                player_root: seat.player_root.clone(),
                position: seat.position,
                stack: seat.stack,
            })
            .collect();

        // Build DealCards command
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

        Ok(SagaHandlerResponse {
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
