//! RegisterPlayer command handler.

use angzarr_client::proto::EventBook;
use angzarr_client::{event_page, pack_event, CommandRejectedError, CommandResult};
use examples_proto::{PlayerRegistered, RegisterPlayer};

use crate::state::PlayerState;

fn register_player_guard(state: &PlayerState) -> CommandResult<()> {
    if state.exists() {
        return Err(CommandRejectedError::new("Player already exists"));
    }
    Ok(())
}

fn register_player_validate(cmd: &RegisterPlayer) -> CommandResult<()> {
    if cmd.display_name.is_empty() {
        return Err(CommandRejectedError::new("display_name is required"));
    }
    if cmd.email.is_empty() {
        return Err(CommandRejectedError::new("email is required"));
    }
    Ok(())
}

fn register_player_compute(cmd: &RegisterPlayer) -> PlayerRegistered {
    PlayerRegistered {
        display_name: cmd.display_name.clone(),
        email: cmd.email.clone(),
        player_type: cmd.player_type,
        ai_model_id: cmd.ai_model_id.clone(),
        registered_at: Some(angzarr_client::now()),
    }
}

pub fn handle_register_player(
    cmd: RegisterPlayer,
    state: &PlayerState,
    seq: u32,
) -> CommandResult<EventBook> {
    register_player_guard(state)?;
    register_player_validate(&cmd)?;

    let event = register_player_compute(&cmd);
    let event_any = pack_event(&event, "examples.PlayerRegistered");

    Ok(EventBook {
        pages: vec![event_page(seq, event_any)],
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::apply_registered;
    use angzarr_client::proto::event_page::Payload;
    use examples_proto::PlayerType;
    use prost::Message;

    fn sample_cmd() -> RegisterPlayer {
        RegisterPlayer {
            display_name: "Alice".into(),
            email: "alice@example.com".into(),
            player_type: PlayerType::Human as i32,
            ai_model_id: String::new(),
        }
    }

    #[test]
    fn handle_register_player_success_emits_registered_event() {
        let state = PlayerState::default();
        let book = handle_register_player(sample_cmd(), &state, 7).expect("handler ok");
        assert_eq!(book.pages.len(), 1);
        let any = match book.pages[0].payload.as_ref() {
            Some(Payload::Event(any)) => any,
            _ => panic!("expected Event payload"),
        };
        assert!(any.type_url.ends_with("examples.PlayerRegistered"));
        let decoded = PlayerRegistered::decode(any.value.as_slice()).expect("decode");
        assert_eq!(decoded.display_name, "Alice");
        assert_eq!(decoded.email, "alice@example.com");
    }

    #[test]
    fn handle_register_player_rejects_duplicate_player() {
        let mut state = PlayerState::default();
        apply_registered(
            &mut state,
            PlayerRegistered {
                display_name: "Alice".into(),
                email: "alice@example.com".into(),
                player_type: PlayerType::Human as i32,
                ai_model_id: String::new(),
                registered_at: None,
            },
        );
        let err = handle_register_player(sample_cmd(), &state, 1).unwrap_err();
        assert!(err.reason.contains("already exists"));
    }

    #[test]
    fn handle_register_player_rejects_empty_name() {
        let state = PlayerState::default();
        let mut cmd = sample_cmd();
        cmd.display_name = String::new();
        let err = handle_register_player(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("display_name"));
    }

    #[test]
    fn handle_register_player_rejects_empty_email() {
        let state = PlayerState::default();
        let mut cmd = sample_cmd();
        cmd.email = String::new();
        let err = handle_register_player(cmd, &state, 1).unwrap_err();
        assert!(err.reason.contains("email"));
    }
}
