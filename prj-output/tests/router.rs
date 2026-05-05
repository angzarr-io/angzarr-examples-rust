//! Integration tests for the pretty output projector router.

use angzarr_client::proto::{event_page, EventBook, EventPage};
use angzarr_client::router::{Built, Handler, HandlerConfig, Router};
use angzarr_client::{full_type_url, Kind};
use examples_proto::{
    ActionTaken, ActionType, BlindPosted, CardsDealt, Currency, FundsDeposited, GameVariant,
    HandComplete, HandStarted, PlayerJoined, PlayerRegistered, PlayerType, PotAwarded, PotWinner,
    TableCreated,
};
use prj_pretty_output::{MemSink, PrettyOutputProjector, PrettyStore};
use prost::Message;
use prost_types::Any;
use std::sync::Arc;

fn make_factory() -> (
    Arc<MemSink>,
    impl Fn() -> PrettyOutputProjector + Send + Sync + 'static,
) {
    let sink = Arc::new(MemSink::new());
    let sink_clone = Arc::clone(&sink);
    let factory = move || {
        let store = PrettyStore::open_in_memory().expect("open sqlite");
        struct SinkRef(Arc<MemSink>);
        impl prj_pretty_output::LineSink for SinkRef {
            fn write_line(&self, line: &str) {
                self.0.write_line(line);
            }
        }
        PrettyOutputProjector::new(store, Box::new(SinkRef(Arc::clone(&sink_clone))))
    };
    (sink, factory)
}

fn build_router(
    factory: impl Fn() -> PrettyOutputProjector + Send + Sync + 'static,
) -> angzarr_client::router::runtime::ProjectorRouter {
    match Router::new("prj-pretty-output")
        .with_handler(factory)
        .build()
        .expect("router builds")
    {
        Built::Projector(pr) => pr,
        other => panic!("expected Projector, got {:?}", other),
    }
}

fn pack<M: Message + prost::Name>(msg: &M) -> Any {
    Any {
        type_url: full_type_url::<M>(),
        value: msg.encode_to_vec(),
    }
}

fn single_event_book(any: Any) -> EventBook {
    EventBook {
        pages: vec![EventPage {
            payload: Some(event_page::Payload::Event(any)),
            ..Default::default()
        }],
        ..Default::default()
    }
}

fn config_of<F: Fn() -> PrettyOutputProjector>(factory: &F) -> HandlerConfig {
    factory().config()
}

#[test]
fn config_exposes_pretty_output_projector_metadata() {
    let (_sink, factory) = make_factory();
    let cfg = config_of(&factory);
    assert_eq!(cfg.kind(), Kind::Projector);
    match cfg {
        HandlerConfig::Projector {
            name,
            domains,
            handled,
        } => {
            assert_eq!(name, "pretty-output");
            assert!(domains.contains(&"player".into()));
            assert!(domains.contains(&"table".into()));
            assert!(domains.contains(&"hand".into()));
            assert!(handled.iter().any(|u| u.ends_with("PlayerRegistered")));
            assert!(handled.iter().any(|u| u.ends_with("HandComplete")));
            assert!(handled.iter().any(|u| u.ends_with("TableCreated")));
            assert!(handled.iter().any(|u| u.ends_with("BlindPosted")));
            assert!(handled.iter().any(|u| u.ends_with("ActionTaken")));
        }
        other => panic!("expected Projector, got {:?}", other),
    }
}

#[test]
fn router_builds_as_projector() {
    let (_sink, factory) = make_factory();
    let router = build_router(factory);
    assert_eq!(router.handler_count(), 1);
}

#[test]
fn dispatch_player_registered_writes_pretty_line() {
    let (sink, factory) = make_factory();
    build_router(factory)
        .dispatch(single_event_book(pack(&PlayerRegistered {
            display_name: "Alice".into(),
            email: "alice@example.com".into(),
            player_type: PlayerType::Human as i32,
            ai_model_id: String::new(),
            registered_at: None,
        })))
        .expect("dispatch ok");
    assert!(sink.output().contains("Alice registered"));
}

#[test]
fn dispatch_funds_deposited_formats_money() {
    let (sink, factory) = make_factory();
    build_router(factory)
        .dispatch(single_event_book(pack(&FundsDeposited {
            amount: Some(Currency {
                amount: 1_000,
                currency_code: "CHIPS".into(),
            }),
            new_balance: Some(Currency {
                amount: 1_000,
                currency_code: "CHIPS".into(),
            }),
            deposited_at: None,
        })))
        .expect("dispatch ok");
    assert!(sink.output().contains("$1,000"));
}

