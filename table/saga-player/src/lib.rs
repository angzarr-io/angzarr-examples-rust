//! Saga: Table -> Player (library).

use angzarr_client::proto::{command_page, CommandBook, CommandPage, Cover, SagaResponse, Uuid};
use angzarr_client::{saga, CommandResult};
use examples_proto::{HandEnded, ReleaseFunds};
use prost::Message;
use prost_types::Any;

/// Translate `table.HandEnded` into a `ReleaseFunds` command per seated player.
pub struct TablePlayerSaga;

#[saga(name = "saga-table-player", source = "table", target = "player")]
impl TablePlayerSaga {
    #[handles(HandEnded)]
    pub fn on_hand_ended(&self, event: HandEnded) -> CommandResult<SagaResponse> {
        let commands: Vec<CommandBook> = event
            .stack_changes
            .keys()
            .filter_map(|player_hex| {
                let player_root = hex::decode(player_hex).ok()?;

                let release_funds = ReleaseFunds {
                    table_root: event.hand_root.clone(),
                };

                let command_any = Any {
                    type_url: "type.googleapis.com/examples.ReleaseFunds".to_string(),
                    value: release_funds.encode_to_vec(),
                };

                Some(CommandBook {
                    cover: Some(Cover {
                        domain: "player".to_string(),
                        root: Some(Uuid { value: player_root }),
                        ..Default::default()
                    }),
                    pages: vec![CommandPage {
                        payload: Some(command_page::Payload::Command(command_any)),
                        ..Default::default()
                    }],
                })
            })
            .collect();

        Ok(SagaResponse {
            commands,
            events: vec![],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use angzarr_client::proto::command_page::Payload;
    use std::collections::HashMap;

    fn extract_command_any(book: &CommandBook) -> &Any {
        let page = book.pages.first().expect("command book has one page");
        match page.payload.as_ref().expect("payload set") {
            Payload::Command(any) => any,
            _ => panic!("expected inline Command payload"),
        }
    }

    #[test]
    fn emits_one_release_funds_per_player_in_stack_changes() {
        let saga = TablePlayerSaga;
        let hand_root = vec![0xCA, 0xFE, 0xBA, 0xBE];

        let player_a = vec![0xAA_u8; 16];
        let player_b = vec![0xBB_u8; 16];
        let player_c = vec![0xCC_u8; 16];

        let mut stack_changes: HashMap<String, i64> = HashMap::new();
        stack_changes.insert(hex::encode(&player_a), 100);
        stack_changes.insert(hex::encode(&player_b), -50);
        stack_changes.insert(hex::encode(&player_c), -50);

        let event = HandEnded {
            hand_root: hand_root.clone(),
            results: vec![],
            stack_changes,
            ended_at: None,
        };

        let response = saga
            .on_hand_ended(event)
            .expect("handler succeeds");

        assert_eq!(response.commands.len(), 3);
        assert!(response.events.is_empty());

        // Collect actual player_roots from the emitted commands; iteration
        // order is HashMap-dependent so we compare as sets.
        let mut seen_roots: Vec<Vec<u8>> = Vec::new();
        for book in &response.commands {
            let cover = book.cover.as_ref().expect("cover set");
            assert_eq!(cover.domain, "player");
            let root = cover.root.as_ref().expect("root set").value.clone();
            seen_roots.push(root);

            let cmd_any = extract_command_any(book);
            assert!(
                cmd_any.type_url.ends_with("examples.ReleaseFunds"),
                "type_url = {}",
                cmd_any.type_url
            );

            let release =
                ReleaseFunds::decode(cmd_any.value.as_slice())
                    .expect("decode ReleaseFunds");
            // Saga populates the table_root field from event.hand_root.
            assert_eq!(release.table_root, hand_root);
        }

        seen_roots.sort();
        let mut expected_roots = vec![player_a, player_b, player_c];
        expected_roots.sort();
        assert_eq!(seen_roots, expected_roots);
    }

    #[test]
    fn empty_stack_changes_yields_no_commands() {
        let saga = TablePlayerSaga;
        let response = saga
            .on_hand_ended(HandEnded {
                hand_root: vec![0x01, 0x02],
                results: vec![],
                stack_changes: HashMap::new(),
                ended_at: None,
            })
            .expect("handler succeeds");

        assert!(response.commands.is_empty());
        assert!(response.events.is_empty());
    }

    #[test]
    fn invalid_hex_keys_are_filtered_out() {
        let saga = TablePlayerSaga;
        let good_player = vec![0x12_u8, 0x34, 0x56];

        let mut stack_changes: HashMap<String, i64> = HashMap::new();
        stack_changes.insert(hex::encode(&good_player), 10);
        stack_changes.insert("not-valid-hex!!".to_string(), -5);
        stack_changes.insert("zz".to_string(), -5);

        let response = saga
            .on_hand_ended(HandEnded {
                hand_root: vec![0xAA],
                results: vec![],
                stack_changes,
                ended_at: None,
            })
            .expect("handler succeeds");

        assert_eq!(response.commands.len(), 1);
        let book = &response.commands[0];
        assert_eq!(book.cover.as_ref().unwrap().domain, "player");
        assert_eq!(
            book.cover.as_ref().unwrap().root.as_ref().unwrap().value,
            good_player
        );
        let cmd_any = extract_command_any(book);
        assert!(cmd_any.type_url.ends_with("examples.ReleaseFunds"));
        let release =
            ReleaseFunds::decode(cmd_any.value.as_slice()).unwrap();
        assert_eq!(release.table_root, vec![0xAA]);
    }
}
