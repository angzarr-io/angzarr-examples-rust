//! Table hand-for-hand handlers — TDA Rule 12.
//!
//! Tournament fan-out target. Three commands cycle a single table
//! through the bubble synchronisation flow:
//!   - EnterTableHandForHand: tournament tells this table to sync.
//!   - MarkTableHandForHandHandComplete: this table reports its hand.
//!   - EndTableHandForHand: tournament tells this table to exit sync.

use angzarr_client::proto::EventBook;
use angzarr_client::CommandResult;
use examples_proto::{
    EndTableHandForHand, EnterTableHandForHand, MarkTableHandForHandHandComplete,
    TableHandForHandEnded, TableHandForHandRoundComplete, TableHandForHandWaiting,
};
use examples_utils::{event_page, pack_event, reject};

use crate::errors::{TableAlreadyInHandForHand, TableNotFound, TableNotInHandForHand};
use crate::state::TableState;

fn guard_exists(state: &TableState) -> CommandResult<()> {
    if !state.exists() {
        return Err(reject(TableNotFound));
    }
    Ok(())
}

// --- EnterTableHandForHand ---

pub fn handle_enter_table_hand_for_hand(
    cmd: EnterTableHandForHand,
    state: &TableState,
    seq: u32,
) -> CommandResult<EventBook> {
    guard_exists(state)?;
    if state.hand_for_hand {
        return Err(reject(TableAlreadyInHandForHand));
    }

    let event = TableHandForHandWaiting {
        entered_at: Some(angzarr_client::now()),
        tournament_root: cmd.tournament_root,
    };
    let event_any = pack_event(&event, "examples.TableHandForHandWaiting");
    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}

// --- MarkTableHandForHandHandComplete ---

pub fn handle_mark_h4h_hand_complete(
    cmd: MarkTableHandForHandHandComplete,
    state: &TableState,
    seq: u32,
) -> CommandResult<EventBook> {
    guard_exists(state)?;
    if !state.hand_for_hand {
        return Err(reject(TableNotInHandForHand));
    }

    let event = TableHandForHandRoundComplete {
        hand_root: cmd.hand_root,
        completed_at: Some(angzarr_client::now()),
        ..Default::default()
    };
    let event_any = pack_event(&event, "examples.TableHandForHandRoundComplete");
    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}

// --- EndTableHandForHand ---

pub fn handle_end_table_hand_for_hand(
    _cmd: EndTableHandForHand,
    state: &TableState,
    seq: u32,
) -> CommandResult<EventBook> {
    guard_exists(state)?;
    if !state.hand_for_hand {
        return Err(reject(TableNotInHandForHand));
    }

    let event = TableHandForHandEnded {
        ended_at: Some(angzarr_client::now()),
    };
    let event_any = pack_event(&event, "examples.TableHandForHandEnded");
    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::apply_table_created;
    use examples_proto::{GameVariant, TableCreated};

    fn created_state() -> TableState {
        let mut s = TableState::default();
        apply_table_created(
            &mut s,
            TableCreated {
                table_name: "T1".into(),
                game_variant: GameVariant::TexasHoldem as i32,
                small_blind: 5,
                big_blind: 10,
                min_buy_in: 100,
                max_buy_in: 1000,
                max_players: 6,
                action_timeout_seconds: 30,
                created_at: None,
            },
        );
        s
    }

    #[test]
    fn enter_h4h_rejects_when_table_missing() {
        let s = TableState::default();
        let err =
            handle_enter_table_hand_for_hand(EnterTableHandForHand::default(), &s, 1).unwrap_err();
        assert_eq!(err.code, "TABLE_NOT_FOUND");
    }

    #[test]
    fn enter_h4h_emits_entered_event() {
        let s = created_state();
        let book =
            handle_enter_table_hand_for_hand(EnterTableHandForHand::default(), &s, 1).expect("ok");
        assert_eq!(book.pages.len(), 1);
    }

    #[test]
    fn enter_h4h_rejects_when_already_in_h4h() {
        let mut s = created_state();
        s.hand_for_hand = true;
        let err =
            handle_enter_table_hand_for_hand(EnterTableHandForHand::default(), &s, 1).unwrap_err();
        assert_eq!(err.code, "TABLE_ALREADY_IN_HAND_FOR_HAND");
    }

    #[test]
    fn mark_complete_rejects_when_not_in_h4h() {
        let s = created_state();
        let err = handle_mark_h4h_hand_complete(MarkTableHandForHandHandComplete::default(), &s, 1)
            .unwrap_err();
        assert_eq!(err.code, "TABLE_NOT_IN_HAND_FOR_HAND");
    }

    #[test]
    fn mark_complete_emits_event_when_in_h4h() {
        let mut s = created_state();
        s.hand_for_hand = true;
        let book =
            handle_mark_h4h_hand_complete(MarkTableHandForHandHandComplete::default(), &s, 1)
                .expect("ok");
        assert_eq!(book.pages.len(), 1);
    }

    #[test]
    fn end_h4h_rejects_when_not_in_h4h() {
        let s = created_state();
        let err = handle_end_table_hand_for_hand(EndTableHandForHand {}, &s, 1).unwrap_err();
        assert_eq!(err.code, "TABLE_NOT_IN_HAND_FOR_HAND");
    }

    #[test]
    fn end_h4h_emits_event_when_in_h4h() {
        let mut s = created_state();
        s.hand_for_hand = true;
        let book = handle_end_table_hand_for_hand(EndTableHandForHand {}, &s, 1).expect("ok");
        assert_eq!(book.pages.len(), 1);
    }
}
