//! RegistrationOrchestrator PM handler.

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
    ConfirmRegistrationFee, Currency, EnrollPlayer, OrchestrationFailure, RegistrationCompleted,
    RegistrationFailed, RegistrationInitiated, RegistrationPhase, RegistrationRequested,
    ReleaseRegistrationFee, TournamentEnrollmentRejected, TournamentPlayerEnrolled,
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
        destinations: &Destinations,
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
    /// Sends EnrollPlayer command to Tournament. Tournament aggregate validates
    /// and either accepts (emits TournamentPlayerEnrolled) or rejects.
    /// This follows the "facts over state rebuilding" philosophy - PM doesn't
    /// rebuild destination state to make business decisions.
    fn handle_registration_requested(
        &self,
        trigger: &EventBook,
        _state: &RegistrationState,
        event_any: &Any,
        destinations: &Destinations,
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

        // Get tournament sequence from destinations (no state rebuilding!)
        let tournament_next_seq = destinations
            .sequence_for("tournament")
            .ok_or_else(|| CommandRejectedError::new("Missing tournament sequence - check output_domains config"))?;

        // Send EnrollPlayer command to Tournament - let aggregate validate
        let enroll_player = EnrollPlayer {
            player_root: player_root.clone(),
            reservation_id: event.reservation_id.clone(),
        };

        let tournament_root = event.tournament_root.clone();

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
        destinations: &Destinations,
    ) -> CommandResult<ProcessManagerResponse> {
        let event: TournamentPlayerEnrolled = event_any.unpack().map_err(|e| {
            CommandRejectedError::new(format!("Failed to decode TournamentPlayerEnrolled: {}", e))
        })?;

        // Get player sequence from destinations (no state rebuilding!)
        let player_next_seq = destinations
            .sequence_for("player")
            .ok_or_else(|| CommandRejectedError::new("Missing player sequence - check output_domains config"))?;

        // Emit ConfirmRegistrationFee to Player
        let confirm = ConfirmRegistrationFee {
            reservation_id: event.reservation_id.clone(),
        };

        let player_root = event.player_root.clone();

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
        destinations: &Destinations,
    ) -> CommandResult<ProcessManagerResponse> {
        let event: TournamentEnrollmentRejected = event_any.unpack().map_err(|e| {
            CommandRejectedError::new(format!(
                "Failed to decode TournamentEnrollmentRejected: {}",
                e
            ))
        })?;

        // Get player sequence from destinations (no state rebuilding!)
        let player_next_seq = destinations
            .sequence_for("player")
            .ok_or_else(|| CommandRejectedError::new("Missing player sequence - check output_domains config"))?;

        // Emit ReleaseRegistrationFee to Player
        let release = ReleaseRegistrationFee {
            reservation_id: event.reservation_id.clone(),
            reason: event.reason.clone(),
        };

        let player_root = event.player_root.clone();

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
            committed: true, // PM events are immediately committed
            cascade_id: None,
            payload: Some(Payload::Event(event)),
        }],
        snapshot: None,
        next_sequence: 0,
    }
}
