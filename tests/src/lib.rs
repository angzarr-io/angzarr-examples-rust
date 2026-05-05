//! Shared testing helpers for poker example tests.
//!
//! This module provides common utilities used across all BDD test files.

use angzarr_client::proto::{
    event_page, page_header, CommandBook, Cover, EventBook, EventPage, PageHeader, Uuid,
};
use examples_proto::{Card, Currency, Rank, Suit};
use prost::Message;
use prost_types::Any;

/// Parse compound cover spec like `domain "X" and correlation_id "Y"`
/// into ordered `(field, value)` pairs. Shared by every domain's
/// cover-related cucumber step (Given setter + Then assertion).
pub fn cover_field_pairs(spec: &str) -> Vec<(String, String)> {
    static FIELD_RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = FIELD_RE.get_or_init(|| regex::Regex::new(r#"(\w+)\s+"([^"]*)""#).unwrap());
    let pairs: Vec<_> = re
        .captures_iter(spec)
        .map(|cap| (cap[1].to_string(), cap[2].to_string()))
        .collect();
    assert!(
        !pairs.is_empty(),
        "No `field \"value\"` pairs in cover spec: {}",
        spec
    );
    pairs
}

/// Apply a single (field, value) pair to a Cover. Used by `Given the
/// command cover has ...` step impls.
pub fn write_cover_field(cover: &mut Cover, field: &str, value: &str) {
    match field {
        "domain" => cover.domain = value.to_string(),
        "correlation_id" => cover.correlation_id = value.to_string(),
        "root_hex" => {
            cover.root = Some(Uuid {
                value: hex::decode(value).expect("valid hex for root_hex"),
            });
        }
        _ => panic!(
            "Unknown cover field '{}'; supported: domain, correlation_id, root_hex",
            field
        ),
    }
}

/// Read a single field off a Cover as a string. Used by `Then the
/// rejection cover has ...` step impls.
pub fn read_cover_field(cover: &Cover, field: &str) -> String {
    match field {
        "domain" => cover.domain.clone(),
        "correlation_id" => cover.correlation_id.clone(),
        "root_hex" => cover
            .root
            .as_ref()
            .map(|r| hex::encode(&r.value))
            .unwrap_or_default(),
        _ => panic!(
            "Unknown cover field '{}'; supported: domain, correlation_id, root_hex",
            field
        ),
    }
}

/// Generate a deterministic 16-byte UUID from a seed string.
///
/// RFC 4122 UUID v5 with `NAMESPACE_OID`. Cross-language byte-identical
/// to Python's `uuid.uuid5(uuid.NAMESPACE_OID, seed).bytes`, so the same
/// seed produces the same 16 bytes in both Python and Rust tests.
///
/// # Examples
///
/// ```
/// use poker_tests::uuid_for;
///
/// let player_id = uuid_for("player-alice");
/// assert_eq!(player_id.len(), 16);
/// ```
pub fn uuid_for(seed: &str) -> Vec<u8> {
    uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, seed.as_bytes())
        .as_bytes()
        .to_vec()
}

/// Generate a deterministic hand root from table root and hand number.
///
/// # Examples
///
/// ```
/// use poker_tests::{uuid_for, generate_hand_root};
///
/// let table_root = uuid_for("test-table");
/// let hand_root = generate_hand_root(&table_root, 1);
/// assert_eq!(hand_root.len(), 16); // UUID v5 output
/// ```
pub fn generate_hand_root(table_root: &[u8], hand_number: i64) -> Vec<u8> {
    let mut data = table_root.to_vec();
    data.extend_from_slice(&hand_number.to_be_bytes());
    uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, &data)
        .as_bytes()
        .to_vec()
}

/// Pack a protobuf command message into a prost Any.
///
/// # Examples
///
/// ```
/// use poker_tests::pack_cmd;
/// use examples_proto::RegisterPlayer;
///
/// let cmd = RegisterPlayer { display_name: "alice".to_string(), ..Default::default() };
/// let any = pack_cmd(&cmd, "RegisterPlayer");
/// assert!(any.type_url.ends_with("RegisterPlayer"));
/// ```
pub fn pack_cmd<T: Message>(cmd: &T, type_name: &str) -> Any {
    Any {
        type_url: format!("type.poker/{}", type_name),
        value: cmd.encode_to_vec(),
    }
}

