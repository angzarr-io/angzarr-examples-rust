//! Integration tests for the output projector router.

use angzarr_client::proto::{event_page, EventBook, EventPage};
use angzarr_client::router::{Built, Handler, HandlerConfig, Router};
use angzarr_client::{full_type_url, Kind};
use examples_proto::{
    ActionTaken, ActionType, BlindPosted, CardsDealt, Currency, FundsDeposited, GameVariant,
    HandComplete, HandStarted, PlayerJoined, PlayerRegistered, PlayerType, PotAwarded, PotWinner,
    TableCreated,
};
use prj_output::OutputProjector;
use prost::Message;
use prost_types::Any;

fn build() -> angzarr_client::router::runtime::ProjectorRouter {
    match Router::new("prj-output")
        .with_handler(|| OutputProjector)
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

fn set_test_log_file() {
    std::env::set_var("HAND_LOG_FILE", "/tmp/angzarr_prj_output_test.log");
}

#[test]
fn config_exposes_output_projector_metadata() {
    let cfg = OutputProjector.config();
    assert_eq!(cfg.kind(), Kind::Projector);
    match cfg {
        HandlerConfig::Projector {
            name,
            domains,
            handled,
        } => {
            assert_eq!(name, "output");
            assert!(domains.contains(&"player".into()));
            assert!(domains.contains(&"table".into()));
            assert!(domains.contains(&"hand".into()));
            assert!(handled.iter().any(|u| u.ends_with("PlayerRegistered")));
            assert!(handled.iter().any(|u| u.ends_with("HandComplete")));
            assert!(handled.iter().any(|u| u.ends_with("TableCreated")));
        }
        other => panic!("expected Projector, got {:?}", other),
    }
}

#[test]
fn router_builds_as_projector() {
    set_test_log_file();
    let router = build();
    assert_eq!(router.handler_count(), 1);
}

#[test]
fn dispatch_player_registered_succeeds() {
    set_test_log_file();
    let router = build();
    let _projection = router
        .dispatch(single_event_book(pack(&PlayerRegistered {
            display_name: "Alice".into(),
            email: "alice@example.com".into(),
            player_type: PlayerType::Human as i32,
            ai_model_id: String::new(),
            registered_at: None,
        })))
        .expect("dispatch ok");
}

// -----------------------------------------------------------------------------
// Handlers — one test per `#[handles]` branch.
// -----------------------------------------------------------------------------

#[test]
fn dispatch_funds_deposited_arm() {
    set_test_log_file();
    let _ = build().dispatch(single_event_book(pack(&FundsDeposited {
        amount: Some(Currency {
            amount: 100,
            currency_code: "CHIPS".into(),
        }),
        new_balance: Some(Currency {
            amount: 1100,
            currency_code: "CHIPS".into(),
        }),
        deposited_at: None,
    })));
}

#[test]
fn dispatch_table_created_arm() {
    set_test_log_file();
    let _ = build().dispatch(single_event_book(pack(&TableCreated {
        table_name: "Main".into(),
        game_variant: GameVariant::TexasHoldem as i32,
        small_blind: 1,
        big_blind: 2,
        min_buy_in: 20,
        max_buy_in: 200,
        max_players: 6,
        action_timeout_seconds: 30,
        created_at: None,
    })));
}

#[test]
fn dispatch_player_joined_arm() {
    set_test_log_file();
    let _ = build().dispatch(single_event_book(pack(&PlayerJoined {
        player_root: vec![0xAB; 16],
        seat_position: 0,
        buy_in_amount: 100,
        stack: 100,
        joined_at: None,
    })));
}

#[test]
fn dispatch_hand_started_arm() {
    set_test_log_file();
    let _ = build().dispatch(single_event_book(pack(&HandStarted {
        hand_root: vec![0xAA; 16],
        hand_number: 1,
        dealer_position: 0,
        small_blind_position: 1,
        big_blind_position: 2,
        active_players: vec![],
        game_variant: GameVariant::TexasHoldem as i32,
        small_blind: 10,
        big_blind: 20,
        started_at: None,
    })));
}

#[test]
fn dispatch_cards_dealt_arm() {
    set_test_log_file();
    let _ = build().dispatch(single_event_book(pack(&CardsDealt {
        table_root: vec![0xAA; 16],
        hand_number: 1,
        game_variant: GameVariant::TexasHoldem as i32,
        player_cards: vec![],
        dealer_position: 0,
        players: vec![],
        dealt_at: None,
        remaining_deck: vec![],
    })));
}

#[test]
fn dispatch_blind_posted_arm() {
    set_test_log_file();
    let _ = build().dispatch(single_event_book(pack(&BlindPosted {
        player_root: vec![0xAB; 16],
        blind_type: "small".into(),
        amount: 10,
        player_stack: 990,
        pot_total: 10,
        posted_at: None,
    })));
}

#[test]
fn dispatch_action_taken_arm() {
    set_test_log_file();
    let _ = build().dispatch(single_event_book(pack(&ActionTaken {
        player_root: vec![0xAB; 16],
        action: ActionType::Call as i32,
        amount: 20,
        player_stack: 970,
        pot_total: 30,
        amount_to_call: 0,
        action_at: None,
    })));
}

#[test]
fn dispatch_pot_awarded_arm() {
    set_test_log_file();
    let _ = build().dispatch(single_event_book(pack(&PotAwarded {
        winners: vec![PotWinner {
            player_root: vec![0xAB; 16],
            amount: 100,
            pot_type: "main".into(),
            winning_hand: None,
        }],
        awarded_at: None,
    })));
}

#[test]
fn dispatch_hand_complete_arm() {
    set_test_log_file();
    let _ = build().dispatch(single_event_book(pack(&HandComplete {
        table_root: vec![0xAA; 16],
        hand_number: 5,
        winners: vec![],
        final_stacks: vec![],
        completed_at: None,
    })));
}
