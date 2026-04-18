//! Hand aggregate library.

pub mod game_rules;
pub mod handlers;
pub mod state;

pub use state::{HandState, PlayerHandState, PotState};

use angzarr_client::proto::EventBook;
use angzarr_client::{aggregate, CommandResult};
use examples_proto::{
    ActionTaken, AwardPot, BettingRoundComplete, BlindPosted, CardsDealt, CommunityCardsDealt,
    DealCards, DealCommunityCards, DrawCompleted, HandComplete, PlayerAction, PostBlind,
    PotAwarded, RequestDraw, RevealCards, ShowdownStarted,
};

use crate::state::{
    apply_action_taken, apply_betting_round_complete, apply_blind_posted, apply_cards_dealt,
    apply_community_cards_dealt, apply_draw_completed, apply_hand_complete, apply_pot_awarded,
    apply_showdown_started, new_hand_state,
};

pub struct HandAggregate;

#[aggregate(domain = "hand", state = HandState)]
impl HandAggregate {
    #[state_factory]
    fn default_state() -> HandState {
        new_hand_state()
    }

    // --- Command handlers ---

    #[handles(DealCards)]
    fn on_deal_cards(
        &self,
        cmd: DealCards,
        state: &HandState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        handlers::handle_deal_cards(cmd, state, seq)
    }

    #[handles(PostBlind)]
    fn on_post_blind(
        &self,
        cmd: PostBlind,
        state: &HandState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        handlers::handle_post_blind(cmd, state, seq)
    }

    #[handles(PlayerAction)]
    fn on_player_action(
        &self,
        cmd: PlayerAction,
        state: &HandState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        handlers::handle_player_action(cmd, state, seq)
    }

    #[handles(DealCommunityCards)]
    fn on_deal_community_cards(
        &self,
        cmd: DealCommunityCards,
        state: &HandState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        handlers::handle_deal_community_cards(cmd, state, seq)
    }

    #[handles(RequestDraw)]
    fn on_request_draw(
        &self,
        cmd: RequestDraw,
        state: &HandState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        handlers::handle_request_draw(cmd, state, seq)
    }

    #[handles(RevealCards)]
    fn on_reveal_cards(
        &self,
        cmd: RevealCards,
        state: &HandState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        handlers::handle_reveal_cards(cmd, state, seq)
    }

    #[handles(AwardPot)]
    fn on_award_pot(
        &self,
        cmd: AwardPot,
        state: &HandState,
        seq: u32,
    ) -> CommandResult<EventBook> {
        handlers::handle_award_pot(cmd, state, seq)
    }

    // --- Event appliers ---

    #[applies(CardsDealt)]
    fn apply_cards_dealt(state: &mut HandState, event: CardsDealt) {
        apply_cards_dealt(state, event);
    }

    #[applies(BlindPosted)]
    fn apply_blind_posted(state: &mut HandState, event: BlindPosted) {
        apply_blind_posted(state, event);
    }

    #[applies(ActionTaken)]
    fn apply_action_taken(state: &mut HandState, event: ActionTaken) {
        apply_action_taken(state, event);
    }

    #[applies(BettingRoundComplete)]
    fn apply_betting_round_complete(state: &mut HandState, event: BettingRoundComplete) {
        apply_betting_round_complete(state, event);
    }

    #[applies(CommunityCardsDealt)]
    fn apply_community_cards_dealt(state: &mut HandState, event: CommunityCardsDealt) {
        apply_community_cards_dealt(state, event);
    }

    #[applies(DrawCompleted)]
    fn apply_draw_completed(state: &mut HandState, event: DrawCompleted) {
        apply_draw_completed(state, event);
    }

    #[applies(ShowdownStarted)]
    fn apply_showdown_started(state: &mut HandState, event: ShowdownStarted) {
        apply_showdown_started(state, event);
    }

    #[applies(PotAwarded)]
    fn apply_pot_awarded(state: &mut HandState, event: PotAwarded) {
        apply_pot_awarded(state, event);
    }

    #[applies(HandComplete)]
    fn apply_hand_complete(state: &mut HandState, event: HandComplete) {
        apply_hand_complete(state, event);
    }
}

#[cfg(test)]
mod applier_tests {
    //! Direct-call tests for `#[applies]` methods and the `#[state_factory]`
    //! on `HandAggregate`. Catch mutants replacing delegation bodies.
    use super::*;
    use examples_proto::{
        ActionType, BettingPhase, Card, GameVariant, PlayerHoleCards, PlayerInHand,
        PlayerStackSnapshot, PotWinner, Rank, Suit,
    };

