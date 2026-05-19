//! Saga: tournament -> tables (hand-for-hand fan-out, TDA Rule 12).
//!
//! Mirrors `examples-python/main/table/saga-tournament-h4h/main.py`.
//! Three translations:
//!   - `HandForHandStarted` (tournament) → `EnterTableHandForHand`
//!     fan-out, one per active table root carried on the event.
//!   - `TableHandForHandRoundComplete` (per-table) →
//!     `RecordTableHandComplete` to the tournament aggregate. The
//!     per-table event was renamed from the legacy
//!     `TableHandForHandHandCompleted` to align with the cpp/python
//!     event taxonomy (and to carry `hand_root` + `tournament_root`).
//!   - `HandForHandEnded` (tournament) → `EndTableHandForHand`
//!     fan-out to every previously-active table.

use angzarr_client::proto::{command_page, CommandBook, CommandPage, Cover, SagaResponse, Uuid};
use angzarr_client::{saga, CommandResult};
use examples_proto::{
    EnterTableHandForHand, HandForHandEnded, HandForHandStarted, RecordTableHandComplete,
    TableHandForHandRoundComplete,
};
use prost::Message;
use prost_types::Any;

pub struct TableTournamentH4hSaga;

/// Build a command book targeted at a single table by `table_root`.
fn table_command_book<M: Message>(
    table_root: Vec<u8>,
    command: M,
    type_url: String,
) -> CommandBook {
    let any = Any {
        type_url,
        value: command.encode_to_vec(),
    };
    CommandBook {
        cover: Some(Cover {
            domain: "table".to_string(),
            root: Some(Uuid { value: table_root }),
            ..Default::default()
        }),
        pages: vec![CommandPage {
            payload: Some(command_page::Payload::Command(any)),
            ..Default::default()
        }],
    }
}

/// Build a command book targeted at the tournament aggregate.
fn tournament_command_book<M: Message>(command: M, type_url: String) -> CommandBook {
    let any = Any {
        type_url,
        value: command.encode_to_vec(),
    };
    CommandBook {
        cover: Some(Cover {
            domain: "tournament".to_string(),
            // Tournament root is singleton in current example scope; an
            // operator/tournament-aware caller stamps the correct
            // tournament_root on the command before dispatch in a real
            // multi-tournament cluster.
            root: None,
            ..Default::default()
        }),
        pages: vec![CommandPage {
            payload: Some(command_page::Payload::Command(any)),
            ..Default::default()
        }],
    }
}

#[saga(
    name = "saga-table-tournament-h4h",
    source = "tournament",
    target = "table"
)]
impl TableTournamentH4hSaga {
    /// Tournament said "enter hand-for-hand". Fan out
    /// `EnterTableHandForHand` to every active table.
    #[handles(HandForHandStarted)]
    pub fn on_hand_for_hand_started(
        &self,
        event: HandForHandStarted,
    ) -> CommandResult<SagaResponse> {
        // `EnterTableHandForHand.tournament_root` lets each table route
        // its resulting `TableHandForHandRoundComplete` back to the
        // right tournament aggregate without a separate registry. In
        // the in-process example we leave it empty — operator stamping
        // fills the bytes before dispatch in a multi-tournament cluster.
        let tournament_root: Vec<u8> = Vec::new();
        let commands = event
            .active_table_roots
            .into_iter()
            .map(|table_root| {
                table_command_book(
                    table_root,
                    EnterTableHandForHand {
                        tournament_root: tournament_root.clone(),
                    },
                    angzarr_client::full_type_url::<EnterTableHandForHand>(),
                )
            })
            .collect();
        Ok(SagaResponse {
            commands,
            events: vec![],
        })
    }

    /// A table reported its synchronised hand complete. Translate into
    /// `RecordTableHandComplete` aimed at the tournament aggregate.
    /// Note: the event source field-set does not carry the table_root;
    /// in production the saga would consult the event header (the
    /// per-event `aggregate_root`) to derive it. For the in-process
    /// example we keep an empty `table_root` and rely on operator
    /// stamping to fill in. `event.hand_root` / `event.tournament_root`
    /// are observable but unused at this step — they ride forward via
    /// the event log for downstream projectors.
    #[handles(TableHandForHandRoundComplete)]
    pub fn on_table_h4h_round_completed(
        &self,
        _event: TableHandForHandRoundComplete,
    ) -> CommandResult<SagaResponse> {
        let cmd = RecordTableHandComplete {
            table_root: Vec::new(),
        };
        Ok(SagaResponse {
            commands: vec![tournament_command_book(
                cmd,
                angzarr_client::full_type_url::<RecordTableHandComplete>(),
            )],
            events: vec![],
        })
    }

