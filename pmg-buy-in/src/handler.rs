//! BuyInOrchestrator PM handler.
//!
//! Design Philosophy:
//! - PM is a coordinator, NOT a decision maker
//! - PM should NOT rebuild destination state to make business decisions
//! - Business logic belongs in aggregates (Table validates seating)
//! - PM receives only sequences for command stamping

use angzarr_client::proto::command_page::Payload as CommandPayload;
use angzarr_client::proto::{
    page_header::SequenceType, CommandBook, CommandPage, Cover, EventBook, MergeStrategy,
    PageHeader, Uuid as ProtoUuid,
};
use angzarr_client::{
    pack_event, CommandRejectedError, CommandResult, Destinations,
    ProcessManagerDomainHandler, ProcessManagerResponse, UnpackAny,
};
use examples_proto::{
    BuyInCompleted, BuyInFailed, BuyInInitiated, BuyInPhase, BuyInRequested, ConfirmBuyIn,
    Currency, OrchestrationFailure, PlayerSeated, ReleaseBuyIn, SeatPlayer, SeatingRejected,
};
use prost::Message;
use prost_types::Any;

use crate::state::BuyInState;

/// PM handler for buy-in orchestration.
#[derive(Clone)]
pub struct BuyInPmHandler;

impl ProcessManagerDomainHandler<BuyInState> for BuyInPmHandler {
    fn event_types(&self) -> Vec<String> {
        vec![
            "BuyInRequested".into(),  // Player domain
            "PlayerSeated".into(),    // Table domain
            "SeatingRejected".into(), // Table domain
        ]
    }

    fn prepare(
        &self,
        _trigger: &EventBook,
        _state: &BuyInState,
        event: &Any,
    ) -> Vec<angzarr_client::proto::Cover> {
        let type_url = &event.type_url;

        // BuyInRequested from Player → need Table sequence
        if type_url.ends_with("BuyInRequested") {
            if let Ok(evt) = event.unpack::<BuyInRequested>() {
                return vec![Cover {
                    domain: "table".to_string(),
                    root: Some(ProtoUuid {
                        value: evt.table_root,
                    }),
                    correlation_id: String::new(),
                    edition: None,
                }];
            }
        }

        // PlayerSeated or SeatingRejected from Table → need Player sequence
        if type_url.ends_with("PlayerSeated") {
            if let Ok(evt) = event.unpack::<PlayerSeated>() {
                return vec![Cover {
                    domain: "player".to_string(),
                    root: Some(ProtoUuid {
                        value: evt.player_root,
                    }),
                    correlation_id: String::new(),
                    edition: None,
                }];
            }
        }

        if type_url.ends_with("SeatingRejected") {
            if let Ok(evt) = event.unpack::<SeatingRejected>() {
                return vec![Cover {
                    domain: "player".to_string(),
                    root: Some(ProtoUuid {
                        value: evt.player_root,
                    }),
                    correlation_id: String::new(),
                    edition: None,
                }];
            }
        }

        vec![]
    }

    fn handle(
        &self,
        trigger: &EventBook,
        state: &BuyInState,
        event: &Any,
        destinations: &Destinations,
    ) -> CommandResult<ProcessManagerResponse> {
        let type_url = &event.type_url;

        if type_url.ends_with("BuyInRequested") {
            return self.handle_buy_in_requested(trigger, state, event, destinations);
        } else if type_url.ends_with("PlayerSeated") {
            return self.handle_player_seated(trigger, state, event, destinations);
        } else if type_url.ends_with("SeatingRejected") {
            return self.handle_seating_rejected(trigger, state, event, destinations);
        }

        Ok(ProcessManagerResponse::default())
    }
}

impl BuyInPmHandler {
    /// Handle BuyInRequested from Player domain.
    ///
    /// Sends SeatPlayer command to Table. Table aggregate validates and either
    /// accepts (emits PlayerSeated) or rejects (emits SeatingRejected).
    /// This follows the "facts over state rebuilding" philosophy.
    fn handle_buy_in_requested(
        &self,
        trigger: &EventBook,
        _state: &BuyInState,
        event_any: &Any,
        destinations: &Destinations,
    ) -> CommandResult<ProcessManagerResponse> {
        let event: BuyInRequested = event_any.unpack().map_err(|e| {
            CommandRejectedError::new(format!("Failed to decode BuyInRequested: {}", e))
        })?;

        // Get player root from trigger
        let player_root = trigger
            .cover
            .as_ref()
            .and_then(|c| c.root.as_ref())
            .map(|r| r.value.clone())
            .ok_or_else(|| CommandRejectedError::new("Missing player root in trigger"))?;

        // Get table sequence from destinations (no state rebuilding!)
        let table_next_seq = destinations
            .sequence_for("table")
            .ok_or_else(|| CommandRejectedError::new("Missing table sequence - check output_domains config"))?;

        // Send SeatPlayer command to Table - let aggregate validate
        let amount = event.amount.as_ref().map(|c| c.amount).unwrap_or(0);
        let seat_player = SeatPlayer {
            player_root: player_root.clone(),
            reservation_id: event.reservation_id.clone(),
            seat: event.seat,
            amount,
        };

        let table_root = event.table_root.clone();

        let command_book = make_command_book(
            "table",
            &table_root,
            "examples.SeatPlayer",
            &seat_player,
            table_next_seq,
        );

        // Emit PM event for tracking
        let pm_event = BuyInInitiated {
            player_root: player_root.clone(),
            table_root: table_root.clone(),
            reservation_id: event.reservation_id.clone(),
            seat: event.seat,
            amount: Some(Currency {
                amount,
                currency_code: "USD".to_string(),
            }),
            phase: BuyInPhase::BuyInSeating as i32,
            initiated_at: Some(angzarr_client::now()),
        };
        let pm_event_any = pack_event(&pm_event, "examples.BuyInInitiated");
        let pm_event_book = make_pm_event_book(pm_event_any);

        Ok(ProcessManagerResponse {
            commands: vec![command_book],
            process_events: Some(pm_event_book),
            facts: vec![],
        })
    }

