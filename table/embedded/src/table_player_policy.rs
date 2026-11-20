//! policy-table-player: each TurnStarted asks the seat's player to decide,
//! table-player-policies' [`TablePlayer`] in angzarr-embedded's markup. The
//! player's DecideMove refuses nothing (angzarr decision 14.99), so the
//! policy has no refusal callback.

use std::convert::Infallible;

use angzarr_embedded::markup::{Commands, PolicyContext};
use angzarr_embedded::v1::Uuid;
use angzarr_embedded_macros::{on_event, policy};
use table_player_policies::{Policies, TablePlayer};

use crate::domain::TurnStarted;
use crate::{PlayerState, TableState};

pub struct TablePlayerPolicy;

#[policy(name = "policy-table-player", source = TableState, target = PlayerState)]
impl TablePlayerPolicy {
    #[on_event(TurnStarted)]
    fn turn_started(
        &self,
        event: &TurnStarted,
        ctx: &PolicyContext,
    ) -> Result<Commands<PlayerState>, Infallible> {
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