    fn dealt_state() -> HandState {
        let mut state = new_hand_state();
        apply_cards_dealt(
            &mut state,
            CardsDealt {
                table_root: vec![0xab],
                hand_number: 1,
                game_variant: GameVariant::TexasHoldem as i32,
                player_cards: vec![],
                dealer_position: 0,
                players: vec![
                    PlayerInHand {
                        player_root: vec![1],
                        position: 0,
                        stack: 500,
                    },
                    PlayerInHand {
                        player_root: vec![2],
                        position: 1,
                        stack: 500,
                    },
                ],
                dealt_at: None,
                remaining_deck: vec![
                    Card {
                        suit: Suit::Clubs as i32,
                        rank: Rank::Two as i32,
                    };
                    10
                ],
            },
        );
        state
    }

    #[test]
    fn default_state_factory_returns_state_with_main_pot() {
        let state = HandAggregate::default_state();
        assert_eq!(state.pots.len(), 1);
        assert_eq!(state.pots[0].pot_type, "main");
    }

    #[test]
    fn apply_cards_dealt_delegates_to_state_fn() {
        let mut state = new_hand_state();
        HandAggregate::apply_cards_dealt(
            &mut state,
            CardsDealt {
                table_root: vec![0xab],
                hand_number: 1,
                game_variant: GameVariant::TexasHoldem as i32,
                player_cards: vec![],
                dealer_position: 0,
                players: vec![PlayerInHand {
                    player_root: vec![1],
                    position: 0,
                    stack: 500,
                }],
                dealt_at: None,
                remaining_deck: vec![],
            },
        );
        assert!(state.exists());
        assert_eq!(state.hand_number, 1);
        assert_eq!(state.current_phase, BettingPhase::Preflop);
    }

    #[test]
    fn apply_blind_posted_delegates_to_state_fn() {
        let mut state = dealt_state();
        HandAggregate::apply_blind_posted(
            &mut state,
            BlindPosted {
                player_root: vec![1],
                blind_type: "small".into(),
                amount: 10,
                player_stack: 490,
                pot_total: 10,
                posted_at: None,
            },
        );
        assert_eq!(state.pots[0].amount, 10);
        assert_eq!(state.current_bet, 10);
        assert_eq!(state.get_player(&[1]).unwrap().stack, 490);
    }

    #[test]
    fn apply_action_taken_delegates_to_state_fn() {
        let mut state = dealt_state();
        HandAggregate::apply_action_taken(
            &mut state,
            ActionTaken {
                player_root: vec![1],
                action: ActionType::Fold as i32,
                amount: 0,
                player_stack: 500,
                pot_total: 0,
                amount_to_call: 0,
                action_at: None,
            },
        );
        assert!(state.get_player(&[1]).unwrap().has_folded);
    }

    #[test]
    fn apply_betting_round_complete_delegates_to_state_fn() {
        let mut state = dealt_state();
        state.current_bet = 50;
        HandAggregate::apply_betting_round_complete(
            &mut state,
            BettingRoundComplete {
                completed_phase: BettingPhase::Preflop as i32,
                pot_total: 100,
                stacks: vec![PlayerStackSnapshot {
                    player_root: vec![1],
                    stack: 400,
                    is_all_in: false,
                    has_folded: false,
                }],
                completed_at: None,
            },
        );
        assert_eq!(state.current_bet, 0);
        assert_eq!(state.get_player(&[1]).unwrap().stack, 400);
    }

    #[test]
    fn apply_community_cards_dealt_delegates_to_state_fn() {
        let mut state = dealt_state();
        HandAggregate::apply_community_cards_dealt(
            &mut state,
            CommunityCardsDealt {
                cards: vec![Card {
                    suit: 0,
                    rank: 10,
                }],
                phase: BettingPhase::Flop as i32,
                all_community_cards: vec![Card {
                    suit: 0,
                    rank: 10,
                }],
                dealt_at: None,
            },
        );
        assert_eq!(state.community_cards.len(), 1);
        assert_eq!(state.current_phase, BettingPhase::Flop);
    }

