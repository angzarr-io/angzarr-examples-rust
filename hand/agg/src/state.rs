//! Hand aggregate state and event appliers.

use std::collections::HashMap;

use examples_proto::{
    ActionTaken, ActionType, BettingPhase, BettingRoundComplete, BlindPosted, Card, CardsDealt,
    CommunityCardsDealt, DrawCompleted, GameVariant, HandComplete, PotAwarded, ShowdownStarted,
};

#[derive(Debug, Clone, Default)]
pub struct PlayerHandState {
    pub player_root: Vec<u8>,
    pub position: i32,
    pub hole_cards: Vec<Card>,
    pub stack: i64,
    pub bet_this_round: i64,
    pub total_invested: i64,
    pub has_acted: bool,
    pub has_folded: bool,
    pub is_all_in: bool,
}

#[derive(Debug, Clone, Default)]
pub struct PotState {
    pub amount: i64,
    pub eligible_players: Vec<Vec<u8>>,
    pub pot_type: String,
}

#[derive(Debug, Default, Clone)]
pub struct HandState {
    pub hand_id: String,
    pub table_root: Vec<u8>,
    pub hand_number: i64,
    pub game_variant: GameVariant,

    pub remaining_deck: Vec<Card>,
    pub players: HashMap<String, PlayerHandState>,
    pub community_cards: Vec<Card>,

    pub current_phase: BettingPhase,
    pub action_on_position: i32,
    pub current_bet: i64,
    pub min_raise: i64,
    pub pots: Vec<PotState>,

    pub dealer_position: i32,
    pub small_blind_position: i32,
    pub big_blind_position: i32,

    pub status: String,
}

impl HandState {
    pub fn exists(&self) -> bool {
        !self.hand_id.is_empty()
    }

    pub fn is_complete(&self) -> bool {
        self.status == "complete"
    }

    pub fn active_player_count(&self) -> usize {
        self.players.values().filter(|p| !p.has_folded).count()
    }

    pub fn get_player(&self, player_root: &[u8]) -> Option<&PlayerHandState> {
        let key = hex::encode(player_root);
        self.players.get(&key)
    }

    pub fn get_player_mut(&mut self, player_root: &[u8]) -> Option<&mut PlayerHandState> {
        let key = hex::encode(player_root);
        self.players.get_mut(&key)
    }

    pub fn total_pot(&self) -> i64 {
        self.pots.iter().map(|p| p.amount).sum()
    }
}

/// Default state factory — starts with one empty "main" pot.
pub fn new_hand_state() -> HandState {
    HandState {
        pots: vec![PotState {
            pot_type: "main".to_string(),
            ..Default::default()
        }],
        ..Default::default()
    }
}

// --- Event appliers ---

pub fn apply_cards_dealt(state: &mut HandState, event: CardsDealt) {
    state.hand_id = format!("{}_{}", hex::encode(&state.table_root), event.hand_number);
    state.table_root = event.table_root;
    state.hand_number = event.hand_number;
    state.game_variant = GameVariant::try_from(event.game_variant).unwrap_or_default();
    state.dealer_position = event.dealer_position;
    state.remaining_deck = event.remaining_deck;
    state.current_phase = BettingPhase::Preflop;
    state.status = "betting".to_string();

    for p in &event.players {
        let key = hex::encode(&p.player_root);
        state.players.insert(
            key,
            PlayerHandState {
                player_root: p.player_root.clone(),
                position: p.position,
                stack: p.stack,
                ..Default::default()
            },
        );
    }

    for pc in &event.player_cards {
        let key = hex::encode(&pc.player_root);
        if let Some(player) = state.players.get_mut(&key) {
            player.hole_cards = pc.cards.clone();
        }
    }
}

pub fn apply_blind_posted(state: &mut HandState, event: BlindPosted) {
    let key = hex::encode(&event.player_root);
    if let Some(player) = state.players.get_mut(&key) {
        player.stack = event.player_stack;
        player.bet_this_round += event.amount;
        player.total_invested += event.amount;
    }
    if let Some(pot) = state.pots.first_mut() {
        pot.amount = event.pot_total;
    }
    if event.amount > state.current_bet {
        state.current_bet = event.amount;
    }
    if event.amount > state.min_raise {
        state.min_raise = event.amount;
    }
}