#[test]
fn dispatch_table_created_then_hand_started_renders_both() {
    let (sink, factory) = make_factory();
    let router = build_router(factory);
    let _ = router
        .dispatch(single_event_book(pack(&TableCreated {
            table_name: "Main".into(),
            game_variant: GameVariant::TexasHoldem as i32,
            small_blind: 5,
            big_blind: 10,
            min_buy_in: 200,
            max_buy_in: 1_000,
            max_players: 6,
            action_timeout_seconds: 30,
            created_at: None,
        })))
        .expect("dispatch table ok");
    let _ = router
        .dispatch(single_event_book(pack(&HandStarted {
            hand_root: vec![0xAA; 16],
            hand_number: 1,
            dealer_position: 0,
            small_blind_position: 1,
            big_blind_position: 2,
            active_players: vec![],
            game_variant: GameVariant::TexasHoldem as i32,
            small_blind: 5,
            big_blind: 10,
            started_at: None,
            ..Default::default()
        })))
        .expect("dispatch hand ok");

    let out = sink.output();
    assert!(out.contains("Table \"Main\" created"));
    assert!(out.contains("HAND #1"));
}

#[test]
fn dispatch_blind_posted_uppercases_type() {
    let (sink, factory) = make_factory();
    build_router(factory)
        .dispatch(single_event_book(pack(&BlindPosted {
            player_root: vec![0xAB; 16],
            blind_type: "small".into(),
            amount: 5,
            player_stack: 495,
            pot_total: 5,
            posted_at: None,
        })))
        .expect("ok");
    assert!(sink.output().contains("SMALL"));
}

#[test]
fn dispatch_action_taken_maps_enum_to_verb() {
    let (sink, factory) = make_factory();
    build_router(factory)
        .dispatch(single_event_book(pack(&ActionTaken {
            player_root: vec![0xAB; 16],
            action: ActionType::Raise as i32,
            amount: 30,
            player_stack: 970,
            pot_total: 50,
            amount_to_call: 0,
            action_at: None,
        })))
        .expect("ok");
    let out = sink.output();
    assert!(out.contains("raises to $30"));
    assert!(out.contains("pot: $50"));
}

#[test]
fn dispatch_player_joined_arm() {
    let (sink, factory) = make_factory();
    build_router(factory)
        .dispatch(single_event_book(pack(&PlayerJoined {
            player_root: vec![0xAB; 16],
            seat_position: 3,
            buy_in_amount: 500,
            stack: 500,
            joined_at: None,
        })))
        .expect("ok");
    let out = sink.output();
    assert!(out.contains("seat 3"));
    assert!(out.contains("$500"));
}

#[test]
fn dispatch_cards_dealt_arm_without_hole_cards_falls_back_to_summary() {
    let (sink, factory) = make_factory();
    build_router(factory)
        .dispatch(single_event_book(pack(&CardsDealt {
            table_root: vec![],
            hand_number: 3,
            game_variant: GameVariant::TexasHoldem as i32,
            player_cards: vec![],
            dealer_position: 0,
            players: vec![],
            dealt_at: None,
            remaining_deck: vec![],
            ..Default::default()
        })))
        .expect("ok");
    assert!(sink.output().contains("Cards dealt"));
}

#[test]
fn dispatch_pot_awarded_lists_winners() {
    let (sink, factory) = make_factory();
    build_router(factory)
        .dispatch(single_event_book(pack(&PotAwarded {
            winners: vec![PotWinner {
                player_root: vec![0xAB; 16],
                amount: 150,
                pot_type: "main".into(),
                winning_hand: None,
            }],
            awarded_at: None,
        })))
        .expect("ok");
    assert!(sink.output().contains("wins $150"));
}

#[test]
fn dispatch_hand_complete_arm() {
    let (sink, factory) = make_factory();
    build_router(factory)
        .dispatch(single_event_book(pack(&HandComplete {
            table_root: vec![0xAA; 16],
            hand_number: 7,
            winners: vec![],
            final_stacks: vec![],
            completed_at: None,
        })))
        .expect("ok");
    assert!(sink.output().contains("HAND #7 complete"));
}