    #[test]
    fn apply_draw_completed_delegates_to_state_fn() {
        let mut state = new_hand_state();
        apply_cards_dealt(
            &mut state,
            CardsDealt {
                table_root: vec![0xab],
                hand_number: 1,
                game_variant: GameVariant::FiveCardDraw as i32,
                player_cards: vec![PlayerHoleCards {
                    player_root: vec![1],
                    cards: vec![
                        Card { suit: 0, rank: 2 },
                        Card { suit: 1, rank: 3 },
                        Card { suit: 2, rank: 4 },
                        Card { suit: 3, rank: 5 },
                        Card { suit: 0, rank: 6 },
                    ],
                }],
                dealer_position: 0,
                players: vec![PlayerInHand {
                    player_root: vec![1],
                    position: 0,
                    stack: 500,
                }],
                dealt_at: None,
                remaining_deck: vec![Card { suit: 1, rank: 7 }; 10],
            },
        );
        let new_cards = vec![
            Card { suit: 1, rank: 7 },
            Card { suit: 1, rank: 7 },
            Card { suit: 2, rank: 4 },
            Card { suit: 3, rank: 5 },
            Card { suit: 0, rank: 6 },
        ];
        HandAggregate::apply_draw_completed(
            &mut state,
            DrawCompleted {
                player_root: vec![1],
                cards_discarded: 2,
                cards_drawn: 2,
                new_cards: new_cards.clone(),
                drawn_at: None,
            },
        );
        assert_eq!(state.get_player(&[1]).unwrap().hole_cards, new_cards);
    }

    #[test]
    fn apply_showdown_started_delegates_to_state_fn() {
        let mut state = dealt_state();
        HandAggregate::apply_showdown_started(
            &mut state,
            ShowdownStarted {
                players_to_show: vec![vec![1]],
                started_at: None,
            },
        );
        assert_eq!(state.status, "showdown");
    }

    #[test]
    fn apply_pot_awarded_delegates_to_state_fn() {
        let mut state = dealt_state();
        let before = state.get_player(&[1]).unwrap().stack;
        HandAggregate::apply_pot_awarded(
            &mut state,
            PotAwarded {
                winners: vec![PotWinner {
                    player_root: vec![1],
                    amount: 100,
                    pot_type: "main".into(),
                    winning_hand: None,
                }],
                awarded_at: None,
            },
        );
        assert_eq!(state.get_player(&[1]).unwrap().stack, before + 100);
    }

    #[test]
    fn apply_hand_complete_delegates_to_state_fn() {
        let mut state = dealt_state();
        HandAggregate::apply_hand_complete(
            &mut state,
            HandComplete {
                table_root: vec![0xab],
                hand_number: 1,
                winners: vec![],
                final_stacks: vec![PlayerStackSnapshot {
                    player_root: vec![1],
                    stack: 600,
                    is_all_in: false,
                    has_folded: false,
                }],
                completed_at: None,
            },
        );
        assert_eq!(state.status, "complete");
        assert!(state.is_complete());
        assert_eq!(state.get_player(&[1]).unwrap().stack, 600);
    }
}

#[cfg(test)]
mod handler_tests {
    //! Direct-call tests for each `#[handles]` method on `HandAggregate`.
    use super::*;
    use examples_proto::{
        ActionType, BettingPhase, Card, GameVariant, PlayerHoleCards, PlayerInHand, PotAward, Rank,
        Suit,
    };

    fn dealt_state() -> HandState {
        let mut state = new_hand_state();
        apply_cards_dealt(
            &mut state,
            CardsDealt {
                table_root: vec![0xab],
                hand_number: 1,
                game_variant: GameVariant::TexasHoldem as i32,
                player_cards: vec![PlayerHoleCards {
                    player_root: vec![1],
                    cards: vec![
                        Card {
                            suit: Suit::Clubs as i32,
                            rank: Rank::Ace as i32,
                        },
                        Card {
                            suit: Suit::Diamonds as i32,
                            rank: Rank::King as i32,
                        },
                    ],
                }],
                dealer_position: 0,
                players: vec![
                    PlayerInHand {
                        player_root: vec![1],
                        position: 0,
                        stack: 500,
                    },
                    PlayerInHand {
                        player_root: vec![2],
                        position: 1,
                        stack: 500,
                    },
                ],
                dealt_at: None,
                remaining_deck: vec![
                    Card {
                        suit: Suit::Clubs as i32,
                        rank: Rank::Two as i32
                    };
                    48
                ],
            },
        );
        state
    }

