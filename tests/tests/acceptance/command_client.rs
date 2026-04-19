//! CommandClient abstraction mirroring examples-python/main/tests/command_client.py.
//!
//! - InProcessClient: calls handler functions directly, maintains an in-memory
//!   event store keyed by (domain, root_hex) so commands replay state from
//!   prior commands in the same scenario. Received events are collected
//!   synchronously as commands produce them.
//! - GrpcClient: feature-gated (`acceptance-test`) — sends via tonic to the
//!   coordinator URLs (PLAYER_URL, TABLE_URL, HAND_URL). Subscribes to the
//!   EventStreamService (STREAM_URL) filtered by correlation_id and drains
//!   into a shared buffer. State queries go through EventQueryService and
//!   are rebuilt locally via the same `apply_*` functions.
//!
//! Factory: if PLAYER_URL is set in the environment, use GrpcClient
//! (requires the `acceptance-test` feature at build time); otherwise,
//! InProcessClient. Default-mode tests use InProcess.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use angzarr_client::proto::{event_page, EventBook, EventPage};
use angzarr_client::try_unpack;
use prost_types::Any;

use agg_hand::state::HandState;
use agg_player::state::PlayerState;
use agg_table::state::TableState;

use examples_proto::{
    ActionTaken, BettingRoundComplete, BlindPosted, BuyInConfirmed, BuyInRequested,
    BuyInReservationReleased, CardsDealt, ChipsAdded, CommunityCardsDealt, DrawCompleted,
    FundsDeposited, FundsReleased, FundsReserved, FundsTransferred, FundsWithdrawn,
    HandComplete, HandEnded, HandStarted, PlayerJoined, PlayerLeft, PlayerRegistered,
    PlayerSatIn, PlayerSatOut, PlayerSeated, PotAwarded, RebuyChipsAdded, RebuyFeeConfirmed,
    RebuyFeeReleased, RebuyRequested, RegistrationFeeConfirmed, RegistrationFeeReleased,
    RegistrationRequested, SeatingRejected, ShowdownStarted, TableCreated,
};

#[derive(Debug, Clone)]
pub struct SendError(pub String);

impl std::fmt::Display for SendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for SendError {}

/// Domain-tagged aggregate state snapshot returned by `get_state`.
pub enum StateSnapshot {
    Player(Box<PlayerState>),
    Table(Box<TableState>),
    Hand(Box<HandState>),
}

/// Shared buffer of events received for the active scenario.
/// - InProcess: populated synchronously whenever a command succeeds.
/// - gRPC: populated asynchronously by a stream-draining task spawned
///   in `set_correlation`.
pub type EventBuffer = Arc<Mutex<Vec<EventPage>>>;

/// Abstract command client.
pub trait CommandClient: Send {
    fn send_command(
        &self,
        domain: &str,
        root: &[u8],
        cmd: Any,
        sequence: u32,
    ) -> Result<EventBook, SendError>;

    /// Rebuild the current aggregate state from persisted events.
    /// Returns None if the aggregate has no events yet.
    fn get_state(&self, domain: &str, root: &[u8]) -> Option<StateSnapshot>;

    /// Handle to the scenario's received-event buffer. Steps poll this
    /// for `within N seconds` assertions.
    fn received_events(&self) -> EventBuffer;

    /// Set per-scenario correlation_id; called by the `before` hook.
    /// In gRPC mode this opens an EventStreamService subscription.
    fn set_correlation(&self, correlation_id: &str);

    fn close(&mut self);
}

// ===========================================================================
// InProcessClient
// ===========================================================================

pub struct InProcessClient {
    store: Mutex<HashMap<(String, String), Vec<Any>>>,
    received: EventBuffer,
}

