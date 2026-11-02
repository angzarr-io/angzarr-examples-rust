//! policy-player-table: each MoveDecided becomes the table's Hit, Stand or
//! DoubleDown, table-player-policies' [`PlayerTable`] in angzarr-embedded's
//! markup. A MoveDecided with no move the table knows does not decode as
//! the domain's (MOVE_ACTION_UNSPECIFIED is the wire's absence): a player
//! bug, which angzarr-embedded dead-letters by its code and args. A
//! refusal of a move is translated into the player's RecordMoveRefused
//! (angzarr decision 14.76), so the player hears of it.

use std::convert::Infallible;

use angzarr_embedded::markup::{CommandContext, Commands, PolicyContext, Refusal};
use angzarr_embedded::v1::Uuid;
use angzarr_embedded_macros::{on_event, on_refused, policy};
use table_player_policies::{PlayerTable, Policies, TableMove};

use crate::domain::{DoubleDown, Hit, MoveDecided, PlayerAggregateFact, Stand, TableRefused};
use crate::{PlayerAggregate, TableAggregate};

/// The player's fact of the table's refusal of `command`.
fn refused(command: TableMove, refusal: Option<TableRefused>) -> PlayerAggregateFact {
    Policies.move_refused(&command, refusal).into()
}

pub struct PlayerTablePolicy;

#[policy(name = "policy-player-table", source = PlayerAggregate, target = TableAggregate)]
impl PlayerTablePolicy {
    #[on_event(MoveDecided)]
    fn move_decided(
        &self,
        event: &MoveDecided,
        _ctx: &PolicyContext,
    ) -> Result<Commands<TableAggregate>, Infallible> {
        let to = Policies.move_decided(event);
        let root = Uuid {
            value: to.root.into(),
        };
        Ok(match to.command {
            TableMove::Hit(hit) => Commands::none().to(root, hit),
            TableMove::Stand(stand) => Commands::none().to(root, stand),
            TableMove::DoubleDown(double) => Commands::none().to(root, double),
        })
    }

    #[on_refused(Hit)]
    fn hit_refused(
        &self,
        refusal: &Refusal<Hit>,
        _ctx: &CommandContext,
    ) -> Result<PlayerAggregateFact, Infallible> {
        let command = TableMove::Hit(refusal.command().clone());
        Ok(refused(command, refusal.refusal::<TableRefused>()))
    }

    #[on_refused(Stand)]
    fn stand_refused(
        &self,
        refusal: &Refusal<Stand>,
        _ctx: &CommandContext,
    ) -> Result<PlayerAggregateFact, Infallible> {
        let command = TableMove::Stand(refusal.command().clone());
        Ok(refused(command, refusal.refusal::<TableRefused>()))
    }

    #[on_refused(DoubleDown)]
    fn double_down_refused(
        &self,
        refusal: &Refusal<DoubleDown>,
        _ctx: &CommandContext,
    ) -> Result<PlayerAggregateFact, Infallible> {
        let command = TableMove::DoubleDown(refusal.command().clone());
        Ok(refused(command, refusal.refusal::<TableRefused>()))
    }
}

#[cfg(test)]
#[path = "player_table_policy.test.rs"]
mod player_table_policy_tests;
