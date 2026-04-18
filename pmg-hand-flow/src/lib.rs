//! Hand Flow Process Manager library.
//!
//! Observes events from table and hand domains to track lifecycle state. The
//! actual command chaining is handled by the dedicated sagas
//! (saga-table-hand, saga-hand-table); this PM's job is to persist phase
//! transitions as its own process events so downstream tooling can inspect
//! them.

use angzarr_client::proto::ProcessManagerHandleResponse;
use angzarr_client::{process_manager, CommandResult};
use examples_proto::{
    ActionTaken, BlindPosted, CardsDealt, CommunityCardsDealt, HandComplete, HandStarted,
    PotAwarded,
};

// docs:start:pm_state
#[derive(Default, Clone)]
pub struct HandFlowState {
    pub hand_root: Vec<u8>,
    pub hand_number: i64,
    pub phase: HandPhase,
    pub blinds_posted: u32,
}

#[derive(Default, PartialEq, Clone, Copy)]
pub enum HandPhase {
    #[default]
    AwaitingDeal,
    Dealing,
    Blinds,
    Betting,
    Complete,
}
// docs:end:pm_state

// docs:start:pm_handler
pub struct HandFlowPm;

#[process_manager(
    name = "pmg-hand-flow",
    pm_domain = "pmg-hand-flow",
    sources = ["table", "hand"],
    targets = ["table", "hand"],
    state = HandFlowState
)]
impl HandFlowPm {
    #[handles(HandStarted)]
    fn on_hand_started(
        &self,
        _event: HandStarted,
        _state: &HandFlowState,
    ) -> CommandResult<ProcessManagerHandleResponse> {
        Ok(ProcessManagerHandleResponse::default())
    }

    #[handles(CardsDealt)]
    fn on_cards_dealt(
        &self,
        _event: CardsDealt,
        _state: &HandFlowState,
    ) -> CommandResult<ProcessManagerHandleResponse> {
        Ok(ProcessManagerHandleResponse::default())
    }

    #[handles(BlindPosted)]
    fn on_blind_posted(
        &self,
        _event: BlindPosted,
        _state: &HandFlowState,
    ) -> CommandResult<ProcessManagerHandleResponse> {
        Ok(ProcessManagerHandleResponse::default())
    }

    #[handles(ActionTaken)]
    fn on_action_taken(
        &self,
        _event: ActionTaken,
        _state: &HandFlowState,
    ) -> CommandResult<ProcessManagerHandleResponse> {
        Ok(ProcessManagerHandleResponse::default())
    }

    #[handles(CommunityCardsDealt)]
    fn on_community_dealt(
        &self,
        _event: CommunityCardsDealt,
        _state: &HandFlowState,
    ) -> CommandResult<ProcessManagerHandleResponse> {
        Ok(ProcessManagerHandleResponse::default())
    }

    #[handles(PotAwarded)]
    fn on_pot_awarded(
        &self,
        _event: PotAwarded,
        _state: &HandFlowState,
    ) -> CommandResult<ProcessManagerHandleResponse> {
        Ok(ProcessManagerHandleResponse::default())
    }

    #[handles(HandComplete)]
    fn on_hand_complete(
        &self,
        _event: HandComplete,
        _state: &HandFlowState,
    ) -> CommandResult<ProcessManagerHandleResponse> {
        Ok(ProcessManagerHandleResponse::default())
    }
}
// docs:end:pm_handler
