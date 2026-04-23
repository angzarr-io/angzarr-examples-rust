//! Integration tests for the HandFlow process manager router.

use std::collections::HashMap;

use angzarr_client::proto::command_page::Payload as CommandPayload;
use angzarr_client::proto::{event_page, EventBook, EventPage, ProcessManagerHandleRequest};
use angzarr_client::router::{Built, Handler, HandlerConfig, Router};
use angzarr_client::{full_type_url, Kind};
use examples_proto::{
    CardsDealt, DealCards, EndHand, GameVariant, HandComplete, HandStarted, PostBlind, PotWinner,
    SeatSnapshot,
};
use pmg_hand_flow::HandFlowPm;
use prost::Message;
use prost_types::Any;

fn build() -> angzarr_client::router::runtime::ProcessManagerRouter {
    match Router::new("pmg-hand-flow")
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
    pm_request_with_state(trigger, EventBook::default())
}

fn pm_request_with_state(trigger: Any, process_state: EventBook) -> ProcessManagerHandleRequest {
    ProcessManagerHandleRequest {
        trigger: Some(EventBook {
            pages: vec![EventPage {
                payload: Some(event_page::Payload::Event(trigger)),
                ..Default::default()
            }],
            ..Default::default()
        }),
        process_state: Some(process_state),
        destination_sequences: HashMap::new(),
    }
}

fn hand_started_event() -> HandStarted {
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

fn first_command_any(book: &angzarr_client::proto::CommandBook) -> Any {
    match book.pages[0].payload.as_ref().unwrap() {
        CommandPayload::Command(any) => any.clone(),
        _ => panic!("expected command payload"),
    }
}

#[test]
fn config_exposes_hand_flow_pm_metadata() {
    let cfg = HandFlowPm.config();
    assert_eq!(cfg.kind(), Kind::ProcessManager);
    match cfg {
        HandlerConfig::ProcessManager {
            name,
            sources,
            targets,
            handled,
            applies,
            ..
        } => {
            assert_eq!(name, "pmg-hand-flow");
            assert!(sources.contains(&"table".into()));
            assert!(sources.contains(&"hand".into()));
            assert!(targets.contains(&"hand".into()));
            assert!(targets.contains(&"table".into()));
            assert!(handled.iter().any(|u| u.ends_with("HandStarted")));
            assert!(handled.iter().any(|u| u.ends_with("CardsDealt")));
            assert!(handled.iter().any(|u| u.ends_with("HandComplete")));
            assert_eq!(handled.len(), 3);
            assert!(applies.iter().any(|u| u.ends_with("HandStarted")));
            assert!(applies.iter().any(|u| u.ends_with("CardsDealt")));
            assert!(applies.iter().any(|u| u.ends_with("HandComplete")));
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
fn dispatch_hand_started_emits_deal_cards_to_hand() {
    let router = build();
    let resp = router
        .dispatch(pm_request(pack(&hand_started_event())))
        .expect("dispatch ok");
    assert_eq!(resp.commands.len(), 1);
    let cmd = &resp.commands[0];
    assert_eq!(cmd.cover.as_ref().unwrap().domain, "hand");
    let any = first_command_any(cmd);
    assert!(any.type_url.ends_with("examples.DealCards"));
    let deal = DealCards::decode(any.value.as_slice()).unwrap();
    assert_eq!(deal.hand_number, 1);
    assert_eq!(deal.players.len(), 3);
}

#[test]
fn dispatch_cards_dealt_emits_post_blind_for_small_blind_player() {
    // Build process_state with a prior HandStarted so state is populated
    // when CardsDealt is handled.
    let prior = EventBook {
        pages: vec![EventPage {
            payload: Some(event_page::Payload::Event(pack(&hand_started_event()))),
            ..Default::default()
        }],
        ..Default::default()
    };
    let cards = CardsDealt {
        table_root: vec![],
        hand_number: 1,
        game_variant: GameVariant::TexasHoldem as i32,
        player_cards: vec![],
        dealer_position: 0,
        players: vec![],
        dealt_at: None,
        remaining_deck: vec![],
    };
    let resp = build()
        .dispatch(pm_request_with_state(pack(&cards), prior))
        .expect("dispatch ok");
    assert_eq!(resp.commands.len(), 1);
    let cmd = &resp.commands[0];
    assert_eq!(cmd.cover.as_ref().unwrap().domain, "hand");
    let any = first_command_any(cmd);
    assert!(any.type_url.ends_with("examples.PostBlind"));
    let post = PostBlind::decode(any.value.as_slice()).unwrap();
    assert_eq!(post.blind_type, "small");
    assert_eq!(post.amount, 5);
    assert_eq!(post.player_root, vec![2u8; 16]);
}

#[test]
fn dispatch_hand_complete_emits_end_hand_to_table() {
    let prior = EventBook {
        pages: vec![EventPage {
            payload: Some(event_page::Payload::Event(pack(&hand_started_event()))),
            ..Default::default()
        }],
        ..Default::default()
    };
    let complete = HandComplete {
        table_root: vec![0xBB; 16],
        hand_number: 1,
        winners: vec![PotWinner {
            player_root: vec![1; 16],
            amount: 15,
            pot_type: "main".into(),
            winning_hand: None,
        }],
        final_stacks: vec![],
        completed_at: None,
    };
    let resp = build()
        .dispatch(pm_request_with_state(pack(&complete), prior))
        .expect("dispatch ok");
    assert_eq!(resp.commands.len(), 1);
    let cmd = &resp.commands[0];
    assert_eq!(cmd.cover.as_ref().unwrap().domain, "table");
    assert_eq!(
        cmd.cover.as_ref().unwrap().root.as_ref().unwrap().value,
        vec![0xBB; 16]
    );
    let any = first_command_any(cmd);
    assert!(any.type_url.ends_with("examples.EndHand"));
    let end = EndHand::decode(any.value.as_slice()).unwrap();
    assert_eq!(end.results.len(), 1);
    assert_eq!(end.results[0].amount, 15);
}
