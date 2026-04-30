//! Hand Flow Process Manager library.
//!
//! Coordinates the workflow between the `table` and `hand` domains. Ported
//! from the Python reference (`examples-python/main/pmg-hand-flow`): the PM
//! is intentionally minimal — three event handlers drive the happy-path
//! choreography while the heavier round-by-round state machine lives in
//! the hand aggregate itself.
//!
//! Flow:
//!   table.HandStarted  → hand.DealCards         (PM, phase = DEALING)
//!   hand.CardsDealt    → hand.PostBlind (small) (PM, phase = BLINDS)
//!   hand.HandComplete  → table.EndHand          (PM, phase = COMPLETE)
//!
//! The PM re-emits each triggering event as its own process event so that
//! the Tier 5 runtime can replay them through `#[applies]` methods to
//! rebuild PM state across restarts.

use angzarr_client::proto::command_page::Payload as CommandPayload;
use angzarr_client::proto::{
    event_page::Payload as EventPayload, page_header::SequenceType, CommandBook, CommandPage,
    Cover, EventBook, EventPage, MergeStrategy, PageHeader, ProcessManagerHandleResponse,
    Uuid as ProtoUuid,
};
use angzarr_client::{process_manager, type_url, CommandResult};
use examples_utils::pack_event;
use examples_proto::{
    CardsDealt, DealCards, EndHand, GameVariant, HandComplete, HandStarted, PlayerInHand, PostBlind,
};
use prost::Message;
use prost_types::Any;

// docs:start:pm_state
#[derive(Debug, Default, PartialEq, Clone, Copy)]
pub enum HandPhase {
    #[default]
    AwaitingDeal,
    Dealing,
    Blinds,
    Complete,
}

#[derive(Default, Clone, Debug)]
pub struct HandFlowState {
    pub hand_root: Vec<u8>,
    pub hand_number: i64,
    pub game_variant: GameVariant,
    pub dealer_position: i32,
    pub small_blind_position: i32,
    pub big_blind_position: i32,
    pub small_blind: i64,
    pub big_blind: i64,
    /// (position, player_root) pairs from the triggering HandStarted event.
    pub active_players: Vec<(i32, Vec<u8>)>,
    pub phase: HandPhase,
}

impl HandFlowState {
    fn player_at_position(&self, pos: i32) -> Option<&[u8]> {
        self.active_players.iter().find_map(|(p, root)| {
            if *p == pos {
                Some(root.as_slice())
            } else {
                None
            }
        })
    }
}
// docs:end:pm_state

// docs:start:pm_handler
pub struct HandFlowPm;

#[process_manager(
    name = "pmg-hand-flow",
    pm_domain = "pmg-hand-flow",
    sources = ["table", "hand"],
    targets = ["hand", "table"],
    state = HandFlowState
)]
impl HandFlowPm {
    /// Table started a new hand → drive `DealCards` into the hand domain.
    #[handles(HandStarted)]
    fn on_hand_started(
        &self,
        event: HandStarted,
        _state: &HandFlowState,
    ) -> CommandResult<ProcessManagerHandleResponse> {
        let players: Vec<PlayerInHand> = event
            .active_players
            .iter()
            .map(|seat| PlayerInHand {
                player_root: seat.player_root.clone(),
                position: seat.position,
                stack: seat.stack,
            })
            .collect();

        let deal_cards = DealCards {
            // Tier 5 dispatch does not surface the source cover, so the
            // table_root cannot be recovered from the trigger. Hand
            // correlation keys off hand_root anyway.
            table_root: Vec::new(),
            hand_number: event.hand_number,
            game_variant: event.game_variant,
            players,
            dealer_position: event.dealer_position,
            small_blind: event.small_blind,
            big_blind: event.big_blind,
            deck_seed: Vec::new(),
        };
        let cmd = make_command("hand", &event.hand_root, "examples.DealCards", &deal_cards);
        let pm_event = pack_event(&event, "examples.HandStarted");

        Ok(ProcessManagerHandleResponse {
            commands: vec![cmd],
            process_events: vec![single_event_book(pm_event)],
            facts: vec![],
        })
    }

    /// Cards were dealt → post the small blind. The big blind is posted
    /// reactively by the hand aggregate once the small blind event lands,
    /// matching the minimal shape of the Python reference.
    #[handles(CardsDealt)]
    fn on_cards_dealt(
        &self,
        event: CardsDealt,
        state: &HandFlowState,
    ) -> CommandResult<ProcessManagerHandleResponse> {
        let small_blind_player = state
            .player_at_position(state.small_blind_position)
            .map(|r| r.to_vec())
            .unwrap_or_default();

        let post_blind = PostBlind {
            player_root: small_blind_player,
            blind_type: "small".to_string(),
            amount: state.small_blind,
        };
        let cmd = make_command("hand", &state.hand_root, "examples.PostBlind", &post_blind);
        let pm_event = pack_event(&event, "examples.CardsDealt");

        Ok(ProcessManagerHandleResponse {
            commands: vec![cmd],
            process_events: vec![single_event_book(pm_event)],
            facts: vec![],
        })
    }

