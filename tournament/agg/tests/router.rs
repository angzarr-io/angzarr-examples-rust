//! Integration tests for the tournament aggregate router.

use agg_tournament::TournamentAggregate;
use angzarr_client::proto::{
    business_response, command_page, event_page, BusinessResponse, CommandBook, CommandPage,
    ContextualCommand, EventBook, EventPage,
};
use angzarr_client::router::{Built, Handler, HandlerConfig, Router};
use angzarr_client::{full_type_url, Kind};
use examples_proto::{
    AdvanceBlindLevel, BlindLevelAdvanced, CloseRegistration, CreateTournament, EliminatePlayer,
    EnrollPlayer, GameVariant, OpenRegistration, PauseTournament, PlayerEliminated, ProcessRebuy,
    RebuyDenied, RebuyProcessed, RegistrationClosed, RegistrationOpened, ResumeTournament,
    TournamentCompleted, TournamentCreated, TournamentEnrollmentRejected, TournamentPaused,
    TournamentPlayerEnrolled, TournamentResumed,
};
use prost::Message;
use prost_types::Any;

fn build() -> angzarr_client::router::runtime::CommandHandlerRouter {
    match Router::new("agg-tournament")
        .with_handler(|| TournamentAggregate)
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

fn tournament_created_event() -> Any {
    pack(&TournamentCreated {
        name: "Spring Classic".into(),
        game_variant: GameVariant::TexasHoldem as i32,
        buy_in: 500,
        starting_stack: 10_000,
        max_players: 9,
        min_players: 2,
        scheduled_start: None,
        rebuy_config: None,
        addon_config: None,
        blind_structure: vec![],
        created_at: None,
    })
}

fn registration_opened_event() -> Any {
    pack(&RegistrationOpened { opened_at: None })
}

fn create_tournament_cmd() -> ContextualCommand {
    contextual(
        CreateTournament {
            name: "Spring Classic".into(),
            game_variant: GameVariant::TexasHoldem as i32,
            buy_in: 500,
            starting_stack: 10_000,
            max_players: 9,
            min_players: 2,
            scheduled_start: None,
            rebuy_config: None,
            addon_config: None,
            blind_structure: vec![],
        },
        vec![],
    )
}

#[test]
fn config_exposes_tournament_metadata() {
    let cfg = TournamentAggregate.config();
    assert_eq!(cfg.kind(), Kind::CommandHandler);
    match cfg {
        HandlerConfig::CommandHandler {
            domain,
            handled,
            applies,
            ..
        } => {
            assert_eq!(domain, "tournament");
            assert!(handled.iter().any(|u| u.ends_with("CreateTournament")));
            assert!(handled.iter().any(|u| u.ends_with("EnrollPlayer")));
            assert!(applies.iter().any(|u| u.ends_with("TournamentCreated")));
            assert!(applies.iter().any(|u| u.ends_with("TournamentCompleted")));
        }
        other => panic!("expected CommandHandler, got {:?}", other),
    }
}

#[test]
fn router_builds_as_command_handler() {
    let router = build();
    assert_eq!(router.handler_count(), 1);
}

#[test]
fn dispatch_create_tournament_emits_tournament_created() {
    let router = build();
    let resp: BusinessResponse = router
        .dispatch(create_tournament_cmd())
        .expect("dispatch ok");
    let eb = match resp.result {
        Some(business_response::Result::Events(eb)) => eb,
        other => panic!("expected events, got {:?}", other),
    };
    assert_eq!(eb.pages.len(), 1);
    let any = match eb.pages[0].payload.as_ref() {
        Some(angzarr_client::proto::event_page::Payload::Event(a)) => a,
        _ => panic!("expected event payload"),
    };
    assert!(any.type_url.ends_with("TournamentCreated"));
}

// -----------------------------------------------------------------------------
// Handlers — one test per `#[handles]` branch.
// -----------------------------------------------------------------------------

fn run(ctx: ContextualCommand) -> Result<BusinessResponse, angzarr_client::ClientError> {
    build().dispatch(ctx)
}

#[test]
fn dispatch_open_registration_arm() {
    let ctx = contextual(OpenRegistration {}, vec![tournament_created_event()]);
    let _ = run(ctx);
}

#[test]
fn dispatch_close_registration_arm() {
    let ctx = contextual(
        CloseRegistration {},
        vec![tournament_created_event(), registration_opened_event()],
    );
    let _ = run(ctx);
}

#[test]
fn dispatch_enroll_player_arm() {
    let ctx = contextual(
        EnrollPlayer {
            player_root: vec![0xAA; 16],
            reservation_id: vec![0xBB; 16],
        },
        vec![tournament_created_event(), registration_opened_event()],
    );
    let _ = run(ctx);
}

#[test]
fn dispatch_process_rebuy_arm() {
    let ctx = contextual(
        ProcessRebuy {
            player_root: vec![0xAA; 16],
            reservation_id: vec![0xCC; 16],
        },
        vec![tournament_created_event()],
    );
    let _ = run(ctx);
}

#[test]
fn dispatch_advance_blind_level_arm() {
    let ctx = contextual(AdvanceBlindLevel {}, vec![tournament_created_event()]);
    let _ = run(ctx);
}

#[test]
fn dispatch_eliminate_player_arm() {
    let ctx = contextual(
        EliminatePlayer {
            player_root: vec![0xAA; 16],
            hand_root: vec![0xDD; 16],
        },
        vec![tournament_created_event()],
    );
    let _ = run(ctx);
}

#[test]
fn dispatch_pause_tournament_arm() {
    let ctx = contextual(
        PauseTournament {
            reason: "break".into(),
        },
        vec![tournament_created_event()],
    );
    let _ = run(ctx);
}

#[test]
fn dispatch_resume_tournament_arm() {
    let ctx = contextual(ResumeTournament {}, vec![tournament_created_event()]);
    let _ = run(ctx);
}

// -----------------------------------------------------------------------------
// Applier branches — exercise every `#[applies]` arm during replay.
// -----------------------------------------------------------------------------

fn replay_then_run(event_any: Any) -> Result<BusinessResponse, angzarr_client::ClientError> {
    let cmd_any = pack(&CreateTournament {
        name: "Another".into(),
        game_variant: GameVariant::TexasHoldem as i32,
        buy_in: 500,
        starting_stack: 10_000,
        max_players: 9,
        min_players: 2,
        scheduled_start: None,
        rebuy_config: None,
        addon_config: None,
        blind_structure: vec![],
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
fn apply_tournament_created_branch() {
    let _ = replay_then_run(tournament_created_event());
}

#[test]
fn apply_registration_opened_branch() {
    let _ = replay_then_run(registration_opened_event());
}

#[test]
fn apply_registration_closed_branch() {
    let _ = replay_then_run(pack(&RegistrationClosed {
        total_registrations: 9,
        closed_at: None,
    }));
}

#[test]
fn apply_player_enrolled_branch() {
    let _ = replay_then_run(pack(&TournamentPlayerEnrolled {
        player_root: vec![0xAA; 16],
        reservation_id: vec![0xBB; 16],
        fee_paid: 500,
        starting_stack: 10_000,
        registration_number: 1,
        enrolled_at: None,
    }));
}

#[test]
fn apply_enrollment_rejected_branch() {
    let _ = replay_then_run(pack(&TournamentEnrollmentRejected {
        player_root: vec![0xAA; 16],
        reservation_id: vec![0xBB; 16],
        reason: "full".into(),
        rejected_at: None,
    }));
}

#[test]
fn apply_rebuy_processed_branch() {
    let _ = replay_then_run(pack(&RebuyProcessed {
        player_root: vec![0xAA; 16],
        reservation_id: vec![0xCC; 16],
        rebuy_cost: 500,
        chips_added: 10_000,
        rebuy_count: 1,
        processed_at: None,
    }));
}

#[test]
fn apply_rebuy_denied_branch() {
    let _ = replay_then_run(pack(&RebuyDenied {
        player_root: vec![0xAA; 16],
        reservation_id: vec![0xCC; 16],
        reason: "max_reached".into(),
        denied_at: None,
    }));
}

#[test]
fn apply_blind_level_advanced_branch() {
    let _ = replay_then_run(pack(&BlindLevelAdvanced {
        level: 2,
        small_blind: 20,
        big_blind: 40,
        ante: 5,
        advanced_at: None,
    }));
}

#[test]
fn apply_player_eliminated_branch() {
    let _ = replay_then_run(pack(&PlayerEliminated {
        player_root: vec![0xAA; 16],
        finish_position: 5,
        hand_root: vec![0xDD; 16],
        payout: 100,
        eliminated_at: None,
    }));
}

#[test]
fn apply_tournament_paused_branch() {
    let _ = replay_then_run(pack(&TournamentPaused {
        reason: "break".into(),
        paused_at: None,
    }));
}

#[test]
fn apply_tournament_resumed_branch() {
    let _ = replay_then_run(pack(&TournamentResumed { resumed_at: None }));
}

#[test]
fn apply_tournament_completed_branch() {
    let _ = replay_then_run(pack(&TournamentCompleted {
        winner_root: vec![0xEE; 16],
        total_prize_pool: 4500,
        results: vec![],
        completed_at: None,
    }));
}
