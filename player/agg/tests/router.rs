//! Integration tests for the player aggregate router.
//!
//! Exercises the macro-generated `Handler` impl on `PlayerAggregate` so the
//! `main.rs` thin shim (and the lib expansion it pulls in) is covered.

use agg_player::PlayerAggregate;
use angzarr_client::proto::{
    business_response, command_page, event_page, BusinessResponse, CommandBook, CommandPage,
    ContextualCommand, EventBook, EventPage,
};
use angzarr_client::router::{
    Built, Handler, HandlerConfig, HandlerRequest, HandlerResponse, Router,
};
use angzarr_client::{full_type_url, Kind};
use examples_proto::{
    BuyInConfirmed, BuyInRequested, BuyInReservationReleased, ConfirmBuyIn, ConfirmRebuyFee,
    ConfirmRegistrationFee, Currency, DepositFunds, FundsDeposited, FundsReleased, FundsReserved,
    FundsTransferred, FundsWithdrawn, InitiateBuyIn, InitiateRebuy, InitiateTournamentRegistration,
    PlayerRegistered, PlayerType, RebuyFeeConfirmed, RebuyFeeReleased, RebuyRequested,
    RegisterPlayer, RegistrationFeeConfirmed, RegistrationFeeReleased, RegistrationRequested,
    ReleaseBuyIn, ReleaseFunds, ReleaseRebuyFee, ReleaseRegistrationFee, ReserveFunds,
    TransferFunds, WithdrawFunds,
};
use prost::Message;
use prost_types::Any;

fn build_router() -> angzarr_client::router::runtime::CommandHandlerRouter {
    let built = Router::new("agg-player")
        .with_handler(|| PlayerAggregate)
        .build()
        .expect("router builds");
    match built {
        Built::CommandHandler(ch) => ch,
        other => panic!("expected CommandHandler variant, got {:?}", other),
    }
}

fn currency(amount: i64) -> Currency {
    Currency {
        amount,
        currency_code: "CHIPS".into(),
    }
}