impl InProcessClient {
    pub fn new() -> Self {
        Self {
            store: Mutex::new(HashMap::new()),
            received: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn append_events(&self, domain: &str, root: &[u8], book: &EventBook) {
        let key = (domain.to_string(), hex::encode(root));
        let mut store = self.store.lock().unwrap();
        let entry = store.entry(key).or_default();
        let mut rcv = self.received.lock().unwrap();
        for page in &book.pages {
            if let Some(event_page::Payload::Event(any)) = &page.payload {
                entry.push(any.clone());
            }
            rcv.push(page.clone());
        }
    }

    fn load_events(&self, domain: &str, root: &[u8]) -> Vec<Any> {
        let key = (domain.to_string(), hex::encode(root));
        self.store.lock().unwrap().get(&key).cloned().unwrap_or_default()
    }

    fn next_seq(&self, domain: &str, root: &[u8]) -> u32 {
        self.load_events(domain, root).len() as u32
    }

    fn dispatch_player(&self, root: &[u8], cmd: &Any) -> Result<EventBook, SendError> {
        use agg_player::handlers::*;
        let state = rebuild_player(&self.load_events("player", root));
        let seq = self.next_seq("player", root);
        let tail = type_tail(&cmd.type_url);

        macro_rules! dispatch {
            ($ty:ty, $h:ident) => {{
                let decoded: $ty = prost::Message::decode(cmd.value.as_slice())
                    .map_err(|e| SendError(format!("decode error: {e}")))?;
                $h(decoded, &state, seq).map_err(|e| SendError(e.reason))
            }};
        }

        match tail {
            "RegisterPlayer" => dispatch!(examples_proto::RegisterPlayer, handle_register_player),
            "DepositFunds" => dispatch!(examples_proto::DepositFunds, handle_deposit_funds),
            "WithdrawFunds" => dispatch!(examples_proto::WithdrawFunds, handle_withdraw_funds),
            "ReserveFunds" => dispatch!(examples_proto::ReserveFunds, handle_reserve_funds),
            "ReleaseFunds" => dispatch!(examples_proto::ReleaseFunds, handle_release_funds),
            "TransferFunds" => dispatch!(examples_proto::TransferFunds, handle_transfer_funds),
            other => Err(SendError(format!("unknown player command: {other}"))),
        }
    }

    fn dispatch_table(&self, root: &[u8], cmd: &Any) -> Result<EventBook, SendError> {
        use agg_table::handlers::*;
        let state = rebuild_table(&self.load_events("table", root));
        let seq = self.next_seq("table", root);
        let tail = type_tail(&cmd.type_url);

        macro_rules! dispatch {
            ($ty:ty, $h:ident) => {{
                let decoded: $ty = prost::Message::decode(cmd.value.as_slice())
                    .map_err(|e| SendError(format!("decode error: {e}")))?;
                $h(decoded, &state, seq).map_err(|e| SendError(e.reason))
            }};
        }

        match tail {
            "CreateTable" => dispatch!(examples_proto::CreateTable, handle_create_table),
            "JoinTable" => dispatch!(examples_proto::JoinTable, handle_join_table),
            "LeaveTable" => dispatch!(examples_proto::LeaveTable, handle_leave_table),
            "StartHand" => dispatch!(examples_proto::StartHand, handle_start_hand),
            "AddRebuyChips" => dispatch!(examples_proto::AddRebuyChips, handle_add_rebuy_chips),
            other => Err(SendError(format!("unknown table command: {other}"))),
        }
    }

    /// Inline saga/PM simulation for InProcess mode: after a table HandStarted
    /// event, feed the hand aggregate a DealCards command (what the real
    /// table-hand saga would do) so a CardsDealt event enters the stream.
    fn run_sagas_for(&self, domain: &str, book: &EventBook) {
        if domain != "table" {
            return;
        }
        for page in &book.pages {
            let Some(event_page::Payload::Event(any)) = &page.payload else { continue };
            if !any.type_url.ends_with("HandStarted") {
                continue;
            }
            let Some(hs) = try_unpack::<HandStarted>(any) else { continue };
            let hand_root = hs.hand_root.clone();
            let players: Vec<examples_proto::PlayerInHand> = hs
                .active_players
                .iter()
                .map(|s| examples_proto::PlayerInHand {
                    player_root: s.player_root.clone(),
                    position: s.position,
                    stack: s.stack,
                })
                .collect();
            let deal = examples_proto::DealCards {
                table_root: hand_root.clone(),
                hand_number: hs.hand_number,
                game_variant: hs.game_variant,
                players,
                dealer_position: hs.dealer_position,
                small_blind: hs.small_blind,
                big_blind: hs.big_blind,
                deck_seed: vec![],
            };
            let packed = Any {
                type_url: "type.googleapis.com/examples.DealCards".to_string(),
                value: prost::Message::encode_to_vec(&deal),
            };
            if let Ok(ev) = self.dispatch_hand(&hand_root, &packed) {
                self.append_events("hand", &hand_root, &ev);
            }
        }
    }

    fn dispatch_hand(&self, root: &[u8], cmd: &Any) -> Result<EventBook, SendError> {
        use agg_hand::handlers::*;
        let state = rebuild_hand(&self.load_events("hand", root));
        let seq = self.next_seq("hand", root);
        let tail = type_tail(&cmd.type_url);

        macro_rules! dispatch {
            ($ty:ty, $h:ident) => {{
                let decoded: $ty = prost::Message::decode(cmd.value.as_slice())
                    .map_err(|e| SendError(format!("decode error: {e}")))?;
                $h(decoded, &state, seq).map_err(|e| SendError(e.reason))
            }};
        }

        match tail {
            "DealCards" => dispatch!(examples_proto::DealCards, handle_deal_cards),
            "PostBlind" => dispatch!(examples_proto::PostBlind, handle_post_blind),
            "PlayerAction" => dispatch!(examples_proto::PlayerAction, handle_player_action),
            "RequestDraw" => dispatch!(examples_proto::RequestDraw, handle_request_draw),
            "DealCommunityCards" => dispatch!(
                examples_proto::DealCommunityCards,
                handle_deal_community_cards
            ),
            "RevealCards" => dispatch!(examples_proto::RevealCards, handle_reveal_cards),
            "AwardPot" => dispatch!(examples_proto::AwardPot, handle_award_pot),
            other => Err(SendError(format!("unknown hand command: {other}"))),
        }
    }
}

impl CommandClient for InProcessClient {
    fn send_command(
        &self,
        domain: &str,
        root: &[u8],
        cmd: Any,
        _sequence: u32,
    ) -> Result<EventBook, SendError> {
        let result = match domain {
            "player" => self.dispatch_player(root, &cmd),
            "table" => self.dispatch_table(root, &cmd),
            "hand" => self.dispatch_hand(root, &cmd),
            other => Err(SendError(format!("unknown domain: {other}"))),
        };
        if let Ok(ref book) = result {
            self.append_events(domain, root, book);
            self.run_sagas_for(domain, book);
        }
        result
    }

    // InProcess intentionally returns None here: its in-memory event store
    // does not include saga/PM-driven events (release-on-leave, hand_count
    // increments, etc.) — the step layer's world bookkeeping is the source
    // of truth for those. The gRPC client's get_state queries the cluster
    // where real sagas/PMs have run.
    fn get_state(&self, _domain: &str, _root: &[u8]) -> Option<StateSnapshot> {
        None
    }

    fn received_events(&self) -> EventBuffer {
        Arc::clone(&self.received)
    }

    fn set_correlation(&self, _correlation_id: &str) {
        self.received.lock().unwrap().clear();
    }

    fn close(&mut self) {}
}

// ===========================================================================
// Aggregate rebuilders (shared between InProcess and gRPC paths)
// ===========================================================================

pub fn rebuild_player(events: &[Any]) -> PlayerState {
    use agg_player::state::*;
    let mut s = PlayerState::default();
    for ev in events {
        if let Some(e) = try_unpack::<PlayerRegistered>(ev) {
            apply_registered(&mut s, e);
        } else if let Some(e) = try_unpack::<FundsDeposited>(ev) {
            apply_deposited(&mut s, e);
        } else if let Some(e) = try_unpack::<FundsWithdrawn>(ev) {
            apply_withdrawn(&mut s, e);
        } else if let Some(e) = try_unpack::<FundsReserved>(ev) {
            apply_reserved(&mut s, e);
        } else if let Some(e) = try_unpack::<FundsReleased>(ev) {
            apply_released(&mut s, e);
        } else if let Some(e) = try_unpack::<FundsTransferred>(ev) {
            apply_transferred(&mut s, e);
        } else if let Some(e) = try_unpack::<BuyInRequested>(ev) {
            apply_buy_in_requested(&mut s, e);
        } else if let Some(e) = try_unpack::<BuyInConfirmed>(ev) {
            apply_buy_in_confirmed(&mut s, e);
        } else if let Some(e) = try_unpack::<BuyInReservationReleased>(ev) {
            apply_buy_in_released(&mut s, e);
        } else if let Some(e) = try_unpack::<RegistrationRequested>(ev) {
            apply_registration_requested(&mut s, e);
        } else if let Some(e) = try_unpack::<RegistrationFeeConfirmed>(ev) {
            apply_registration_confirmed(&mut s, e);
        } else if let Some(e) = try_unpack::<RegistrationFeeReleased>(ev) {
            apply_registration_released(&mut s, e);
        } else if let Some(e) = try_unpack::<RebuyRequested>(ev) {
            apply_rebuy_requested(&mut s, e);
        } else if let Some(e) = try_unpack::<RebuyFeeConfirmed>(ev) {
            apply_rebuy_confirmed(&mut s, e);
        } else if let Some(e) = try_unpack::<RebuyFeeReleased>(ev) {
            apply_rebuy_released(&mut s, e);
        }
    }
    s
}

pub fn rebuild_table(events: &[Any]) -> TableState {
    use agg_table::state::*;
    let mut s = TableState::default();
    for ev in events {
        if let Some(e) = try_unpack::<TableCreated>(ev) {
            apply_table_created(&mut s, e);
        } else if let Some(e) = try_unpack::<PlayerJoined>(ev) {
            apply_player_joined(&mut s, e);
        } else if let Some(e) = try_unpack::<PlayerLeft>(ev) {
            apply_player_left(&mut s, e);
        } else if let Some(e) = try_unpack::<PlayerSatOut>(ev) {
            apply_player_sat_out(&mut s, e);
        } else if let Some(e) = try_unpack::<PlayerSatIn>(ev) {
            apply_player_sat_in(&mut s, e);
        } else if let Some(e) = try_unpack::<HandStarted>(ev) {
            apply_hand_started(&mut s, e);
        } else if let Some(e) = try_unpack::<HandEnded>(ev) {
            apply_hand_ended(&mut s, e);
        } else if let Some(e) = try_unpack::<ChipsAdded>(ev) {
            apply_chips_added(&mut s, e);
        } else if let Some(e) = try_unpack::<PlayerSeated>(ev) {
            apply_player_seated(&mut s, e);
        } else if let Some(e) = try_unpack::<SeatingRejected>(ev) {
            apply_seating_rejected(&mut s, e);
        } else if let Some(e) = try_unpack::<RebuyChipsAdded>(ev) {
            apply_rebuy_chips_added(&mut s, e);
        }
    }
    s
}

pub fn rebuild_hand(events: &[Any]) -> HandState {
    use agg_hand::state::*;
    let mut s = HandState::default();
    for ev in events {
        if let Some(e) = try_unpack::<CardsDealt>(ev) {
            apply_cards_dealt(&mut s, e);
        } else if let Some(e) = try_unpack::<BlindPosted>(ev) {
            apply_blind_posted(&mut s, e);
        } else if let Some(e) = try_unpack::<ActionTaken>(ev) {
            apply_action_taken(&mut s, e);
        } else if let Some(e) = try_unpack::<BettingRoundComplete>(ev) {
            apply_betting_round_complete(&mut s, e);
        } else if let Some(e) = try_unpack::<CommunityCardsDealt>(ev) {
            apply_community_cards_dealt(&mut s, e);
        } else if let Some(e) = try_unpack::<DrawCompleted>(ev) {
            apply_draw_completed(&mut s, e);
        } else if let Some(e) = try_unpack::<ShowdownStarted>(ev) {
            apply_showdown_started(&mut s, e);
        } else if let Some(e) = try_unpack::<PotAwarded>(ev) {
            apply_pot_awarded(&mut s, e);
        } else if let Some(e) = try_unpack::<HandComplete>(ev) {
            apply_hand_complete(&mut s, e);
        }
    }
    s
}

pub fn type_tail(type_url: &str) -> &str {
    match type_url.rsplit_once('/') {
        Some((_, tail)) => tail.rsplit('.').next().unwrap_or(tail),
        None => type_url.rsplit('.').next().unwrap_or(type_url),
    }
}

/// Poll the received-events buffer until a predicate holds or the timeout elapses.
/// Returns true if satisfied, false on timeout. Works in both modes.
pub fn wait_for_events<F>(buffer: &EventBuffer, timeout: Duration, predicate: F) -> bool
where
    F: Fn(&[EventPage]) -> bool,
{
    let deadline = Instant::now() + timeout;
    loop {
        {
            let events = buffer.lock().unwrap();
            if predicate(&events) {
                return true;
            }
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

pub fn event_type_matches(page: &EventPage, type_suffix: &str) -> bool {
    match &page.payload {
        Some(event_page::Payload::Event(any)) => any.type_url.ends_with(type_suffix),
        _ => false,
    }
}

// ===========================================================================
// GrpcClient — compiled only with the `acceptance-test` feature.
// ===========================================================================

#[cfg(feature = "acceptance-test")]
pub mod grpc {
    use super::*;
    use angzarr_client::proto::{
        command_handler_coordinator_service_client::CommandHandlerCoordinatorServiceClient,
        command_page, event_query_service_client::EventQueryServiceClient,
        event_stream_service_client::EventStreamServiceClient, page_header, CommandBook,
        CommandPage, CommandRequest, Cover, EventStreamFilter, PageHeader, Query, SyncMode,
        Uuid as ProtoUuid,
    };
    use std::env;
    use std::sync::Arc;
    use tokio::sync::Mutex as AsyncMutex;
    use tokio_stream::StreamExt;
    use tonic::transport::Channel;

    pub fn player_url() -> String {
        env::var("PLAYER_URL").unwrap_or_else(|_| "http://localhost:1310".to_string())
    }
    pub fn table_url() -> String {
        env::var("TABLE_URL").unwrap_or_else(|_| "http://localhost:1311".to_string())
    }
    pub fn hand_url() -> String {
        env::var("HAND_URL").unwrap_or_else(|_| "http://localhost:1312".to_string())
    }
    pub fn stream_url() -> String {
        env::var("STREAM_URL").unwrap_or_else(|_| "http://localhost:1340".to_string())
    }

    pub struct GrpcClient {
        rt: tokio::runtime::Runtime,
        player: CommandHandlerCoordinatorServiceClient<Channel>,
        table: CommandHandlerCoordinatorServiceClient<Channel>,
        hand: CommandHandlerCoordinatorServiceClient<Channel>,
        query_player: EventQueryServiceClient<Channel>,
        query_table: EventQueryServiceClient<Channel>,
        query_hand: EventQueryServiceClient<Channel>,
        stream_channel: Channel,
        correlation_id: AsyncMutex<String>,
        received: EventBuffer,
        stream_task: AsyncMutex<Option<tokio::task::JoinHandle<()>>>,
    }

    impl GrpcClient {
        pub fn new() -> Result<Self, SendError> {
            let rt = tokio::runtime::Runtime::new()
                .map_err(|e| SendError(format!("runtime: {e}")))?;
            let (pc, tc, hc, sc) = rt.block_on(async {
                let p = connect_channel(&player_url()).await?;
                let t = connect_channel(&table_url()).await?;
                let h = connect_channel(&hand_url()).await?;
                let s = connect_channel(&stream_url()).await?;
                Ok::<_, SendError>((p, t, h, s))
            })?;
            Ok(Self {
                rt,
                player: CommandHandlerCoordinatorServiceClient::new(pc.clone()),
                table: CommandHandlerCoordinatorServiceClient::new(tc.clone()),
                hand: CommandHandlerCoordinatorServiceClient::new(hc.clone()),
                query_player: EventQueryServiceClient::new(pc),
                query_table: EventQueryServiceClient::new(tc),
                query_hand: EventQueryServiceClient::new(hc),
                stream_channel: sc,
                correlation_id: AsyncMutex::new(String::new()),
                received: Arc::new(Mutex::new(Vec::new())),
                stream_task: AsyncMutex::new(None),
            })
        }

        fn current_correlation(&self) -> String {
            self.rt
                .block_on(async { self.correlation_id.lock().await.clone() })
        }

        fn query_client(
            &self,
            domain: &str,
        ) -> Result<EventQueryServiceClient<Channel>, SendError> {
            match domain {
                "player" => Ok(self.query_player.clone()),
                "table" => Ok(self.query_table.clone()),
                "hand" => Ok(self.query_hand.clone()),
                other => Err(SendError(format!("unknown domain: {other}"))),
            }
        }

        fn load_events_for(&self, domain: &str, root: &[u8]) -> Vec<Any> {
            let mut client = match self.query_client(domain) {
                Ok(c) => c,
                Err(_) => return Vec::new(),
            };
            let query = Query {
                cover: Some(Cover {
                    domain: domain.to_string(),
                    root: Some(ProtoUuid { value: root.to_vec() }),
                    correlation_id: String::new(),
                    edition: None,
                }),
                selection: None,
            };
            let book = match self
                .rt
                .block_on(async move { client.get_event_book(query).await })
            {
                Ok(resp) => resp.into_inner(),
                Err(_) => return Vec::new(),
            };
            book.pages
                .into_iter()
                .filter_map(|p| match p.payload {
                    Some(event_page::Payload::Event(any)) => Some(any),
                    _ => None,
                })
                .collect()
        }
    }

    async fn connect_channel(url: &str) -> Result<Channel, SendError> {
        Channel::from_shared(url.to_string())
            .map_err(|e| SendError(format!("bad url {url}: {e}")))?
            .connect()
            .await
            .map_err(|e| SendError(format!("connect {url}: {e}")))
    }

    impl CommandClient for GrpcClient {
        fn send_command(
            &self,
            domain: &str,
            root: &[u8],
            cmd: Any,
            sequence: u32,
        ) -> Result<EventBook, SendError> {
            let correlation_id = self.current_correlation();
            let request = CommandRequest {
                command: Some(CommandBook {
                    cover: Some(Cover {
                        domain: domain.to_string(),
                        root: Some(ProtoUuid { value: root.to_vec() }),
                        correlation_id,
                        ..Default::default()
                    }),
                    pages: vec![CommandPage {
                        header: Some(PageHeader {
                            sequence_type: Some(page_header::SequenceType::Sequence(sequence)),
                        }),
                        payload: Some(command_page::Payload::Command(cmd)),
                        ..Default::default()
                    }],
                    ..Default::default()
                }),
                sync_mode: SyncMode::Simple as i32,
                cascade_error_mode: 0,
                cascade_id: None,
            };
            let mut client = match domain {
                "player" => self.player.clone(),
                "table" => self.table.clone(),
                "hand" => self.hand.clone(),
                other => return Err(SendError(format!("unknown domain: {other}"))),
            };
            let resp = self
                .rt
                .block_on(async move { client.handle_command(request).await })
                .map_err(|s| SendError(format!("{s}")))?;
            let book = resp
                .into_inner()
                .events
                .ok_or_else(|| SendError("response had no events".to_string()))?;
            // Mirror: also record response-cover events into received
            // (the stream will independently deliver them; duplicates are
            // acceptable — predicates are "has event X" style).
            {
                let mut rcv = self.received.lock().unwrap();
                for page in &book.pages {
                    rcv.push(page.clone());
                }
            }
            Ok(book)
        }

        fn get_state(&self, domain: &str, root: &[u8]) -> Option<StateSnapshot> {
            let events = self.load_events_for(domain, root);
            if events.is_empty() {
                return None;
            }
            match domain {
                "player" => Some(StateSnapshot::Player(Box::new(rebuild_player(&events)))),
                "table" => Some(StateSnapshot::Table(Box::new(rebuild_table(&events)))),
                "hand" => Some(StateSnapshot::Hand(Box::new(rebuild_hand(&events)))),
                _ => None,
            }
        }

        fn received_events(&self) -> EventBuffer {
            Arc::clone(&self.received)
        }

        fn set_correlation(&self, correlation_id: &str) {
            self.received.lock().unwrap().clear();
            let cid = correlation_id.to_string();
            let channel = self.stream_channel.clone();
            let received = Arc::clone(&self.received);
            // Cancel any prior subscription.
            self.rt.block_on(async {
                let mut guard = self.stream_task.lock().await;
                if let Some(h) = guard.take() {
                    h.abort();
                }
                *self.correlation_id.lock().await = cid.clone();
                let handle = tokio::spawn(async move {
                    let mut client = EventStreamServiceClient::new(channel);
                    let filter = EventStreamFilter {
                        correlation_id: cid,
                    };
                    let stream = match client.subscribe(filter).await {
                        Ok(s) => s.into_inner(),
                        Err(_) => return,
                    };
                    tokio::pin!(stream);
                    while let Some(item) = stream.next().await {
                        match item {
                            Ok(book) => {
                                let mut rcv = received.lock().unwrap();
                                for page in book.pages {
                                    rcv.push(page);
                                }
                            }
                            Err(_) => break,
                        }
                    }
                });
                *guard = Some(handle);
            });
        }

        fn close(&mut self) {
            self.rt.block_on(async {
                let mut guard = self.stream_task.lock().await;
                if let Some(h) = guard.take() {
                    h.abort();
                }
            });
        }
    }
}

/// Create an appropriate client based on environment.
pub fn create_client() -> Box<dyn CommandClient> {
    #[cfg(feature = "acceptance-test")]
    {
        if std::env::var("PLAYER_URL").is_ok() {
            match grpc::GrpcClient::new() {
                Ok(c) => return Box::new(c),
                Err(e) => panic!("failed to create GrpcClient: {e}"),
            }
        }
    }
    Box::new(InProcessClient::new())
}
