//! CommandClient abstraction mirroring examples-python/main/tests/command_client.py.
//!
//! - InProcessClient: calls handler functions directly, maintains an in-memory
//!   event store keyed by (domain, root_hex) so commands replay state from
//!   prior commands in the same scenario.
//! - GrpcClient: feature-gated (`acceptance-test`) — sends via tonic to the
//!   coordinator URLs (PLAYER_URL, TABLE_URL, HAND_URL).
//!
//! Factory: if PLAYER_URL is set in the environment, use GrpcClient
//! (requires the `acceptance-test` feature at build time); otherwise,
//! InProcessClient. Default-mode tests use InProcess.

use std::collections::HashMap;
use std::sync::Mutex;

use angzarr_client::proto::{event_page, EventBook};
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

/// Abstract command client.
pub trait CommandClient: Send {
    fn send_command(
        &self,
        domain: &str,
        root: &[u8],
        cmd: Any,
        sequence: u32,
    ) -> Result<EventBook, SendError>;

    fn close(&mut self);
}

/// In-process client — runs handler functions directly against an in-memory
/// event store. State is rebuilt per command from the prior events for that
/// (domain, root).
pub struct InProcessClient {
    store: Mutex<HashMap<(String, String), Vec<Any>>>,
}

impl InProcessClient {
    pub fn new() -> Self {
        Self {
            store: Mutex::new(HashMap::new()),
        }
    }

    fn append_events(&self, domain: &str, root: &[u8], book: &EventBook) {
        let key = (domain.to_string(), hex::encode(root));
        let mut store = self.store.lock().unwrap();
        let entry = store.entry(key).or_default();
        for page in &book.pages {
            if let Some(event_page::Payload::Event(any)) = &page.payload {
                entry.push(any.clone());
            }
        }
    }

    fn load_events(&self, domain: &str, root: &[u8]) -> Vec<Any> {
        let key = (domain.to_string(), hex::encode(root));
        self.store.lock().unwrap().get(&key).cloned().unwrap_or_default()
    }

    fn rebuild_player(&self, root: &[u8]) -> PlayerState {
        use agg_player::state::*;
        let mut s = PlayerState::default();
        for ev in self.load_events("player", root) {
            if let Some(e) = try_unpack::<PlayerRegistered>(&ev) {
                apply_registered(&mut s, e);
            } else if let Some(e) = try_unpack::<FundsDeposited>(&ev) {
                apply_deposited(&mut s, e);
            } else if let Some(e) = try_unpack::<FundsWithdrawn>(&ev) {
                apply_withdrawn(&mut s, e);
            } else if let Some(e) = try_unpack::<FundsReserved>(&ev) {
                apply_reserved(&mut s, e);
            } else if let Some(e) = try_unpack::<FundsReleased>(&ev) {
                apply_released(&mut s, e);
            } else if let Some(e) = try_unpack::<FundsTransferred>(&ev) {
                apply_transferred(&mut s, e);
            } else if let Some(e) = try_unpack::<BuyInRequested>(&ev) {
                apply_buy_in_requested(&mut s, e);
            } else if let Some(e) = try_unpack::<BuyInConfirmed>(&ev) {
                apply_buy_in_confirmed(&mut s, e);
            } else if let Some(e) = try_unpack::<BuyInReservationReleased>(&ev) {
                apply_buy_in_released(&mut s, e);
            } else if let Some(e) = try_unpack::<RegistrationRequested>(&ev) {
                apply_registration_requested(&mut s, e);
            } else if let Some(e) = try_unpack::<RegistrationFeeConfirmed>(&ev) {
                apply_registration_confirmed(&mut s, e);
            } else if let Some(e) = try_unpack::<RegistrationFeeReleased>(&ev) {
                apply_registration_released(&mut s, e);
            } else if let Some(e) = try_unpack::<RebuyRequested>(&ev) {
                apply_rebuy_requested(&mut s, e);
            } else if let Some(e) = try_unpack::<RebuyFeeConfirmed>(&ev) {
                apply_rebuy_confirmed(&mut s, e);
            } else if let Some(e) = try_unpack::<RebuyFeeReleased>(&ev) {
                apply_rebuy_released(&mut s, e);
            }
        }
        s
    }

    fn rebuild_table(&self, root: &[u8]) -> TableState {
        use agg_table::state::*;
        let mut s = TableState::default();
        for ev in self.load_events("table", root) {
            if let Some(e) = try_unpack::<TableCreated>(&ev) {
                apply_table_created(&mut s, e);
            } else if let Some(e) = try_unpack::<PlayerJoined>(&ev) {
                apply_player_joined(&mut s, e);
            } else if let Some(e) = try_unpack::<PlayerLeft>(&ev) {
                apply_player_left(&mut s, e);
            } else if let Some(e) = try_unpack::<PlayerSatOut>(&ev) {
                apply_player_sat_out(&mut s, e);
            } else if let Some(e) = try_unpack::<PlayerSatIn>(&ev) {
                apply_player_sat_in(&mut s, e);
            } else if let Some(e) = try_unpack::<HandStarted>(&ev) {
                apply_hand_started(&mut s, e);
            } else if let Some(e) = try_unpack::<HandEnded>(&ev) {
                apply_hand_ended(&mut s, e);
            } else if let Some(e) = try_unpack::<ChipsAdded>(&ev) {
                apply_chips_added(&mut s, e);
            } else if let Some(e) = try_unpack::<PlayerSeated>(&ev) {
                apply_player_seated(&mut s, e);
            } else if let Some(e) = try_unpack::<SeatingRejected>(&ev) {
                apply_seating_rejected(&mut s, e);
            } else if let Some(e) = try_unpack::<RebuyChipsAdded>(&ev) {
                apply_rebuy_chips_added(&mut s, e);
            }
        }
        s
    }

