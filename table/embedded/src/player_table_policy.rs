//! PlayerTablePolicy: the scaffold of `io.angzarr.examples.v1.PlayerTablePolicy`, written once by
//! `angzarr scaffold rust` and yours from then on: `--missing` adds the
//! stubs it lacks.
//!
//! A thin adapter over table-player-policies' [`PlayerTable`]; each hand
//! edit to the scaffold's output is marked `HAND-EDIT`.

use angzarr_embedded::next::markup::{Commands, PolicyContext};
use angzarr_embedded::next::v1::Uuid;
use angzarr_embedded_macros::{on_event, policy};
use table_player_policies::{PlayerTable, Policies, TableMove};

use crate::domain::*;
use crate::player_aggregate::PlayerAggregate;
use crate::table_aggregate::TableAggregate;

pub struct PlayerTablePolicy;

// HAND-EDIT: the scaffold names it "player-table-policy" (from the message
// name); the spec's name is policy-player-table.
#[policy(name = "policy-player-table", source = PlayerAggregate, target = TableAggregate)]
impl PlayerTablePolicy {
    #[on_event(MoveDecided)]
    fn move_decided(&self, event: &MoveDecided, ctx: &PolicyContext) -> Commands<TableAggregate> {
        // HAND-EDIT: the body.
        let _ = ctx;
        match Policies.move_decided(&event.0) {
            Ok(to) => {
                let root = Uuid {
                    value: to.root.into(),
                };
                match to.command {
                    TableMove::Hit(hit) => Commands::none().to(root, Hit(hit)),
                    TableMove::Stand(stand) => Commands::none().to(root, Stand(stand)),
                    TableMove::DoubleDown(double) => Commands::none().to(root, DoubleDown(double)),
                }
            }
            // A player bug (bot_play.feature, EU-1604: dead-lettered and
            // alerted). A markup policy handler is infallible, so it can
            // only send nothing; the dead letter needs angzarr-embedded.
            Err(_bug) => Commands::none(),
        }
    }
}

#[cfg(test)]
#[path = "player_table_policy.test.rs"]
mod player_table_policy_tests;
