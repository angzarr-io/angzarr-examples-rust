//! Saga: Hand -> Player (library).
//!
//! Exports the saga handler so tests can construct it directly.

use angzarr_client::proto::{command_page, CommandBook, CommandPage, Cover, SagaResponse, Uuid};
use angzarr_client::{saga, CommandResult};
use examples_proto::{Currency, DepositFunds, PotAwarded};
use prost::Message;
use prost_types::Any;

/// Translate `hand.PotAwarded` into one `DepositFunds` command per winner.
pub struct HandPlayerSaga;

#[saga(name = "saga-hand-player", source = "hand", target = "player")]
impl HandPlayerSaga {
    #[handles(PotAwarded)]
    pub fn on_pot_awarded(&self, event: PotAwarded) -> CommandResult<SagaResponse> {
        let commands: Vec<CommandBook> = event
            .winners
            .into_iter()
            .map(|winner| {
                let deposit = DepositFunds {
                    amount: Some(Currency {
                        amount: winner.amount,
                        currency_code: "CHIPS".to_string(),
                    }),
                };
                let cmd_any = Any {
                    type_url: "type.googleapis.com/examples.DepositFunds".to_string(),
                    value: deposit.encode_to_vec(),
                };
                CommandBook {
                    cover: Some(Cover {
                        domain: "player".to_string(),
                        root: Some(Uuid {
                            value: winner.player_root,
                        }),
                        ..Default::default()
                    }),
                    pages: vec![CommandPage {
                        payload: Some(command_page::Payload::Command(cmd_any)),
                        ..Default::default()
                    }],
                }
            })
            .collect();

        Ok(SagaResponse {
            commands,
            events: vec![],
        })
    }
}

