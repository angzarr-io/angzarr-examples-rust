//! Hand Flow Process Manager library.
//!
//! Coordinates the workflow between the `table` and `hand` domains. Handler
//! coverage now mirrors the Python reference
//! (`examples-python/main/hand-flow/main.py` wrapping `hand_process.py`):
//! every event the Python PM handles has a matching `#[handles]` arm here,
//! with corresponding `#[applies]` rules so PM state survives restart via
//! Tier 5 `process_events` replay.
//!
//! Persisted phase tracking uses [`state_machine::Phase`] (9 phases), which
//! mirrors Python's `HandPhase` one-to-one (`AwaitingDeal` ↔ `WAITING_FOR_START`,
//! `AwardingPot` newly added).
//!
//! Handler coverage:
//!   table.HandStarted           → init state. The `saga-table-hand` saga
//!                                  emits `DealCards`; the PM is state-only.
//!   hand.CardsDealt             → `PostBlind` (small)
//!   hand.BlindPosted            → `PostBlind` (big) on small, else state-only
//!   hand.ActionTaken            → state-only (BettingRoundComplete drives next phase)
//!   hand.BettingRoundComplete   → `DealCommunityCards` | `AwardPot` per variant
//!   hand.CommunityCardsDealt    → state-only (reset round, enter Betting)
//!   hand.ShowdownStarted        → `AwardPot`
//!   hand.PotAwarded             → state-only (phase = AwardingPot)
//!   hand.HandComplete           → `EndHand` (Rust convenience over Python,
//!                                  whose hand→table saga handles this)

pub mod state_machine;

pub use state_machine::Phase as HandPhase;
pub use state_machine::{
    Action as SmAction, BettingPhase as SmBettingPhase, BlindKind, Command as SmCommand,
    HandProcess, Phase as SmPhase, PlayerState as SmPlayer,
};

use std::collections::HashMap;

use angzarr_client::proto::command_page::Payload as CommandPayload;
use angzarr_client::proto::{
    event_page::Payload as EventPayload, page_header::SequenceType, CommandBook, CommandPage,
    Cover, EventBook, EventPage, MergeStrategy, PageHeader, ProcessManagerHandleResponse,
    SyncMode, Uuid as ProtoUuid,
};
use angzarr_client::{process_manager, type_url, CommandResult};
use examples_proto::{
    ActionTaken, ActionType, AwardPot, BettingPhase, BettingRoundComplete, BlindPosted, CardsDealt,
    CommunityCardsDealt, DealCommunityCards, EndHand, GameVariant, HandComplete, HandStarted,
    PostBlind, PotAward, PotAwarded, ShowdownStarted,
};
use examples_utils::pack_event;
use prost::Message;
use prost_types::Any;

/// Per-player state tracked across betting rounds. Mirrors python
/// `hand_process.PlayerState`.
#[derive(Default, Clone, Debug)]
pub struct PMPlayerState {
    pub player_root: Vec<u8>,
    pub position: i32,
    pub stack: i64,
    pub bet_this_round: i64,
    pub total_invested: i64,
    pub has_acted: bool,
    pub has_folded: bool,
    pub is_all_in: bool,
}

// docs:start:pm_state
#[derive(Default, Clone, Debug)]
pub struct HandFlowState {
    // --- Routing / cover info ---
    pub hand_root: Vec<u8>,
    pub hand_number: i64,
    pub game_variant: GameVariant,
    pub dealer_position: i32,
    pub small_blind_position: i32,
    pub big_blind_position: i32,
    pub small_blind: i64,
    pub big_blind: i64,
    /// (position, player_root) pairs from the triggering HandStarted event.
    /// Kept for backwards-compatibility with consumers that read the
    /// pre-expansion shape.
    pub active_players: Vec<(i32, Vec<u8>)>,
    pub phase: HandPhase,