pub fn apply_action_taken(state: &mut HandState, event: ActionTaken) {
    let key = hex::encode(&event.player_root);
    if let Some(player) = state.players.get_mut(&key) {
        player.stack = event.player_stack;
        player.has_acted = true;

        match ActionType::try_from(event.action).unwrap_or_default() {
            ActionType::Fold => {
                player.has_folded = true;
            }
            ActionType::AllIn => {
                player.is_all_in = true;
                player.bet_this_round += event.amount;
                player.total_invested += event.amount;
            }
            ActionType::Bet | ActionType::Raise | ActionType::Call => {
                player.bet_this_round += event.amount;
                player.total_invested += event.amount;
            }
            _ => {}
        }
    }
    if let Some(pot) = state.pots.first_mut() {
        pot.amount = event.pot_total;
    }
    state.current_bet = event.amount_to_call;
}

pub fn apply_betting_round_complete(state: &mut HandState, event: BettingRoundComplete) {
    for player in state.players.values_mut() {
        player.bet_this_round = 0;
        player.has_acted = false;
    }
    state.current_bet = 0;

    for snap in &event.stacks {
        let key = hex::encode(&snap.player_root);
        if let Some(player) = state.players.get_mut(&key) {
            player.stack = snap.stack;
            player.is_all_in = snap.is_all_in;
            player.has_folded = snap.has_folded;
        }
    }

    if state.game_variant == GameVariant::FiveCardDraw {
        let completed = BettingPhase::try_from(event.completed_phase).unwrap_or_default();
        if completed == BettingPhase::Preflop {
            state.current_phase = BettingPhase::Draw;
        }
    }
}

pub fn apply_community_cards_dealt(state: &mut HandState, event: CommunityCardsDealt) {
    let cards_dealt = event.cards.len();
    if state.remaining_deck.len() >= cards_dealt {
        state.remaining_deck = state.remaining_deck[cards_dealt..].to_vec();
    }
    state.community_cards = event.all_community_cards;
    state.current_phase = BettingPhase::try_from(event.phase).unwrap_or_default();
    for player in state.players.values_mut() {
        player.bet_this_round = 0;
        player.has_acted = false;
    }
    state.current_bet = 0;
}

pub fn apply_draw_completed(state: &mut HandState, event: DrawCompleted) {
    let key = hex::encode(&event.player_root);
    if let Some(player) = state.players.get_mut(&key) {
        player.hole_cards = event.new_cards;
    }
    let cards_drawn = event.cards_drawn as usize;
    if state.remaining_deck.len() >= cards_drawn {
        state.remaining_deck = state.remaining_deck[cards_drawn..].to_vec();
    }
}

pub fn apply_showdown_started(state: &mut HandState, _event: ShowdownStarted) {
    state.status = "showdown".to_string();
}

pub fn apply_pot_awarded(state: &mut HandState, event: PotAwarded) {
    for winner in &event.winners {
        let key = hex::encode(&winner.player_root);
        if let Some(player) = state.players.get_mut(&key) {
            player.stack += winner.amount;
        }
    }
}

pub fn apply_hand_complete(state: &mut HandState, event: HandComplete) {
    state.status = "complete".to_string();
    for snap in &event.final_stacks {
        let key = hex::encode(&snap.player_root);
        if let Some(player) = state.players.get_mut(&key) {
            player.stack = snap.stack;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_community_cards_dealt_applies_correctly() {
        let event = CommunityCardsDealt {
            cards: vec![
                Card { suit: 0, rank: 10 },
                Card { suit: 1, rank: 11 },
                Card { suit: 2, rank: 12 },
            ],
            phase: BettingPhase::Flop as i32,
            all_community_cards: vec![
                Card { suit: 0, rank: 10 },
                Card { suit: 1, rank: 11 },
                Card { suit: 2, rank: 12 },
            ],
            dealt_at: None,
        };

        let mut state = new_hand_state();
        apply_community_cards_dealt(&mut state, event);

        assert_eq!(state.community_cards.len(), 3);
        assert_eq!(state.current_phase, BettingPhase::Flop);
    }

    #[test]
    fn new_hand_state_has_one_main_pot() {
        let state = new_hand_state();
        assert_eq!(state.pots.len(), 1);
        assert_eq!(state.pots[0].pot_type, "main");
    }
}
