//! policy-table-player: each TurnStarted asks the seat's player to decide,
//! table-player-policies' [`TablePlayer`] in angzarr-embedded's markup.

use std::convert::Infallible;

use angzarr_embedded::next::markup::{Commands, PolicyContext};
use angzarr_embedded::next::v1::Uuid;
use angzarr_embedded_macros::{on_event, policy};
use table_player_policies::{Policies, TablePlayer};

use crate::domain::TurnStarted;
use crate::{Player, TableAggregate};

pub struct TablePlayerPolicy;

#[policy(name = "policy-table-player", source = TableAggregate, target = Player)]
impl TablePlayerPolicy {
    #[on_event(TurnStarted)]
    fn turn_started(
        &self,
        event: &TurnStarted,
        ctx: &PolicyContext,
    ) -> Result<Commands<Player>, Infallible> {
        let ask = Policies.turn_started(&ctx.source_root().value, event);
        Ok(Commands::none().to(
            Uuid {
                value: ask.root.into(),
            },
            ask.command,
        ))
    }
}

#[cfg(test)]
#[path = "table_player_policy.test.rs"]
mod table_player_policy_tests;
