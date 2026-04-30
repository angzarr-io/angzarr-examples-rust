//! Saga: Player -> Table.
//!
//! Player owns the intent to sit out / sit in. The table aggregate accepts
//! these as facts (no validation) since the player has authority over their
//! own participation state.
//!
//! Flow:
//!   * `player.PlayerSittingOut`     → `table.PlayerSatOut` (fact)
//!   * `player.PlayerReturningToPlay` → `table.PlayerSatIn`  (fact)

use angzarr_client::proto::{
    event_page::Payload as EventPayload, page_header::SequenceType, Cover, EventBook, EventPage,
    PageHeader, SagaResponse, Uuid as ProtoUuid,
};
use angzarr_client::{saga, type_url, CommandResult};
use examples_proto::{PlayerReturningToPlay, PlayerSatIn, PlayerSatOut, PlayerSittingOut};
use prost::Message;
use prost_types::Any;

pub struct PlayerTableSaga;

#[saga(name = "saga-player-table", source = "player", target = "table")]
impl PlayerTableSaga {
    /// `player.PlayerSittingOut` → `table.PlayerSatOut` fact.
    #[handles(PlayerSittingOut)]
    pub fn on_player_sitting_out(&self, event: PlayerSittingOut) -> CommandResult<SagaResponse> {
        let fact = PlayerSatOut {
            // Player root not surfaced through the saga dispatch; the fact's
            // recipient is the table identified by `table_root`. Table aggregate
            // validates the fact arrives correlated to a known seat by matching
            // on its own seating state.
            player_root: Vec::new(),
            sat_out_at: event.sat_out_at,
        };
        Ok(SagaResponse {
            commands: vec![],
            events: vec![fact_book(&event.table_root, &fact, "examples.PlayerSatOut")],
        })
    }

    /// `player.PlayerReturningToPlay` → `table.PlayerSatIn` fact.
    #[handles(PlayerReturningToPlay)]
    pub fn on_player_returning_to_play(
        &self,
        event: PlayerReturningToPlay,
    ) -> CommandResult<SagaResponse> {
        let fact = PlayerSatIn {
            player_root: Vec::new(),
            sat_in_at: event.sat_in_at,
        };
        Ok(SagaResponse {
            commands: vec![],
            events: vec![fact_book(&event.table_root, &fact, "examples.PlayerSatIn")],
        })
    }
}

fn fact_book<M: Message>(table_root: &[u8], fact: &M, proto_type_name: &str) -> EventBook {
    EventBook {
        cover: Some(Cover {
            domain: "table".to_string(),
            root: Some(ProtoUuid {
                value: table_root.to_vec(),
            }),
            correlation_id: String::new(),
            edition: None,
        }),
        pages: vec![EventPage {
            header: Some(PageHeader {
                sequence_type: Some(SequenceType::Sequence(0)),
            }),
            created_at: Some(angzarr_client::now()),
            no_commit: false,
            cascade_id: None,
            payload: Some(EventPayload::Event(Any {
                type_url: type_url(proto_type_name),
                value: fact.encode_to_vec(),
            })),
        }],
        snapshot: None,
        next_sequence: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sitting_out_emits_fact_to_table() {
        let saga = PlayerTableSaga;
        let resp = saga
            .on_player_sitting_out(PlayerSittingOut {
                table_root: vec![0xaa],
                sat_out_at: None,
            })
            .expect("ok");
        assert!(resp.commands.is_empty());
        assert_eq!(resp.events.len(), 1);
        let cover = resp.events[0].cover.as_ref().expect("cover");
        assert_eq!(cover.domain, "table");
        assert_eq!(cover.root.as_ref().expect("root").value, vec![0xaa]);
    }

    #[test]
    fn returning_emits_fact_to_table() {
        let saga = PlayerTableSaga;
        let resp = saga
            .on_player_returning_to_play(PlayerReturningToPlay {
                table_root: vec![0xbb],
                sat_in_at: None,
            })
            .expect("ok");
        assert_eq!(resp.events.len(), 1);
        let payload = match resp.events[0].pages[0].payload.as_ref().unwrap() {
            EventPayload::Event(any) => any,
            _ => panic!("inline event"),
        };
        assert!(payload.type_url.ends_with("examples.PlayerSatIn"));
    }
}
