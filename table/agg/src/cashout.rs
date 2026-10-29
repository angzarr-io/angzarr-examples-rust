//! The cash-out identity (table.proto, PlayerCashedOut; EU-1461):
//! UUIDv5(namespace = the table root read as a UUID, name = "cashout/" +
//! the decimal sequence of the cash-out event), as 16 bytes, so replay and
//! redelivery always produce the same id.
//!
//! UUIDv5 (RFC 9562) is SHA-1 over the namespace and the name, here
//! written out rather than taken from the `uuid` crate: agg-table has no
//! dependency beyond the generated types, and adding one is a decision for
//! its owner (the `uuid` crate, with `v5`, is already in the workspace's
//! graph through angzarr-proto).

/// The cash-out id for the event at `sequence` in the table at
/// `table_root`.
pub fn cashout_id(table_root: &[u8; 16], sequence: u64) -> [u8; 16] {
    uuid_v5(table_root, format!("cashout/{sequence}").as_bytes())
}

/// UUIDv5 of `name` in `namespace`.
pub fn uuid_v5(namespace: &[u8; 16], name: &[u8]) -> [u8; 16] {
    let digest = sha1(&[&namespace[..], name].concat());
    let mut id = [0; 16];
    id.copy_from_slice(&digest[..16]);
    id[6] = (id[6] & 0x0f) | 0x50; // version 5
    id[8] = (id[8] & 0x3f) | 0x80; // the RFC 9562 variant
    id
}

/// SHA-1 (FIPS 180-4) of `message`.
pub fn sha1(message: &[u8]) -> [u8; 20] {
    let mut h: [u32; 5] = [
        0x6745_2301,
        0xEFCD_AB89,
        0x98BA_DCFE,
        0x1032_5476,
        0xC3D2_E1F0,
    ];
    // Padding: 0x80, zeros to 56 mod 64, then the bit length, big-endian.
    let mut data = message.to_vec();
    data.push(0x80);
    while data.len() % 64 != 56 {
        data.push(0);
    }
    data.extend_from_slice(&((message.len() as u64).wrapping_mul(8)).to_be_bytes());
    for block in data.chunks_exact(64) {
        let mut w = [0u32; 80];
        for (i, word) in block.chunks_exact(4).enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let [mut a, mut b, mut c, mut d, mut e] = h;
        for (i, wi) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | (!b & d), 0x5A82_7999),
                20..=39 => (b ^ c ^ d, 0x6ED9_EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1B_BCDC),
                _ => (b ^ c ^ d, 0xCA62_C1D6),
            };
            let t = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(*wi);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = t;
        }
        for (hi, x) in h.iter_mut().zip([a, b, c, d, e]) {
            *hi = hi.wrapping_add(x);
        }
    }
    let mut digest = [0; 20];
    for (out, word) in digest.chunks_exact_mut(4).zip(h) {
        out.copy_from_slice(&word.to_be_bytes());
    }
    digest
}

#[cfg(test)]
#[path = "cashout.test.rs"]
mod cashout_tests;