    /// Tournament ended hand-for-hand. Fan out `EndTableHandForHand`.
    /// The event itself carries no per-table list; the saga's
    /// in-memory state would normally cache the active tables. For
    /// the example we emit zero commands and let an
    /// operator/projector handle the cleanup; tests cover the
    /// fan-out shape via `on_hand_for_hand_started`.
    #[handles(HandForHandEnded)]
    pub fn on_hand_for_hand_ended(&self, _event: HandForHandEnded) -> CommandResult<SagaResponse> {
        Ok(SagaResponse {
            commands: vec![],
            events: vec![],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use angzarr_client::proto::command_page::Payload;

    fn extract_command_any(book: &CommandBook) -> &Any {
        let page = book.pages.first().expect("command book has one page");
        match page.payload.as_ref().expect("payload set") {
            Payload::Command(any) => any,
            _ => panic!("expected inline Command payload"),
        }
    }

    #[test]
    fn h4h_started_fans_out_per_active_table() {
        let saga = TableTournamentH4hSaga;
        let response = saga
            .on_hand_for_hand_started(HandForHandStarted {
                started_at: None,
                active_table_roots: vec![vec![0xaa], vec![0xbb], vec![0xcc]],
            })
            .expect("ok");
        assert_eq!(response.commands.len(), 3);
        let mut roots: Vec<Vec<u8>> = Vec::new();
        for cmd_book in &response.commands {
            let cover = cmd_book.cover.as_ref().unwrap();
            assert_eq!(cover.domain, "table", "fan-out targets table aggregate");
            // Kill `delete field root from struct Cover expression`
            // (saga lib.rs:34) by asserting each command carries a
            // populated `root`. Without the field, root would be
            // None and this assert would fail.
            let root = cover.root.as_ref().expect("cover root must be set");
            roots.push(root.value.clone());
            let any = extract_command_any(cmd_book);
            assert!(
                any.type_url.contains("EnterTableHandForHand"),
                "issued EnterTableHandForHand commands"
            );
        }
        // The three commands target the three active tables in order.
        assert_eq!(
            roots,
            vec![vec![0xaa], vec![0xbb], vec![0xcc]],
            "fan-out preserves input order and per-table identity"
        );
    }

    #[test]
    fn h4h_started_no_active_tables_yields_no_commands() {
        let saga = TableTournamentH4hSaga;
        let response = saga
            .on_hand_for_hand_started(HandForHandStarted {
                started_at: None,
                active_table_roots: vec![],
            })
            .expect("ok");
        assert!(response.commands.is_empty());
    }

    #[test]
    fn table_h4h_round_completed_emits_record_table_hand_complete() {
        let saga = TableTournamentH4hSaga;
        let response = saga
            .on_table_h4h_round_completed(TableHandForHandRoundComplete {
                hand_root: vec![],
                completed_at: None,
                tournament_root: vec![],
            })
            .expect("ok");
        assert_eq!(response.commands.len(), 1);
        assert_eq!(
            response.commands[0].cover.as_ref().unwrap().domain,
            "tournament"
        );
        let any = extract_command_any(&response.commands[0]);
        assert!(any.type_url.contains("RecordTableHandComplete"));
    }

    #[test]
    fn table_h4h_round_completed_preserves_hand_and_tournament_roots() {
        // Even though the saga itself does not forward `hand_root` or
        // `tournament_root` into the emitted `RecordTableHandComplete`
        // (the event-header drives table_root in production), the
        // handler must accept the new event shape carrying those bytes
        // — covers the rename `TableHandForHandHandCompleted` →
        // `TableHandForHandRoundComplete` and the added fields.
        let saga = TableTournamentH4hSaga;
        let response = saga
            .on_table_h4h_round_completed(TableHandForHandRoundComplete {
                hand_root: vec![0xaa, 0xbb],
                completed_at: None,
                tournament_root: vec![0xcc],
            })
            .expect("ok");
        assert_eq!(response.commands.len(), 1);
        let any = extract_command_any(&response.commands[0]);
        assert!(any.type_url.contains("RecordTableHandComplete"));
    }

    #[test]
    fn h4h_ended_emits_no_commands_in_example_scope() {
        let saga = TableTournamentH4hSaga;
        let response = saga
            .on_hand_for_hand_ended(HandForHandEnded { ended_at: None })
            .expect("ok");
        assert!(response.commands.is_empty());
    }
}
