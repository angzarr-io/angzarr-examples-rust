//! TablePlayerPolicy: the scaffold of `io.angzarr.examples.v1.TablePlayerPolicy`, written once by
//! `angzarr scaffold rust` and yours from then on: `--missing` adds the
//! stubs it lacks.
//!
//! A thin adapter over table-player-policies' [`TablePlayer`]; each hand
//! edit to the scaffold's output is marked `HAND-EDIT`.

use angzarr_embedded::next::markup::{Commands, PolicyContext};
use angzarr_embedded::next::v1::Uuid;
use angzarr_embedded_macros::{on_event, policy};
use table_player_policies::{Policies, TablePlayer};

use crate::domain::*;
use crate::player_aggregate::PlayerAggregate;
use crate::table_aggregate::TableAggregate;

pub struct TablePlayerPolicy;

// HAND-EDIT: the scaffold names it "table-player-policy" (from the message
// name); the spec's name is policy-table-player.
#[policy(name = "policy-table-player", source = TableAggregate, target = PlayerAggregate)]
impl TablePlayerPolicy {
    #[on_event(TurnStarted)]
    fn turn_started(&self, event: &TurnStarted, ctx: &PolicyContext) -> Commands<PlayerAggregate> {
        // HAND-EDIT: the body. The table root is the trigger's own stream's.
        let table_root = ctx
            .trigger()
            .stream
            .as_ref()
            .and_then(|cover| cover.root.as_ref())
            .map(|root| root.value.to_vec())
            .unwrap_or_default();
        let ask = Policies.turn_started(&table_root, &event.0);
        Commands::none().to(
            Uuid {
                value: ask.root.into(),
            },
            DecideMove(ask.command),
        )
    }
}

#[cfg(test)]
#[path = "table_player_policy.test.rs"]
mod table_player_policy_tests;