    /// Handle PlayerSeated from Table domain.
    ///
    /// Emits ConfirmBuyIn to Player.
    fn handle_player_seated(
        &self,
        _trigger: &EventBook,
        _state: &BuyInState,
        event_any: &Any,
        destinations: &Destinations,
    ) -> CommandResult<ProcessManagerResponse> {
        let event: PlayerSeated = event_any.unpack().map_err(|e| {
            CommandRejectedError::new(format!("Failed to decode PlayerSeated: {}", e))
        })?;

        // Get player sequence from destinations (no state rebuilding!)
        let player_next_seq = destinations
            .sequence_for("player")
            .ok_or_else(|| CommandRejectedError::new("Missing player sequence - check output_domains config"))?;

        // Emit ConfirmBuyIn to Player
        let confirm = ConfirmBuyIn {
            reservation_id: event.reservation_id.clone(),
        };

        let player_root = event.player_root.clone();

        let command_book = make_command_book(
            "player",
            &player_root,
            "examples.ConfirmBuyIn",
            &confirm,
            player_next_seq,
        );

        // Emit PM completion event
        let pm_event = BuyInCompleted {
            player_root: player_root.clone(),
            table_root: vec![],
            reservation_id: event.reservation_id.clone(),
            seat: event.seat_position,
            amount: Some(Currency {
                amount: event.stack,
                currency_code: "USD".to_string(),
            }),
            completed_at: Some(angzarr_client::now()),
        };
        let pm_event_any = pack_event(&pm_event, "examples.BuyInCompleted");
        let pm_event_book = make_pm_event_book(pm_event_any);

        Ok(ProcessManagerResponse {
            commands: vec![command_book],
            process_events: Some(pm_event_book),
            facts: vec![],
        })
    }

    /// Handle SeatingRejected from Table domain.
    ///
    /// Emits ReleaseBuyIn to Player to release reserved funds.
    fn handle_seating_rejected(
        &self,
        _trigger: &EventBook,
        _state: &BuyInState,
        event_any: &Any,
        destinations: &Destinations,
    ) -> CommandResult<ProcessManagerResponse> {
        let event: SeatingRejected = event_any.unpack().map_err(|e| {
            CommandRejectedError::new(format!("Failed to decode SeatingRejected: {}", e))
        })?;

        // Get player sequence from destinations (no state rebuilding!)
        let player_next_seq = destinations
            .sequence_for("player")
            .ok_or_else(|| CommandRejectedError::new("Missing player sequence - check output_domains config"))?;

        // Emit ReleaseBuyIn to Player
        let release = ReleaseBuyIn {
            reservation_id: event.reservation_id.clone(),
            reason: event.reason.clone(),
        };

        let player_root = event.player_root.clone();

        let command_book = make_command_book(
            "player",
            &player_root,
            "examples.ReleaseBuyIn",
            &release,
            player_next_seq,
        );

        // Emit PM failure event
        let pm_event = BuyInFailed {
            player_root: player_root.clone(),
            table_root: vec![],
            reservation_id: event.reservation_id.clone(),
            failure: Some(OrchestrationFailure {
                code: "SEATING_REJECTED".to_string(),
                message: event.reason.clone(),
                failed_at_phase: "SEATING".to_string(),
                failed_at: Some(angzarr_client::now()),
            }),
        };
        let pm_event_any = pack_event(&pm_event, "examples.BuyInFailed");
        let pm_event_book = make_pm_event_book(pm_event_any);

        Ok(ProcessManagerResponse {
            commands: vec![command_book],
            process_events: Some(pm_event_book),
            facts: vec![],
        })
    }
}

/// Helper to create a CommandBook for PM commands.
fn make_command_book<M: Message>(
    domain: &str,
    root: &[u8],
    type_url: &str,
    message: &M,
    seq: u32,
) -> CommandBook {
    CommandBook {
        cover: Some(Cover {
            domain: domain.to_string(),
            root: Some(ProtoUuid {
                value: root.to_vec(),
            }),
            correlation_id: uuid::Uuid::new_v4().to_string(),
            edition: None,
        }),
        pages: vec![CommandPage {
            header: Some(PageHeader {
                sequence_type: Some(SequenceType::Sequence(seq)),
            }),
            merge_strategy: MergeStrategy::MergeCommutative as i32,
            payload: Some(CommandPayload::Command(Any {
                type_url: angzarr_client::type_url(type_url),
                value: message.encode_to_vec(),
            })),
        }],
    }
}

/// Helper to create a PM EventBook for process events.
fn make_pm_event_book(event: Any) -> EventBook {
    use angzarr_client::proto::event_page::Payload;
    use angzarr_client::proto::EventPage;

    EventBook {
        cover: None,
        pages: vec![EventPage {
            header: Some(PageHeader {
                sequence_type: Some(SequenceType::Sequence(0)),
            }),
            created_at: Some(angzarr_client::now()),
            committed: true,
            cascade_id: None,
            payload: Some(Payload::Event(event)),
        }],
        snapshot: None,
        next_sequence: 0,
    }
}
