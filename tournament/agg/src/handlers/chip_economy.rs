//! Chip economy handlers — ColorUp (TDA Rule 24A) and RebalanceTables
//! (TDA Rule 14). Both are operator-issued commands that just emit
//! events; the downstream saga performs per-table chip math and seat
//! movement.

use angzarr_client::proto::EventBook;
use angzarr_client::CommandResult;
use examples_proto::{ColorUp, ColorUpCompleted, PlayerMovedBetweenTables, RebalanceTables};
use examples_utils::{event_page, pack_event, reject};

use crate::errors::{TournamentNotFound, TournamentNotRunning};
use crate::state::TournamentState;

fn guard_running(state: &TournamentState) -> CommandResult<()> {
    if !state.exists() {
        return Err(reject(TournamentNotFound));
    }
    if !state.is_running() {
        return Err(reject(TournamentNotRunning));
    }
    Ok(())
}

// --- ColorUp ---

pub fn handle_color_up(
    cmd: ColorUp,
    state: &TournamentState,
    seq: u32,
) -> CommandResult<EventBook> {
    guard_running(state)?;

    let event = ColorUpCompleted {
        retired_denomination: cmd.retire_denomination,
        new_denomination: cmd.new_denomination,
        completed_at: Some(angzarr_client::now()),
        per_player_awards: Vec::new(),
        chips_added_by_rescue: 0,
        chips_removed_by_race: 0,
    };
    let event_any = pack_event(&event, "examples.ColorUpCompleted");
    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}

// --- RebalanceTables ---
//
// The aggregate emits a synthetic PlayerMovedBetweenTables with empty
// roots; the downstream saga is responsible for translating it into
// concrete LeaveTable / SeatPlayer fan-outs at the participating
// tables.

pub fn handle_rebalance_tables(
    _cmd: RebalanceTables,
    state: &TournamentState,
    seq: u32,
) -> CommandResult<EventBook> {
    guard_running(state)?;

    let event = PlayerMovedBetweenTables {
        player_root: Vec::new(),
        source_table_root: Vec::new(),
        destination_table_root: Vec::new(),
        destination_seat: 0,
        stack: 0,
        moved_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.PlayerMovedBetweenTables");
    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::apply_created;
    use examples_proto::{BlindLevel, GameVariant, TournamentCreated, TournamentStatus};

    fn running_state() -> TournamentState {
        let mut s = TournamentState::default();
        apply_created(
            &mut s,
            TournamentCreated {
                name: "T".into(),
                game_variant: GameVariant::TexasHoldem as i32,
                buy_in: 500,
                starting_stack: 10_000,
                max_players: 9,
                min_players: 2,
                scheduled_start: None,
                rebuy_config: None,
                addon_config: None,
                blind_structure: vec![BlindLevel {
                    level: 1,
                    small_blind: 25,
                    big_blind: 50,
                    ante: 0,
                    duration_minutes: 20,
                }],
                created_at: None,
                ..Default::default()
            },
        );
        s.status = TournamentStatus::TournamentRunning;
        s
    }

    #[test]
    fn color_up_emits_completed_event() {
        let s = running_state();
        let book = handle_color_up(
            ColorUp {
                retire_denomination: 25,
                new_denomination: 100,
            },
            &s,
            1,
        )
        .expect("ok");
        assert_eq!(book.pages.len(), 1);
    }

    #[test]
    fn color_up_rejects_when_not_running() {
        let s = TournamentState::default();
        let err = handle_color_up(
            ColorUp {
                retire_denomination: 25,
                new_denomination: 100,
            },
            &s,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "TOURNAMENT_NOT_FOUND");
    }

    #[test]
    fn rebalance_tables_emits_move_event() {
        let s = running_state();
        let book = handle_rebalance_tables(
            RebalanceTables {
                ..Default::default()
            },
            &s,
            1,
        )
        .expect("ok");
        assert_eq!(book.pages.len(), 1);
    }

    #[test]
    fn rebalance_tables_rejects_when_not_running() {
        let mut s = TournamentState::default();
        s.tournament_id = "ok".into();
        let err = handle_rebalance_tables(
            RebalanceTables {
                ..Default::default()
            },
            &s,
            1,
        )
        .unwrap_err();
        assert_eq!(err.code, "TOURNAMENT_NOT_RUNNING");
    }
}
