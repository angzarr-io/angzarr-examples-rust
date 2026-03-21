//! RebuyOrchestrator PM handler.

use angzarr_client::proto::command_page::Payload as CommandPayload;
use angzarr_client::proto::event_page::Payload as EventPayload;
use angzarr_client::proto::{
    page_header::SequenceType, CommandBook, CommandPage, Cover, EventBook, EventPage,
    MergeStrategy, PageHeader, Uuid as ProtoUuid,
};
use angzarr_client::{
    pack_event, CommandRejectedError, CommandResult, EventBookExt, ProcessManagerDomainHandler,
    ProcessManagerResponse, UnpackAny,
};
use examples_proto::{
    AddRebuyChips, ConfirmRebuyFee, Currency, OrchestrationFailure, ProcessRebuy, RebuyChipsAdded,
    RebuyCompleted, RebuyDenied, RebuyFailed, RebuyInitiated, RebuyPhase, RebuyProcessed,
    RebuyRequested, ReleaseRebuyFee, TournamentState, TournamentStatus,
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

        // RebuyRequested from Player → need Tournament + Table state
        if type_url.ends_with("RebuyRequested") {
            if let Ok(evt) = event.unpack::<RebuyRequested>() {
                return vec![
                    Cover {
                        domain: "tournament".to_string(),
                        root: Some(ProtoUuid {
                            value: evt.tournament_root,
                        }),
                        correlation_id: String::new(),
                        edition: None,
                    },
                    Cover {
                        domain: "table".to_string(),
                        root: Some(ProtoUuid {
                            value: evt.table_root,
                        }),
                        correlation_id: String::new(),
                        edition: None,
                    },
                ];
            }
        }

        // RebuyProcessed → need Table + Player state
        if type_url.ends_with("RebuyProcessed") {
            if let Ok(evt) = event.unpack::<RebuyProcessed>() {
                return vec![
                    // We need to track the player somehow - for now use the PM state
                    Cover {
                        domain: "player".to_string(),
                        root: Some(ProtoUuid {
                            value: evt.player_root.clone(),
                        }),
                        correlation_id: String::new(),
                        edition: None,
                    },
                ];
            }
        }

        // RebuyDenied → need Player state
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

        // RebuyChipsAdded → need Player state
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
        destinations: &[EventBook],
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
    /// Validates Tournament + Table state and emits ProcessRebuy command if valid.
    fn handle_rebuy_requested(
        &self,
        trigger: &EventBook,
        _state: &RebuyState,
        event_any: &Any,
        destinations: &[EventBook],
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

        // Get Tournament and Table EventBooks from destinations
        if destinations.len() < 2 {
            return Err(CommandRejectedError::new(
                "Missing tournament or table destination",
            ));
        }

        let tournament_event_book = &destinations[0];
        let table_event_book = &destinations[1];

        // Rebuild states for validation
        let tournament_state = rebuild_tournament_state(tournament_event_book)?;
        let table_state = rebuild_table_state(table_event_book)?;

        // Validate tournament is running
        if tournament_state.status != TournamentStatus::TournamentRunning {
            return self.emit_failure(
                &player_root,
                &event.tournament_root,
                &event.reservation_id,
                "TOURNAMENT_NOT_RUNNING",
                "Tournament is not in progress".to_string(),
            );
        }

        // Validate rebuy is enabled
        if !tournament_state.rebuy_enabled {
            return self.emit_failure(
                &player_root,
                &event.tournament_root,
                &event.reservation_id,
                "REBUY_NOT_ENABLED",
                "Rebuys are not enabled for this tournament".to_string(),
            );
        }

        // Validate rebuy window (level cutoff)
        if tournament_state.current_level > tournament_state.rebuy_level_cutoff {
            return self.emit_failure(
                &player_root,
                &event.tournament_root,
                &event.reservation_id,
                "REBUY_WINDOW_CLOSED",
                format!(
                    "Rebuy window closed after level {}",
                    tournament_state.rebuy_level_cutoff
                ),
            );
        }

        // Validate player is registered
        let player_hex = hex::encode(&player_root);
        let player_registration = tournament_state.registered_players.get(&player_hex);
        if player_registration.is_none() {
            return self.emit_failure(
                &player_root,
                &event.tournament_root,
                &event.reservation_id,
                "NOT_REGISTERED",
                "Player is not registered for this tournament".to_string(),
            );
        }

        let reg = player_registration.unwrap();

        // Validate rebuy count
        if tournament_state.max_rebuys > 0 && reg.rebuys_used >= tournament_state.max_rebuys {
            return self.emit_failure(
                &player_root,
                &event.tournament_root,
                &event.reservation_id,
                "MAX_REBUYS_REACHED",
                format!(
                    "Maximum rebuys ({}) already used",
                    tournament_state.max_rebuys
                ),
            );
        }

        // Validate player is seated at table
        let seat_opt = table_state.find_seat_by_player(&player_root);
        if seat_opt.is_none() {
            return self.emit_failure(
                &player_root,
                &event.tournament_root,
                &event.reservation_id,
                "NOT_SEATED",
                "Player is not seated at the table".to_string(),
            );
        }

        let seat_pos = *seat_opt.unwrap();
        if seat_pos != event.seat {
            return self.emit_failure(
                &player_root,
                &event.tournament_root,
                &event.reservation_id,
                "SEAT_MISMATCH",
                "Seat position does not match".to_string(),
            );
        }

        // Validate stack threshold
        let current_stack = table_state.get_stack(&player_root).unwrap_or(0);
        if current_stack > tournament_state.stack_threshold {
            return self.emit_failure(
                &player_root,
                &event.tournament_root,
                &event.reservation_id,
                "STACK_TOO_HIGH",
                format!(
                    "Stack {} exceeds rebuy threshold {}",
                    current_stack, tournament_state.stack_threshold
                ),
            );
        }

        // All validations passed - emit ProcessRebuy command to Tournament
        let process_rebuy = ProcessRebuy {
            player_root: player_root.clone(),
            reservation_id: event.reservation_id.clone(),
        };

        let tournament_root = event.tournament_root.clone();
        let tournament_next_seq = tournament_event_book.next_sequence();

        let command_book = make_command_book(
            "tournament",
            &tournament_root,
            "examples.ProcessRebuy",
            &process_rebuy,
            tournament_next_seq,
        );

        // Emit PM event for tracking
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
            chips_to_add: tournament_state.rebuy_chips,
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
        _destinations: &[EventBook],
    ) -> CommandResult<ProcessManagerResponse> {
        let event: RebuyProcessed = event_any.unpack().map_err(|e| {
            CommandRejectedError::new(format!("Failed to decode RebuyProcessed: {}", e))
        })?;

        // We need table_root and seat from PM state (set during RebuyInitiated)
        // For now, assume we can get it from the event or state
        let add_chips = AddRebuyChips {
            player_root: event.player_root.clone(),
            reservation_id: event.reservation_id.clone(),
            seat: state.seat, // From PM state
            amount: event.chips_added,
        };

        // Note: We need table_root from PM state
        let table_root = state.table_root.clone();
        // We don't have the table EventBook here, so use sequence 0 (MergeCommutative will handle)
        let command_book = make_command_book(
            "table",
            &table_root,
            "examples.AddRebuyChips",
            &add_chips,
            0, // Using 0 with commutative merge
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
        destinations: &[EventBook],
    ) -> CommandResult<ProcessManagerResponse> {
        let event: RebuyDenied = event_any.unpack().map_err(|e| {
            CommandRejectedError::new(format!("Failed to decode RebuyDenied: {}", e))
        })?;

        // Get player EventBook from destinations
        let player_event_book = destinations
            .first()
            .ok_or_else(|| CommandRejectedError::new("Missing player destination"))?;

        // Emit ReleaseRebuyFee to Player
        let release = ReleaseRebuyFee {
            reservation_id: event.reservation_id.clone(),
            reason: event.reason.clone(),
        };

        let player_root = event.player_root.clone();
        let player_next_seq = player_event_book.next_sequence();

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
    /// Emits ConfirmRebuyFee to Player.
    fn handle_chips_added(
        &self,
        _trigger: &EventBook,
        state: &RebuyState,
        event_any: &Any,
        destinations: &[EventBook],
    ) -> CommandResult<ProcessManagerResponse> {
        let event: RebuyChipsAdded = event_any.unpack().map_err(|e| {
            CommandRejectedError::new(format!("Failed to decode RebuyChipsAdded: {}", e))
        })?;

        // Get player EventBook from destinations
        let player_event_book = destinations
            .first()
            .ok_or_else(|| CommandRejectedError::new("Missing player destination"))?;

        // Emit ConfirmRebuyFee to Player
        let confirm = ConfirmRebuyFee {
            reservation_id: event.reservation_id.clone(),
        };

        let player_root = event.player_root.clone();
        let player_next_seq = player_event_book.next_sequence();

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
            tournament_root: state.tournament_root.clone(),
            table_root: state.table_root.clone(),
            reservation_id: event.reservation_id.clone(),
            fee: Some(Currency {
                amount: state.fee,
                currency_code: "USD".to_string(),
            }),
            chips_added: event.amount,
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

    /// Emit a failure response.
    fn emit_failure(
        &self,
        player_root: &[u8],
        tournament_root: &[u8],
        reservation_id: &[u8],
        code: &str,
        message: String,
    ) -> CommandResult<ProcessManagerResponse> {
        let pm_event = RebuyFailed {
            player_root: player_root.to_vec(),
            tournament_root: tournament_root.to_vec(),
            reservation_id: reservation_id.to_vec(),
            failure: Some(OrchestrationFailure {
                code: code.to_string(),
                message,
                failed_at_phase: "VALIDATION".to_string(),
                failed_at: Some(angzarr_client::now()),
            }),
        };
        let pm_event_any = pack_event(&pm_event, "examples.RebuyFailed");
        let pm_event_book = make_pm_event_book(pm_event_any);

        Ok(ProcessManagerResponse {
            commands: vec![],
            process_events: Some(pm_event_book),
            facts: vec![],
        })
    }
}

// Helper to rebuild tournament state
fn rebuild_tournament_state(event_book: &EventBook) -> CommandResult<TournamentStateHelper> {
    use angzarr_client::UnpackAny;

    let mut state = TournamentStateHelper::default();

    // Check for snapshot
    if let Some(snapshot) = &event_book.snapshot {
        if let Some(state_any) = &snapshot.state {
            if let Ok(proto_state) = state_any.unpack::<TournamentState>() {
                state.status = TournamentStatus::try_from(proto_state.status).unwrap_or_default();
                if let Some(rebuy_config) = &proto_state.rebuy_config {
                    state.rebuy_enabled = rebuy_config.enabled;
                    state.max_rebuys = rebuy_config.max_rebuys;
                    state.rebuy_level_cutoff = rebuy_config.rebuy_level_cutoff;
                    state.stack_threshold = rebuy_config.stack_threshold;
                    state.rebuy_chips = rebuy_config.rebuy_chips;
                }
                state.current_level = proto_state.current_level;
                for (player_hex, reg) in &proto_state.registered_players {
                    state.registered_players.insert(
                        player_hex.clone(),
                        PlayerRegistrationHelper {
                            rebuys_used: reg.rebuys_used,
                        },
                    );
                }
            }
        }
    }

    // Apply events
    for page in &event_book.pages {
        if let Some(EventPayload::Event(event_any)) = &page.payload {
            let type_url = &event_any.type_url;

            if type_url.ends_with("TournamentCreated") {
                if let Ok(evt) = event_any.unpack::<examples_proto::TournamentCreated>() {
                    state.status = TournamentStatus::TournamentCreated;
                    if let Some(rebuy_config) = &evt.rebuy_config {
                        state.rebuy_enabled = rebuy_config.enabled;
                        state.max_rebuys = rebuy_config.max_rebuys;
                        state.rebuy_level_cutoff = rebuy_config.rebuy_level_cutoff;
                        state.stack_threshold = rebuy_config.stack_threshold;
                        state.rebuy_chips = rebuy_config.rebuy_chips;
                    }
                }
            } else if type_url.ends_with("TournamentStarted") {
                state.status = TournamentStatus::TournamentRunning;
            } else if type_url.ends_with("TournamentPlayerEnrolled") {
                if let Ok(evt) = event_any.unpack::<examples_proto::TournamentPlayerEnrolled>() {
                    let player_hex = hex::encode(&evt.player_root);
                    state
                        .registered_players
                        .insert(player_hex, PlayerRegistrationHelper { rebuys_used: 0 });
                }
            } else if type_url.ends_with("RebuyProcessed") {
                if let Ok(evt) = event_any.unpack::<RebuyProcessed>() {
                    let player_hex = hex::encode(&evt.player_root);
                    if let Some(reg) = state.registered_players.get_mut(&player_hex) {
                        reg.rebuys_used = evt.rebuy_count;
                    }
                }
            } else if type_url.ends_with("BlindLevelAdvanced") {
                if let Ok(evt) = event_any.unpack::<examples_proto::BlindLevelAdvanced>() {
                    state.current_level = evt.level;
                }
            }
        }
    }

    Ok(state)
}

// Helper to rebuild table state
fn rebuild_table_state(event_book: &EventBook) -> CommandResult<TableStateHelper> {
    use angzarr_client::UnpackAny;

    let mut state = TableStateHelper::default();

    // Apply events
    for page in &event_book.pages {
        if let Some(EventPayload::Event(event_any)) = &page.payload {
            let type_url = &event_any.type_url;

            if type_url.ends_with("PlayerJoined") {
                if let Ok(evt) = event_any.unpack::<examples_proto::PlayerJoined>() {
                    state
                        .seats
                        .insert(evt.seat_position, (evt.player_root, evt.stack));
                }
            } else if type_url.ends_with("PlayerSeated") {
                if let Ok(evt) = event_any.unpack::<examples_proto::PlayerSeated>() {
                    state
                        .seats
                        .insert(evt.seat_position, (evt.player_root, evt.stack));
                }
            } else if type_url.ends_with("PlayerLeft") {
                if let Ok(evt) = event_any.unpack::<examples_proto::PlayerLeft>() {
                    state.seats.remove(&evt.seat_position);
                }
            } else if type_url.ends_with("RebuyChipsAdded") {
                if let Ok(evt) = event_any.unpack::<RebuyChipsAdded>() {
                    if let Some((_, stack)) = state.seats.get_mut(&evt.seat) {
                        *stack = evt.new_stack;
                    }
                }
            }
        }
    }

    Ok(state)
}

/// Minimal tournament state for PM validation.
#[derive(Default)]
struct TournamentStateHelper {
    status: TournamentStatus,
    rebuy_enabled: bool,
    max_rebuys: i32,
    rebuy_level_cutoff: i32,
    stack_threshold: i64,
    rebuy_chips: i64,
    current_level: i32,
    registered_players: std::collections::HashMap<String, PlayerRegistrationHelper>,
}

#[derive(Default, Clone)]
struct PlayerRegistrationHelper {
    rebuys_used: i32,
}

/// Minimal table state for PM validation.
#[derive(Default)]
struct TableStateHelper {
    seats: std::collections::HashMap<i32, (Vec<u8>, i64)>, // position -> (player_root, stack)
}

impl TableStateHelper {
    fn find_seat_by_player(&self, player_root: &[u8]) -> Option<&i32> {
        for (pos, (root, _)) in &self.seats {
            if root == player_root {
                return Some(pos);
            }
        }
        None
    }

    fn get_stack(&self, player_root: &[u8]) -> Option<i64> {
        for (root, stack) in self.seats.values() {
            if root == player_root {
                return Some(*stack);
            }
        }
        None
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
            payload: Some(Payload::Event(event)),
        }],
        snapshot: None,
        next_sequence: 0,
    }
}
