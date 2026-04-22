//! Integration tests for the OO hand-flow process manager router.

use std::collections::HashMap;

use angzarr_client::proto::{
    event_page, EventBook, EventPage, ProcessManagerHandleRequest, ProcessManagerHandleResponse,
};
use angzarr_client::router::{Built, Handler, HandlerConfig, Router};
use angzarr_client::{full_type_url, Kind};
use examples_proto::{
    ActionTaken, ActionType, BettingPhase, BlindPosted, CardsDealt, CommunityCardsDealt,
    GameVariant, HandStarted, PotAwarded,
};
use hand_flow_oo::HandFlowPm;
use prost::Message;
use prost_types::Any;

fn build() -> angzarr_client::router::runtime::ProcessManagerRouter {
    match Router::new("hand-flow")
        .with_handler(|| HandFlowPm)
        .build()
        .expect("router builds")
    {
        Built::ProcessManager(pr) => pr,
        other => panic!("expected ProcessManager, got {:?}", other),
    }
}

fn pack<M: Message + prost::Name>(msg: &M) -> Any {
    Any {
        type_url: full_type_url::<M>(),
        value: msg.encode_to_vec(),
    }
}

fn pm_request(trigger: Any) -> ProcessManagerHandleRequest {
    ProcessManagerHandleRequest {
        trigger: Some(EventBook {
            pages: vec![EventPage {
                payload: Some(event_page::Payload::Event(trigger)),
                ..Default::default()
            }],
            ..Default::default()
        }),
        process_state: Some(EventBook::default()),
        destination_sequences: HashMap::new(),
    }
}

fn hand_started_event() -> Any {
    pack(&HandStarted {
        hand_root: vec![0x11; 16],
        hand_number: 1,
        dealer_position: 0,
        small_blind_position: 1,
        big_blind_position: 2,
        active_players: vec![],
        game_variant: GameVariant::TexasHoldem as i32,
        small_blind: 10,
        big_blind: 20,
        started_at: None,
    })
}

#[test]
fn config_exposes_hand_flow_oo_metadata() {
    let cfg = HandFlowPm.config();
    assert_eq!(cfg.kind(), Kind::ProcessManager);
    match cfg {
        HandlerConfig::ProcessManager {
            name,
            sources,
            targets,
            handled,
            ..
        } => {
            assert_eq!(name, "hand-flow");
            assert!(sources.contains(&"table".into()));
            assert!(sources.contains(&"hand".into()));
            assert!(targets.contains(&"table".into()));
            assert!(targets.contains(&"hand".into()));
            assert!(handled.iter().any(|u| u.ends_with("HandStarted")));
            assert!(handled.iter().any(|u| u.ends_with("CardsDealt")));
            assert!(handled.iter().any(|u| u.ends_with("BlindPosted")));
            assert!(handled.iter().any(|u| u.ends_with("PotAwarded")));
        }
        other => panic!("expected ProcessManager, got {:?}", other),
    }
}

#[test]
fn router_builds_as_process_manager() {
    let router = build();
    assert_eq!(router.handler_count(), 1);
}

#[test]
fn dispatch_hand_started_returns_default() {
    let router = build();
    let resp = router.dispatch(pm_request(hand_started_event())).expect("dispatch ok");
    assert!(resp.commands.is_empty());
}

// -----------------------------------------------------------------------------
// Handlers — one test per `#[handles]` branch.
// -----------------------------------------------------------------------------

fn run(req: ProcessManagerHandleRequest) -> Result<ProcessManagerHandleResponse, angzarr_client::ClientError> {
    build().dispatch(req)
}

#[test]
fn dispatch_cards_dealt_arm() {
    let evt = pack(&CardsDealt {
        table_root: vec![0xAA; 16],
        hand_number: 1,
        game_variant: GameVariant::TexasHoldem as i32,
        player_cards: vec![],
        dealer_position: 0,
        players: vec![],
        dealt_at: None,
        remaining_deck: vec![],
    });
    let _ = run(pm_request(evt));
}

#[test]
fn dispatch_blind_posted_arm() {
    let evt = pack(&BlindPosted {
        player_root: vec![0x01; 8],
        blind_type: "small".into(),
        amount: 10,
        player_stack: 990,
        pot_total: 10,
        posted_at: None,
    });
    let _ = run(pm_request(evt));
}

#[test]
fn dispatch_action_taken_arm() {
    let evt = pack(&ActionTaken {
        player_root: vec![0x01; 8],
        action: ActionType::Call as i32,
        amount: 20,
        player_stack: 980,
        pot_total: 30,
        amount_to_call: 0,
        action_at: None,
    });
    let _ = run(pm_request(evt));
}

#[test]
fn dispatch_community_cards_dealt_arm() {
    let evt = pack(&CommunityCardsDealt {
        cards: vec![],
        phase: BettingPhase::Flop as i32,
        all_community_cards: vec![],
        dealt_at: None,
    });
    let _ = run(pm_request(evt));
}

#[test]
fn dispatch_pot_awarded_arm() {
    let evt = pack(&PotAwarded {
        winners: vec![],
        awarded_at: None,
    });
    let _ = run(pm_request(evt));
}
