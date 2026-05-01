//! Cross-aggregate state helpers for synchronous DECISION pre-validation.
//!
//! The reservation PM needs to peek at the destination Table / Tournament
//! event book before issuing the player primitives, so it can short-circuit
//! invalid buy-ins / rebuys / registrations without waiting on a downstream
//! reject. Mirrors `examples-python/main/reservation/pmg/{table_state,
//! tournament_state}.py`.
//!
//! The async [`angzarr_client::traits::QueryClient`] trait can't be driven
//! from the PM's synchronous `#[handles]` signature without nesting tokio
//! runtimes. We expose a sync [`CrossAggregateQuery`] trait instead — easy
//! to back with an in-memory event book in tests and trivial to skip
//! (return None) in cluster runs.

use std::collections::HashMap;

use angzarr_client::proto::{event_page::Payload as EventPayload, EventBook};
use angzarr_client::type_name_from_url;
use examples_proto::{
    PlayerJoined, PlayerLeft, PlayerSeated, RegistrationClosed, RegistrationOpened, TableCreated,
    TournamentCreated, TournamentPlayerEnrolled, TournamentStarted, TournamentStatus,
};
use prost::Message;
use prost_types::Any;

/// Decode an `Any` payload by short name match, ignoring the package prefix.
///
/// The proto package can drift between source generations
/// (`examples.X` vs `angzarr_client.proto.examples.X`) without changing the
/// wire format. We match on the short name (last `.`-segment) and decode by
/// the message type's wire bytes — equivalent to python's
/// `event.Unpack(MyMsg())` which doesn't enforce the prefix.
fn decode_any<T: Message + Default>(any: &Any) -> Result<T, prost::DecodeError> {
    T::decode(any.value.as_slice())
}

/// Synchronous cross-aggregate query interface.
///
/// Returns `None` when the aggregate is unknown or the underlying transport
/// can't be driven synchronously. Soft-skip semantics: callers must treat
/// `None` as "no live state available, skip pre-validation."
pub trait CrossAggregateQuery: Send + Sync {
    fn get_event_book(&self, domain: &str, root: &[u8]) -> Option<EventBook>;
}

/// Minimal table state for PM pre-validation.
#[derive(Debug, Clone, Default)]
pub struct TableStateHelper {
    pub table_id: String,
    pub table_name: String,
    pub min_buy_in: i64,
    pub max_buy_in: i64,
    pub max_players: i32,
    pub seats: HashMap<i32, Vec<u8>>,
}

impl TableStateHelper {
    pub fn find_seat_by_player(&self, player_root: &[u8]) -> Option<i32> {
        self.seats
            .iter()
            .find(|(_, root)| root.as_slice() == player_root)
            .map(|(seat, _)| *seat)
    }
}

/// Minimal tournament state for PM pre-validation + fee lookup.
#[derive(Debug, Clone)]
pub struct TournamentStateHelper {
    pub status: TournamentStatus,
    pub registration_open: bool,
    pub max_players: i32,
    pub registered_count: i32,
    pub buy_in: i64,
    pub starting_stack: i64,
    pub registered_players: std::collections::HashSet<String>,
    pub rebuy_allowed: bool,
    pub rebuy_cost: i64,
    pub rebuy_chips: i64,
}

impl Default for TournamentStateHelper {
    fn default() -> Self {
        Self {
            status: TournamentStatus::Unspecified,
            registration_open: true,
            max_players: 0,
            registered_count: 0,
            buy_in: 0,
            starting_stack: 0,
            registered_players: Default::default(),
            rebuy_allowed: false,
            rebuy_cost: 0,
            rebuy_chips: 0,
        }
    }
}