    fn rebuild_hand(&self, root: &[u8]) -> HandState {
        use agg_hand::state::*;
        let mut s = HandState::default();
        for ev in self.load_events("hand", root) {
            if let Some(e) = try_unpack::<CardsDealt>(&ev) {
                apply_cards_dealt(&mut s, e);
            } else if let Some(e) = try_unpack::<BlindPosted>(&ev) {
                apply_blind_posted(&mut s, e);
            } else if let Some(e) = try_unpack::<ActionTaken>(&ev) {
                apply_action_taken(&mut s, e);
            } else if let Some(e) = try_unpack::<BettingRoundComplete>(&ev) {
                apply_betting_round_complete(&mut s, e);
            } else if let Some(e) = try_unpack::<CommunityCardsDealt>(&ev) {
                apply_community_cards_dealt(&mut s, e);
            } else if let Some(e) = try_unpack::<DrawCompleted>(&ev) {
                apply_draw_completed(&mut s, e);
            } else if let Some(e) = try_unpack::<ShowdownStarted>(&ev) {
                apply_showdown_started(&mut s, e);
            } else if let Some(e) = try_unpack::<PotAwarded>(&ev) {
                apply_pot_awarded(&mut s, e);
            } else if let Some(e) = try_unpack::<HandComplete>(&ev) {
                apply_hand_complete(&mut s, e);
            }
        }
        s
    }

    fn next_seq(&self, domain: &str, root: &[u8]) -> u32 {
        self.load_events(domain, root).len() as u32
    }

    fn dispatch_player(&self, root: &[u8], cmd: &Any) -> Result<EventBook, SendError> {
        use agg_player::handlers::*;
        let state = self.rebuild_player(root);
        let seq = self.next_seq("player", root);
        let t = cmd.type_url.as_str();
        let tail = type_tail(t);

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
            other => Err(SendError(format!(
                "unknown player command: {other}"
            ))),
        }
    }

    fn dispatch_table(&self, root: &[u8], cmd: &Any) -> Result<EventBook, SendError> {
        use agg_table::handlers::*;
        let state = self.rebuild_table(root);
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

    fn dispatch_hand(&self, root: &[u8], cmd: &Any) -> Result<EventBook, SendError> {
        use agg_hand::handlers::*;
        let state = self.rebuild_hand(root);
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

fn type_tail(type_url: &str) -> &str {
    match type_url.rsplit_once('/') {
        Some((_, tail)) => tail.rsplit('.').next().unwrap_or(tail),
        None => type_url.rsplit('.').next().unwrap_or(type_url),
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
        }
        result
    }

    fn close(&mut self) {}
}

// ===========================================================================
// GrpcClient — compiled only with the `acceptance-test` feature.
// ===========================================================================

#[cfg(feature = "acceptance-test")]
pub mod grpc {
    use super::*;
    use angzarr_client::proto::{
        command_handler_coordinator_service_client::CommandHandlerCoordinatorServiceClient,
        command_page, page_header, CommandBook, CommandPage, CommandRequest, Cover, PageHeader,
        SyncMode, Uuid as ProtoUuid,
    };
    use std::env;
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

    pub struct GrpcClient {
        rt: tokio::runtime::Runtime,
        player: CommandHandlerCoordinatorServiceClient<Channel>,
        table: CommandHandlerCoordinatorServiceClient<Channel>,
        hand: CommandHandlerCoordinatorServiceClient<Channel>,
    }

    impl GrpcClient {
        pub fn new() -> Result<Self, SendError> {
            let rt = tokio::runtime::Runtime::new()
                .map_err(|e| SendError(format!("runtime: {e}")))?;
            let (player, table, hand) = rt.block_on(async {
                let p = connect(&player_url()).await?;
                let t = connect(&table_url()).await?;
                let h = connect(&hand_url()).await?;
                Ok::<_, SendError>((p, t, h))
            })?;
            Ok(Self { rt, player, table, hand })
        }
    }

    async fn connect(
        url: &str,
    ) -> Result<CommandHandlerCoordinatorServiceClient<Channel>, SendError> {
        let channel = Channel::from_shared(url.to_string())
            .map_err(|e| SendError(format!("bad url {url}: {e}")))?
            .connect()
            .await
            .map_err(|e| SendError(format!("connect {url}: {e}")))?;
        Ok(CommandHandlerCoordinatorServiceClient::new(channel))
    }

    impl CommandClient for GrpcClient {
        fn send_command(
            &self,
            domain: &str,
            root: &[u8],
            cmd: Any,
            sequence: u32,
        ) -> Result<EventBook, SendError> {
            let request = CommandRequest {
                command: Some(CommandBook {
                    cover: Some(Cover {
                        domain: domain.to_string(),
                        root: Some(ProtoUuid { value: root.to_vec() }),
                        correlation_id: uuid::Uuid::new_v4().to_string(),
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
            resp.into_inner()
                .events
                .ok_or_else(|| SendError("response had no events".to_string()))
        }

        fn close(&mut self) {}
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
