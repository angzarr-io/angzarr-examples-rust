//! Tournament command handler implementing CommandHandlerDomainHandler.

use angzarr_client::proto::{CommandBook, EventBook, Notification};
use angzarr_client::{
    dispatch_command, CommandHandlerDomainHandler, CommandResult, RejectionHandlerResponse,
    StateRouter,
};
use prost_types::Any;

use crate::handlers;
use crate::state::{TournamentState, STATE_ROUTER};

/// Tournament command handler.
#[derive(Clone)]
pub struct TournamentHandler;

impl TournamentHandler {
    pub fn new() -> Self {
        Self
    }
}

impl Default for TournamentHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandHandlerDomainHandler for TournamentHandler {
    type State = TournamentState;

    fn command_types(&self) -> Vec<String> {
        vec![
            "CreateTournament".into(),
            "OpenRegistration".into(),
            "CloseRegistration".into(),
            "EnrollPlayer".into(),
            "ProcessRebuy".into(),
            "AdvanceBlindLevel".into(),
            "EliminatePlayer".into(),
            "PauseTournament".into(),
            "ResumeTournament".into(),
        ]
    }

    fn state_router(&self) -> &StateRouter<Self::State> {
        &STATE_ROUTER
    }

    fn handle(
        &self,
        cmd: &CommandBook,
        payload: &Any,
        state: &Self::State,
        seq: u32,
    ) -> CommandResult<EventBook> {
        dispatch_command!(payload, cmd, state, seq, {
            "CreateTournament" => handlers::handle_create_tournament,
            "OpenRegistration" => handlers::handle_open_registration,
            "CloseRegistration" => handlers::handle_close_registration,
            "EnrollPlayer" => handlers::handle_enroll_player,
            "ProcessRebuy" => handlers::handle_process_rebuy,
            "AdvanceBlindLevel" => handlers::handle_advance_blind_level,
            "EliminatePlayer" => handlers::handle_eliminate_player,
            "PauseTournament" => handlers::handle_pause_tournament,
            "ResumeTournament" => handlers::handle_resume_tournament,
        })
    }

    fn on_rejected(
        &self,
        _notification: &Notification,
        _state: &Self::State,
        _target_domain: &str,
        _target_command: &str,
    ) -> CommandResult<RejectionHandlerResponse> {
        // No cross-domain rejection handling yet
        Ok(RejectionHandlerResponse::default())
    }
}
