//! Process Manager: Hand Flow
//!
//! Orchestrates the flow of poker hands by:
//! 1. Subscribing to table and hand domain events
//! 2. Managing hand process state machines
//! 3. Sending commands to drive hands forward
//!
//! This example was originally macro-based (OO pattern) but has been converted
//! to manual implementation to work with the updated Destinations API.

use angzarr_client::proto::{Cover, EventBook, Uuid};
use angzarr_client::{
    run_process_manager_server, CommandRejectedError, CommandResult, Destinations,
    ProcessManagerDomainHandler, ProcessManagerResponse, ProcessManagerRouter, UnpackAny,
};
use examples_proto::{
    ActionTaken, BlindPosted, CardsDealt, CommunityCardsDealt, HandStarted, PotAwarded,
};
use prost_types::Any;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// The PM's aggregate state (rebuilt from its own events).
#[derive(Clone, Default)]
pub struct PMState {
    /// Current hand being tracked (if any).
    pub hand_root: Option<Vec<u8>>,
    /// Whether the hand is in progress.
    pub hand_in_progress: bool,
}

/// Hand Flow Process Manager handler.
struct HandFlowPmHandler;

impl ProcessManagerDomainHandler<PMState> for HandFlowPmHandler {
    fn event_types(&self) -> Vec<String> {
        vec![
            "HandStarted".into(),
            "CardsDealt".into(),
            "BlindPosted".into(),
            "ActionTaken".into(),
            "CommunityCardsDealt".into(),
            "PotAwarded".into(),
        ]
    }

    fn prepare(&self, _trigger: &EventBook, _state: &PMState, event: &Any) -> Vec<Cover> {
        if event.type_url.ends_with("HandStarted") {
            if let Ok(evt) = event.unpack::<HandStarted>() {
                return vec![Cover {
                    domain: "hand".to_string(),
                    root: Some(Uuid {
                        value: evt.hand_root,
                    }),
                    ..Default::default()
                }];
            }
        }
        vec![]
    }

    fn handle(
        &self,
        _trigger: &EventBook,
        _state: &PMState,
        event: &Any,
        _destinations: &Destinations,
    ) -> CommandResult<ProcessManagerResponse> {
        let type_url = &event.type_url;

        if type_url.ends_with("HandStarted") {
            return self.handle_hand_started(event);
        } else if type_url.ends_with("CardsDealt") {
            return self.handle_cards_dealt(event);
        } else if type_url.ends_with("BlindPosted") {
            return self.handle_blind_posted(event);
        } else if type_url.ends_with("ActionTaken") {
            return self.handle_action_taken(event);
        } else if type_url.ends_with("CommunityCardsDealt") {
            return self.handle_community_dealt(event);
        } else if type_url.ends_with("PotAwarded") {
            return self.handle_pot_awarded(event);
        }

        Ok(ProcessManagerResponse::default())
    }
}

impl HandFlowPmHandler {
    fn handle_hand_started(&self, event_any: &Any) -> CommandResult<ProcessManagerResponse> {
        let _event: HandStarted = event_any.unpack().map_err(|e| {
            CommandRejectedError::new(format!("Failed to decode HandStarted: {}", e))
        })?;
        // Initialize hand process (not persisted in this simplified version)
        // The saga-table-hand will send DealCards, so we don't emit commands here.
        Ok(ProcessManagerResponse::default())
    }

    fn handle_cards_dealt(&self, event_any: &Any) -> CommandResult<ProcessManagerResponse> {
        let _event: CardsDealt = event_any.unpack().map_err(|e| {
            CommandRejectedError::new(format!("Failed to decode CardsDealt: {}", e))
        })?;
        // Post small blind command
        // In a real implementation, we'd track state to know which blind to post.
        Ok(ProcessManagerResponse::default())
    }

    fn handle_blind_posted(&self, event_any: &Any) -> CommandResult<ProcessManagerResponse> {
        let _event: BlindPosted = event_any.unpack().map_err(|e| {
            CommandRejectedError::new(format!("Failed to decode BlindPosted: {}", e))
        })?;
        // In a full implementation, we'd check if both blinds are posted
        // and then start the betting round.
        Ok(ProcessManagerResponse::default())
    }

    fn handle_action_taken(&self, event_any: &Any) -> CommandResult<ProcessManagerResponse> {
        let _event: ActionTaken = event_any.unpack().map_err(|e| {
            CommandRejectedError::new(format!("Failed to decode ActionTaken: {}", e))
        })?;
        // In a full implementation, we'd check if betting is complete
        // and advance to the next phase.
        Ok(ProcessManagerResponse::default())
    }

    fn handle_community_dealt(&self, event_any: &Any) -> CommandResult<ProcessManagerResponse> {
        let _event: CommunityCardsDealt = event_any.unpack().map_err(|e| {
            CommandRejectedError::new(format!("Failed to decode CommunityCardsDealt: {}", e))
        })?;
        // Start new betting round after community cards.
        Ok(ProcessManagerResponse::default())
    }

    fn handle_pot_awarded(&self, event_any: &Any) -> CommandResult<ProcessManagerResponse> {
        let _event: PotAwarded = event_any.unpack().map_err(|e| {
            CommandRejectedError::new(format!("Failed to decode PotAwarded: {}", e))
        })?;
        // Hand is complete. Clean up.
        Ok(ProcessManagerResponse::default())
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    println!("Starting Hand Flow process manager");

    let router = ProcessManagerRouter::new("hand-flow", "hand-flow", |_| PMState::default())
        .domain("table", HandFlowPmHandler)
        .domain("hand", HandFlowPmHandler);

    run_process_manager_server("hand-flow", 50092, router)
        .await
        .expect("Server failed");
}