    // --- Per-round betting state (mirrors python `HandProcess`) ---
    /// Most recently completed/active betting phase as a `BettingPhase` i32.
    pub betting_phase: i32,
    pub current_bet: i64,
    pub min_raise: i64,
    pub pot_total: i64,
    pub small_blind_posted: bool,
    pub big_blind_posted: bool,
    pub action_on: i32,
    pub last_aggressor: i32,
    pub community_card_count: i32,
    pub players: HashMap<i32, PMPlayerState>,
}
// docs:end:pm_state

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

    fn position_for_root(&self, root: &[u8]) -> Option<i32> {
        self.players.iter().find_map(|(pos, p)| {
            if p.player_root == root {
                Some(*pos)
            } else {
                None
            }
        })
    }

    /// Next un-folded, un-all-in seat after `after`. Returns `-1` if none.
    fn next_active_position(&self, after: i32) -> i32 {
        let mut positions: Vec<i32> = self.players.keys().copied().collect();
        positions.sort();
        let n = positions.len();
        if n == 0 {
            return -1;
        }
        let start = positions
            .iter()
            .position(|p| *p > after)
            .unwrap_or(0);
        for i in 0..n {
            let idx = (start + i) % n;
            let pos = positions[idx];
            if let Some(p) = self.players.get(&pos) {
                if !p.has_folded && !p.is_all_in {
                    return pos;
                }
            }
        }
        -1
    }
}

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
    /// Table started a new hand → initialize PM state. The
    /// `saga-table-hand` saga emits `DealCards`; the PM is state-only on
    /// this trigger to mirror python's `HandFlowPM.on_hand_started`
    /// (`hand-flow/main.py:93`), which just calls `start_hand` without
    /// returning a command.
    #[handles(HandStarted)]
    fn on_hand_started(
        &self,
        event: HandStarted,
        _state: &HandFlowState,
    ) -> CommandResult<ProcessManagerHandleResponse> {
        let pm_event = pack_event(&event, "examples.HandStarted");
        Ok(ProcessManagerHandleResponse {
            commands: vec![],
            process_events: vec![single_event_book(pm_event)],
            facts: vec![],
        })
    }

    /// Cards dealt → post the small blind. Mirrors python
    /// `handle_cards_dealt` (`hand_process.py:306`).
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

    /// Blind posted → if it was the small, post the big; if it was the
    /// big, betting begins (no command — players act). Mirrors python
    /// `handle_blind_posted` (`hand_process.py:320`).
    #[handles(BlindPosted)]
    fn on_blind_posted(
        &self,
        event: BlindPosted,
        state: &HandFlowState,
    ) -> CommandResult<ProcessManagerHandleResponse> {
        let cmds = if event.blind_type == "small" {
            let big_blind_player = state
                .player_at_position(state.big_blind_position)
                .map(|r| r.to_vec())
                .unwrap_or_default();
            let post_big = PostBlind {
                player_root: big_blind_player,
                blind_type: "big".to_string(),
                amount: state.big_blind,
            };
            vec![make_command(
                "hand",
                &state.hand_root,
                "examples.PostBlind",
                &post_big,
            )]
        } else {
            vec![]
        };
        let pm_event = pack_event(&event, "examples.BlindPosted");
        Ok(ProcessManagerHandleResponse {
            commands: cmds,
            process_events: vec![single_event_book(pm_event)],
            facts: vec![],
        })
    }

    /// Player acted → state-only. Phase advancement is event-driven via
    /// `BettingRoundComplete`, matching the unified Tier 5 hand-aggregate
    /// design. Mirrors the state-update half of python `handle_action_taken`
    /// (`hand_process.py:350`); python's command emission half collapses
    /// here because the hand aggregate emits `BettingRoundComplete` once
    /// the round terminates.
    #[handles(ActionTaken)]
    fn on_action_taken(
        &self,
        event: ActionTaken,
        _state: &HandFlowState,
    ) -> CommandResult<ProcessManagerHandleResponse> {
        let pm_event = pack_event(&event, "examples.ActionTaken");
        Ok(ProcessManagerHandleResponse {
            commands: vec![],
            process_events: vec![single_event_book(pm_event)],
            facts: vec![],
        })
    }

    /// Betting round complete → deal next street or go to showdown.
    /// Mirrors python `_end_betting_round_cmd` (`hand_process.py:573`).
    #[handles(BettingRoundComplete)]
    fn on_betting_round_complete(
        &self,
        event: BettingRoundComplete,
        state: &HandFlowState,
    ) -> CommandResult<ProcessManagerHandleResponse> {
        let completed = BettingPhase::try_from(event.completed_phase).ok();
        let cmds = match (state.game_variant, completed) {
            (GameVariant::FiveCardDraw, Some(BettingPhase::Preflop)) => {
                // Five-card draw: enter draw phase; no command (player draws drive next).
                vec![]
            }
            (_, Some(BettingPhase::Preflop)) => {
                let cmd = DealCommunityCards { count: 3 };
                vec![make_command(
                    "hand",
                    &state.hand_root,
                    "examples.DealCommunityCards",
                    &cmd,
                )]
            }
            (_, Some(BettingPhase::Flop)) | (_, Some(BettingPhase::Turn)) => {
                let cmd = DealCommunityCards { count: 1 };
                vec![make_command(
                    "hand",
                    &state.hand_root,
                    "examples.DealCommunityCards",
                    &cmd,
                )]
            }
            (_, Some(BettingPhase::River)) | (_, Some(BettingPhase::Draw)) => {
                vec![make_award_pot_command(state)]
            }
            _ => vec![],
        };
        let pm_event = pack_event(&event, "examples.BettingRoundComplete");
        Ok(ProcessManagerHandleResponse {
            commands: cmds,
            process_events: vec![single_event_book(pm_event)],
            facts: vec![],
        })
    }

    /// Community cards dealt → state-only (round reset, action moves to
    /// first seat after dealer). Mirrors python `handle_community_cards_dealt`
    /// (`hand_process.py:411`).
    #[handles(CommunityCardsDealt)]
    fn on_community_cards_dealt(
        &self,
        event: CommunityCardsDealt,
        _state: &HandFlowState,
    ) -> CommandResult<ProcessManagerHandleResponse> {
        let pm_event = pack_event(&event, "examples.CommunityCardsDealt");
        Ok(ProcessManagerHandleResponse {
            commands: vec![],
            process_events: vec![single_event_book(pm_event)],
            facts: vec![],
        })
    }

    /// Showdown started → award the pot. Mirrors python
    /// `handle_showdown_started` (`hand_process.py:436`).
    #[handles(ShowdownStarted)]
    fn on_showdown_started(
        &self,
        event: ShowdownStarted,
        state: &HandFlowState,
    ) -> CommandResult<ProcessManagerHandleResponse> {
        let cmd = make_award_pot_command(state);
        let pm_event = pack_event(&event, "examples.ShowdownStarted");
        Ok(ProcessManagerHandleResponse {
            commands: vec![cmd],
            process_events: vec![single_event_book(pm_event)],
            facts: vec![],
        })
    }

    /// Pot awarded → state-only. Phase moves to `AwardingPot`; the
    /// follow-up `HandComplete` event drives the `EndHand` emit. Mirrors
    /// python `handle_pot_awarded` (`hand_process.py:447`).
    #[handles(PotAwarded)]
    fn on_pot_awarded(
        &self,
        event: PotAwarded,
        _state: &HandFlowState,
    ) -> CommandResult<ProcessManagerHandleResponse> {
        let pm_event = pack_event(&event, "examples.PotAwarded");
        Ok(ProcessManagerHandleResponse {
            commands: vec![],
            process_events: vec![single_event_book(pm_event)],
            facts: vec![],
        })
    }

    /// Hand completed → instruct the table to end the hand. Rust-only
    /// convenience: python emits `EndHand` from its hand→table saga, but
    /// the unified Rust topology drives it from the PM with the full
    /// winners list at hand.
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
            .iter()
            .map(|seat| (seat.position, seat.player_root.clone()))
            .collect();
        state.players.clear();
        for seat in event.active_players {
            state.players.insert(
                seat.position,
                PMPlayerState {
                    player_root: seat.player_root,
                    position: seat.position,
                    stack: seat.stack,
                    ..Default::default()
                },
            );
        }
        state.phase = HandPhase::Dealing;
        state.min_raise = state.big_blind;
        state.betting_phase = BettingPhase::Preflop as i32;
    }

    #[applies(CardsDealt)]
    fn apply_cards_dealt(state: &mut HandFlowState, _event: CardsDealt) {
        state.phase = HandPhase::PostingBlinds;
    }

    #[applies(BlindPosted)]
    fn apply_blind_posted(state: &mut HandFlowState, event: BlindPosted) {
        if let Some(pos) = state.position_for_root(&event.player_root) {
            if let Some(p) = state.players.get_mut(&pos) {
                p.stack = event.player_stack;
                p.bet_this_round = event.amount;
                p.total_invested = event.amount;
            }
        }
        state.pot_total = event.pot_total;
        state.current_bet = event.amount;
        if event.blind_type == "small" {
            state.small_blind_posted = true;
        } else if event.blind_type == "big" {
            state.big_blind_posted = true;
            state.phase = HandPhase::Betting;
            state.action_on = state.next_active_position(state.big_blind_position);
        }
    }

    #[applies(ActionTaken)]
    fn apply_action_taken(state: &mut HandFlowState, event: ActionTaken) {
        if let Some(pos) = state.position_for_root(&event.player_root) {
            if let Some(p) = state.players.get_mut(&pos) {
                p.stack = event.player_stack;
                p.has_acted = true;
                let action = ActionType::try_from(event.action).unwrap_or(ActionType::ActionUnspecified);
                match action {
                    ActionType::Fold => p.has_folded = true,
                    ActionType::AllIn => {
                        p.is_all_in = true;
                        p.bet_this_round += event.amount;
                        p.total_invested += event.amount;
                    }
                    ActionType::Call | ActionType::Bet | ActionType::Raise => {
                        p.bet_this_round += event.amount;
                        p.total_invested += event.amount;
                    }
                    _ => {}
                }
                let aggressive = matches!(
                    action,
                    ActionType::Bet | ActionType::Raise | ActionType::AllIn
                );
                if aggressive && p.bet_this_round > state.current_bet {
                    let raise_amt = p.bet_this_round - state.current_bet;
                    state.current_bet = p.bet_this_round;
                    state.min_raise = state.min_raise.max(raise_amt);
                    state.last_aggressor = pos;
                    for (k, other) in state.players.iter_mut() {
                        if *k != pos && !other.has_folded && !other.is_all_in {
                            other.has_acted = false;
                        }
                    }
                }
            }
        }
        state.pot_total = event.pot_total;
        let next = state.next_active_position(state.action_on);
        state.action_on = next;
    }

    #[applies(BettingRoundComplete)]
    fn apply_betting_round_complete(state: &mut HandFlowState, event: BettingRoundComplete) {
        state.betting_phase = event.completed_phase;
        state.pot_total = event.pot_total;
        let completed = BettingPhase::try_from(event.completed_phase).ok();
        match (state.game_variant, completed) {
            (GameVariant::FiveCardDraw, Some(BettingPhase::Preflop)) => {
                state.phase = HandPhase::Draw;
            }
            (_, Some(BettingPhase::Preflop))
            | (_, Some(BettingPhase::Flop))
            | (_, Some(BettingPhase::Turn)) => {
                state.phase = HandPhase::DealingCommunity;
            }
            (_, Some(BettingPhase::River)) | (_, Some(BettingPhase::Draw)) => {
                state.phase = HandPhase::Showdown;
            }
            _ => {}
        }
    }

    #[applies(CommunityCardsDealt)]
    fn apply_community_cards_dealt(state: &mut HandFlowState, event: CommunityCardsDealt) {
        state.community_card_count = event.all_community_cards.len() as i32;
        state.betting_phase = event.phase;
        for p in state.players.values_mut() {
            p.bet_this_round = 0;
            p.has_acted = false;
        }
        state.current_bet = 0;
        state.phase = HandPhase::Betting;
        state.action_on = state.next_active_position(state.dealer_position);
    }

    #[applies(ShowdownStarted)]
    fn apply_showdown_started(state: &mut HandFlowState, _event: ShowdownStarted) {
        state.phase = HandPhase::Showdown;
    }

    #[applies(PotAwarded)]
    fn apply_pot_awarded(state: &mut HandFlowState, _event: PotAwarded) {
        state.phase = HandPhase::AwardingPot;
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
    // PM-emitted commands: sync_mode = DECISION so the PM gets the
    // accept/reject answer synchronously (projectors + sagas still
    // run async). The HandFlow PM branches on whether each downstream
    // command was accepted (e.g. a rejected PostBlind / EndHand needs
    // immediate compensation, not eventual-consistency reconciliation).
    // See angzarr-project proto: `SYNC_MODE_DECISION` and the PM
    // coordinator's per-command override path
    // (`core/main/src/orchestration/process_manager/mod.rs`).
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
                sync_mode: Some(SyncMode::Decision as i32),
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

/// Build an `AwardPot` for everyone still in the hand. Splits the pot
/// evenly with the remainder going to the lowest-position survivor —
/// matches python `_build_auto_award_command` (`hand_process.py:660`).
fn make_award_pot_command(state: &HandFlowState) -> CommandBook {
    let mut survivors: Vec<&PMPlayerState> = state
        .players
        .values()
        .filter(|p| !p.has_folded)
        .collect();
    survivors.sort_by_key(|p| p.position);
    let n = survivors.len() as i64;
    let split = if n > 0 { state.pot_total / n } else { 0 };
    let remainder = if n > 0 { state.pot_total % n } else { 0 };
    let awards: Vec<PotAward> = survivors
        .iter()
        .enumerate()
        .map(|(i, p)| PotAward {
            player_root: p.player_root.clone(),
            amount: split + if (i as i64) < remainder { 1 } else { 0 },
            pot_type: "main".to_string(),
        })
        .collect();
    let cmd = AwardPot { awards };
    make_command("hand", &state.hand_root, "examples.AwardPot", &cmd)
}

fn single_event_book(event: Any) -> EventBook {
    EventBook {
        cover: None,
        pages: vec![EventPage {
            header: Some(PageHeader {
                sync_mode: None,
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
            ..Default::default()
        }
    }

    fn first_event_any(book: &EventBook) -> Any {
        match book.pages[0].payload.as_ref().unwrap() {
            EventPayload::Event(any) => any.clone(),
            _ => panic!("expected event payload"),
        }
    }

    #[test]
    fn hand_started_emits_pm_event_for_replay_and_no_command() {
        let pm = HandFlowPm;
        let event = sample_hand_started();
        let resp = pm
            .on_hand_started(event.clone(), &HandFlowState::default())
            .unwrap();

        assert!(resp.commands.is_empty(), "PM no longer emits DealCards on HandStarted (saga emits)");
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

    #[test]
    fn blind_posted_small_emits_post_blind_big() {
        let pm = HandFlowPm;
        let mut state = HandFlowState::default();
        // Seed via the apply path so we exercise the same wiring as replay.
        HandFlowPm::apply_hand_started(&mut state, sample_hand_started());

        let event = BlindPosted {
            player_root: vec![2; 16],
            blind_type: "small".to_string(),
            amount: 5,
            player_stack: 495,
            pot_total: 5,
            posted_at: None,
        };
        let resp = pm.on_blind_posted(event, &state).unwrap();
        assert_eq!(resp.commands.len(), 1);
        let page = &resp.commands[0].pages[0];
        let any = match &page.payload {
            Some(CommandPayload::Command(any)) => any.clone(),
            _ => panic!("expected command payload"),
        };
        assert!(any.type_url.ends_with("examples.PostBlind"));
        let post = PostBlind::decode(any.value.as_slice()).unwrap();
        assert_eq!(post.blind_type, "big");
        assert_eq!(post.amount, 10);
        assert_eq!(post.player_root, vec![3u8; 16]);
        // PM-emitted commands carry SyncMode::Decision so the PM gets
        // the accept/reject answer synchronously.
        assert_eq!(
            page.header.as_ref().and_then(|h| h.sync_mode),
            Some(SyncMode::Decision as i32)
        );
    }

    #[test]
    fn betting_round_complete_river_emits_award_pot() {
        let pm = HandFlowPm;
        let mut state = HandFlowState::default();
        HandFlowPm::apply_hand_started(&mut state, sample_hand_started());
        state.pot_total = 30;

        let event = BettingRoundComplete {
            completed_phase: BettingPhase::River as i32,
            pot_total: 30,
            stacks: vec![],
            completed_at: None,
        };
        let resp = pm.on_betting_round_complete(event, &state).unwrap();
        assert_eq!(resp.commands.len(), 1);
        let any = match &resp.commands[0].pages[0].payload {
            Some(CommandPayload::Command(any)) => any.clone(),
            _ => panic!("expected command payload"),
        };
        assert!(any.type_url.ends_with("examples.AwardPot"));
    }

    #[test]
    fn pot_awarded_marks_phase_awarding_pot() {
        let mut state = HandFlowState::default();
        HandFlowPm::apply_hand_started(&mut state, sample_hand_started());
        HandFlowPm::apply_pot_awarded(
            &mut state,
            PotAwarded {
                winners: vec![],
                awarded_at: None,
            },
        );
        assert_eq!(state.phase, HandPhase::AwardingPot);
    }
}
