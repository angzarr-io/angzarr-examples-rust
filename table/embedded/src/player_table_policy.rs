//! policy-player-table: each MoveDecided becomes the table's Hit, Stand or
//! DoubleDown, table-player-policies' [`PlayerTable`] in angzarr-embedded's
//! markup. A MoveDecided with no move the table knows does not decode as
//! the domain's (MOVE_ACTION_UNSPECIFIED is the wire's absence): a player
//! bug, which angzarr-embedded dead-letters by its code and args.

use std::convert::Infallible;

use angzarr_embedded::next::markup::{Commands, PolicyContext};
use angzarr_embedded::next::v1::Uuid;
use angzarr_embedded_macros::{on_event, policy};
use table_player_policies::{PlayerTable, Policies, TableMove};

use crate::domain::MoveDecided;
use crate::{Player, TableAggregate};

pub struct PlayerTablePolicy;

#[policy(name = "policy-player-table", source = Player, target = TableAggregate)]
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
}

#[cfg(test)]
#[path = "player_table_policy.test.rs"]
mod player_table_policy_tests;
