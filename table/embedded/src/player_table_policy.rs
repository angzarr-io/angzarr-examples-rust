//! policy-player-table: each MoveDecided becomes the table's Hit, Stand or
//! DoubleDown, table-player-policies' [`PlayerTable`] in angzarr-embedded's
//! markup. A MoveDecided with no move the table knows does not decode as
//! the domain's (MOVE_ACTION_UNSPECIFIED is the wire's absence): a player
//! bug, which angzarr-embedded dead-letters by its code and args. A
//! refusal of a move is translated into the player's RecordMoveRefused
//! (angzarr decision 14.89), so the player hears of it.

use std::convert::Infallible;

use angzarr_embedded::markup::{Commands, PolicyContext};
use angzarr_embedded::v1::Uuid;
use angzarr_embedded_macros::{on_event, on_refused, policy};
use table_player_policies::{PlayerTable, Policies, TableMove};

use crate::domain::{
    Bug, MoveDecided, PlayerStateFact, Refused, TableStateCommand, TableStateRefusal,
};
use crate::{PlayerState, TableState};

pub struct PlayerTablePolicy;

#[policy(name = "policy-player-table", source = PlayerState, target = TableState)]
impl PlayerTablePolicy {
    #[on_event(MoveDecided)]
    fn move_decided(
        &self,
        event: &MoveDecided,
        _ctx: &PolicyContext,
    ) -> Result<Commands<TableState>, Infallible> {
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

    /// The table refused a move this policy sent: the player's fact of it
    /// (angzarr decision 14.89), the refusal by its type's name.
    #[on_refused]
    fn refused(&self, refused: &Refused) -> Result<PlayerStateFact, Bug> {
        let (command, refusal) = TableStateRefusal::read(refused)?;
        let command = match command {
            TableStateCommand::Hit(hit) => TableMove::Hit(hit),
            TableStateCommand::Stand(stand) => TableMove::Stand(stand),
            TableStateCommand::DoubleDown(double) => TableMove::DoubleDown(double),
            _ => return Err(Bug::new("refused-command-not-sent")),
        };
        Ok(Policies.move_refused(&command, &refusal).into())
    }
}

#[cfg(test)]
#[path = "player_table_policy.test.rs"]
mod player_table_policy_tests;
