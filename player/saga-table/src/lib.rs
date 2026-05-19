//! Saga: Player -> Table.
//!
//! Player owns the intent to sit out / sit in. The table aggregate accepts
//! these as facts (no validation) since the player has authority over their
//! own participation state.
//!
//! Flow:
//!   * `player.PlayerSittingOut`     → `table.PlayerSatOut` (fact)
//!   * `player.PlayerReturningToPlay` → `table.PlayerSatIn`  (fact)
//!
//! `player_root` is extracted from `source_cover.root.value` — the originating
//! player aggregate's UUID. Mirrors Python's
//! `player_root = source_cover.proto().root.value` pattern.

use angzarr_client::proto::{
    event_page::Payload as EventPayload, page_header::SequenceType, Cover, EventBook, EventPage,
    ExternalDeferredSequence, PageHeader, SagaResponse, Uuid as ProtoUuid,
};
use angzarr_client::{full_type_url, saga, CommandResult};
use examples_proto::{PlayerReturningToPlay, PlayerSatIn, PlayerSatOut, PlayerSittingOut};
use prost::{Message, Name};
use prost_types::Any;

pub struct PlayerTableSaga;

fn player_root_from(source_cover: &Option<Cover>) -> Vec<u8> {
    source_cover
        .as_ref()
        .and_then(|c| c.root.as_ref())
        .map(|r| r.value.clone())
        .unwrap_or_default()
}

#[saga(name = "saga-player-table", source = "player", target = "table")]
impl PlayerTableSaga {
    /// `player.PlayerSittingOut` → `table.PlayerSatOut` fact.
    #[handles(PlayerSittingOut)]
    pub fn on_player_sitting_out(
        &self,
        event: PlayerSittingOut,
        source_cover: Option<Cover>,
    ) -> CommandResult<SagaResponse> {
        let player_root = player_root_from(&source_cover);
        let external_id = format!("sitout-{}", hex::encode(&player_root));
        let fact = PlayerSatOut {
            player_root,
            sat_out_at: event.sat_out_at,
        };
        Ok(SagaResponse {
            commands: vec![],
            events: vec![fact_book::<PlayerSatOut>(
                &event.table_root,
                &fact,
                &external_id,
            )],
        })
    }

    /// `player.PlayerReturningToPlay` → `table.PlayerSatIn` fact.
    #[handles(PlayerReturningToPlay)]
    pub fn on_player_returning_to_play(
        &self,
        event: PlayerReturningToPlay,
        source_cover: Option<Cover>,
    ) -> CommandResult<SagaResponse> {
        let player_root = player_root_from(&source_cover);
        let external_id = format!("sitin-{}", hex::encode(&player_root));
        let fact = PlayerSatIn {
            player_root,
            sat_in_at: event.sat_in_at,
        };
        Ok(SagaResponse {
            commands: vec![],
            events: vec![fact_book::<PlayerSatIn>(
                &event.table_root,
                &fact,
                &external_id,
            )],
        })
    }
}

fn fact_book<M: Message + Name>(table_root: &[u8], fact: &M, external_id: &str) -> EventBook {
    // ExternalDeferred header keys dedup on a stable string derived from the
    // originating aggregate (e.g. `sitout-<player_root_hex>`). Mirrors Python's
    // `external_deferred=ExternalDeferredSequence(external_id=…)` shape so AMQP
    // redelivery resolves to the cached fact at the table's pipeline.
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
                sync_mode: None,
                sequence_type: Some(SequenceType::ExternalDeferred(ExternalDeferredSequence {
                    external_id: external_id.to_string(),
                    description: String::new(),
                })),
            }),
            created_at: Some(angzarr_client::now()),
            no_commit: false,
            cascade_id: None,
            payload: Some(EventPayload::Event(Any {
                type_url: full_type_url::<M>(),
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

    fn player_cover(player_root: Vec<u8>) -> Option<Cover> {
        Some(Cover {
            domain: "player".to_string(),
            root: Some(ProtoUuid { value: player_root }),
            correlation_id: String::new(),
            edition: None,
        })
    }

    #[test]
    fn sitting_out_emits_fact_to_table_with_player_root_from_source_cover() {
        let saga = PlayerTableSaga;
        let player_root = vec![0x11; 16];
        let resp = saga
            .on_player_sitting_out(
                PlayerSittingOut {
                    table_root: vec![0xaa],
                    sat_out_at: None,
                },
                player_cover(player_root.clone()),
            )
            .expect("ok");
        assert!(resp.commands.is_empty());
        assert_eq!(resp.events.len(), 1);

        let cover = resp.events[0].cover.as_ref().expect("cover");
        assert_eq!(cover.domain, "table");
        assert_eq!(cover.root.as_ref().expect("root").value, vec![0xaa]);

        // Decode the fact and check the player_root was lifted from source_cover.
        let payload = match resp.events[0].pages[0].payload.as_ref().unwrap() {
            EventPayload::Event(any) => any,
            _ => panic!("inline event"),
        };
        let fact = PlayerSatOut::decode(&*payload.value).expect("decode");
        assert_eq!(fact.player_root, player_root);

        // Header carries an ExternalDeferred id derived from the player root.
        let header = resp.events[0].pages[0].header.as_ref().expect("header");
        match header.sequence_type.as_ref().expect("seq type") {
            SequenceType::ExternalDeferred(ext) => {
                assert_eq!(
                    ext.external_id,
                    format!("sitout-{}", hex::encode(&player_root))
                );
            }
            other => panic!("expected ExternalDeferred, got {:?}", other),
        }
    }

    #[test]
    fn returning_emits_fact_to_table_with_player_root_from_source_cover() {
        let saga = PlayerTableSaga;
        let player_root = vec![0x22; 16];
        let resp = saga
            .on_player_returning_to_play(
                PlayerReturningToPlay {
                    table_root: vec![0xbb],
                    sat_in_at: None,
                },
                player_cover(player_root.clone()),
            )
            .expect("ok");
        assert_eq!(resp.events.len(), 1);
        let payload = match resp.events[0].pages[0].payload.as_ref().unwrap() {
            EventPayload::Event(any) => any,
            _ => panic!("inline event"),
        };
        assert!(payload.type_url.ends_with("examples.PlayerSatIn"));
        let fact = PlayerSatIn::decode(&*payload.value).expect("decode");
        assert_eq!(fact.player_root, player_root);
    }

    #[test]
    fn sitting_out_with_no_source_cover_leaves_player_root_empty() {
        // If the framework didn't surface a source cover, the saga still
        // emits a well-formed fact — just with empty player_root, matching
        // Python's `if source_cover is not None else b""` fallback.
        let saga = PlayerTableSaga;
        let resp = saga
            .on_player_sitting_out(
                PlayerSittingOut {
                    table_root: vec![0xaa],
                    sat_out_at: None,
                },
                None,
            )
            .expect("ok");
        let payload = match resp.events[0].pages[0].payload.as_ref().unwrap() {
            EventPayload::Event(any) => any,
            _ => panic!("inline event"),
        };
        let fact = PlayerSatOut::decode(&*payload.value).expect("decode");
        assert!(fact.player_root.is_empty());
    }
}
