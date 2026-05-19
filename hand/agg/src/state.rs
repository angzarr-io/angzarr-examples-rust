//! Hand aggregate state and event appliers.

use std::collections::HashMap;

use examples_proto::{
    ActionTaken, ActionType, BettingPhase, BettingRoundComplete, BlindPosted, BringInCorrected,
    ButtonCardReplaced, Card, CardsDealt, CommunityCardsDealt, DrawCompleted, FouledDeckDetected,
    GameVariant, HandComplete, HandRedealt, MisdealDeclared, PotAwarded, PrematureFlopDetected,
    PrematureRiverDetected, PrematureStudCardDetected, PrematureTurnDetected,
    SeventhStreetCardReplaced, ShowdownStarted, StudCommunityCardDealt, StudDoorCardSelected,
    StudDownCardConverted, StudStreet, StudStreetDealt,
};

use crate::substantial_action::is_substantial_action;

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
    /// Stud variants — per-player face-up cards accumulated across
    /// streets. Used by `apply_seventh_street_card_replaced` to burn the
    /// exposed-original.
    pub up_cards: Vec<Card>,
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
    pub small_blind: i64,
    pub big_blind: i64,

    pub status: String,

    // --- TDA Rule 36 — substantial-action tracking (mirrors Python
    //     `_HandState.substantial_action_occurred`) -----------------------
    /// Post-blind actions on the current street, recorded by
    /// `apply_action_taken`. Recomputed (cleared) by
    /// `apply_betting_round_complete` and the community-cards applier.
    pub current_street_actions: Vec<ActionType>,
    /// Once true, stays true for the rest of the hand. Misdeal calls
    /// after this point are rejected per TDA Rule 35-D.
    pub substantial_action_occurred: bool,

    // --- Stud street tracking (mirrors Python `_HandState.current_stud_street`)
    /// Updated by `apply_stud_street_dealt`. `StudStreet::Unspecified`
    /// means the hand is not on a stud street yet (or game is not stud).
    pub current_stud_street: StudStreet,

    // --- Bring-in correction window (mirrors Python
    //     `_HandState.bring_in_correction_window_open`) -------------------
    /// Open by default once the bring-in is posted; closed once the
    /// next-to-act player has acted (which currently triggers via
    /// `apply_action_taken` post-bring-in).
    pub bring_in_correction_window_open: bool,
    pub bring_in_corrected: bool,
    pub bring_in_current_player: Vec<u8>,

    // --- Misdeal / fouled-deck / premature-street status -----------------
    pub misdeal_declared: bool,
    pub misdeal_reason: String,
    pub fouled_deck: bool,
    pub fouled_deck_duplicate: String,
    pub premature_flop: bool,
    pub premature_turn: bool,
    pub premature_river: bool,
    pub premature_stud: bool,
    pub button_card_replaced: bool,

    // --- Redeal / blind-level tracking (PR #12 decision 1) ----------------
    pub redeal_count: i64,
    pub current_blind_level: i64,
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
    state.table_root = event.table_root;
    state.hand_number = event.hand_number;
    state.hand_id = format!("{}_{}", hex::encode(&state.table_root), state.hand_number);
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
        // A short-stacked player whose blind/ante consumes their entire
        // stack is committed all-in for the hand (TDA Rule 38).
        if event.player_stack == 0 {
            player.is_all_in = true;
        }
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
    match event.blind_type.as_str() {
        "small" => state.small_blind = event.amount,
        "big" => state.big_blind = event.amount,
        "bring_in" => {
            // WSOP §SC Stud #5 / Robert's #5 — bring-in post opens the
            // correction window; the very next action closes it (see
            // `apply_action_taken`).
            state.bring_in_correction_window_open = true;
            state.bring_in_current_player = event.player_root.clone();
        }
        _ => {}
    }
}

