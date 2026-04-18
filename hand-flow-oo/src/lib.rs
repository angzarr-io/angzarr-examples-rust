//! Process Manager: Hand Flow (OO/Tier 5 example) library.
//!
//! Demonstrates the unified `#[process_manager]` macro for a PM that subscribes
//! to both table and hand domain events.

use angzarr_client::proto::ProcessManagerHandleResponse;
use angzarr_client::{process_manager, CommandResult};
use examples_proto::{
    ActionTaken, BlindPosted, CardsDealt, CommunityCardsDealt, HandStarted, PotAwarded,
};

#[derive(Clone, Default)]
pub struct PMState {
    pub hand_root: Option<Vec<u8>>,
    pub hand_in_progress: bool,
}

pub struct HandFlowPm;

#[process_manager(
    name = "hand-flow",
    pm_domain = "hand-flow",
    sources = ["table", "hand"],
    targets = ["table", "hand"],
    state = PMState
)]
impl HandFlowPm {
    #[handles(HandStarted)]
    fn on_hand_started(
        &self,
        _event: HandStarted,
        _state: &PMState,
    ) -> CommandResult<ProcessManagerHandleResponse> {
        Ok(ProcessManagerHandleResponse::default())
    }

    #[handles(CardsDealt)]
    fn on_cards_dealt(
        &self,
        _event: CardsDealt,
        _state: &PMState,
    ) -> CommandResult<ProcessManagerHandleResponse> {
        Ok(ProcessManagerHandleResponse::default())
    }

    #[handles(BlindPosted)]
    fn on_blind_posted(
        &self,
        _event: BlindPosted,
        _state: &PMState,
    ) -> CommandResult<ProcessManagerHandleResponse> {
        Ok(ProcessManagerHandleResponse::default())
    }

    #[handles(ActionTaken)]
    fn on_action_taken(
        &self,
        _event: ActionTaken,
        _state: &PMState,
    ) -> CommandResult<ProcessManagerHandleResponse> {
        Ok(ProcessManagerHandleResponse::default())
    }

    #[handles(CommunityCardsDealt)]
    fn on_community_dealt(
        &self,
        _event: CommunityCardsDealt,
        _state: &PMState,
    ) -> CommandResult<ProcessManagerHandleResponse> {
        Ok(ProcessManagerHandleResponse::default())
    }

    #[handles(PotAwarded)]
    fn on_pot_awarded(
        &self,
        _event: PotAwarded,
        _state: &PMState,
    ) -> CommandResult<ProcessManagerHandleResponse> {
        Ok(ProcessManagerHandleResponse::default())
    }
}
