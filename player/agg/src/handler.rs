//! Player command handler implementing CommandHandlerDomainHandler.

use angzarr_client::proto::{CommandBook, EventBook, Notification};
use angzarr_client::{
    dispatch_command, CommandHandlerDomainHandler, CommandResult, RejectionHandlerResponse,
    StateRouter,
};
use prost_types::Any;

use crate::handlers;
use crate::state::{PlayerState, STATE_ROUTER};

/// Player command handler.
#[derive(Clone)]
pub struct PlayerHandler;

impl PlayerHandler {
    pub fn new() -> Self {
        Self
    }
}

impl Default for PlayerHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandHandlerDomainHandler for PlayerHandler {
    type State = PlayerState;

    fn command_types(&self) -> Vec<String> {
        vec![
            "RegisterPlayer".into(),
            "DepositFunds".into(),
            "WithdrawFunds".into(),
            "ReserveFunds".into(),
            "ReleaseFunds".into(),
            // Buy-in orchestration
            "InitiateBuyIn".into(),
            "ConfirmBuyIn".into(),
            "ReleaseBuyIn".into(),
            // Registration orchestration
            "InitiateTournamentRegistration".into(),
            "ConfirmRegistrationFee".into(),
            "ReleaseRegistrationFee".into(),
            // Rebuy orchestration
            "InitiateRebuy".into(),
            "ConfirmRebuyFee".into(),
            "ReleaseRebuyFee".into(),
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
            "RegisterPlayer" => handlers::handle_register_player,
            "DepositFunds" => handlers::handle_deposit_funds,
            "WithdrawFunds" => handlers::handle_withdraw_funds,
            "ReserveFunds" => handlers::handle_reserve_funds,
            "ReleaseFunds" => handlers::handle_release_funds,
            // Buy-in orchestration
            "InitiateBuyIn" => handlers::handle_initiate_buy_in,
            "ConfirmBuyIn" => handlers::handle_confirm_buy_in,
            "ReleaseBuyIn" => handlers::handle_release_buy_in,
            // Registration orchestration
            "InitiateTournamentRegistration" => handlers::handle_initiate_tournament_registration,
            "ConfirmRegistrationFee" => handlers::handle_confirm_registration_fee,
            "ReleaseRegistrationFee" => handlers::handle_release_registration_fee,
            // Rebuy orchestration
            "InitiateRebuy" => handlers::handle_initiate_rebuy,
            "ConfirmRebuyFee" => handlers::handle_confirm_rebuy_fee,
            "ReleaseRebuyFee" => handlers::handle_release_rebuy_fee,
        })
    }

    fn on_rejected(
        &self,
        notification: &Notification,
        state: &Self::State,
        target_domain: &str,
        target_command: &str,
    ) -> CommandResult<RejectionHandlerResponse> {
        // Handle JoinTable rejection from table domain
        if target_domain == "table" && target_command.ends_with("JoinTable") {
            return handlers::handle_join_rejected(notification, state);
        }

        // Default: let framework handle
        Ok(RejectionHandlerResponse::default())
    }
}
