//! Integration tests for the table aggregate router.

use agg_table::TableAggregate;
use angzarr_client::proto::{
    business_response, command_page, event_page, BusinessResponse, CommandBook, CommandPage,
    ContextualCommand, Cover, EventBook, EventPage,
};
use angzarr_client::router::{Built, Handler, HandlerConfig, Router};
use angzarr_client::{full_type_url, Kind};
use examples_proto::{
    AddRebuyChips, ChipsAdded, CreateTable, EndHand, GameVariant, HandEnded, HandStarted,
    JoinTable, LeaveTable, PlayerJoined, PlayerLeft, PlayerSatIn, PlayerSatOut, PlayerSeated,
    RebuyChipsAdded, SeatPlayer, SeatSnapshot, SeatingRejected, StartHand, TableCreated,
};
use prost::Message;
use prost_types::Any;

fn build() -> angzarr_client::router::runtime::CommandHandlerRouter {
    match Router::new("agg-table")
        .with_handler(|| TableAggregate)
        .build()
        .expect("router builds")
    {
        Built::CommandHandler(ch) => ch,
        other => panic!("expected CommandHandler, got {:?}", other),
    }
}

fn pack<M: Message + prost::Name>(msg: &M) -> Any {
    Any {
        type_url: full_type_url::<M>(),
        value: msg.encode_to_vec(),
    }
}

fn contextual<M: Message + prost::Name>(cmd: M, prior: Vec<Any>) -> ContextualCommand {
    let any = pack(&cmd);
    let pages: Vec<EventPage> = prior
        .into_iter()
        .enumerate()
        .map(|(i, a)| EventPage {
            header: Some(angzarr_client::proto::PageHeader {
                sequence_type: Some(angzarr_client::proto::page_header::SequenceType::Sequence(
                    i as u32,
                )),
            }),
            payload: Some(event_page::Payload::Event(a)),
            ..Default::default()
        })
        .collect();
    let next_seq = pages.len() as u32;
    ContextualCommand {
        command: Some(CommandBook {
            cover: Some(Cover {
                domain: "table".to_string(),
                ..Default::default()
            }),
            pages: vec![CommandPage {
                payload: Some(command_page::Payload::Command(any)),
                ..Default::default()
            }],
            ..Default::default()
        }),
        events: Some(EventBook {
            pages,
            next_sequence: next_seq,
            ..Default::default()
        }),
    }
}

fn table_created_event() -> Any {
    pack(&TableCreated {
        table_name: "Main".into(),
        game_variant: GameVariant::TexasHoldem as i32,
        small_blind: 1,
        big_blind: 2,
        min_buy_in: 20,
        max_buy_in: 200,
        max_players: 6,
        action_timeout_seconds: 30,
        created_at: None,
    })
}

fn create_table_cmd() -> ContextualCommand {
    contextual(
        CreateTable {
            table_name: "Main".into(),
            game_variant: GameVariant::TexasHoldem as i32,
            small_blind: 1,
            big_blind: 2,
            min_buy_in: 20,
            max_buy_in: 200,
            max_players: 6,
            action_timeout_seconds: 30,
        },
        vec![],
    )
}

#[test]
fn config_exposes_table_aggregate_metadata() {
    let cfg = TableAggregate.config();
    assert_eq!(cfg.kind(), Kind::CommandHandler);
    match cfg {
        HandlerConfig::CommandHandler {
            domain,
            handled,
            applies,
            ..
        } => {
            assert_eq!(domain, "table");
            assert!(handled.iter().any(|u| u.ends_with("CreateTable")));
            assert!(handled.iter().any(|u| u.ends_with("JoinTable")));
            assert!(applies.iter().any(|u| u.ends_with("TableCreated")));
        }
        other => panic!("expected CommandHandler, got {:?}", other),
    }
}

#[test]
fn router_builds_single_handler() {
    let router = build();
    assert_eq!(router.handler_count(), 1);
}

#[test]
fn dispatch_create_table_returns_table_created_event() {
    let router = build();
    let resp: BusinessResponse = router.dispatch(create_table_cmd()).expect("dispatch ok");
    let eb = match resp.result {
        Some(business_response::Result::Events(eb)) => eb,
        other => panic!("expected events, got {:?}", other),
    };
    assert_eq!(eb.pages.len(), 1);
    let any = match eb.pages[0].payload.as_ref() {
        Some(angzarr_client::proto::event_page::Payload::Event(a)) => a,
        _ => panic!("expected event payload"),
    };
    assert!(any.type_url.ends_with("TableCreated"));
}

// -----------------------------------------------------------------------------
// Handlers — one test per `#[handles]` branch.
// -----------------------------------------------------------------------------

fn run(ctx: ContextualCommand) -> Result<BusinessResponse, angzarr_client::ClientError> {
    build().dispatch(ctx)
}

#[test]
fn dispatch_join_table_arm() {
    let ctx = contextual(
        JoinTable {
            player_root: vec![0xAA; 16],
            preferred_seat: -1,
            buy_in_amount: 100,
        },
        vec![table_created_event()],
    );
    let _ = run(ctx);
}

#[test]
fn dispatch_leave_table_arm() {
    let ctx = contextual(
        LeaveTable {
            player_root: vec![0xAA; 16],
        },
        vec![table_created_event()],
    );
    let _ = run(ctx);
}

