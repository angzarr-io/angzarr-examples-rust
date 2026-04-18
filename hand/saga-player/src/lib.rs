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

#[cfg(test)]
mod tests {
    use super::*;
    use angzarr_client::proto::command_page::Payload;
    use examples_proto::{HandRanking, PotWinner};

    fn winner(player_root: Vec<u8>, amount: i64) -> PotWinner {
        PotWinner {
            player_root,
            amount,
            pot_type: "main".to_string(),
            winning_hand: Some(HandRanking::default()),
        }
    }

    fn extract_command_any(book: &CommandBook) -> &Any {
        let page = book.pages.first().expect("command book has one page");
        match page.payload.as_ref().expect("payload set") {
            Payload::Command(any) => any,
            _ => panic!("expected inline Command payload"),
        }
    }

    #[test]
    fn emits_one_deposit_funds_per_winner() {
        let saga = HandPlayerSaga;
        let event = PotAwarded {
            winners: vec![
                winner(vec![0xAA; 16], 100),
                winner(vec![0xBB; 16], 250),
                winner(vec![0xCC; 16], 75),
            ],
            awarded_at: None,
        };

        let response = saga
            .on_pot_awarded(event)
            .expect("handler succeeds");

        assert_eq!(response.commands.len(), 3);
        assert!(response.events.is_empty());

        let expected = [
            (vec![0xAA_u8; 16], 100_i64),
            (vec![0xBB_u8; 16], 250),
            (vec![0xCC_u8; 16], 75),
        ];
        for (book, (expected_root, expected_amount)) in
            response.commands.iter().zip(expected.iter())
        {
            let cover = book.cover.as_ref().expect("cover set");
            assert_eq!(cover.domain, "player");
            assert_eq!(
                &cover.root.as_ref().expect("root set").value,
                expected_root
            );

            let cmd_any = extract_command_any(book);
            assert!(
                cmd_any.type_url.ends_with("examples.DepositFunds"),
                "type_url = {}",
                cmd_any.type_url
            );

            let deposit = DepositFunds::decode(cmd_any.value.as_slice())
                .expect("decode DepositFunds");
            let currency = deposit.amount.expect("amount set");
            assert_eq!(currency.amount, *expected_amount);
            assert_eq!(currency.currency_code, "CHIPS");
        }
    }

    #[test]
    fn empty_winners_yields_no_commands() {
        let saga = HandPlayerSaga;
        let event = PotAwarded {
            winners: vec![],
            awarded_at: None,
        };

        let response = saga
            .on_pot_awarded(event)
            .expect("handler succeeds");

        assert!(response.commands.is_empty());
        assert!(response.events.is_empty());
    }

    #[test]
    fn single_winner_targets_that_player_root() {
        let saga = HandPlayerSaga;
        let root = vec![0x01, 0x02, 0x03, 0x04];
        let response = saga
            .on_pot_awarded(PotAwarded {
                winners: vec![winner(root.clone(), 500)],
                awarded_at: None,
            })
            .expect("handler succeeds");

        assert_eq!(response.commands.len(), 1);
        let cover = response.commands[0].cover.as_ref().unwrap();
        assert_eq!(cover.domain, "player");
        assert_eq!(cover.root.as_ref().unwrap().value, root);

        let cmd_any = extract_command_any(&response.commands[0]);
        assert!(cmd_any.type_url.ends_with("examples.DepositFunds"));
        let deposit = DepositFunds::decode(cmd_any.value.as_slice()).unwrap();
        assert_eq!(deposit.amount.unwrap().amount, 500);
    }
}
