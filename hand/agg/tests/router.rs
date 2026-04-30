//! Integration tests for the hand aggregate router.

use agg_hand::HandAggregate;
use angzarr_client::proto::{
    business_response, command_page, event_page, BusinessResponse, CommandBook, CommandPage,
    ContextualCommand, Cover, EventBook, EventPage,
};
use angzarr_client::router::{Built, Handler, HandlerConfig, Router};
use angzarr_client::{full_type_url, Kind};
use examples_proto::{
    ActionTaken, ActionType, AwardPot, BettingPhase, BettingRoundComplete, BlindPosted, CardsDealt,
    CommunityCardsDealt, DealCards, DealCommunityCards, DrawCompleted, GameVariant, HandComplete,
    PlayerAction, PlayerInHand, PostBlind, PotAward, PotAwarded, RequestDraw, RevealCards,
    ShowdownStarted,
};
use prost::Message;
use prost_types::Any;

fn build() -> angzarr_client::router::runtime::CommandHandlerRouter {
    match Router::new("agg-hand")
        .with_handler(|| HandAggregate)
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
                domain: "hand".to_string(),
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

fn deal_cards_cmd() -> DealCards {
    DealCards {
        table_root: vec![0xAA; 16],
        hand_number: 1,
        game_variant: GameVariant::TexasHoldem as i32,
        players: vec![
            PlayerInHand {
                player_root: vec![0x01; 8],
                position: 0,
                stack: 1000,
            },
            PlayerInHand {
                player_root: vec![0x02; 8],
                position: 1,
                stack: 1000,
            },
        ],
        dealer_position: 0,
        small_blind: 10,
        big_blind: 20,
        deck_seed: vec![0xDD; 32],
    }
}

fn cards_dealt_event() -> Any {
    pack(&CardsDealt {
        table_root: vec![0xAA; 16],
        hand_number: 1,
        game_variant: GameVariant::TexasHoldem as i32,
        player_cards: vec![],
        dealer_position: 0,
        players: vec![
            PlayerInHand {
                player_root: vec![0x01; 8],
                position: 0,
                stack: 1000,
            },
            PlayerInHand {
                player_root: vec![0x02; 8],
                position: 1,
                stack: 1000,
            },
        ],
        dealt_at: None,
        remaining_deck: vec![],
    })
}

#[test]
fn config_exposes_hand_aggregate_metadata() {
    let cfg = HandAggregate.config();
    assert_eq!(cfg.kind(), Kind::CommandHandler);
    match cfg {
        HandlerConfig::CommandHandler {
            domain,
            handled,
            applies,
            state_factory,
            ..
        } => {
            assert_eq!(domain, "hand");
            assert!(handled.iter().any(|u| u.ends_with("DealCards")));
            assert!(handled.iter().any(|u| u.ends_with("PostBlind")));
            assert!(handled.iter().any(|u| u.ends_with("PlayerAction")));
            assert!(applies.iter().any(|u| u.ends_with("CardsDealt")));
            assert!(applies.iter().any(|u| u.ends_with("HandComplete")));
            assert!(state_factory.is_some());
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
fn dispatch_deal_cards_returns_cards_dealt() {
    let router = build();
    let ctx = contextual(deal_cards_cmd(), vec![]);
    let resp: BusinessResponse = router.dispatch(ctx).expect("dispatch ok");
    let eb = match resp.result {
        Some(business_response::Result::Events(eb)) => eb,
        other => panic!("expected events, got {:?}", other),
    };
    assert_eq!(eb.pages.len(), 1);
}

// -----------------------------------------------------------------------------
// Handlers — one test per `#[handles]` branch.
// -----------------------------------------------------------------------------

fn run(ctx: ContextualCommand) -> Result<BusinessResponse, angzarr_client::ClientError> {
    build().dispatch(ctx)
}

#[test]
fn dispatch_post_blind_arm() {
    let ctx = contextual(
        PostBlind {
            player_root: vec![0x01; 8],
            blind_type: "small".into(),
            amount: 10,
        },
        vec![cards_dealt_event()],
    );
    let _ = run(ctx);
}

#[test]
fn dispatch_player_action_arm() {
    let ctx = contextual(
        PlayerAction {
            player_root: vec![0x01; 8],
            action: ActionType::Call as i32,
            amount: 20,
        },
        vec![cards_dealt_event()],
    );
    let _ = run(ctx);
}

#[test]
fn dispatch_deal_community_cards_arm() {
    let ctx = contextual(DealCommunityCards { count: 3 }, vec![cards_dealt_event()]);
    let _ = run(ctx);
}

#[test]
fn dispatch_request_draw_arm() {
    let ctx = contextual(
        RequestDraw {
            player_root: vec![0x01; 8],
            card_indices: vec![0, 1],
        },
        vec![cards_dealt_event()],
    );
    let _ = run(ctx);
}

#[test]
fn dispatch_reveal_cards_arm() {
    let ctx = contextual(
        RevealCards {
            player_root: vec![0x01; 8],
            muck: false,
        },
        vec![cards_dealt_event()],
    );
    let _ = run(ctx);
}

#[test]
fn dispatch_award_pot_arm() {
    let ctx = contextual(
        AwardPot {
            awards: vec![PotAward {
                player_root: vec![0x01; 8],
                amount: 100,
                pot_type: "main".into(),
            }],
        },
        vec![cards_dealt_event()],
    );
    let _ = run(ctx);
}

// -----------------------------------------------------------------------------
// Applier branches — exercise every `#[applies]` arm during replay.
// -----------------------------------------------------------------------------

fn replay_then_run(event_any: Any) -> Result<BusinessResponse, angzarr_client::ClientError> {
    let cmd_any = pack(&deal_cards_cmd());
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
fn apply_cards_dealt_branch() {
    let _ = replay_then_run(cards_dealt_event());
}

#[test]
fn apply_blind_posted_branch() {
    let _ = replay_then_run(pack(&BlindPosted {
        player_root: vec![0x01; 8],
        blind_type: "small".into(),
        amount: 10,
        player_stack: 990,
        pot_total: 10,
        posted_at: None,
    }));
}

#[test]
fn apply_action_taken_branch() {
    let _ = replay_then_run(pack(&ActionTaken {
        player_root: vec![0x01; 8],
        action: ActionType::Call as i32,
        amount: 20,
        player_stack: 980,
        pot_total: 30,
        amount_to_call: 0,
        action_at: None,
    }));
}

#[test]
fn apply_betting_round_complete_branch() {
    let _ = replay_then_run(pack(&BettingRoundComplete {
        completed_phase: BettingPhase::Preflop as i32,
        pot_total: 60,
        stacks: vec![],
        completed_at: None,
    }));
}

#[test]
fn apply_community_cards_dealt_branch() {
    let _ = replay_then_run(pack(&CommunityCardsDealt {
        cards: vec![],
        phase: BettingPhase::Flop as i32,
        all_community_cards: vec![],
        dealt_at: None,
    }));
}

#[test]
fn apply_draw_completed_branch() {
    let _ = replay_then_run(pack(&DrawCompleted {
        player_root: vec![0x01; 8],
        cards_discarded: 2,
        cards_drawn: 2,
        new_cards: vec![],
        drawn_at: None,
    }));
}

#[test]
fn apply_showdown_started_branch() {
    let _ = replay_then_run(pack(&ShowdownStarted {
        players_to_show: vec![vec![0x01; 8], vec![0x02; 8]],
        started_at: None,
    }));
}

#[test]
fn apply_pot_awarded_branch() {
    let _ = replay_then_run(pack(&PotAwarded {
        winners: vec![],
        awarded_at: None,
    }));
}

#[test]
fn apply_hand_complete_branch() {
    let _ = replay_then_run(pack(&HandComplete {
        table_root: vec![0xAA; 16],
        hand_number: 1,
        winners: vec![],
        final_stacks: vec![],
        completed_at: None,
    }));
}
