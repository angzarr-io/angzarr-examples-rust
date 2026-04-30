//! HandFlow PM state machine — phase, action-on, and bet tracking.
//!
//! The minimal `#[handles]` arms in [`crate::HandFlowPm`] cover the
//! happy-path choreography that the runtime actually drives in cluster
//! runs (table reactively posts blinds, hand aggregate runs the betting
//! rounds). This module ports the richer state-machine semantics from
//! `examples-python/main/hand-flow/hand_process.py` so that BDD scenarios
//! exercising phase transitions, action-on tracking, raise-reopens-action,
//! all-in handling, timeouts, and pot/stack accounting have real
//! production code to dispatch through.
//!
//! Naming mirrors the python reference for cross-language audit.

use std::collections::BTreeMap;

/// High-level workflow phases. Mirrors python `HandPhase` plus the
/// finer-grained sub-phases the BDD scenarios assert on.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Phase {
    #[default]
    AwaitingDeal,
    Dealing,
    PostingBlinds,
    Betting,
    DealingCommunity,
    Draw,
    Showdown,
    Complete,
}

impl Phase {
    pub fn as_str(&self) -> &'static str {
        match self {
            Phase::AwaitingDeal => "AWAITING_DEAL",
            Phase::Dealing => "DEALING",
            Phase::PostingBlinds => "POSTING_BLINDS",
            Phase::Betting => "BETTING",
            Phase::DealingCommunity => "DEALING_COMMUNITY",
            Phase::Draw => "DRAW",
            Phase::Showdown => "SHOWDOWN",
            Phase::Complete => "COMPLETE",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "AWAITING_DEAL" => Phase::AwaitingDeal,
            "DEALING" => Phase::Dealing,
            "POSTING_BLINDS" => Phase::PostingBlinds,
            "BETTING" => Phase::Betting,
            "DEALING_COMMUNITY" => Phase::DealingCommunity,
            "DRAW" => Phase::Draw,
            "SHOWDOWN" => Phase::Showdown,
            "COMPLETE" => Phase::Complete,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BettingPhase {
    Preflop,
    Flop,
    Turn,
    River,
    Draw,
}

impl BettingPhase {
    pub fn as_str(&self) -> &'static str {
        match self {
            BettingPhase::Preflop => "PREFLOP",
            BettingPhase::Flop => "FLOP",
            BettingPhase::Turn => "TURN",
            BettingPhase::River => "RIVER",
            BettingPhase::Draw => "DRAW",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "PREFLOP" => BettingPhase::Preflop,
            "FLOP" => BettingPhase::Flop,
            "TURN" => BettingPhase::Turn,
            "RIVER" => BettingPhase::River,
            "DRAW" => BettingPhase::Draw,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Fold,
    Check,
    Call,
    Bet,
    Raise,
    AllIn,
}

impl Action {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.to_ascii_uppercase().as_str() {
            "FOLD" => Action::Fold,
            "CHECK" => Action::Check,
            "CALL" => Action::Call,
            "BET" => Action::Bet,
            "RAISE" => Action::Raise,
            "ALL_IN" | "ALLIN" => Action::AllIn,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone)]
pub struct PlayerState {
    pub position: i32,
    pub player_root: String,
    pub stack: i64,
    pub bet_this_round: i64,
    pub has_acted: bool,
    pub has_folded: bool,
    pub is_all_in: bool,
}

impl PlayerState {
    pub fn new(position: i32, player_root: String, stack: i64) -> Self {
        Self {
            position,
            player_root,
            stack,
            bet_this_round: 0,
            has_acted: false,
            has_folded: false,
            is_all_in: false,
        }
    }
}

/// Non-empty command emitted by a state-machine transition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    PostBlind { kind: BlindKind },
    DealCommunityCards { count: i32 },
    AwardPot,
    PlayerAction { action: Action },
    TimeoutCancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlindKind {
    Small,
    Big,
}

#[derive(Debug, Default, Clone)]
pub struct HandProcess {
    pub phase: Phase,
    pub betting_phase: Option<BettingPhase>,
    pub game_variant: String,
    pub dealer_position: i32,
    pub small_blind: i64,
    pub big_blind: i64,
    pub current_bet: i64,
    pub action_on: i32,
    pub pot_total: i64,
    pub players: BTreeMap<i32, PlayerState>,
    pub small_blind_posted: bool,
    pub big_blind_posted: bool,
    /// Most recent commands emitted by a transition (cleared on each call).
    pub emitted: Vec<Command>,
}

impl HandProcess {
    pub fn new() -> Self {
        Self::default()
    }

    fn clear_emitted(&mut self) {
        self.emitted.clear();
    }

    /// Apply `HandStarted`: record table parameters and seat the players.
    pub fn on_hand_started(
        &mut self,
        hand_number: i64,
        game_variant: &str,
        dealer_position: i32,
        small_blind: i64,
        big_blind: i64,
        players: &[PlayerState],
    ) {
        let _ = hand_number;
        self.clear_emitted();
        self.game_variant = game_variant.to_string();
        self.dealer_position = dealer_position;
        self.small_blind = small_blind;
        self.big_blind = big_blind;
        self.players.clear();
        for p in players {
            self.players.insert(p.position, p.clone());
        }
        self.phase = Phase::Dealing;
    }

    /// Drive the next phase from whatever sub-state we're in. Mirrors
    /// `process_manager handles the event` in the gherkin: dealing →
    /// posting_blinds, posting_blinds → betting, betting → next phase.
    pub fn handle_event(&mut self) {
        self.clear_emitted();
        match self.phase {
            Phase::Dealing => {
                self.phase = Phase::PostingBlinds;
                self.emitted.push(Command::PostBlind {
                    kind: BlindKind::Small,
                });
            }
            Phase::PostingBlinds => {
                if self.small_blind_posted && !self.big_blind_posted {
                    self.emitted.push(Command::PostBlind {
                        kind: BlindKind::Big,
                    });
                    self.big_blind_posted = true;
                } else {
                    self.phase = Phase::Betting;
                    let n = self.players.len().max(1) as i32;
                    self.action_on = (self.dealer_position + 2).rem_euclid(n);
                }
            }
            Phase::Betting => {
                self.advance_betting();
            }
            Phase::Showdown => {
                self.phase = Phase::Complete;
                self.emitted.push(Command::TimeoutCancel);
            }
            _ => {}
        }
    }

    fn advance_betting(&mut self) {
        let n = self.players.len() as i32;
        if n == 0 {
            return;
        }

        // Advance action_on to the next un-folded, un-all-in seat.
        let mut next = (self.action_on + 1).rem_euclid(n);
        for _ in 0..n {
            if let Some(p) = self.players.get(&next) {
                if !p.has_folded && !p.is_all_in {
                    break;
                }
            }
            next = (next + 1).rem_euclid(n);
        }
        self.action_on = next;

        // If only one un-folded remains, hand goes to COMPLETE w/ AwardPot.
        let active_count = self.players.values().filter(|p| !p.has_folded).count();
        if active_count <= 1 {
            self.phase = Phase::Complete;
            self.emitted.push(Command::AwardPot);
            return;
        }

        // Round-end check: every active (non-folded, non-all-in) seat acted.
        let all_acted = self
            .players
            .values()
            .filter(|p| !p.has_folded && !p.is_all_in)
            .all(|p| p.has_acted);
        if !all_acted {
            return;
        }

        // Round complete — pick the next phase.
        let variant = self.game_variant.clone();
        let phase = self.betting_phase;
        if variant == "FIVE_CARD_DRAW" && phase == Some(BettingPhase::Preflop) {
            self.phase = Phase::Draw;
            return;
        }
        match phase {
            Some(BettingPhase::Preflop) => {
                self.phase = Phase::DealingCommunity;
                self.emitted.push(Command::DealCommunityCards { count: 3 });
            }
            Some(BettingPhase::Flop) | Some(BettingPhase::Turn) => {
                self.phase = Phase::DealingCommunity;
                self.emitted.push(Command::DealCommunityCards { count: 1 });
            }
            Some(BettingPhase::River) | Some(BettingPhase::Draw) => {
                self.phase = Phase::Showdown;
                self.emitted.push(Command::AwardPot);
            }
            None => {}
        }
    }

    /// Drive a betting-round termination from the outside (e.g. when the
    /// runtime decides the round is done and asks the PM what's next).
    /// Mirrors python's `pm.end_betting_round()`.
    pub fn end_betting_round(&mut self) {
        self.clear_emitted();
        let variant = self.game_variant.clone();
        if variant == "FIVE_CARD_DRAW" && self.betting_phase == Some(BettingPhase::Preflop) {
            self.phase = Phase::Draw;
            return;
        }
        match self.betting_phase {
            Some(BettingPhase::Preflop) => {
                self.phase = Phase::DealingCommunity;
                self.emitted.push(Command::DealCommunityCards { count: 3 });
            }
            Some(BettingPhase::Flop) | Some(BettingPhase::Turn) => {
                self.phase = Phase::DealingCommunity;
                self.emitted.push(Command::DealCommunityCards { count: 1 });
            }
            Some(BettingPhase::River) | Some(BettingPhase::Draw) => {
                self.phase = Phase::Showdown;
                self.emitted.push(Command::AwardPot);
            }
            None => {}
        }
    }

    /// Apply a player action (used by `ActionTaken`-style triggers in the
    /// gherkin). Updates the player's flags. The caller is expected to
    /// have set `current_bet` etc. via the dedicated helpers.
    pub fn apply_action(&mut self, position: i32, action: Action) {
        if let Some(p) = self.players.get_mut(&position) {
            p.has_acted = true;
            match action {
                Action::Raise => {
                    // Raise reopens action for everyone else.
                    let raiser = position;
                    for (k, p) in self.players.iter_mut() {
                        if *k != raiser {
                            p.has_acted = false;
                        }
                    }
                }
                Action::Fold => {
                    if let Some(p) = self.players.get_mut(&position) {
                        p.has_folded = true;
                    }
                }
                Action::AllIn => {
                    if let Some(p) = self.players.get_mut(&position) {
                        p.is_all_in = true;
                    }
                }
                _ => {}
            }
        }
    }

    /// Auto-action on timeout: FOLD if facing a bet you haven't matched,
    /// CHECK otherwise. Returns the synthetic `PlayerAction` command the
    /// PM would send. Mirrors python's timeout handler.
    pub fn timeout(&mut self) -> Command {
        self.clear_emitted();
        if self.current_bet > 0 {
            if let Some(p) = self.players.get(&self.action_on) {
                if p.bet_this_round < self.current_bet {
                    let cmd = Command::PlayerAction {
                        action: Action::Fold,
                    };
                    self.emitted.push(cmd.clone());
                    return cmd;
                }
            }
        }
        let cmd = Command::PlayerAction {
            action: Action::Check,
        };
        self.emitted.push(cmd.clone());
        cmd
    }

    /// `handle_last_draw`: in 5-card draw, after every player has drawn,
    /// transition into the post-draw betting round.
    pub fn handle_last_draw(&mut self) {
        self.clear_emitted();
        self.phase = Phase::Betting;
        self.betting_phase = Some(BettingPhase::Draw);
    }

    /// `apply_community_cards_dealt`: reset per-round bet tracking and
    /// move action to the first seat after the dealer.
    pub fn apply_community_cards_dealt(&mut self, betting_phase: BettingPhase) {
        self.clear_emitted();
        for p in self.players.values_mut() {
            p.bet_this_round = 0;
            p.has_acted = false;
        }
        self.current_bet = 0;
        self.betting_phase = Some(betting_phase);
        let n = self.players.len().max(1) as i32;
        self.action_on = (self.dealer_position + 1).rem_euclid(n);
    }

    /// `apply_pot_awarded`: PotAwarded → COMPLETE + cancel timeouts.
    pub fn apply_pot_awarded(&mut self) {
        self.clear_emitted();
        self.phase = Phase::Complete;
        self.emitted.push(Command::TimeoutCancel);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn two_players() -> Vec<PlayerState> {
        vec![
            PlayerState::new(0, "player-1".into(), 500),
            PlayerState::new(1, "player-2".into(), 500),
        ]
    }

    #[test]
    fn hand_started_seats_players_and_enters_dealing() {
        let mut p = HandProcess::new();
        let players = two_players();
        p.on_hand_started(1, "TEXAS_HOLDEM", 0, 5, 10, &players);
        assert_eq!(p.phase, Phase::Dealing);
        assert_eq!(p.players.len(), 2);
        assert_eq!(p.dealer_position, 0);
    }

    #[test]
    fn dealing_to_blinds_emits_post_small() {
        let mut p = HandProcess::new();
        p.on_hand_started(1, "TEXAS_HOLDEM", 0, 5, 10, &two_players());
        p.handle_event();
        assert_eq!(p.phase, Phase::PostingBlinds);
        assert!(matches!(
            p.emitted[0],
            Command::PostBlind {
                kind: BlindKind::Small
            }
        ));
    }
}