    /// Hand completed → instruct the table to end the hand.
    #[handles(HandComplete)]
    pub fn on_hand_complete(
        &self,
        event: HandComplete,
        state: &HandFlowState,
    ) -> CommandResult<ProcessManagerHandleResponse> {
        let end_hand = EndHand {
            hand_root: state.hand_root.clone(),
            results: event
                .winners
                .iter()
                .map(|w| examples_proto::PotResult {
                    winner_root: w.player_root.clone(),
                    amount: w.amount,
                    pot_type: w.pot_type.clone(),
                    winning_hand: w.winning_hand.clone(),
                })
                .collect(),
        };
        let cmd = make_command("table", &event.table_root, "examples.EndHand", &end_hand);
        let pm_event = pack_event(&event, "examples.HandComplete");

        Ok(ProcessManagerHandleResponse {
            commands: vec![cmd],
            process_events: vec![single_event_book(pm_event)],
            facts: vec![],
        })
    }

    // --- Appliers: rebuild PM state from its own process_events ---

    #[applies(HandStarted)]
    fn apply_hand_started(state: &mut HandFlowState, event: HandStarted) {
        state.hand_root = event.hand_root;
        state.hand_number = event.hand_number;
        state.game_variant = GameVariant::try_from(event.game_variant).unwrap_or_default();
        state.dealer_position = event.dealer_position;
        state.small_blind_position = event.small_blind_position;
        state.big_blind_position = event.big_blind_position;
        state.small_blind = event.small_blind;
        state.big_blind = event.big_blind;
        state.active_players = event
            .active_players
            .into_iter()
            .map(|seat| (seat.position, seat.player_root))
            .collect();
        state.phase = HandPhase::Dealing;
    }

    #[applies(CardsDealt)]
    fn apply_cards_dealt(state: &mut HandFlowState, _event: CardsDealt) {
        state.phase = HandPhase::Blinds;
    }

    #[applies(HandComplete)]
    fn apply_hand_complete(state: &mut HandFlowState, _event: HandComplete) {
        state.phase = HandPhase::Complete;
    }
}
// docs:end:pm_handler

fn make_command<M: Message>(
    domain: &str,
    root: &[u8],
    proto_type_name: &str,
    message: &M,
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
                sequence_type: Some(SequenceType::Sequence(0)),
            }),
            merge_strategy: MergeStrategy::MergeCommutative as i32,
            payload: Some(CommandPayload::Command(Any {
                type_url: type_url(proto_type_name),
                value: message.encode_to_vec(),
            })),
        }],
    }
}

fn single_event_book(event: Any) -> EventBook {
    EventBook {
        cover: None,
        pages: vec![EventPage {
            header: Some(PageHeader {
                sequence_type: Some(SequenceType::Sequence(0)),
            }),
            created_at: Some(angzarr_client::now()),
            no_commit: false,
            cascade_id: None,
            payload: Some(EventPayload::Event(event)),
        }],
        snapshot: None,
        next_sequence: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use examples_proto::SeatSnapshot;

    fn sample_hand_started() -> HandStarted {
        HandStarted {
            hand_root: vec![0xAA; 16],
            hand_number: 1,
            dealer_position: 0,
            small_blind_position: 1,
            big_blind_position: 2,
            active_players: vec![
                SeatSnapshot {
                    position: 0,
                    player_root: vec![1; 16],
                    stack: 500,
                },
                SeatSnapshot {
                    position: 1,
                    player_root: vec![2; 16],
                    stack: 500,
                },
                SeatSnapshot {
                    position: 2,
                    player_root: vec![3; 16],
                    stack: 500,
                },
            ],
            game_variant: GameVariant::TexasHoldem as i32,
            small_blind: 5,
            big_blind: 10,
            started_at: None,
        }
    }

    fn first_event_any(book: &EventBook) -> Any {
        match book.pages[0].payload.as_ref().unwrap() {
            EventPayload::Event(any) => any.clone(),
            _ => panic!("expected event payload"),
        }
    }

    #[test]
    fn hand_started_emits_pm_event_for_replay() {
        let pm = HandFlowPm;
        let event = sample_hand_started();
        let resp = pm
            .on_hand_started(event.clone(), &HandFlowState::default())
            .unwrap();

        let pm_book = resp
            .process_events
            .into_iter()
            .next()
            .expect("process_events");
        let any = first_event_any(&pm_book);
        assert!(any.type_url.ends_with("examples.HandStarted"));
        let decoded = HandStarted::decode(any.value.as_slice()).unwrap();
        assert_eq!(decoded.hand_number, 1);
    }
}