/// Create an empty CommandBook with a given root and domain.
///
/// # Examples
///
/// ```
/// use poker_tests::{uuid_for, command_book};
///
/// let root = uuid_for("player-alice");
/// let book = command_book(&root, "player");
/// assert_eq!(book.cover.unwrap().domain, "player");
/// ```
pub fn command_book(root: &[u8], domain: &str) -> CommandBook {
    CommandBook {
        cover: Some(Cover {
            domain: domain.to_string(),
            root: Some(Uuid {
                value: root.to_vec(),
            }),
            ..Default::default()
        }),
        pages: vec![],
    }
}

/// Create an EventBook from a list of events and a root.
pub fn event_book(root: &[u8], domain: &str, events: &[Any]) -> EventBook {
    EventBook {
        cover: Some(Cover {
            domain: domain.to_string(),
            root: Some(Uuid {
                value: root.to_vec(),
            }),
            ..Default::default()
        }),
        pages: events
            .iter()
            .enumerate()
            .map(|(i, e)| EventPage {
                header: Some(PageHeader {
                    sync_mode: None,
                    sequence_type: Some(page_header::SequenceType::Sequence(i as u32)),
                }),
                payload: Some(event_page::Payload::Event(e.clone())),
                created_at: None,
                no_commit: false,
                cascade_id: None,
            })
            .collect(),
        next_sequence: events.len() as u32,
        snapshot: None,
    }
}

/// Create a Currency value with the default currency code.
///
/// # Examples
///
/// ```
/// use poker_tests::currency;
///
/// let c = currency(1000);
/// assert_eq!(c.amount, 1000);
/// assert_eq!(c.currency_code, "CHIPS");
/// ```
pub fn currency(amount: i64) -> Currency {
    Currency {
        amount,
        currency_code: "CHIPS".to_string(),
    }
}

/// Parse a card string like "As" (Ace of spades) to a Card proto.
///
/// Format: `{rank}{suit}` where:
/// - rank: A, K, Q, J, T (or 10), 9, 8, 7, 6, 5, 4, 3, 2
/// - suit: s (spades), h (hearts), d (diamonds), c (clubs)
///
/// # Examples
///
/// ```
/// use poker_tests::parse_card;
/// use examples_proto::{Card, Rank, Suit};
///
/// let card = parse_card("As");
/// assert_eq!(card.rank, Rank::Ace as i32);
/// assert_eq!(card.suit, Suit::Spades as i32);
///
/// let card = parse_card("Th");
/// assert_eq!(card.rank, Rank::Ten as i32);
/// assert_eq!(card.suit, Suit::Hearts as i32);
/// ```
pub fn parse_card(s: &str) -> Card {
    let s = s.trim();
    if s.len() < 2 {
        return Card::default();
    }
    let (rank_char, suit_char) = s.split_at(s.len() - 1);

    let rank = match rank_char {
        "A" => Rank::Ace,
        "K" => Rank::King,
        "Q" => Rank::Queen,
        "J" => Rank::Jack,
        "T" | "10" => Rank::Ten,
        "9" => Rank::Nine,
        "8" => Rank::Eight,
        "7" => Rank::Seven,
        "6" => Rank::Six,
        "5" => Rank::Five,
        "4" => Rank::Four,
        "3" => Rank::Three,
        "2" => Rank::Two,
        _ => Rank::Two,
    };

    let suit = match suit_char {
        "s" => Suit::Spades,
        "h" => Suit::Hearts,
        "d" => Suit::Diamonds,
        "c" => Suit::Clubs,
        _ => Suit::Spades,
    };

    Card {
        rank: rank as i32,
        suit: suit as i32,
    }
}

