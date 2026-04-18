//! Projector: Output (library).
//!
//! Subscribes to player, table, and hand domain events and writes formatted
//! game logs to a file.

use angzarr_client::{projector, CommandResult};
use examples_proto::{
    ActionTaken, BlindPosted, CardsDealt, FundsDeposited, HandComplete, HandStarted, PlayerJoined,
    PlayerRegistered, PotAwarded, TableCreated,
};
use std::fs::OpenOptions;
use std::io::Write;
use std::sync::Mutex;

lazy_static::lazy_static! {
    static ref LOG_FILE: Mutex<std::fs::File> = Mutex::new(
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(std::env::var("HAND_LOG_FILE").unwrap_or_else(|_| "hand_log.txt".into()))
            .expect("Failed to open log file")
    );
}

fn write_log(msg: &str) {
    let timestamp = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3f");
    let mut file = LOG_FILE.lock().unwrap();
    writeln!(file, "[{}] {}", timestamp, msg).ok();
    file.flush().ok();
}

fn truncate_id(player_root: &[u8]) -> String {
    if player_root.len() >= 4 {
        hex::encode(&player_root[..4])
    } else {
        hex::encode(player_root)
    }
}

// docs:start:projector_oo
/// Output projector — writes human-readable event logs across player, table
/// and hand domains.
pub struct OutputProjector;

#[projector(name = "output", domains = ["player", "table", "hand"])]
impl OutputProjector {
    #[handles(PlayerRegistered)]
    fn on_player_registered(&self, event: PlayerRegistered) -> CommandResult<()> {
        write_log(&format!(
            "PLAYER registered: {} ({})",
            event.display_name, event.email
        ));
        Ok(())
    }

    #[handles(FundsDeposited)]
    fn on_funds_deposited(&self, event: FundsDeposited) -> CommandResult<()> {
        let amount = event.amount.as_ref().map(|m| m.amount).unwrap_or(0);
        let balance = event.new_balance.as_ref().map(|m| m.amount).unwrap_or(0);
        write_log(&format!(
            "PLAYER deposited {}, balance: {}",
            amount, balance
        ));
        Ok(())
    }

    #[handles(TableCreated)]
    fn on_table_created(&self, event: TableCreated) -> CommandResult<()> {
        write_log(&format!(
            "TABLE created: {} (variant {})",
            event.table_name, event.game_variant
        ));
        Ok(())
    }

    #[handles(PlayerJoined)]
    fn on_player_joined(&self, event: PlayerJoined) -> CommandResult<()> {
        let player_id = truncate_id(&event.player_root);
        write_log(&format!(
            "TABLE player {} joined with {} chips",
            player_id, event.stack
        ));
        Ok(())
    }

    #[handles(HandStarted)]
    fn on_hand_started(&self, event: HandStarted) -> CommandResult<()> {
        write_log(&format!(
            "TABLE hand #{} started, {} players, dealer at position {}",
            event.hand_number,
            event.active_players.len(),
            event.dealer_position
        ));
        Ok(())
    }

    #[handles(CardsDealt)]
    fn on_cards_dealt(&self, event: CardsDealt) -> CommandResult<()> {
        write_log(&format!(
            "HAND cards dealt to {} players",
            event.player_cards.len()
        ));
        Ok(())
    }

    #[handles(BlindPosted)]
    fn on_blind_posted(&self, event: BlindPosted) -> CommandResult<()> {
        let player_id = truncate_id(&event.player_root);
        write_log(&format!(
            "HAND player {} posted {:?} blind: {}",
            player_id, event.blind_type, event.amount
        ));
        Ok(())
    }

    #[handles(ActionTaken)]
    fn on_action_taken(&self, event: ActionTaken) -> CommandResult<()> {
        let player_id = truncate_id(&event.player_root);
        write_log(&format!(
            "HAND player {}: {:?} {}",
            player_id, event.action, event.amount
        ));
        Ok(())
    }

    #[handles(PotAwarded)]
    fn on_pot_awarded(&self, event: PotAwarded) -> CommandResult<()> {
        let winners: Vec<String> = event
            .winners
            .iter()
            .map(|w| format!("{} wins {}", truncate_id(&w.player_root), w.amount))
            .collect();
        write_log(&format!("HAND pot awarded: {}", winners.join(", ")));
        Ok(())
    }

    #[handles(HandComplete)]
    fn on_hand_complete(&self, event: HandComplete) -> CommandResult<()> {
        write_log(&format!("HAND #{} complete", event.hand_number));
        Ok(())
    }
}
// docs:end:projector_oo

