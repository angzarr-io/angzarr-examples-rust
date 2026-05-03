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

/// High-level workflow phases. Mirrors python `HandPhase`
/// (`hand-flow/hand_process.py:21`) one-to-one — `AwaitingDeal` is the
/// equivalent of Python's `WAITING_FOR_START`; the rest match by name.
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
    AwardingPot,
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
            Phase::AwardingPot => "AWARDING_POT",
            Phase::Complete => "COMPLETE",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "AWAITING_DEAL" | "WAITING_FOR_START" => Phase::AwaitingDeal,
            "DEALING" => Phase::Dealing,
            "POSTING_BLINDS" => Phase::PostingBlinds,
            "BETTING" => Phase::Betting,
            "DEALING_COMMUNITY" => Phase::DealingCommunity,
            "DRAW" => Phase::Draw,
            "SHOWDOWN" => Phase::Showdown,
            "AWARDING_POT" => Phase::AwardingPot,
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

    /// Returns `true` iff the current betting round is complete: at most
    /// one active (non-folded, non-all-in) seat OR every active seat has
    /// `has_acted == true` AND `bet_this_round >= current_bet`.
    ///
    /// The bet-level check matters for the BB-option case: at preflop end
    /// every non-BB player who called has `bet_this_round == current_bet`,
    /// but the BB still has `has_acted == false` (action wasn't yet taken
    /// — only the blind was posted). The blind-posting helpers must NOT
    /// set `has_acted = true` for the BB, or the option vanishes.
    ///
    /// Used by EU-0445 to assert the round stays open after non-BB players
    /// match the blind. Mirrors python `_is_betting_complete` in
    /// `hand-flow/hand_process.py:556`.
    pub fn is_betting_complete(&self) -> bool {
        let active: Vec<&PlayerState> = self
            .players
            .values()
            .filter(|p| !p.has_folded && !p.is_all_in)
            .collect();
        if active.len() <= 1 {
            return true;
        }
        active
            .iter()
            .all(|p| p.has_acted && p.bet_this_round >= self.current_bet)
    }

    /// Advance `action_on` to the next un-folded, un-all-in seat strictly
    /// after the current `action_on`, wrapping to the lowest seat if
    /// needed. Sparse-seat-safe: walks `players.keys()` in sorted order
    /// (BTreeMap), so positions like `[0, 1, 3]` are visited in order
    /// without collapsing to `len()` modulus.
    ///
    /// Does NOT touch `phase` or emit commands. The richer
    /// [`advance_betting`](Self::advance_betting) (private) does both
    /// advancement AND round-end handling; that helper is for the runtime
    /// driver, while this one is for tests and for callers that want to
    /// step action without ending the round.
    pub fn advance_action_on(&mut self) {
        if self.players.is_empty() {
            return;
        }
        let cur = self.action_on;
        let after = self.players.range((cur + 1)..);
        let wrap = self.players.range(..=cur);
        for (pos, p) in after.chain(wrap) {
            if !p.has_folded && !p.is_all_in {
                self.action_on = *pos;
                return;
            }
        }
    }

    /// Apply a player action with full chip accounting: updates
    /// `bet_this_round`, `stack`, and `pot_total` in addition to the flag
    /// updates done by [`apply_action`]. `amount` is the chips put in
    /// THIS action — i.e. for a CALL it's the additional chips needed to
    /// match `current_bet`, not the running total. For BET / RAISE / ALL_IN
    /// the player's `bet_this_round` is bumped by `amount`, and if the new
    /// `bet_this_round` exceeds `current_bet`, `current_bet` is raised to
    /// match.
    ///
    /// Mirrors the chip-accounting half of python's `handle_action_taken`
    /// (`hand-flow/hand_process.py:350`). The flag/raise-reopens-action
    /// half is delegated to [`apply_action`].
    pub fn apply_player_action(&mut self, position: i32, action: Action, amount: i64) {
        if let Some(p) = self.players.get_mut(&position) {
            p.stack -= amount;
            p.bet_this_round += amount;
            if matches!(action, Action::Bet | Action::Raise | Action::AllIn)
                && p.bet_this_round > self.current_bet
            {
                self.current_bet = p.bet_this_round;
            }
        }
        self.pot_total += amount;
        self.apply_action(position, action);
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

    fn three_players_after_blinds() -> HandProcess {
        // Dealer at 0, SB at 1 (posted 5), BB at 2 (posted 10).
        // current_bet = 10, pot = 15. SB and BB have bet_this_round set;
        // BB has has_acted=false (option still open).
        let mut p = HandProcess::new();
        p.players
            .insert(0, PlayerState::new(0, "p1".into(), 1000));
        let mut sb = PlayerState::new(1, "p2".into(), 995);
        sb.bet_this_round = 5;
        p.players.insert(1, sb);
        let mut bb = PlayerState::new(2, "p3".into(), 990);
        bb.bet_this_round = 10;
        p.players.insert(2, bb);
        p.dealer_position = 0;
        p.current_bet = 10;
        p.pot_total = 15;
        p.action_on = 0;
        p.betting_phase = Some(BettingPhase::Preflop);
        p.phase = Phase::Betting;
        p
    }

    #[test]
    fn is_betting_complete_false_when_bb_has_option() {
        // EU-0445 production-side check: after the dealer (UTG with 3
        // players) and SB call, both have has_acted=true and bet_this_round
        // matches current_bet — but BB still has has_acted=false. The
        // round must NOT be reported complete.
        let mut p = three_players_after_blinds();
        p.apply_player_action(0, Action::Call, 10);
        p.apply_player_action(1, Action::Call, 5);
        assert!(
            !p.is_betting_complete(),
            "BB option should keep round open: {:?}",
            p.players
        );
    }

    #[test]
    fn is_betting_complete_true_when_bb_acts() {
        // After BB checks (or calls), all three have has_acted=true and
        // bet_this_round == current_bet, so the round is complete.
        let mut p = three_players_after_blinds();
        p.apply_player_action(0, Action::Call, 10);
        p.apply_player_action(1, Action::Call, 5);
        p.apply_player_action(2, Action::Check, 0);
        assert!(p.is_betting_complete());
    }

    #[test]
    fn is_betting_complete_true_with_one_active_left() {
        // Heads-up after one folds: only one active seat, round trivially
        // complete (the remaining player wins by default).
        let mut p = HandProcess::new();
        p.players
            .insert(0, PlayerState::new(0, "p1".into(), 500));
        let mut p2 = PlayerState::new(1, "p2".into(), 500);
        p2.has_folded = true;
        p.players.insert(1, p2);
        assert!(p.is_betting_complete());
    }

    #[test]
    fn apply_player_action_call_updates_chip_state() {
        let mut p = three_players_after_blinds();
        let pot_before = p.pot_total;
        p.apply_player_action(0, Action::Call, 10);
        let actor = &p.players[&0];
        assert_eq!(actor.bet_this_round, 10);
        assert_eq!(actor.stack, 990);
        assert!(actor.has_acted);
        assert_eq!(p.pot_total, pot_before + 10);
        assert_eq!(p.current_bet, 10, "CALL must not raise current_bet");
    }

    #[test]
    fn apply_player_action_raise_lifts_current_bet_and_reopens_action() {
        let mut p = three_players_after_blinds();
        // Pre-mark dealer as acted to verify a subsequent raise re-opens it.
        p.players.get_mut(&0).unwrap().has_acted = true;
        p.players.get_mut(&1).unwrap().has_acted = true;
        // BB raises to 30 (puts in 20 chips on top of their 10 blind).
        p.apply_player_action(2, Action::Raise, 20);
        assert_eq!(p.current_bet, 30);
        assert_eq!(p.players[&2].bet_this_round, 30);
        assert_eq!(p.players[&2].stack, 970);
        assert!(p.players[&2].has_acted, "raiser keeps has_acted=true");
        assert!(!p.players[&0].has_acted, "raise reopens action for dealer");
        assert!(!p.players[&1].has_acted, "raise reopens action for SB");
    }

    #[test]
    fn advance_action_on_wraps_and_skips_folded() {
        let mut p = HandProcess::new();
        p.players
            .insert(0, PlayerState::new(0, "p1".into(), 500));
        let mut p2 = PlayerState::new(1, "p2".into(), 500);
        p2.has_folded = true;
        p.players.insert(1, p2);
        p.players
            .insert(2, PlayerState::new(2, "p3".into(), 500));
        p.action_on = 0;
        p.advance_action_on();
        assert_eq!(p.action_on, 2, "must skip folded seat 1");
        p.advance_action_on();
        assert_eq!(p.action_on, 0, "must wrap from highest seat back to 0");
    }

    #[test]
    fn advance_action_on_handles_sparse_seats() {
        // Seats 0 and 3 only — `len() % seat_max` would silently mis-walk.
        let mut p = HandProcess::new();
        p.players
            .insert(0, PlayerState::new(0, "p1".into(), 500));
        p.players
            .insert(3, PlayerState::new(3, "p2".into(), 500));
        p.action_on = 0;
        p.advance_action_on();
        assert_eq!(p.action_on, 3);
        p.advance_action_on();
        assert_eq!(p.action_on, 0);
    }
}