#[test]
fn dispatch_start_hand_arm() {
    let ctx = contextual(StartHand {}, vec![table_created_event()]);
    let _ = run(ctx);
}

#[test]
fn dispatch_end_hand_arm() {
    let ctx = contextual(
        EndHand {
            hand_root: vec![0xBB; 16],
            results: vec![],
        },
        vec![table_created_event()],
    );
    let _ = run(ctx);
}

#[test]
fn dispatch_seat_player_arm() {
    let ctx = contextual(
        SeatPlayer {
            player_root: vec![0xCC; 16],
            reservation_id: vec![0xDD; 16],
            seat: 0,
            amount: 100,
        },
        vec![table_created_event()],
    );
    let _ = run(ctx);
}

#[test]
fn dispatch_add_rebuy_chips_arm() {
    let ctx = contextual(
        AddRebuyChips {
            player_root: vec![0xCC; 16],
            reservation_id: vec![0xDD; 16],
            seat: 0,
            amount: 100,
        },
        vec![table_created_event()],
    );
    let _ = run(ctx);
}

// -----------------------------------------------------------------------------
// Applier branches — exercise every `#[applies]` arm during replay.
// -----------------------------------------------------------------------------

fn replay_then_noop(event_any: Any) -> Result<BusinessResponse, angzarr_client::ClientError> {
    // Use a harmless CreateTable command; the replay pass will still execute
    // the target applier even though the create will fail because a table
    // already exists.
    let cmd_any = pack(&CreateTable {
        table_name: "Another".into(),
        game_variant: GameVariant::TexasHoldem as i32,
        small_blind: 1,
        big_blind: 2,
        min_buy_in: 20,
        max_buy_in: 200,
        max_players: 6,
        action_timeout_seconds: 30,
    });
    let ctx = ContextualCommand {
        command: Some(CommandBook {
            pages: vec![CommandPage {
                payload: Some(command_page::Payload::Command(cmd_any)),
                ..Default::default()
            }],
            ..Default::default()
        }),
        events: Some(EventBook {
            pages: vec![EventPage {
                header: Some(angzarr_client::proto::PageHeader {
                    sequence_type: Some(
                        angzarr_client::proto::page_header::SequenceType::Sequence(0),
                    ),
                }),
                payload: Some(event_page::Payload::Event(event_any)),
                ..Default::default()
            }],
            next_sequence: 1,
            ..Default::default()
        }),
    };
    build().dispatch(ctx)
}

#[test]
fn apply_table_created_branch() {
    let _ = replay_then_noop(table_created_event());
}

#[test]
fn apply_player_joined_branch() {
    let _ = replay_then_noop(pack(&PlayerJoined {
        player_root: vec![0xAA; 16],
        seat_position: 0,
        buy_in_amount: 100,
        stack: 100,
        joined_at: None,
    }));
}

#[test]
fn apply_player_left_branch() {
    let _ = replay_then_noop(pack(&PlayerLeft {
        player_root: vec![0xAA; 16],
        seat_position: 0,
        chips_cashed_out: 100,
        left_at: None,
    }));
}

#[test]
fn apply_player_sat_out_branch() {
    let _ = replay_then_noop(pack(&PlayerSatOut {
        player_root: vec![0xAA; 16],
        sat_out_at: None,
    }));
}

#[test]
fn apply_player_sat_in_branch() {
    let _ = replay_then_noop(pack(&PlayerSatIn {
        player_root: vec![0xAA; 16],
        sat_in_at: None,
    }));
}

#[test]
fn apply_hand_started_branch() {
    let _ = replay_then_noop(pack(&HandStarted {
        hand_root: vec![0xBB; 16],
        hand_number: 1,
        dealer_position: 0,
        small_blind_position: 1,
        big_blind_position: 2,
        active_players: vec![SeatSnapshot {
            position: 0,
            player_root: vec![0xAA; 16],
            stack: 100,
        }],
        game_variant: GameVariant::TexasHoldem as i32,
        small_blind: 1,
        big_blind: 2,
        started_at: None,
    }));
}

#[test]
fn apply_hand_ended_branch() {
    let _ = replay_then_noop(pack(&HandEnded {
        hand_root: vec![0xBB; 16],
        results: vec![],
        stack_changes: Default::default(),
        ended_at: None,
    }));
}

#[test]
fn apply_chips_added_branch() {
    let _ = replay_then_noop(pack(&ChipsAdded {
        player_root: vec![0xAA; 16],
        amount: 100,
        new_stack: 200,
        added_at: None,
    }));
}

#[test]
fn apply_player_seated_branch() {
    let _ = replay_then_noop(pack(&PlayerSeated {
        player_root: vec![0xAA; 16],
        reservation_id: vec![0xDD; 16],
        seat_position: 0,
        stack: 100,
        seated_at: None,
    }));
}

#[test]
fn apply_seating_rejected_branch() {
    let _ = replay_then_noop(pack(&SeatingRejected {
        player_root: vec![0xAA; 16],
        reservation_id: vec![0xDD; 16],
        requested_seat: 0,
        reason: "seat_occupied".into(),
        rejected_at: None,
    }));
}

#[test]
fn apply_rebuy_chips_added_branch() {
    let _ = replay_then_noop(pack(&RebuyChipsAdded {
        player_root: vec![0xAA; 16],
        reservation_id: vec![0xDD; 16],
        seat: 0,
        amount: 100,
        new_stack: 200,
        added_at: None,
    }));
}
