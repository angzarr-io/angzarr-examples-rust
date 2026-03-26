//! RebuyOrchestrator PM handler.
//!
//! Design Philosophy:
//! - PM is a coordinator, NOT a decision maker
//! - PM should NOT rebuild destination state to make business decisions
//! - Business logic belongs in aggregates (Tournament validates rebuy eligibility)
//! - PM receives only sequences for command stamping

use angzarr_client::proto::command_page::Payload as CommandPayload;
use angzarr_client::proto::{
    page_header::SequenceType, CommandBook, CommandPage, Cover, EventBook, EventPage,
    MergeStrategy, PageHeader, Uuid as ProtoUuid,
};
use angzarr_client::{
    pack_event, CommandRejectedError, CommandResult, Destinations, ProcessManagerDomainHandler,
    ProcessManagerResponse, UnpackAny,
};
use examples_proto::{
    AddRebuyChips, ConfirmRebuyFee, Currency, OrchestrationFailure, ProcessRebuy, RebuyChipsAdded,
    RebuyCompleted, RebuyDenied, RebuyFailed, RebuyInitiated, RebuyPhase, RebuyProcessed,
    RebuyRequested, ReleaseRebuyFee,
};
use prost::Message;
use prost_types::Any;

use crate::state::RebuyState;

/// PM handler for rebuy orchestration.
#[derive(Clone)]
pub struct RebuyPmHandler;

impl ProcessManagerDomainHandler<RebuyState> for RebuyPmHandler {
    fn event_types(&self) -> Vec<String> {
        vec![
            "RebuyRequested".into(),  // Player domain
            "RebuyProcessed".into(),  // Tournament domain
            "RebuyDenied".into(),     // Tournament domain
            "RebuyChipsAdded".into(), // Table domain
        ]
    }

