//! policy-table-player: each TurnStarted asks the seat's player to decide,
//! table-player-policies' [`TablePlayer`] in angzarr-embedded's markup. A
//! refusal of DecideMove is translated into the table's
//! RecordMoveUndecided (angzarr decision 14.76).

use std::convert::Infallible;

use angzarr_embedded::markup::{CommandContext, Commands, PolicyContext, Refusal};
use angzarr_embedded::v1::Uuid;
use angzarr_embedded_macros::{on_event, on_refused, policy};
use table_player_policies::{Policies, TablePlayer};

use crate::domain::{DecideMove, DecideMoveRefused, TableAggregateFact, TurnStarted};
use crate::{PlayerAggregate, TableAggregate};

pub struct TablePlayerPolicy;

#[policy(name = "policy-table-player", source = TableAggregate, target = PlayerAggregate)]
impl TablePlayerPolicy {
    #[on_event(TurnStarted)]
    fn turn_started(
        &self,
        event: &TurnStarted,
        ctx: &PolicyContext,
    ) -> Result<Commands<PlayerAggregate>, Infallible> {
        let ask = Policies.turn_started(&ctx.source_root().value, event);
        Ok(Commands::none().to(
            Uuid {
                value: ask.root.into(),
            },
            ask.command,
        ))
    }

    #[on_refused(DecideMove)]
    fn decide_move_refused(
        &self,
        refusal: &Refusal<DecideMove>,
        _ctx: &CommandContext,
    ) -> Result<TableAggregateFact, Infallible> {
        let undecided =
            Policies.decide_move_refused(refusal.command(), refusal.refusal::<DecideMoveRefused>());
        Ok(undecided.into())
    }
}

#[cfg(test)]
#[path = "table_player_policy.test.rs"]
mod table_player_policy_tests;