/// Parse multiple card strings separated by whitespace.
///
/// # Examples
///
/// ```
/// use poker_tests::parse_cards;
///
/// let cards = parse_cards("As Ks Qs");
/// assert_eq!(cards.len(), 3);
/// ```
pub fn parse_cards(s: &str) -> Vec<Card> {
    s.split_whitespace().map(parse_card).collect()
}

/// Format a Card to a human-readable string.
///
/// # Examples
///
/// ```
/// use poker_tests::{parse_card, format_card};
///
/// let card = parse_card("As");
/// assert_eq!(format_card(&card), "A♠");
/// ```
pub fn format_card(card: &Card) -> String {
    let rank = match Rank::try_from(card.rank).unwrap_or(Rank::Two) {
        Rank::Ace => "A",
        Rank::King => "K",
        Rank::Queen => "Q",
        Rank::Jack => "J",
        Rank::Ten => "T",
        Rank::Nine => "9",
        Rank::Eight => "8",
        Rank::Seven => "7",
        Rank::Six => "6",
        Rank::Five => "5",
        Rank::Four => "4",
        Rank::Three => "3",
        Rank::Two => "2",
        _ => "?",
    };

    let suit = match Suit::try_from(card.suit).unwrap_or(Suit::Spades) {
        Suit::Spades => "♠",
        Suit::Hearts => "♥",
        Suit::Diamonds => "♦",
        Suit::Clubs => "♣",
        _ => "?",
    };

    format!("{}{}", rank, suit)
}

/// Format multiple cards to a human-readable string.
pub fn format_cards(cards: &[Card]) -> String {
    cards.iter().map(format_card).collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uuid_for_deterministic() {
        let a = uuid_for("test");
        let b = uuid_for("test");
        assert_eq!(a, b);
        assert_eq!(a.len(), 16);
    }

    #[test]
    fn test_uuid_for_different_seeds() {
        let a = uuid_for("alice");
        let b = uuid_for("bob");
        assert_ne!(a, b);
    }

    #[test]
    fn test_uuid_for_matches_python_uuid5_namespace_oid() {
        // Pinned cross-language byte identity. Python equivalent:
        //   uuid.uuid5(uuid.NAMESPACE_OID, "table-1").bytes.hex()
        // Any drift here means Python's `uuid_for` would emit different
        // bytes and break cucumber assertions like
        //   `the rejection field "table_root_hex" equals "..."`.
        let bytes = uuid_for("table-1");
        let hex = hex::encode(&bytes);
        assert_eq!(hex, "eba6a19b488f5e1097a5a34a92553679");
    }

    #[test]
    fn test_parse_card() {
        let card = parse_card("As");
        assert_eq!(card.rank, Rank::Ace as i32);
        assert_eq!(card.suit, Suit::Spades as i32);
    }

    #[test]
    fn test_parse_cards() {
        let cards = parse_cards("As Kh Qd Jc");
        assert_eq!(cards.len(), 4);
        assert_eq!(cards[0].rank, Rank::Ace as i32);
        assert_eq!(cards[1].rank, Rank::King as i32);
        assert_eq!(cards[2].rank, Rank::Queen as i32);
        assert_eq!(cards[3].rank, Rank::Jack as i32);
    }

    #[test]
    fn test_format_card() {
        let card = parse_card("As");
        assert_eq!(format_card(&card), "A♠");
    }

    #[test]
    fn test_currency() {
        let c = currency(1000);
        assert_eq!(c.amount, 1000);
        assert_eq!(c.currency_code, "CHIPS");
    }

    #[test]
    fn test_command_book() {
        let root = uuid_for("test");
        let book = command_book(&root, "player");
        let cover = book.cover.unwrap();
        assert_eq!(cover.domain, "player");
        assert_eq!(cover.root.unwrap().value, root);
    }

    #[test]
    fn test_event_book() {
        let root = uuid_for("test");
        let events: Vec<Any> = vec![];
        let book = event_book(&root, "player", &events);
        let cover = book.cover.unwrap();
        assert_eq!(cover.domain, "player");
        assert_eq!(book.pages.len(), 0);
    }
}