    fn prepare(
        &self,
        _trigger: &EventBook,
        _state: &RebuyState,
        event: &Any,
    ) -> Vec<angzarr_client::proto::Cover> {
        let type_url = &event.type_url;

        // RebuyRequested from Player → need Tournament sequence
        if type_url.ends_with("RebuyRequested") {
            if let Ok(evt) = event.unpack::<RebuyRequested>() {
                return vec![Cover {
                    domain: "tournament".to_string(),
                    root: Some(ProtoUuid {
                        value: evt.tournament_root,
                    }),
                    correlation_id: String::new(),
                    edition: None,
                }];
            }
        }

        // RebuyProcessed → need Table sequence (to add chips)
        if type_url.ends_with("RebuyProcessed") {
            if let Ok(_evt) = event.unpack::<RebuyProcessed>() {
                // Table root is stored in PM state from RebuyInitiated
                return vec![];
            }
        }

        // RebuyDenied → need Player sequence
        if type_url.ends_with("RebuyDenied") {
            if let Ok(evt) = event.unpack::<RebuyDenied>() {
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

        // RebuyChipsAdded → need Player sequence
        if type_url.ends_with("RebuyChipsAdded") {
            if let Ok(evt) = event.unpack::<RebuyChipsAdded>() {
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
        state: &RebuyState,
        event: &Any,
        destinations: &Destinations,
    ) -> CommandResult<ProcessManagerResponse> {
        let type_url = &event.type_url;

        if type_url.ends_with("RebuyRequested") {
            return self.handle_rebuy_requested(trigger, state, event, destinations);
        } else if type_url.ends_with("RebuyProcessed") {
            return self.handle_rebuy_processed(trigger, state, event, destinations);
        } else if type_url.ends_with("RebuyDenied") {
            return self.handle_rebuy_denied(trigger, state, event, destinations);
        } else if type_url.ends_with("RebuyChipsAdded") {
            return self.handle_chips_added(trigger, state, event, destinations);
        }

        Ok(ProcessManagerResponse::default())
    }
}

impl RebuyPmHandler {
    /// Handle RebuyRequested from Player domain.
    ///
    /// Sends ProcessRebuy command to Tournament. Tournament aggregate validates
    /// rebuy eligibility and either accepts (emits RebuyProcessed) or denies
    /// (emits RebuyDenied). This follows the "facts over state rebuilding" philosophy.
    fn handle_rebuy_requested(
        &self,
        trigger: &EventBook,
        _state: &RebuyState,
        event_any: &Any,
        destinations: &Destinations,
    ) -> CommandResult<ProcessManagerResponse> {
        let event: RebuyRequested = event_any.unpack().map_err(|e| {
            CommandRejectedError::new(format!("Failed to decode RebuyRequested: {}", e))
        })?;

        // Get player root from trigger
        let player_root = trigger
            .cover
            .as_ref()
            .and_then(|c| c.root.as_ref())
            .map(|r| r.value.clone())
            .ok_or_else(|| CommandRejectedError::new("Missing player root in trigger"))?;

        // Get tournament sequence from destinations (no state rebuilding!)
        let tournament_next_seq = destinations
            .sequence_for("tournament")
            .ok_or_else(|| {
                CommandRejectedError::new(
                    "Missing tournament sequence - check output_domains config",
                )
            })?;

        // Send ProcessRebuy command to Tournament - let aggregate validate
        let process_rebuy = ProcessRebuy {
            player_root: player_root.clone(),
            reservation_id: event.reservation_id.clone(),
        };

        let tournament_root = event.tournament_root.clone();

        let command_book = make_command_book(
            "tournament",
            &tournament_root,
            "examples.ProcessRebuy",
            &process_rebuy,
            tournament_next_seq,
        );

        // Emit PM event for tracking (includes table_root and seat for later phases)
        let fee = event.fee.as_ref().map(|c| c.amount).unwrap_or(0);
        let pm_event = RebuyInitiated {
            player_root: player_root.clone(),
            tournament_root: tournament_root.clone(),
            table_root: event.table_root.clone(),
            reservation_id: event.reservation_id.clone(),
            seat: event.seat,
            fee: Some(Currency {
                amount: fee,
                currency_code: "USD".to_string(),
            }),
            chips_to_add: 0, // Will be set by Tournament
            phase: RebuyPhase::RebuyApproving as i32,
            initiated_at: Some(angzarr_client::now()),
        };
        let pm_event_any = pack_event(&pm_event, "examples.RebuyInitiated");
        let pm_event_book = make_pm_event_book(pm_event_any);

        Ok(ProcessManagerResponse {
            commands: vec![command_book],
            process_events: Some(pm_event_book),
            facts: vec![],
        })
    }

    /// Handle RebuyProcessed from Tournament domain.
    ///
    /// Emits AddRebuyChips to Table.
    fn handle_rebuy_processed(
        &self,
        _trigger: &EventBook,
        state: &RebuyState,
        event_any: &Any,
        destinations: &Destinations,
    ) -> CommandResult<ProcessManagerResponse> {
        let event: RebuyProcessed = event_any.unpack().map_err(|e| {
            CommandRejectedError::new(format!("Failed to decode RebuyProcessed: {}", e))
        })?;

        // Get table sequence from destinations
        let table_next_seq = destinations.sequence_for("table").unwrap_or(0);

        // Add chips to table - table_root and seat come from PM state
        let add_chips = AddRebuyChips {
            player_root: event.player_root.clone(),
            reservation_id: event.reservation_id.clone(),
            seat: state.seat,
            amount: event.chips_added,
        };

        let table_root = state.table_root.clone();

        let command_book = make_command_book(
            "table",
            &table_root,
            "examples.AddRebuyChips",
            &add_chips,
            table_next_seq,
        );

        Ok(ProcessManagerResponse {
            commands: vec![command_book],
            process_events: None,
            facts: vec![],
        })
    }

    /// Handle RebuyDenied from Tournament domain.
    ///
    /// Emits ReleaseRebuyFee to Player.
    fn handle_rebuy_denied(
        &self,
        _trigger: &EventBook,
        _state: &RebuyState,
        event_any: &Any,
        destinations: &Destinations,
    ) -> CommandResult<ProcessManagerResponse> {
        let event: RebuyDenied = event_any.unpack().map_err(|e| {
            CommandRejectedError::new(format!("Failed to decode RebuyDenied: {}", e))
        })?;

        // Get player sequence from destinations (no state rebuilding!)
        let player_next_seq = destinations.sequence_for("player").ok_or_else(|| {
            CommandRejectedError::new("Missing player sequence - check output_domains config")
        })?;

        // Emit ReleaseRebuyFee to Player
        let release = ReleaseRebuyFee {
            reservation_id: event.reservation_id.clone(),
            reason: event.reason.clone(),
        };

        let player_root = event.player_root.clone();

        let command_book = make_command_book(
            "player",
            &player_root,
            "examples.ReleaseRebuyFee",
            &release,
            player_next_seq,
        );

        // Emit PM failure event
        let pm_event = RebuyFailed {
            player_root: player_root.clone(),
            tournament_root: vec![],
            reservation_id: event.reservation_id.clone(),
            failure: Some(OrchestrationFailure {
                code: "REBUY_DENIED".to_string(),
                message: event.reason.clone(),
                failed_at_phase: "APPROVING".to_string(),
                failed_at: Some(angzarr_client::now()),
            }),
        };
        let pm_event_any = pack_event(&pm_event, "examples.RebuyFailed");
        let pm_event_book = make_pm_event_book(pm_event_any);

        Ok(ProcessManagerResponse {
            commands: vec![command_book],
            process_events: Some(pm_event_book),
            facts: vec![],
        })
    }

    /// Handle RebuyChipsAdded from Table domain.
    ///
    /// Emits ConfirmRebuyFee to Player to finalize.
    fn handle_chips_added(
        &self,
        _trigger: &EventBook,
        _state: &RebuyState,
        event_any: &Any,
        destinations: &Destinations,
    ) -> CommandResult<ProcessManagerResponse> {
        let event: RebuyChipsAdded = event_any.unpack().map_err(|e| {
            CommandRejectedError::new(format!("Failed to decode RebuyChipsAdded: {}", e))
        })?;

        // Get player sequence from destinations (no state rebuilding!)
        let player_next_seq = destinations.sequence_for("player").ok_or_else(|| {
            CommandRejectedError::new("Missing player sequence - check output_domains config")
        })?;

        // Emit ConfirmRebuyFee to Player
        let confirm = ConfirmRebuyFee {
            reservation_id: event.reservation_id.clone(),
        };

        let player_root = event.player_root.clone();

        let command_book = make_command_book(
            "player",
            &player_root,
            "examples.ConfirmRebuyFee",
            &confirm,
            player_next_seq,
        );

        // Emit PM completion event
        let pm_event = RebuyCompleted {
            player_root: player_root.clone(),
            tournament_root: vec![],
            table_root: vec![],
            reservation_id: event.reservation_id.clone(),
            chips_added: event.amount,
            fee: Some(Currency {
                amount: event.amount,
                currency_code: "USD".to_string(),
            }),
            completed_at: Some(angzarr_client::now()),
        };
        let pm_event_any = pack_event(&pm_event, "examples.RebuyCompleted");
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
