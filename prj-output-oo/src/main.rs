//! Projector: Output (OO Pattern)
//!
//! Subscribes to player, table, and hand domain events.
//! Writes formatted game logs to a file.
//!
//! This demonstrates the OO pattern where:
//! - `#[projector(name, inputs)]` decorates the impl block
//! - `#[handles(EventType, domain = "...")]` marks event handler methods

use angzarr_client::proto::Projection;
use angzarr_client::{handles, projector, run_projector_server};
use examples_proto::{
    ActionTaken, BlindPosted, CardsDealt, FundsDeposited, HandComplete, HandStarted,
    PlayerJoined, PlayerRegistered, PotAwarded, TableCreated,
};
use std::fs::OpenOptions;
use std::io::Write;
use std::sync::Mutex;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

lazy_static::lazy_static! {
    static ref LOG_FILE: Mutex<std::fs::File> = Mutex::new(
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(std::env::var("HAND_LOG_FILE").unwrap_or_else(|_| "hand_log_oo.txt".into()))
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
/// Output projector using OO-style decorators with multi-domain support.
pub struct OutputProjector;

#[projector(name = "output", inputs = ["player", "table", "hand"])]
impl OutputProjector {
    #[handles(PlayerRegistered, domain = "player")]
    fn project_registered(&self, event: &PlayerRegistered) -> Projection {
        write_log(&format!(
            "PLAYER registered: {} ({})",
            event.display_name, event.email
        ));
        Projection::default()
    }

    #[handles(FundsDeposited, domain = "player")]
    fn project_deposited(&self, event: &FundsDeposited) -> Projection {
        let amount = event.amount.as_ref().map(|m| m.amount).unwrap_or(0);
        let balance = event.new_balance.as_ref().map(|m| m.amount).unwrap_or(0);
        write_log(&format!("PLAYER deposited {}, balance: {}", amount, balance));
        Projection::default()
    }

    #[handles(TableCreated, domain = "table")]
    fn project_table_created(&self, event: &TableCreated) -> Projection {
        write_log(&format!(
            "TABLE created: {} (variant {})",
            event.table_name, event.game_variant
        ));
        Projection::default()
    }

    #[handles(PlayerJoined, domain = "table")]
    fn project_player_joined(&self, event: &PlayerJoined) -> Projection {
        let player_id = truncate_id(&event.player_root);
        write_log(&format!(
            "TABLE player {} joined with {} chips",
            player_id, event.stack
        ));
        Projection::default()
    }

    #[handles(HandStarted, domain = "table")]
    fn project_hand_started(&self, event: &HandStarted) -> Projection {
        write_log(&format!(
            "TABLE hand #{} started, {} players, dealer at position {}",
            event.hand_number,
            event.active_players.len(),
            event.dealer_position
        ));
        Projection::default()
    }

    #[handles(CardsDealt, domain = "hand")]
    fn project_cards_dealt(&self, event: &CardsDealt) -> Projection {
        write_log(&format!(
            "HAND cards dealt to {} players",
            event.player_cards.len()
        ));
        Projection::default()
    }

    #[handles(BlindPosted, domain = "hand")]
    fn project_blind_posted(&self, event: &BlindPosted) -> Projection {
        let player_id = truncate_id(&event.player_root);
        write_log(&format!(
            "HAND player {} posted {:?} blind: {}",
            player_id, event.blind_type, event.amount
        ));
        Projection::default()
    }

    #[handles(ActionTaken, domain = "hand")]
    fn project_action_taken(&self, event: &ActionTaken) -> Projection {
        let player_id = truncate_id(&event.player_root);
        write_log(&format!(
            "HAND player {}: {:?} {}",
            player_id, event.action, event.amount
        ));
        Projection::default()
    }

    #[handles(PotAwarded, domain = "hand")]
    fn project_pot_awarded(&self, event: &PotAwarded) -> Projection {
        let winners: Vec<String> = event
            .winners
            .iter()
            .map(|w| format!("{} wins {}", truncate_id(&w.player_root), w.amount))
            .collect();
        write_log(&format!("HAND pot awarded: {}", winners.join(", ")));
        Projection::default()
    }

    #[handles(HandComplete, domain = "hand")]
    fn project_hand_complete(&self, event: &HandComplete) -> Projection {
        write_log(&format!("HAND #{} complete", event.hand_number));
        Projection::default()
    }
}
// docs:end:projector_oo

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    println!("Starting Output projector (OO pattern)");

    let projector = OutputProjector;
    let router = projector.into_router();

    run_projector_server("output", 50391, router)
        .await
        .expect("Server failed");
}
