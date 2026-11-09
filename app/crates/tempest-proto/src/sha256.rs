//! SHA-256 (FIPS 180-4), written here to keep the crate free of dependencies. Used to recognise the user's DSI file.

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

const INITIAL: [u32; 8] = [
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
];

/// SHA-256 digest of `data`.
pub fn sha256(data: &[u8]) -> [u8; 32] {
    let mut state = INITIAL;
    let (blocks, tail) = data.as_chunks::<64>();
    for block in blocks {
        compress(&mut state, block);
    }
    for block in final_blocks(tail, data.len()).as_chunks::<64>().0 {
        compress(&mut state, block);
    }
    let mut digest = [0u8; 32];
    for (out, word) in digest.as_chunks_mut::<4>().0.iter_mut().zip(state) {
        *out = word.to_be_bytes();
    }
    digest
}

/// Lowercase hexadecimal digest of `data`.
pub fn sha256_hex(data: &[u8]) -> String {
    to_hex(&sha256(data))
}

/// Lowercase hexadecimal text of `bytes`.
pub fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The unprocessed tail followed by the 0x80 marker, zeros and the 64-bit bit length: one or two blocks.
fn final_blocks(tail: &[u8], total_len: usize) -> Vec<u8> {
    let mut out = tail.to_vec();
    out.push(0x80);
    let padded = out.len().div_ceil(64) * 64;
    let padded = if padded - out.len() < 8 {
        padded + 64
    } else {
        padded
    };
    out.resize(padded - 8, 0);
    out.extend_from_slice(&((total_len as u64) * 8).to_be_bytes());
    out
}

fn compress(state: &mut [u32; 8], block: &[u8; 64]) {
    let mut w = [0u32; 64];
    for (slot, bytes) in w.iter_mut().zip(block.as_chunks::<4>().0) {
        *slot = u32::from_be_bytes(*bytes);
    }
    for i in 16..64 {
        let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
        let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
        w[i] = w[i - 16]
            .wrapping_add(s0)
            .wrapping_add(w[i - 7])
            .wrapping_add(s1);
    }
    let mut v = *state;
    for i in 0..64 {
        v = round(v, K[i].wrapping_add(w[i]));
    }
    for (s, x) in state.iter_mut().zip(v) {
        *s = s.wrapping_add(x);
    }
}

/// One round; `kw` is the round constant plus the schedule word.
fn round(v: [u32; 8], kw: u32) -> [u32; 8] {
    let [a, b, c, d, e, f, g, h] = v;
    let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
    let ch = (e & f) ^ (!e & g);
    let t1 = h.wrapping_add(s1).wrapping_add(ch).wrapping_add(kw);
    let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
    let maj = (a & b) ^ (a & c) ^ (b & c);
    let t2 = s0.wrapping_add(maj);
    [t1.wrapping_add(t2), a, b, c, d.wrapping_add(t1), e, f, g]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sha256_when_empty_returns_standard_vector() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn test_sha256_when_abc_returns_standard_vector() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn test_sha256_when_two_block_message_returns_standard_vector() {
        let msg = b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq";

        assert_eq!(
            sha256_hex(msg),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    #[test]
    fn test_sha256_when_million_a_returns_standard_vector() {
        let msg = vec![b'a'; 1_000_000];

        assert_eq!(
            sha256_hex(&msg),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
    }

    #[test]
    fn test_sha256_when_padding_boundary_lengths_returns_distinct_digests() {
        let digests: Vec<_> = [55usize, 56, 63, 64, 65]
            .iter()
            .map(|n| sha256(&vec![0u8; *n]))
            .collect();

        for (i, a) in digests.iter().enumerate() {
            for b in &digests[i + 1..] {
                assert_ne!(a, b);
            }
        }
        assert_eq!(
            sha256_hex(&[0u8; 55]),
            "02779466cdec163811d078815c633f21901413081449002f24aa3e80f0b88ef7"
        );
    }

    #[test]
    fn test_sha256_when_one_bit_differs_returns_different_digest() {
        assert_ne!(sha256(b"abc"), sha256(b"abd"));
    }
}