pub fn apply_action_taken(state: &mut HandState, event: ActionTaken) {
    let action = ActionType::try_from(event.action).unwrap_or_default();
    let key = hex::encode(&event.player_root);
    if let Some(player) = state.players.get_mut(&key) {
        player.stack = event.player_stack;
        player.has_acted = true;

        match action {
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

    // TDA Rule 36 substantial-action tracking: posted blinds are NOT in
    // the action stream (they have their own event). Only `ActionTaken`
    // post-blind actions count. Once SA fires, it stays true for the
    // rest of the hand (the flag is sticky).
    state.current_street_actions.push(action);
    if !state.substantial_action_occurred && is_substantial_action(&state.current_street_actions) {
        state.substantial_action_occurred = true;
    }

    // WSOP §SC Stud #5 / Robert's #5 — the bring-in correction window
    // closes the moment the next-to-act player acts. A single
    // `ActionTaken` event after the bring-in post is enough to close
    // it; the window is opened by `apply_blind_posted` for the
    // bring-in (kind="bring_in").
    if state.bring_in_correction_window_open {
        state.bring_in_correction_window_open = false;
    }
}

pub fn apply_betting_round_complete(state: &mut HandState, event: BettingRoundComplete) {
    for player in state.players.values_mut() {
        player.bet_this_round = 0;
        player.has_acted = false;
    }
    state.current_bet = 0;
    // Clear per-street action history; substantial_action_occurred is
    // sticky for the hand so we DON'T reset it here.
    state.current_street_actions.clear();

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
    state.current_street_actions.clear();
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

// --- PR #12 cascade event appliers (mirror Python `apply_*` in
//     hand.py:1884-2396). All idempotent — re-applying yields the same
//     state. --------------------------------------------------------------

pub fn apply_misdeal_declared(state: &mut HandState, event: MisdealDeclared) {
    state.misdeal_declared = true;
    state.misdeal_reason = event.reason;
}

pub fn apply_fouled_deck_detected(state: &mut HandState, event: FouledDeckDetected) {
    state.fouled_deck = true;
    state.fouled_deck_duplicate = event.duplicate_card;
}

pub fn apply_hand_redealt(state: &mut HandState, event: HandRedealt) {
    state.redeal_count += 1;
    state.current_blind_level = event.level;
    state.table_root = event.table_root;
    state.hand_number = event.hand_number;
    state.dealer_position = event.dealer_position;
    state.small_blind = event.small_blind;
    state.big_blind = event.big_blind;
    // The redeal IS the recovery — reset misdeal/fouled/premature
    // flags so the next CardsDealt starts from a clean ledger.
    state.misdeal_declared = false;
    state.misdeal_reason.clear();
    state.fouled_deck = false;
    state.fouled_deck_duplicate.clear();
    state.premature_flop = false;
    state.premature_turn = false;
    state.premature_river = false;
    state.premature_stud = false;
}

pub fn apply_button_card_replaced(state: &mut HandState, _event: ButtonCardReplaced) {
    state.button_card_replaced = true;
}

pub fn apply_premature_flop_detected(state: &mut HandState, _event: PrematureFlopDetected) {
    state.premature_flop = true;
}

pub fn apply_premature_turn_detected(state: &mut HandState, _event: PrematureTurnDetected) {
    state.premature_turn = true;
}

pub fn apply_premature_river_detected(state: &mut HandState, _event: PrematureRiverDetected) {
    state.premature_river = true;
}

pub fn apply_stud_street_dealt(state: &mut HandState, event: StudStreetDealt) {
    state.current_stud_street = StudStreet::try_from(event.street).unwrap_or_default();
    // Push each player's new upcards into their accumulated up_cards.
    for up in event.up_cards {
        let key = hex::encode(&up.player_root);
        if let Some(player) = state.players.get_mut(&key) {
            player.up_cards.extend(up.up_cards);
        }
    }
}

pub fn apply_stud_community_card_dealt(state: &mut HandState, event: StudCommunityCardDealt) {
    state.current_stud_street = StudStreet::try_from(event.street).unwrap_or_default();
    if let Some(card) = event.card {
        state.community_cards.push(card);
    }
}

pub fn apply_stud_door_card_selected(state: &mut HandState, event: StudDoorCardSelected) {
    let key = hex::encode(&event.player_root);
    if let (Some(player), Some(card)) = (state.players.get_mut(&key), event.door_card) {
        // Promote the door card from face-down to face-up: the chosen
        // card is removed from hole_cards (kept face-down) and pushed
        // onto up_cards.
        if let Some(idx) = player
            .hole_cards
            .iter()
            .position(|c| c.suit == card.suit && c.rank == card.rank)
        {
            player.hole_cards.remove(idx);
        }
        player.up_cards.push(card);
    }
}

pub fn apply_stud_down_card_converted(state: &mut HandState, event: StudDownCardConverted) {
    let key = hex::encode(&event.player_root);
    if let (Some(player), Some(card)) = (state.players.get_mut(&key), event.exposed_card) {
        if let Some(idx) = player
            .hole_cards
            .iter()
            .position(|c| c.suit == card.suit && c.rank == card.rank)
        {
            player.hole_cards.remove(idx);
        }
        player.up_cards.push(card);
    }
}

pub fn apply_seventh_street_card_replaced(state: &mut HandState, event: SeventhStreetCardReplaced) {
    let key = hex::encode(&event.player_root);
    if let Some(player) = state.players.get_mut(&key) {
        if let Some(orig) = event.original_card {
            // Burn the exposed-original from the player's up_cards.
            player
                .up_cards
                .retain(|c| !(c.suit == orig.suit && c.rank == orig.rank));
        }
    }
}

pub fn apply_bring_in_corrected(state: &mut HandState, event: BringInCorrected) {
    state.bring_in_corrected = true;
    state.bring_in_current_player = event.correct_root.clone();
    state.bring_in_correction_window_open = false;
    let key = hex::encode(&event.incorrect_root);
    if let Some(player) = state.players.get_mut(&key) {
        player.stack += event.returned_amount;
        player.bet_this_round = (player.bet_this_round - event.returned_amount).max(0);
        player.total_invested = (player.total_invested - event.returned_amount).max(0);
    }
}

pub fn apply_premature_stud_card_detected(
    state: &mut HandState,
    _event: PrematureStudCardDetected,
) {
    state.premature_stud = true;
}
