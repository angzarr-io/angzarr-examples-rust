//! Integration tests for the player aggregate router (post-reservation refactor).
//!
//! Exercises the macro-generated `Handler` impl on `PlayerAggregate` so the
//! `main.rs` thin shim (and the lib expansion it pulls in) is covered. The
//! reservation lifecycle commands moved to `agg-reservation`; this file only
//! exercises bankroll primitives.

use agg_player::PlayerAggregate;
use angzarr_client::full_type_url;
use angzarr_client::proto::{
    business_response, command_page, event_page, CommandBook, CommandPage, ContextualCommand,
    Cover, EventBook, EventPage,
};
use angzarr_client::router::{Built, Router};
use examples_proto::{
    Currency, DeductReservedFunds, DepositFunds, FundsDeposited, FundsReserved, PlayerRegistered,
    PlayerType, RegisterPlayer, ReleaseFunds, ReserveFunds,
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
                sync_mode: None,
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
                domain: "player".to_string(),
                ..Default::default()
            }),
            pages: vec![CommandPage {
                payload: Some(command_page::Payload::Command(any)),
                ..Default::default()
            }],
        }),
        events: Some(EventBook {
            pages,
            next_sequence: next_seq,
            ..Default::default()
        }),
    }
}

fn dispatch(
    router: &angzarr_client::router::runtime::CommandHandlerRouter,
    cc: ContextualCommand,
) -> EventBook {
    let resp = router.dispatch(cc).expect("dispatch ok");
    match resp.result {
        Some(business_response::Result::Events(book)) => book,
        other => panic!("expected events, got {:?}", other.is_some()),
    }
}

fn pack<M: Message + prost::Name>(msg: &M) -> Any {
    Any {
        type_url: full_type_url::<M>(),
        value: msg.encode_to_vec(),
    }
}

fn registered() -> Any {
    pack(&PlayerRegistered {
        display_name: "Alice".into(),
        email: "alice@example.com".into(),
        player_type: PlayerType::Human as i32,
        ai_model_id: String::new(),
        registered_at: None,
    })
}

fn deposited(amount: i64) -> Any {
    pack(&FundsDeposited {
        amount: Some(currency(amount)),
        new_balance: Some(currency(amount)),
        deposited_at: None,
    })
}

fn reserved(amount: i64, key: &[u8]) -> Any {
    pack(&FundsReserved {
        amount: Some(currency(amount)),
        key: key.to_vec(),
        new_available_balance: Some(currency(0)),
        new_reserved_balance: Some(currency(amount)),
        reserved_at: None,
    })
}

#[test]
fn router_dispatches_register_player() {
    let router = build_router();
    let book = dispatch(
        &router,
        contextual(
            RegisterPlayer {
                display_name: "Alice".into(),
                email: "alice@example.com".into(),
                player_type: PlayerType::Human as i32,
                ai_model_id: String::new(),
            },
            vec![],
        ),
    );
    assert_eq!(book.pages.len(), 1);
}

#[test]
fn router_dispatches_deposit_funds() {
    let router = build_router();
    let book = dispatch(
        &router,
        contextual(
            DepositFunds {
                amount: Some(currency(500)),
            },
            vec![registered()],
        ),
    );
    assert_eq!(book.pages.len(), 1);
}

#[test]
fn router_dispatches_reserve_funds() {
    let router = build_router();
    let book = dispatch(
        &router,
        contextual(
            ReserveFunds {
                amount: Some(currency(200)),
                key: vec![0xab, 0xcd],
            },
            vec![registered(), deposited(1_000)],
        ),
    );
    assert_eq!(book.pages.len(), 1);
}

#[test]
fn router_dispatches_release_funds() {
    let router = build_router();
    let book = dispatch(
        &router,
        contextual(
            ReleaseFunds {
                key: vec![0xab, 0xcd],
            },
            vec![registered(), deposited(1_000), reserved(200, &[0xab, 0xcd])],
        ),
    );
    assert_eq!(book.pages.len(), 1);
}

#[test]
fn router_dispatches_deduct_reserved_funds() {
    let router = build_router();
    let book = dispatch(
        &router,
        contextual(
            DeductReservedFunds {
                amount: Some(currency(200)),
                key: vec![0xab, 0xcd],
                reservation_id: vec![0xaa],
            },
            vec![registered(), deposited(1_000), reserved(200, &[0xab, 0xcd])],
        ),
    );
    assert_eq!(book.pages.len(), 1);
}