#[cfg(test)]
mod handler_tests {
    //! Direct-call tests for each `#[handles]` method on `OutputProjector`.
    //! Each projector handler returns `CommandResult<()>` and writes to the
    //! log file as a side effect. These tests just assert `Ok(())` is
    //! returned, catching mutants that would replace the body with a stub
    //! that returns `Err` (Default::default for Result<(), E> is Err when E
    //! implements Default).
    use super::*;
    use examples_proto::{ActionType, Currency, GameVariant, PlayerType, PotWinner};

    fn set_test_log() {
        // Point the log file at /tmp to avoid clobbering the default file.
        std::env::set_var("HAND_LOG_FILE", "/tmp/angzarr_prj_output_unit_test.log");
    }

    #[test]
    fn on_player_registered_returns_ok() {
        set_test_log();
        let projector = OutputProjector;
        let result = projector.on_player_registered(PlayerRegistered {
            display_name: "Alice".into(),
            email: "alice@example.com".into(),
            player_type: PlayerType::Human as i32,
            ai_model_id: String::new(),
            registered_at: None,
        });
        assert!(result.is_ok());
    }

    #[test]
    fn on_funds_deposited_returns_ok() {
        set_test_log();
        let projector = OutputProjector;
        let result = projector.on_funds_deposited(FundsDeposited {
            amount: Some(Currency {
                amount: 100,
                currency_code: "CHIPS".into(),
            }),
            new_balance: Some(Currency {
                amount: 1000,
                currency_code: "CHIPS".into(),
            }),
            deposited_at: None,
        });
        assert!(result.is_ok());
    }

    #[test]
    fn on_table_created_returns_ok() {
        set_test_log();
        let projector = OutputProjector;
        let result = projector.on_table_created(TableCreated {
            table_name: "Main".into(),
            game_variant: GameVariant::TexasHoldem as i32,
            small_blind: 5,
            big_blind: 10,
            min_buy_in: 100,
            max_buy_in: 1000,
            max_players: 6,
            action_timeout_seconds: 30,
            created_at: None,
        });
        assert!(result.is_ok());
    }

    #[test]
    fn on_player_joined_returns_ok() {
        set_test_log();
        let projector = OutputProjector;
        let result = projector.on_player_joined(PlayerJoined {
            player_root: vec![0xAB; 16],
            seat_position: 0,
            buy_in_amount: 100,
            stack: 100,
            joined_at: None,
        });
        assert!(result.is_ok());
    }

    #[test]
    fn on_hand_started_returns_ok() {
        set_test_log();
        let projector = OutputProjector;
        let result = projector.on_hand_started(HandStarted {
            hand_root: vec![0xAA; 16],
            hand_number: 1,
            dealer_position: 0,
            small_blind_position: 1,
            big_blind_position: 2,
            active_players: vec![],
            game_variant: GameVariant::TexasHoldem as i32,
            small_blind: 5,
            big_blind: 10,
            started_at: None,
        });
        assert!(result.is_ok());
    }

    #[test]
    fn on_cards_dealt_returns_ok() {
        set_test_log();
        let projector = OutputProjector;
        let result = projector.on_cards_dealt(CardsDealt {
            table_root: vec![0xAA; 16],
            hand_number: 1,
            game_variant: GameVariant::TexasHoldem as i32,
            player_cards: vec![],
            dealer_position: 0,
            players: vec![],
            dealt_at: None,
            remaining_deck: vec![],
        });
        assert!(result.is_ok());
    }

    #[test]
    fn on_blind_posted_returns_ok() {
        set_test_log();
        let projector = OutputProjector;
        let result = projector.on_blind_posted(BlindPosted {
            player_root: vec![0xAB; 16],
            blind_type: "small".into(),
            amount: 10,
            player_stack: 990,
            pot_total: 10,
            posted_at: None,
        });
        assert!(result.is_ok());
    }

    #[test]
    fn on_action_taken_returns_ok() {
        set_test_log();
        let projector = OutputProjector;
        let result = projector.on_action_taken(ActionTaken {
            player_root: vec![0xAB; 16],
            action: ActionType::Call as i32,
            amount: 20,
            player_stack: 970,
            pot_total: 30,
            amount_to_call: 0,
            action_at: None,
        });
        assert!(result.is_ok());
    }

    #[test]
    fn on_pot_awarded_returns_ok() {
        set_test_log();
        let projector = OutputProjector;
        let result = projector.on_pot_awarded(PotAwarded {
            winners: vec![PotWinner {
                player_root: vec![0xAB; 16],
                amount: 100,
                pot_type: "main".into(),
                winning_hand: None,
            }],
            awarded_at: None,
        });
        assert!(result.is_ok());
    }

    #[test]
    fn on_hand_complete_returns_ok() {
        set_test_log();
        let projector = OutputProjector;
        let result = projector.on_hand_complete(HandComplete {
            table_root: vec![0xAA; 16],
            hand_number: 7,
            winners: vec![],
            final_stacks: vec![],
            completed_at: None,
        });
        assert!(result.is_ok());
    }
}
