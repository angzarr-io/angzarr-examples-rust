//! PlayerAggregate: the scaffold of `io.angzarr.examples.v1.PlayerState`, written once by
//! `angzarr scaffold rust` and yours from then on: `--missing` adds the
//! stubs it lacks.
//!
//! HAND-NOTE: only the player domain's CONTRACT, kept because the table's
//! policies need a type to compile against (a `#[policy]`'s source and
//! target are domain types, and `Commands<PlayerAggregate>` holds only
//! commands it decides). It is never registered: the real player is the
//! Python bot, a child process that angzarr-embedded-host registers as the
//! "player" domain (tests/remote_player.rs). Its stubs are the scaffold's
//! and never run.

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