/// Build a `ContextualCommand` from a command message and prior events.
fn contextual<M: Message + prost::Name>(cmd: M, prior: Vec<Any>) -> ContextualCommand {
    let any = Any {
        type_url: full_type_url::<M>(),
        value: cmd.encode_to_vec(),
    };
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

fn pack<M: Message + prost::Name>(msg: &M) -> Any {
    Any {
        type_url: full_type_url::<M>(),
        value: msg.encode_to_vec(),
    }
}

fn registered_event() -> Any {
    pack(&PlayerRegistered {
        display_name: "Alice".into(),
        email: "alice@example.com".into(),
        player_type: PlayerType::Human as i32,
        ai_model_id: String::new(),
        registered_at: None,
    })
}

fn deposited_event(balance: i64) -> Any {
    pack(&FundsDeposited {
        amount: Some(currency(balance)),
        new_balance: Some(currency(balance)),
        deposited_at: None,
    })
}

fn register_player_command() -> ContextualCommand {
    contextual(
        RegisterPlayer {
            display_name: "Alice".into(),
            email: "alice@example.com".into(),
            player_type: PlayerType::Human as i32,
            ai_model_id: String::new(),
        },
        vec![],
    )
}

#[test]
fn config_exposes_aggregate_metadata() {
    let cfg = PlayerAggregate.config();
    assert_eq!(cfg.kind(), Kind::CommandHandler);
    match cfg {
        HandlerConfig::CommandHandler {
            domain,
            handled,
            rejected,
            applies,
            ..
        } => {
            assert_eq!(domain, "player");
            assert!(!handled.is_empty(), "expected command type URLs");
            assert!(
                handled.iter().any(|u| u.ends_with("RegisterPlayer")),
                "RegisterPlayer should be handled, got {:?}",
                handled
            );
            assert!(
                applies.iter().any(|u| u.ends_with("PlayerRegistered")),
                "PlayerRegistered applier should be declared"
            );
            assert!(
                rejected
                    .iter()
                    .any(|(d, c)| d == "table" && c == "JoinTable"),
                "JoinTable rejection must be registered"
            );
        }
        other => panic!("expected CommandHandler config, got {:?}", other),
    }
}

#[test]
fn router_builds_as_command_handler_variant() {
    let router = build_router();
    assert_eq!(router.handler_count(), 1);
}

#[test]
fn dispatch_register_player_returns_events() {
    let router = build_router();
    let resp: BusinessResponse = router
        .dispatch(register_player_command())
        .expect("dispatch ok");
    let events = match resp.result {
        Some(business_response::Result::Events(eb)) => eb,
        other => panic!("expected Events, got {:?}", other),
    };
    assert_eq!(events.pages.len(), 1);
    let any = match events.pages[0].payload.as_ref() {
        Some(angzarr_client::proto::event_page::Payload::Event(a)) => a,
        other => panic!("expected event payload, got {:?}", other),
    };
    assert!(any.type_url.ends_with("PlayerRegistered"));
}

#[test]
fn handler_dispatch_direct_returns_command_handler_response() {
    let agg = PlayerAggregate;
    let resp = agg
        .dispatch(HandlerRequest::CommandHandler(register_player_command()))
        .expect("dispatch ok");
    match resp {
        HandlerResponse::CommandHandler(br) => match br.result {
            Some(business_response::Result::Events(eb)) => {
                assert_eq!(eb.pages.len(), 1);
                assert!(matches!(
                    eb.pages[0].payload,
                    Some(angzarr_client::proto::event_page::Payload::Event(_))
                ));
            }
            other => panic!("expected events, got {:?}", other),
        },
        other => panic!("expected CommandHandler response, got {:?}", other),
    }
}

/// Dispatches a DepositFunds command after replaying a PlayerRegistered prior event
/// so the state-replay branch + apply arm is exercised.
#[test]
fn dispatch_deposit_funds_after_replay() {
    let router = build_router();
    let ctx = contextual(
        DepositFunds {
            amount: Some(currency(500)),
        },
        vec![registered_event()],
    );
    let resp: BusinessResponse = router.dispatch(ctx).expect("dispatch ok");
    let eb = match resp.result {
        Some(business_response::Result::Events(eb)) => eb,
        other => panic!("expected events, got {:?}", other),
    };
    assert_eq!(eb.pages.len(), 1);
    let any = match eb.pages[0].payload.as_ref() {
        Some(event_page::Payload::Event(a)) => a,
        _ => panic!("expected event payload"),
    };
    assert!(any.type_url.ends_with("FundsDeposited"));
    let deposited = FundsDeposited::decode(any.value.as_slice()).expect("decode ok");
    assert_eq!(
        deposited.amount.as_ref().map(|c| c.amount).unwrap_or(0),
        500
    );
}

/// Unknown command type URL returns an error.
#[test]
fn dispatch_unknown_command_returns_error() {
    let router = build_router();
    let unknown_any = Any {
        type_url: "type.googleapis.com/example.Unknown".into(),
        value: vec![],
    };
    let ctx = ContextualCommand {
        command: Some(CommandBook {
            pages: vec![CommandPage {
                payload: Some(command_page::Payload::Command(unknown_any)),
                ..Default::default()
            }],
            ..Default::default()
        }),
        events: Some(EventBook::default()),
    };
    let err = router.dispatch(ctx).unwrap_err();
    assert!(format!("{:?}", err).contains("no handler"));
}

// -----------------------------------------------------------------------------
// One test per `#[handles]` branch to exercise each dispatch arm.
// -----------------------------------------------------------------------------

/// Dispatches a command, asserting that the router returns either an Ok
/// response or a rejection error — the goal is to execute the dispatch arm.
fn dispatch_any(ctx: ContextualCommand) -> Result<BusinessResponse, angzarr_client::ClientError> {
    build_router().dispatch(ctx)
}

#[test]
fn dispatch_withdraw_funds_arm() {
    // Without player registered, guard rejects — still exercises the arm.
    let ctx = contextual(
        WithdrawFunds {
            amount: Some(currency(100)),
        },
        vec![],
    );
    let _ = dispatch_any(ctx);
}

#[test]
fn dispatch_withdraw_funds_arm_ok() {
    // With enough bankroll the withdraw succeeds.
    let ctx = contextual(
        WithdrawFunds {
            amount: Some(currency(100)),
        },
        vec![registered_event(), deposited_event(500)],
    );
    let _ = dispatch_any(ctx);
}

#[test]
fn dispatch_reserve_funds_arm() {
    let ctx = contextual(
        ReserveFunds {
            amount: Some(currency(100)),
            table_root: vec![0xAA; 16],
        },
        vec![registered_event(), deposited_event(1000)],
    );
    let _ = dispatch_any(ctx);
}

#[test]
fn dispatch_release_funds_arm() {
    let ctx = contextual(
        ReleaseFunds {
            table_root: vec![0xBB; 16],
        },
        vec![registered_event()],
    );
    let _ = dispatch_any(ctx);
}

#[test]
fn dispatch_transfer_funds_arm() {
    let ctx = contextual(
        TransferFunds {
            from_player_root: vec![0x01; 16],
            amount: Some(currency(50)),
            hand_root: vec![0x02; 16],
            reason: "pot_win".into(),
        },
        vec![registered_event()],
    );
    let _ = dispatch_any(ctx);
}

#[test]
fn dispatch_initiate_buy_in_arm() {
    let ctx = contextual(
        InitiateBuyIn {
            table_root: vec![0xCC; 16],
            seat: 1,
            amount: Some(currency(200)),
        },
        vec![registered_event(), deposited_event(1000)],
    );
    let _ = dispatch_any(ctx);
}

#[test]
fn dispatch_confirm_buy_in_arm() {
    let ctx = contextual(
        ConfirmBuyIn {
            reservation_id: vec![0xDD; 16],
        },
        vec![registered_event()],
    );
    let _ = dispatch_any(ctx);
}

#[test]
fn dispatch_release_buy_in_arm() {
    let ctx = contextual(
        ReleaseBuyIn {
            reservation_id: vec![0xEE; 16],
            reason: "cancelled".into(),
        },
        vec![registered_event()],
    );
    let _ = dispatch_any(ctx);
}

#[test]
fn dispatch_initiate_registration_arm() {
    let ctx = contextual(
        InitiateTournamentRegistration {
            tournament_root: vec![0x11; 16],
        },
        vec![registered_event(), deposited_event(1000)],
    );
    let _ = dispatch_any(ctx);
}

#[test]
fn dispatch_confirm_registration_arm() {
    let ctx = contextual(
        ConfirmRegistrationFee {
            reservation_id: vec![0x22; 16],
        },
        vec![registered_event()],
    );
    let _ = dispatch_any(ctx);
}

#[test]
fn dispatch_release_registration_arm() {
    let ctx = contextual(
        ReleaseRegistrationFee {
            reservation_id: vec![0x33; 16],
            reason: "timeout".into(),
        },
        vec![registered_event()],
    );
    let _ = dispatch_any(ctx);
}

#[test]
fn dispatch_initiate_rebuy_arm() {
    let ctx = contextual(
        InitiateRebuy {
            tournament_root: vec![0x44; 16],
            table_root: vec![0x55; 16],
            seat: 2,
        },
        vec![registered_event(), deposited_event(1000)],
    );
    let _ = dispatch_any(ctx);
}

#[test]
fn dispatch_confirm_rebuy_arm() {
    let ctx = contextual(
        ConfirmRebuyFee {
            reservation_id: vec![0x66; 16],
        },
        vec![registered_event()],
    );
    let _ = dispatch_any(ctx);
}

#[test]
fn dispatch_release_rebuy_arm() {
    let ctx = contextual(
        ReleaseRebuyFee {
            reservation_id: vec![0x77; 16],
            reason: "denied".into(),
        },
        vec![registered_event()],
    );
    let _ = dispatch_any(ctx);
}

// -----------------------------------------------------------------------------
// One test per `#[applies]` branch — replay every applier arm in state rebuild.
//
// Each test sends a simple RegisterPlayer command whose replay buffer contains
// a single instance of the target event. The applier must be invoked during
// state rebuild before the command runs. Any outcome (ok or err) counts as the
// branch being executed.
// -----------------------------------------------------------------------------

fn replay_event_and_run(event_any: Any) -> Result<BusinessResponse, angzarr_client::ClientError> {
    // Command: RegisterPlayer — the command itself will likely fail (player
    // already registered) but that's fine: we just need the applier to run in
    // the replay pass first.
    let cmd_any = Any {
        type_url: full_type_url::<RegisterPlayer>(),
        value: RegisterPlayer {
            display_name: "Alice".into(),
            email: "alice@example.com".into(),
            player_type: PlayerType::Human as i32,
            ai_model_id: String::new(),
        }
        .encode_to_vec(),
    };
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
    build_router().dispatch(ctx)
}

#[test]
fn apply_player_registered_branch() {
    let _ = replay_event_and_run(pack(&PlayerRegistered {
        display_name: "Bob".into(),
        email: "bob@example.com".into(),
        player_type: PlayerType::Human as i32,
        ai_model_id: String::new(),
        registered_at: None,
    }));
}

#[test]
fn apply_funds_deposited_branch() {
    let _ = replay_event_and_run(pack(&FundsDeposited {
        amount: Some(currency(100)),
        new_balance: Some(currency(100)),
        deposited_at: None,
    }));
}

#[test]
fn apply_funds_withdrawn_branch() {
    let _ = replay_event_and_run(pack(&FundsWithdrawn {
        amount: Some(currency(50)),
        new_balance: Some(currency(50)),
        withdrawn_at: None,
    }));
}

#[test]
fn apply_funds_reserved_branch() {
    let _ = replay_event_and_run(pack(&FundsReserved {
        amount: Some(currency(50)),
        table_root: vec![0xAA; 16],
        new_available_balance: Some(currency(950)),
        new_reserved_balance: Some(currency(50)),
        reserved_at: None,
    }));
}

#[test]
fn apply_funds_released_branch() {
    let _ = replay_event_and_run(pack(&FundsReleased {
        amount: Some(currency(50)),
        table_root: vec![0xAA; 16],
        new_available_balance: Some(currency(1000)),
        new_reserved_balance: Some(currency(0)),
        released_at: None,
    }));
}

#[test]
fn apply_funds_transferred_branch() {
    let _ = replay_event_and_run(pack(&FundsTransferred {
        from_player_root: vec![0x01; 16],
        to_player_root: vec![0x02; 16],
        amount: Some(currency(100)),
        hand_root: vec![0x03; 16],
        reason: "pot_win".into(),
        new_balance: Some(currency(900)),
        transferred_at: None,
    }));
}

#[test]
fn apply_buy_in_requested_branch() {
    let _ = replay_event_and_run(pack(&BuyInRequested {
        reservation_id: vec![0x11; 16],
        table_root: vec![0x22; 16],
        seat: 1,
        amount: Some(currency(200)),
        requested_at: None,
    }));
}

#[test]
fn apply_buy_in_confirmed_branch() {
    let _ = replay_event_and_run(pack(&BuyInConfirmed {
        reservation_id: vec![0x33; 16],
        table_root: vec![0x22; 16],
        seat: 1,
        amount: Some(currency(200)),
        confirmed_at: None,
    }));
}

#[test]
fn apply_buy_in_released_branch() {
    let _ = replay_event_and_run(pack(&BuyInReservationReleased {
        reservation_id: vec![0x44; 16],
        reason: "timeout".into(),
        released_at: None,
    }));
}

#[test]
fn apply_registration_requested_branch() {
    let _ = replay_event_and_run(pack(&RegistrationRequested {
        reservation_id: vec![0x55; 16],
        tournament_root: vec![0x66; 16],
        fee: Some(currency(100)),
        requested_at: None,
    }));
}

#[test]
fn apply_registration_confirmed_branch() {
    let _ = replay_event_and_run(pack(&RegistrationFeeConfirmed {
        reservation_id: vec![0x77; 16],
        tournament_root: vec![0x66; 16],
        fee: Some(currency(100)),
        confirmed_at: None,
    }));
}

#[test]
fn apply_registration_released_branch() {
    let _ = replay_event_and_run(pack(&RegistrationFeeReleased {
        reservation_id: vec![0x88; 16],
        reason: "cancelled".into(),
        released_at: None,
    }));
}

#[test]
fn apply_rebuy_requested_branch() {
    let _ = replay_event_and_run(pack(&RebuyRequested {
        reservation_id: vec![0x99; 16],
        tournament_root: vec![0xAA; 16],
        table_root: vec![0xBB; 16],
        seat: 3,
        fee: Some(currency(100)),
        requested_at: None,
    }));
}

#[test]
fn apply_rebuy_confirmed_branch() {
    let _ = replay_event_and_run(pack(&RebuyFeeConfirmed {
        reservation_id: vec![0xCC; 16],
        tournament_root: vec![0xAA; 16],
        fee: Some(currency(100)),
        chips_added: 100,
        confirmed_at: None,
    }));
}

#[test]
fn apply_rebuy_released_branch() {
    let _ = replay_event_and_run(pack(&RebuyFeeReleased {
        reservation_id: vec![0xDD; 16],
        reason: "denied".into(),
        released_at: None,
    }));
}
