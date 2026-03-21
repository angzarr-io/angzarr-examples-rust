//! BuyInOrchestrator PM handler.

use angzarr_client::proto::command_page::Payload as CommandPayload;
use angzarr_client::proto::event_page::Payload as EventPayload;
use angzarr_client::proto::{
    page_header::SequenceType, CommandBook, CommandPage, Cover, EventBook, MergeStrategy,
    PageHeader, Uuid as ProtoUuid,
};
use angzarr_client::{
    pack_event, CommandRejectedError, CommandResult, EventBookExt, ProcessManagerDomainHandler,
    ProcessManagerResponse, UnpackAny,
};
use examples_proto::{
    BuyInCompleted, BuyInFailed, BuyInInitiated, BuyInPhase, BuyInRequested, ConfirmBuyIn,
    Currency, OrchestrationFailure, PlayerSeated, ReleaseBuyIn, SeatPlayer, SeatingRejected,
    TableState,
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

        // BuyInRequested from Player → need Table state
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

        // PlayerSeated or SeatingRejected from Table → need Player state
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
        destinations: &[EventBook],
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
    /// Validates Table state and emits SeatPlayer command if valid.
    fn handle_buy_in_requested(
        &self,
        trigger: &EventBook,
        _state: &BuyInState,
        event_any: &Any,
        destinations: &[EventBook],
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

        // Get table EventBook from destinations
        let table_event_book = destinations
            .first()
            .ok_or_else(|| CommandRejectedError::new("Missing table destination"))?;

        // Rebuild table state to validate
        let table_state = rebuild_table_state(table_event_book)?;

        // Validate buy-in amount
        let amount = event.amount.as_ref().map(|c| c.amount).unwrap_or(0);
        if amount < table_state.min_buy_in {
            return self.emit_failure(
                &player_root,
                &event.table_root,
                &event.reservation_id,
                "INVALID_AMOUNT",
                format!("Buy-in must be at least {}", table_state.min_buy_in),
            );
        }
        if amount > table_state.max_buy_in {
            return self.emit_failure(
                &player_root,
                &event.table_root,
                &event.reservation_id,
                "INVALID_AMOUNT",
                format!("Buy-in must be at most {}", table_state.max_buy_in),
            );
        }

        // Validate seat availability
        let requested_seat = event.seat;
        if requested_seat >= 0 {
            // Specific seat requested
            if requested_seat >= table_state.max_players {
                return self.emit_failure(
                    &player_root,
                    &event.table_root,
                    &event.reservation_id,
                    "INVALID_SEAT",
                    format!("Seat {} does not exist", requested_seat),
                );
            }
            if table_state.seats.contains_key(&requested_seat) {
                return self.emit_failure(
                    &player_root,
                    &event.table_root,
                    &event.reservation_id,
                    "SEAT_OCCUPIED",
                    format!("Seat {} is already occupied", requested_seat),
                );
            }
        } else {
            // Any seat - check if table has space
            if table_state.next_available_seat().is_none() {
                return self.emit_failure(
                    &player_root,
                    &event.table_root,
                    &event.reservation_id,
                    "TABLE_FULL",
                    "Table is full".to_string(),
                );
            }
        }

        // Check if player already seated
        if table_state.find_seat_by_player(&player_root).is_some() {
            return self.emit_failure(
                &player_root,
                &event.table_root,
                &event.reservation_id,
                "ALREADY_SEATED",
                "Player is already seated at this table".to_string(),
            );
        }

        // All validations passed - emit SeatPlayer command to Table
        let seat_player = SeatPlayer {
            player_root: player_root.clone(),
            reservation_id: event.reservation_id.clone(),
            seat: event.seat,
            amount,
        };

        let table_root = event.table_root.clone();
        let table_next_seq = table_event_book.next_sequence();

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

        // Create PM event book
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
        destinations: &[EventBook],
    ) -> CommandResult<ProcessManagerResponse> {
        let event: PlayerSeated = event_any.unpack().map_err(|e| {
            CommandRejectedError::new(format!("Failed to decode PlayerSeated: {}", e))
        })?;

        // Get player EventBook from destinations
        let player_event_book = destinations
            .first()
            .ok_or_else(|| CommandRejectedError::new("Missing player destination"))?;

        // Emit ConfirmBuyIn to Player
        let confirm = ConfirmBuyIn {
            reservation_id: event.reservation_id.clone(),
        };

        let player_root = event.player_root.clone();
        let player_next_seq = player_event_book.next_sequence();

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
            table_root: vec![], // We don't have table_root in PlayerSeated, could track in PM state
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
        destinations: &[EventBook],
    ) -> CommandResult<ProcessManagerResponse> {
        let event: SeatingRejected = event_any.unpack().map_err(|e| {
            CommandRejectedError::new(format!("Failed to decode SeatingRejected: {}", e))
        })?;

        // Get player EventBook from destinations
        let player_event_book = destinations
            .first()
            .ok_or_else(|| CommandRejectedError::new("Missing player destination"))?;

        // Emit ReleaseBuyIn to Player
        let release = ReleaseBuyIn {
            reservation_id: event.reservation_id.clone(),
            reason: event.reason.clone(),
        };

        let player_root = event.player_root.clone();
        let player_next_seq = player_event_book.next_sequence();

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

    /// Emit a failure response (no commands, just PM failure event).
    fn emit_failure(
        &self,
        player_root: &[u8],
        table_root: &[u8],
        reservation_id: &[u8],
        code: &str,
        message: String,
    ) -> CommandResult<ProcessManagerResponse> {
        let pm_event = BuyInFailed {
            player_root: player_root.to_vec(),
            table_root: table_root.to_vec(),
            reservation_id: reservation_id.to_vec(),
            failure: Some(OrchestrationFailure {
                code: code.to_string(),
                message,
                failed_at_phase: "VALIDATION".to_string(),
                failed_at: Some(angzarr_client::now()),
            }),
        };
        let pm_event_any = pack_event(&pm_event, "examples.BuyInFailed");
        let pm_event_book = make_pm_event_book(pm_event_any);

        Ok(ProcessManagerResponse {
            commands: vec![],
            process_events: Some(pm_event_book),
            facts: vec![],
        })
    }
}

// Helper to rebuild table state from EventBook
fn rebuild_table_state(event_book: &EventBook) -> CommandResult<TableStateHelper> {
    use angzarr_client::UnpackAny;

    let mut state = TableStateHelper::default();

    // Check for snapshot first
    if let Some(snapshot) = &event_book.snapshot {
        if let Some(state_any) = &snapshot.state {
            if let Ok(proto_state) = state_any.unpack::<TableState>() {
                state.table_id = proto_state.table_id;
                state.table_name = proto_state.table_name;
                state.min_buy_in = proto_state.min_buy_in;
                state.max_buy_in = proto_state.max_buy_in;
                state.max_players = proto_state.max_players;
                for seat in &proto_state.seats {
                    state.seats.insert(seat.position, seat.player_root.clone());
                }
            }
        }
    }

    // Apply events
    for page in &event_book.pages {
        if let Some(EventPayload::Event(event_any)) = &page.payload {
            let type_url = &event_any.type_url;

            if type_url.ends_with("TableCreated") {
                if let Ok(evt) = event_any.unpack::<examples_proto::TableCreated>() {
                    state.table_id = format!("table_{}", evt.table_name);
                    state.table_name = evt.table_name;
                    state.min_buy_in = evt.min_buy_in;
                    state.max_buy_in = evt.max_buy_in;
                    state.max_players = evt.max_players;
                }
            } else if type_url.ends_with("PlayerJoined") {
                if let Ok(evt) = event_any.unpack::<examples_proto::PlayerJoined>() {
                    state.seats.insert(evt.seat_position, evt.player_root);
                }
            } else if type_url.ends_with("PlayerSeated") {
                if let Ok(evt) = event_any.unpack::<PlayerSeated>() {
                    state.seats.insert(evt.seat_position, evt.player_root);
                }
            } else if type_url.ends_with("PlayerLeft") {
                if let Ok(evt) = event_any.unpack::<examples_proto::PlayerLeft>() {
                    state.seats.remove(&evt.seat_position);
                }
            }
        }
    }

    Ok(state)
}

/// Minimal table state for PM validation.
#[derive(Default)]
struct TableStateHelper {
    table_id: String,
    table_name: String,
    min_buy_in: i64,
    max_buy_in: i64,
    max_players: i32,
    seats: std::collections::HashMap<i32, Vec<u8>>, // position -> player_root
}

impl TableStateHelper {
    fn find_seat_by_player(&self, player_root: &[u8]) -> Option<i32> {
        for (pos, root) in &self.seats {
            if root == player_root {
                return Some(*pos);
            }
        }
        None
    }

    fn next_available_seat(&self) -> Option<i32> {
        (0..self.max_players).find(|i| !self.seats.contains_key(i))
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
        cover: None, // PM cover is set by framework
        pages: vec![EventPage {
            header: Some(PageHeader {
                sequence_type: Some(SequenceType::Sequence(0)), // Framework sets correct seq
            }),
            created_at: Some(angzarr_client::now()),
            payload: Some(Payload::Event(event)),
        }],
        snapshot: None,
        next_sequence: 0,
    }
}
