//! The player bot's domain, as the table's policies name it: its command
//! set (what policy-table-player sends it) and its event set (what
//! policy-player-table reads), with no stand-in aggregate. The bot itself
//! is Python, a child process angzarr-embedded-host registers as the
//! "player" domain.

use angzarr_embedded::next::markup::{CommandTarget, Decides, EventSource};
use examples_proto::{domain, v1};
use prost::Name;

/// The player domain.
pub struct Player;

impl CommandTarget for Player {
    const DOMAIN: &'static str = "player";

    fn command_types() -> Vec<String> {
        vec![v1::DecideMove::full_name()]
    }
}

impl Decides<domain::DecideMove> for Player {}

impl EventSource for Player {
    const DOMAIN: &'static str = "player";

    fn event_types() -> Vec<String> {
        vec![v1::MoveDecided::full_name()]
    }
}
