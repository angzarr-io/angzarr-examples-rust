//! RegistrationOrchestrator PM handler.

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
    ConfirmRegistrationFee, Currency, EnrollPlayer, OrchestrationFailure, RegistrationCompleted,
    RegistrationFailed, RegistrationInitiated, RegistrationPhase, RegistrationRequested,
    ReleaseRegistrationFee, TournamentEnrollmentRejected, TournamentPlayerEnrolled,
    TournamentState, TournamentStatus,
};
use prost::Message;
use prost_types::Any;

use crate::state::RegistrationState;

/// PM handler for registration orchestration.
#[derive(Clone)]
pub struct RegistrationPmHandler;

impl ProcessManagerDomainHandler<RegistrationState> for RegistrationPmHandler {
    fn event_types(&self) -> Vec<String> {
        vec![
            "RegistrationRequested".into(),        // Player domain
            "TournamentPlayerEnrolled".into(),     // Tournament domain
            "TournamentEnrollmentRejected".into(), // Tournament domain
        ]
    }

    fn prepare(
        &self,
        _trigger: &EventBook,
        _state: &RegistrationState,
        event: &Any,
    ) -> Vec<angzarr_client::proto::Cover> {
        let type_url = &event.type_url;

        // RegistrationRequested from Player → need Tournament state
        if type_url.ends_with("RegistrationRequested") {
            if let Ok(evt) = event.unpack::<RegistrationRequested>() {
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

        // TournamentPlayerEnrolled or TournamentEnrollmentRejected → need Player state
        if type_url.ends_with("TournamentPlayerEnrolled") {
            if let Ok(evt) = event.unpack::<TournamentPlayerEnrolled>() {
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

        if type_url.ends_with("TournamentEnrollmentRejected") {
            if let Ok(evt) = event.unpack::<TournamentEnrollmentRejected>() {
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
        state: &RegistrationState,
        event: &Any,
        destinations: &[EventBook],
    ) -> CommandResult<ProcessManagerResponse> {
        let type_url = &event.type_url;

        if type_url.ends_with("RegistrationRequested") {
            return self.handle_registration_requested(trigger, state, event, destinations);
        } else if type_url.ends_with("TournamentPlayerEnrolled") {
            return self.handle_player_enrolled(trigger, state, event, destinations);
        } else if type_url.ends_with("TournamentEnrollmentRejected") {
            return self.handle_enrollment_rejected(trigger, state, event, destinations);
        }

        Ok(ProcessManagerResponse::default())
    }
}

impl RegistrationPmHandler {
    /// Handle RegistrationRequested from Player domain.
    ///
    /// Validates Tournament state and emits EnrollPlayer command if valid.
    fn handle_registration_requested(
        &self,
        trigger: &EventBook,
        _state: &RegistrationState,
        event_any: &Any,
        destinations: &[EventBook],
    ) -> CommandResult<ProcessManagerResponse> {
        let event: RegistrationRequested = event_any.unpack().map_err(|e| {
            CommandRejectedError::new(format!("Failed to decode RegistrationRequested: {}", e))
        })?;

        // Get player root from trigger
        let player_root = trigger
            .cover
            .as_ref()
            .and_then(|c| c.root.as_ref())
            .map(|r| r.value.clone())
            .ok_or_else(|| CommandRejectedError::new("Missing player root in trigger"))?;

        // Get tournament EventBook from destinations
        let tournament_event_book = destinations
            .first()
            .ok_or_else(|| CommandRejectedError::new("Missing tournament destination"))?;

        // Rebuild tournament state to validate
        let tournament_state = rebuild_tournament_state(tournament_event_book)?;

        // Validate registration is open
        if tournament_state.status != TournamentStatus::TournamentRegistrationOpen {
            return self.emit_failure(
                &player_root,
                &event.tournament_root,
                &event.reservation_id,
                "REGISTRATION_CLOSED",
                "Tournament registration is not open".to_string(),
            );
        }

        // Validate capacity
        if tournament_state.registered_count >= tournament_state.max_players as usize {
            return self.emit_failure(
                &player_root,
                &event.tournament_root,
                &event.reservation_id,
                "TOURNAMENT_FULL",
                "Tournament is full".to_string(),
            );
        }

        // Check if player already registered
        let player_hex = hex::encode(&player_root);
        if tournament_state.registered_players.contains(&player_hex) {
            return self.emit_failure(
                &player_root,
                &event.tournament_root,
                &event.reservation_id,
                "ALREADY_REGISTERED",
                "Player is already registered for this tournament".to_string(),
            );
        }

        // All validations passed - emit EnrollPlayer command to Tournament
        let enroll_player = EnrollPlayer {
            player_root: player_root.clone(),
            reservation_id: event.reservation_id.clone(),
        };

        let tournament_root = event.tournament_root.clone();
        let tournament_next_seq = tournament_event_book.next_sequence();

        let command_book = make_command_book(
            "tournament",
            &tournament_root,
            "examples.EnrollPlayer",
            &enroll_player,
            tournament_next_seq,
        );

        // Emit PM event for tracking
        let fee = event.fee.as_ref().map(|c| c.amount).unwrap_or(0);
        let pm_event = RegistrationInitiated {
            player_root: player_root.clone(),
            tournament_root: tournament_root.clone(),
            reservation_id: event.reservation_id.clone(),
            fee: Some(Currency {
                amount: fee,
                currency_code: "USD".to_string(),
            }),
            phase: RegistrationPhase::RegistrationEnrolling as i32,
            initiated_at: Some(angzarr_client::now()),
        };
        let pm_event_any = pack_event(&pm_event, "examples.RegistrationInitiated");
        let pm_event_book = make_pm_event_book(pm_event_any);

        Ok(ProcessManagerResponse {
            commands: vec![command_book],
            process_events: Some(pm_event_book),
            facts: vec![],
        })
    }

    /// Handle TournamentPlayerEnrolled from Tournament domain.
    ///
    /// Emits ConfirmRegistrationFee to Player.
    fn handle_player_enrolled(
        &self,
        _trigger: &EventBook,
        _state: &RegistrationState,
        event_any: &Any,
        destinations: &[EventBook],
    ) -> CommandResult<ProcessManagerResponse> {
        let event: TournamentPlayerEnrolled = event_any.unpack().map_err(|e| {
            CommandRejectedError::new(format!("Failed to decode TournamentPlayerEnrolled: {}", e))
        })?;

        // Get player EventBook from destinations
        let player_event_book = destinations
            .first()
            .ok_or_else(|| CommandRejectedError::new("Missing player destination"))?;

        // Emit ConfirmRegistrationFee to Player
        let confirm = ConfirmRegistrationFee {
            reservation_id: event.reservation_id.clone(),
        };

        let player_root = event.player_root.clone();
        let player_next_seq = player_event_book.next_sequence();

        let command_book = make_command_book(
            "player",
            &player_root,
            "examples.ConfirmRegistrationFee",
            &confirm,
            player_next_seq,
        );

        // Emit PM completion event
        let pm_event = RegistrationCompleted {
            player_root: player_root.clone(),
            tournament_root: vec![],
            reservation_id: event.reservation_id.clone(),
            fee: Some(Currency {
                amount: event.fee_paid,
                currency_code: "USD".to_string(),
            }),
            starting_stack: event.starting_stack,
            completed_at: Some(angzarr_client::now()),
        };
        let pm_event_any = pack_event(&pm_event, "examples.RegistrationCompleted");
        let pm_event_book = make_pm_event_book(pm_event_any);

        Ok(ProcessManagerResponse {
            commands: vec![command_book],
            process_events: Some(pm_event_book),
            facts: vec![],
        })
    }

    /// Handle TournamentEnrollmentRejected from Tournament domain.
    ///
    /// Emits ReleaseRegistrationFee to Player.
    fn handle_enrollment_rejected(
        &self,
        _trigger: &EventBook,
        _state: &RegistrationState,
        event_any: &Any,
        destinations: &[EventBook],
    ) -> CommandResult<ProcessManagerResponse> {
        let event: TournamentEnrollmentRejected = event_any.unpack().map_err(|e| {
            CommandRejectedError::new(format!(
                "Failed to decode TournamentEnrollmentRejected: {}",
                e
            ))
        })?;

        // Get player EventBook from destinations
        let player_event_book = destinations
            .first()
            .ok_or_else(|| CommandRejectedError::new("Missing player destination"))?;

        // Emit ReleaseRegistrationFee to Player
        let release = ReleaseRegistrationFee {
            reservation_id: event.reservation_id.clone(),
            reason: event.reason.clone(),
        };

        let player_root = event.player_root.clone();
        let player_next_seq = player_event_book.next_sequence();

        let command_book = make_command_book(
            "player",
            &player_root,
            "examples.ReleaseRegistrationFee",
            &release,
            player_next_seq,
        );

        // Emit PM failure event
        let pm_event = RegistrationFailed {
            player_root: player_root.clone(),
            tournament_root: vec![],
            reservation_id: event.reservation_id.clone(),
            failure: Some(OrchestrationFailure {
                code: "ENROLLMENT_REJECTED".to_string(),
                message: event.reason.clone(),
                failed_at_phase: "ENROLLING".to_string(),
                failed_at: Some(angzarr_client::now()),
            }),
        };
        let pm_event_any = pack_event(&pm_event, "examples.RegistrationFailed");
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
        tournament_root: &[u8],
        reservation_id: &[u8],
        code: &str,
        message: String,
    ) -> CommandResult<ProcessManagerResponse> {
        let pm_event = RegistrationFailed {
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
        let pm_event_any = pack_event(&pm_event, "examples.RegistrationFailed");
        let pm_event_book = make_pm_event_book(pm_event_any);

        Ok(ProcessManagerResponse {
            commands: vec![],
            process_events: Some(pm_event_book),
            facts: vec![],
        })
    }
}

// Helper to rebuild tournament state from EventBook
fn rebuild_tournament_state(event_book: &EventBook) -> CommandResult<TournamentStateHelper> {
    use angzarr_client::UnpackAny;

    let mut state = TournamentStateHelper::default();

    // Check for snapshot first
    if let Some(snapshot) = &event_book.snapshot {
        if let Some(state_any) = &snapshot.state {
            if let Ok(proto_state) = state_any.unpack::<TournamentState>() {
                state.status = TournamentStatus::try_from(proto_state.status).unwrap_or_default();
                state.max_players = proto_state.max_players;
                state.buy_in = proto_state.buy_in;
                state.starting_stack = proto_state.starting_stack;
                state.registered_count = proto_state.registered_players.len();
                for (player_hex, _) in &proto_state.registered_players {
                    state.registered_players.insert(player_hex.clone());
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
                    state.max_players = evt.max_players;
                    state.buy_in = evt.buy_in;
                    state.starting_stack = evt.starting_stack;
                }
            } else if type_url.ends_with("RegistrationOpened") {
                state.status = TournamentStatus::TournamentRegistrationOpen;
            } else if type_url.ends_with("RegistrationClosed") {
                state.status = TournamentStatus::TournamentRunning;
            } else if type_url.ends_with("TournamentPlayerEnrolled") {
                if let Ok(evt) = event_any.unpack::<TournamentPlayerEnrolled>() {
                    let player_hex = hex::encode(&evt.player_root);
                    state.registered_players.insert(player_hex);
                    state.registered_count = state.registered_players.len();
                }
            } else if type_url.ends_with("PlayerUnregistered") {
                if let Ok(evt) = event_any.unpack::<examples_proto::PlayerUnregistered>() {
                    let player_hex = hex::encode(&evt.player_root);
                    state.registered_players.remove(&player_hex);
                    state.registered_count = state.registered_players.len();
                }
            } else if type_url.ends_with("TournamentStarted") {
                state.status = TournamentStatus::TournamentRunning;
            } else if type_url.ends_with("TournamentCompleted") {
                state.status = TournamentStatus::TournamentCompleted;
            } else if type_url.ends_with("TournamentPaused") {
                state.status = TournamentStatus::TournamentPaused;
            }
        }
    }

    Ok(state)
}

/// Minimal tournament state for PM validation.
#[derive(Default)]
struct TournamentStateHelper {
    status: TournamentStatus,
    max_players: i32,
    buy_in: i64,
    starting_stack: i64,
    registered_count: usize,
    registered_players: std::collections::HashSet<String>, // player_root hex
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