    #[test]
    fn on_deal_cards_delegates_to_handler() {
        let agg = HandAggregate;
        let state = new_hand_state();
        let book = agg
            .on_deal_cards(
                DealCards {
                    table_root: vec![0xab, 0xcd],
                    hand_number: 1,
                    game_variant: GameVariant::TexasHoldem as i32,
                    players: vec![
                        PlayerInHand {
                            player_root: vec![1],
                            position: 0,
                            stack: 500,
                        },
                        PlayerInHand {
                            player_root: vec![2],
                            position: 1,
                            stack: 500,
                        },
                    ],
                    dealer_position: 0,
                    small_blind: 5,
                    big_blind: 10,
                    deck_seed: vec![0u8; 32],
                },
                &state,
                0,
            )
            .expect("handler should succeed");
        assert_eq!(book.pages.len(), 1);
    }

    #[test]
    fn on_post_blind_delegates_to_handler() {
        let agg = HandAggregate;
        let state = dealt_state();
        let book = agg
            .on_post_blind(
                PostBlind {
                    player_root: vec![1],
                    blind_type: "small".into(),
                    amount: 5,
                },
                &state,
                1,
            )
            .expect("handler should succeed");
        assert_eq!(book.pages.len(), 1);
    }

    #[test]
    fn on_player_action_delegates_to_handler() {
        let agg = HandAggregate;
        let state = dealt_state();
        let book = agg
            .on_player_action(
                PlayerAction {
                    player_root: vec![1],
                    action: ActionType::Fold as i32,
                    amount: 0,
                },
                &state,
                2,
            )
            .expect("handler should succeed");
        assert_eq!(book.pages.len(), 1);
    }

    #[test]
    fn on_deal_community_cards_delegates_to_handler() {
        let agg = HandAggregate;
        let state = dealt_state();
        let book = agg
            .on_deal_community_cards(DealCommunityCards { count: 3 }, &state, 3)
            .expect("handler should succeed");
        assert_eq!(book.pages.len(), 1);
    }

    #[test]
    fn on_request_draw_delegates_to_handler() {
        let agg = HandAggregate;
        // Need Five Card Draw variant + Draw phase.
        let mut state = new_hand_state();
        apply_cards_dealt(
            &mut state,
            CardsDealt {
                table_root: vec![0xab],
                hand_number: 1,
                game_variant: GameVariant::FiveCardDraw as i32,
                player_cards: vec![PlayerHoleCards {
                    player_root: vec![1],
                    cards: vec![
                        Card { suit: 0, rank: 2 },
                        Card { suit: 1, rank: 3 },
                        Card { suit: 2, rank: 4 },
                        Card { suit: 3, rank: 5 },
                        Card { suit: 0, rank: 6 },
                    ],
                }],
                dealer_position: 0,
                players: vec![
                    PlayerInHand {
                        player_root: vec![1],
                        position: 0,
                        stack: 500,
                    },
                    PlayerInHand {
                        player_root: vec![2],
                        position: 1,
                        stack: 500,
                    },
                ],
                dealt_at: None,
                remaining_deck: vec![Card { suit: 1, rank: 7 }; 10],
            },
        );
        state.current_phase = BettingPhase::Draw;
        let book = agg
            .on_request_draw(
                RequestDraw {
                    player_root: vec![1],
                    card_indices: vec![0, 1],
                },
                &state,
                4,
            )
            .expect("handler should succeed");
        assert_eq!(book.pages.len(), 1);
    }

    #[test]
    fn on_reveal_cards_delegates_to_handler() {
        let agg = HandAggregate;
        let state = dealt_state();
        let book = agg
            .on_reveal_cards(
                RevealCards {
                    player_root: vec![1],
                    muck: false,
                },
                &state,
                5,
            )
            .expect("handler should succeed");
        assert_eq!(book.pages.len(), 1);
    }

    #[test]
    fn on_award_pot_delegates_to_handler() {
        let agg = HandAggregate;
        let mut state = dealt_state();
        // Seed the pot.
        apply_blind_posted(
            &mut state,
            BlindPosted {
                player_root: vec![1],
                blind_type: "small".into(),
                amount: 100,
                player_stack: 400,
                pot_total: 100,
                posted_at: None,
            },
        );
        let book = agg
            .on_award_pot(
                AwardPot {
                    awards: vec![PotAward {
                        player_root: vec![1],
                        amount: 100,
                        pot_type: "main".into(),
                    }],
                },
                &state,
                6,
            )
            .expect("handler should succeed");
        // handle_award_pot emits two events (PotAwarded + HandComplete).
        assert_eq!(book.pages.len(), 2);
    }
}
