use super::*;

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[test]
fn uuid_v5_matches_the_rfc_example_for_the_dns_namespace() {
    // NAMESPACE_DNS, 6ba7b810-9dad-11d1-80b4-00c04fd430c8; Python's
    // uuid.uuid5(NAMESPACE_DNS, "python.org").
    let dns = [
        0x6b, 0xa7, 0xb8, 0x10, 0x9d, 0xad, 0x11, 0xd1, 0x80, 0xb4, 0x00, 0xc0, 0x4f, 0xd4, 0x30,
        0xc8,
    ];
    assert_eq!(
        hex(&uuid_v5(&dns, b"python.org")),
        "886313e13b8a53729b900c9aee199e5d"
    );
}

#[test]
fn eu_1461_a_cash_out_id_is_derived_from_the_table_and_the_event_position() {
    // Python's uuid.uuid5(UUID(bytes=b"\x7a" * 16), "cashout/4").
    assert_eq!(
        hex(&cashout_id(&[0x7a; 16], 4)),
        "22fd5002d8ea53d1a0f6393fc8d3bbdf"
    );
    assert_ne!(cashout_id(&[0x7a; 16], 4), cashout_id(&[0x7a; 16], 5));
    assert_ne!(cashout_id(&[0x7a; 16], 4), cashout_id(&[0x7b; 16], 4));
}
