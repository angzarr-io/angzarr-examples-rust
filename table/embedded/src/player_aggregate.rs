//! PlayerAggregate: the scaffold of `io.angzarr.examples.v1.PlayerState`, written once by
//! `angzarr scaffold rust` and yours from then on: `--missing` adds the
//! stubs it lacks.
//!
//! HAND-NOTE: only the player domain's CONTRACT, so the table's policies
//! type-check (a policy's source and target are domains, and
//! `Commands<PlayerAggregate>` holds only commands it decides). The real
//! player is the Python bot over gRPC; angzarr-embedded's `main` cannot host
//! an out-of-process domain, so register the remote player, not this, once
//! it can. Its stubs are the scaffold's and are never meant to run.

use std::convert::Infallible;

use angzarr_embedded::next::markup::Context;
use angzarr_embedded_macros::{aggregate, applies, handles_cmd};

use crate::domain::*;

#[derive(Clone, Debug, Default)]
pub struct PlayerAggregate {}

#[aggregate(domain = "player")]
impl PlayerAggregate {
    #[handles_cmd(DecideMove)]
    fn decide_move(&self, cmd: DecideMove, ctx: &Context) -> Result<MoveDecided, Infallible> {
        todo!()
    }

    #[applies(MoveDecided)]
    fn move_decided(&mut self, event: MoveDecided) {
        todo!()
    }
}