/// Rebuild [`TableStateHelper`] from an EventBook.
pub fn table_state_from_event_book(book: &EventBook) -> TableStateHelper {
    let mut state = TableStateHelper::default();
    for page in &book.pages {
        let Some(EventPayload::Event(any)) = page.payload.as_ref() else {
            continue;
        };
        let name = type_name_from_url(&any.type_url);
        let short = name.rsplit('.').next().unwrap_or(name);
        match short {
            "TableCreated" => {
                if let Ok(e) = decode_any::<TableCreated>(any) {
                    state.table_id = format!("table_{}", e.table_name);
                    state.table_name = e.table_name;
                    state.min_buy_in = e.min_buy_in;
                    state.max_buy_in = e.max_buy_in;
                    state.max_players = e.max_players;
                }
            }
            "PlayerJoined" => {
                if let Ok(e) = decode_any::<PlayerJoined>(any) {
                    state.seats.insert(e.seat_position, e.player_root);
                }
            }
            "PlayerSeated" => {
                if let Ok(e) = decode_any::<PlayerSeated>(any) {
                    state.seats.insert(e.seat_position, e.player_root);
                }
            }
            "PlayerLeft" => {
                if let Ok(e) = decode_any::<PlayerLeft>(any) {
                    state.seats.remove(&e.seat_position);
                }
            }
            _ => {}
        }
    }
    state
}

/// Rebuild [`TournamentStateHelper`] from an EventBook.
pub fn tournament_state_from_event_book(book: &EventBook) -> TournamentStateHelper {
    let mut state = TournamentStateHelper::default();
    for page in &book.pages {
        let Some(EventPayload::Event(any)) = page.payload.as_ref() else {
            continue;
        };
        let name = type_name_from_url(&any.type_url);
        let short = name.rsplit('.').next().unwrap_or(name);
        match short {
            "TournamentCreated" => {
                if let Ok(e) = decode_any::<TournamentCreated>(any) {
                    state.status = TournamentStatus::TournamentCreated;
                    state.max_players = e.max_players;
                    state.buy_in = e.buy_in;
                    state.starting_stack = e.starting_stack;
                    state.registration_open = true;
                    if let Some(rb) = e.rebuy_config.as_ref() {
                        state.rebuy_allowed = rb.enabled;
                        state.rebuy_cost = rb.rebuy_cost;
                        state.rebuy_chips = rb.rebuy_chips;
                    }
                }
            }
            "RegistrationOpened" => {
                if let Ok(_e) = decode_any::<RegistrationOpened>(any) {
                    state.status = TournamentStatus::TournamentRegistrationOpen;
                    state.registration_open = true;
                }
            }
            "RegistrationClosed" => {
                if let Ok(_e) = decode_any::<RegistrationClosed>(any) {
                    state.registration_open = false;
                }
            }
            "TournamentStarted" => {
                if let Ok(_e) = decode_any::<TournamentStarted>(any) {
                    state.status = TournamentStatus::TournamentRunning;
                    state.registration_open = false;
                }
            }
            "TournamentPlayerEnrolled" => {
                if let Ok(e) = decode_any::<TournamentPlayerEnrolled>(any) {
                    state.registered_players.insert(hex::encode(&e.player_root));
                    state.registered_count = state.registered_players.len() as i32;
                }
            }
            _ => {}
        }
    }
    state
}

/// Helper consumers fetch via the [`CrossAggregateQuery`] trait. Returns
/// the default helper when the client is None or the query yields nothing.
pub fn fetch_table_state(
    query: Option<&dyn CrossAggregateQuery>,
    table_root: &[u8],
) -> TableStateHelper {
    let Some(q) = query else {
        return TableStateHelper::default();
    };
    if table_root.is_empty() {
        return TableStateHelper::default();
    }
    match q.get_event_book("table", table_root) {
        Some(book) => table_state_from_event_book(&book),
        None => TableStateHelper::default(),
    }
}

pub fn fetch_tournament_state(
    query: Option<&dyn CrossAggregateQuery>,
    tournament_root: &[u8],
) -> TournamentStateHelper {
    let Some(q) = query else {
        return TournamentStateHelper::default();
    };
    if tournament_root.is_empty() {
        return TournamentStateHelper::default();
    }
    match q.get_event_book("tournament", tournament_root) {
        Some(book) => tournament_state_from_event_book(&book),
        None => TournamentStateHelper::default(),
    }
}
