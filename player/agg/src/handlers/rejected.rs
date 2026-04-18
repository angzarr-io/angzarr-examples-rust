//! Rejection handlers for saga/PM compensation.

use angzarr_client::proto::{BusinessResponse, EventBook, Notification, RejectionNotification};
use angzarr_client::{
    emit_compensation_events, event_page, now, pack_event, unpack, CommandResult,
};
use examples_proto::{Currency, FundsReleased};
use tracing::warn;

use crate::state::PlayerState;

// docs:start:rejected_handler

/// Handle JoinTable rejection by releasing reserved funds.
///
/// Called when the JoinTable command (issued by saga-player-table after
/// FundsReserved) is rejected by the Table aggregate.
pub fn handle_join_rejected(
    notification: &Notification,
    state: &PlayerState,
) -> CommandResult<BusinessResponse> {
    let rejection = notification
        .payload
        .as_ref()
        .and_then(|any| unpack::<RejectionNotification>(any).ok())
        .unwrap_or_default();

    warn!(
        rejection_reason = %rejection.rejection_reason,
        "Player compensation for JoinTable rejection"
    );

    let table_root = rejection
        .rejected_command
        .as_ref()
        .and_then(|cmd| cmd.cover.as_ref())
        .map(|cover| {
            cover
                .root
                .as_ref()
                .map(|r| r.value.clone())
                .unwrap_or_default()
        })
        .unwrap_or_default();

    let table_key = hex::encode(&table_root);
    let reserved_amount = state
        .table_reservations
        .get(&table_key)
        .copied()
        .unwrap_or(0);
    let new_reserved = state.reserved_funds - reserved_amount;
    let new_available = state.bankroll - new_reserved;

    let event = FundsReleased {
        amount: Some(Currency {
            amount: reserved_amount,
            currency_code: "CHIPS".to_string(),
        }),
        table_root,
        new_available_balance: Some(Currency {
            amount: new_available,
            currency_code: "CHIPS".to_string(),
        }),
        new_reserved_balance: Some(Currency {
            amount: new_reserved,
            currency_code: "CHIPS".to_string(),
        }),
        released_at: Some(now()),
    };

    let event_any = pack_event(&event, "examples.FundsReleased");

    let event_book = EventBook {
        cover: notification.cover.clone(),
        pages: vec![event_page(0, event_any)],
        snapshot: None,
        next_sequence: 0,
    };

    Ok(emit_compensation_events(event_book))
}

// docs:end:rejected_handler

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{apply_registered, apply_reserved};
    use angzarr_client::proto::business_response::Result as BizResult;
    use angzarr_client::proto::event_page::Payload;
    use angzarr_client::proto::{
        CommandBook, CommandPage, Cover, PageHeader, Uuid as ProtoUuid,
    };
    use examples_proto::{Currency, FundsReserved, PlayerRegistered, PlayerType};
    use prost::Message;
    use prost_types::Any;

    fn currency(amount: i64) -> Currency {
        Currency {
            amount,
            currency_code: "CHIPS".into(),
        }
    }

    fn state_with_reservation(table_id: &[u8], amount: i64) -> PlayerState {
        let mut s = PlayerState::default();
        apply_registered(
            &mut s,
            PlayerRegistered {
                display_name: "A".into(),
                email: "a@x".into(),
                player_type: PlayerType::Human as i32,
                ai_model_id: String::new(),
                registered_at: None,
            },
        );
        s.bankroll = 1000;
        apply_reserved(
            &mut s,
            FundsReserved {
                amount: Some(currency(amount)),
                table_root: table_id.to_vec(),
                new_available_balance: Some(currency(1000 - amount)),
                new_reserved_balance: Some(currency(amount)),
                reserved_at: None,
            },
        );
        s
    }

    fn make_notification(table_root: Vec<u8>) -> Notification {
        // Build a RejectionNotification whose rejected_command.cover.root.value
        // carries the table_root that handle_join_rejected should extract.
        let rejected_command = CommandBook {
            cover: Some(Cover {
                domain: "table".into(),
                root: Some(ProtoUuid {
                    value: table_root,
                }),
                correlation_id: String::new(),
                edition: None,
            }),
            pages: vec![CommandPage {
                header: Some(PageHeader {
                    sequence_type: None,
                }),
                merge_strategy: 0,
                payload: None,
            }],
        };
        let rejection = RejectionNotification {
            rejected_command: Some(rejected_command),
            rejection_reason: "seat_occupied".into(),
        };
        let payload_any = Any {
            type_url: format!(
                "{}{}",
                angzarr_client::TYPE_URL_PREFIX,
                "angzarr.RejectionNotification"
            ),
            value: rejection.encode_to_vec(),
        };
        Notification {
            cover: Some(Cover {
                domain: "player".into(),
                root: Some(ProtoUuid {
                    value: vec![0xff; 16],
                }),
                correlation_id: String::new(),
                edition: None,
            }),
            payload: Some(payload_any),
            sent_at: None,
            metadata: Default::default(),
        }
    }

    #[test]
    fn handle_join_rejected_extracts_table_root_and_builds_release_event() {
        let table_root = vec![1u8, 2, 3, 4];
        let state = state_with_reservation(&table_root, 400);
        let notification = make_notification(table_root.clone());

        let response = handle_join_rejected(&notification, &state).expect("ok");
        let events = match response.result {
            Some(BizResult::Events(book)) => book,
            other => panic!("expected Events variant, got {:?}", other.is_some()),
        };
        assert_eq!(events.pages.len(), 1);
        let any = match events.pages[0].payload.as_ref() {
            Some(Payload::Event(a)) => a,
            _ => panic!("expected event payload"),
        };
        assert!(any.type_url.ends_with("examples.FundsReleased"));
        let decoded = FundsReleased::decode(any.value.as_slice()).expect("decode");
        assert_eq!(decoded.table_root, table_root);
        assert_eq!(decoded.amount.unwrap().amount, 400);
        assert_eq!(decoded.new_reserved_balance.unwrap().amount, 0);
    }

    #[test]
    fn handle_join_rejected_with_missing_reservation_emits_zero_amount() {
        let state = state_with_reservation(&[9, 9], 100);
        // rejected command references a different table
        let notification = make_notification(vec![0xab, 0xcd]);
        let response = handle_join_rejected(&notification, &state).expect("ok");
        let events = match response.result {
            Some(BizResult::Events(book)) => book,
            _ => panic!("expected Events"),
        };
        let any = match events.pages[0].payload.as_ref() {
            Some(Payload::Event(a)) => a,
            _ => panic!(),
        };
        let decoded = FundsReleased::decode(any.value.as_slice()).unwrap();
        assert_eq!(decoded.amount.unwrap().amount, 0);
    }
}
