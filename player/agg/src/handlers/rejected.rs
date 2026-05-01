//! Rejection handlers for saga/PM compensation.

use angzarr_client::proto::{BusinessResponse, EventBook, Notification, RejectionNotification};
use angzarr_client::{emit_compensation_events, now, unpack, CommandResult};
use examples_proto::{Currency, FundsReleased};
use examples_utils::{event_page, pack_event};
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

    let key = rejection
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

    let key_hex = hex::encode(&key);
    let reserved_amount = state.table_reservations.get(&key_hex).copied().unwrap_or(0);
    let new_reserved = state.reserved_funds - reserved_amount;
    let new_available = state.bankroll - new_reserved;

    let event = FundsReleased {
        amount: Some(Currency {
            amount: reserved_amount,
            currency_code: "CHIPS".to_string(),
        }),
        key,
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
