//! MD5 (RFC 1321), for the RTS's `__hsbase_MD5*` entry points that `GHC.Internal.Fingerprint` uses.
//! The context is the RTS's `MD5Context` layout in the 88 bytes an address points at: four state
//! words, the byte count as two words, and a 64-byte block buffer, all little-endian.

const S: [u32; 64] = [
    7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20,
    4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
];

// RFC 1321 constants; independent of the message and update boundaries.
const K: [u32; 64] = [
    0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee, 0xf57c0faf, 0x4787c62a, 0xa8304613, 0xfd469501,
    0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be, 0x6b901122, 0xfd987193, 0xa679438e, 0x49b40821,
    0xf61e2562, 0xc040b340, 0x265e5a51, 0xe9b6c7aa, 0xd62f105d, 0x02441453, 0xd8a1e681, 0xe7d3fbc8,
    0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed, 0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a,
    0xfffa3942, 0x8771f681, 0x6d9d6122, 0xfde5380c, 0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70,
    0x289b7ec6, 0xeaa127fa, 0xd4ef3085, 0x04881d05, 0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665,
    0xf4292244, 0x432aff97, 0xab9423a7, 0xfc93a039, 0x655b59c3, 0x8f0ccc92, 0xffeff47d, 0x85845dd1,
    0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1, 0xf7537e82, 0xbd3af235, 0x2ad7d2bb, 0xeb86d391,
];

pub const CONTEXT_SIZE: usize = 88;

fn word(ctx: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([ctx[at], ctx[at + 1], ctx[at + 2], ctx[at + 3]])
}

fn put_word(ctx: &mut [u8], at: usize, w: u32) {
    ctx[at..at + 4].copy_from_slice(&w.to_le_bytes());
}

fn block(state: &mut [u32; 4], m: &[u8]) {
    let mut w = [0u32; 16];
    for (i, slot) in w.iter_mut().enumerate() {
        *slot = u32::from_le_bytes([m[4 * i], m[4 * i + 1], m[4 * i + 2], m[4 * i + 3]]);
    }
    let [mut a, mut b, mut c, mut d] = *state;
    // Constant steps let the compiler use immediate rotations/word offsets.
    // Rotating the argument names avoids moving all four state words per step.
    macro_rules! step {
        ($a:ident, $b:ident, $f:expr, $i:expr, $g:expr) => {{
            let mixed = $a.wrapping_add($f).wrapping_add(K[$i]).wrapping_add(w[$g]);
            $a = $b.wrapping_add(mixed.rotate_left(S[$i]));
        }};
    }
    // Round 1.
    step!(a, b, (b & c) | (!b & d), 0, 0);
    step!(d, a, (a & b) | (!a & c), 1, 1);
    step!(c, d, (d & a) | (!d & b), 2, 2);
    step!(b, c, (c & d) | (!c & a), 3, 3);
    step!(a, b, (b & c) | (!b & d), 4, 4);
    step!(d, a, (a & b) | (!a & c), 5, 5);
    step!(c, d, (d & a) | (!d & b), 6, 6);
    step!(b, c, (c & d) | (!c & a), 7, 7);
    step!(a, b, (b & c) | (!b & d), 8, 8);
    step!(d, a, (a & b) | (!a & c), 9, 9);
    step!(c, d, (d & a) | (!d & b), 10, 10);
    step!(b, c, (c & d) | (!c & a), 11, 11);
    step!(a, b, (b & c) | (!b & d), 12, 12);
    step!(d, a, (a & b) | (!a & c), 13, 13);
    step!(c, d, (d & a) | (!d & b), 14, 14);
    step!(b, c, (c & d) | (!c & a), 15, 15);
    // Round 2.
    step!(a, b, (d & b) | (!d & c), 16, 1);
    step!(d, a, (c & a) | (!c & b), 17, 6);
    step!(c, d, (b & d) | (!b & a), 18, 11);
    step!(b, c, (a & c) | (!a & d), 19, 0);
    step!(a, b, (d & b) | (!d & c), 20, 5);
    step!(d, a, (c & a) | (!c & b), 21, 10);
    step!(c, d, (b & d) | (!b & a), 22, 15);
    step!(b, c, (a & c) | (!a & d), 23, 4);
    step!(a, b, (d & b) | (!d & c), 24, 9);
    step!(d, a, (c & a) | (!c & b), 25, 14);
    step!(c, d, (b & d) | (!b & a), 26, 3);
    step!(b, c, (a & c) | (!a & d), 27, 8);
    step!(a, b, (d & b) | (!d & c), 28, 13);
    step!(d, a, (c & a) | (!c & b), 29, 2);
    step!(c, d, (b & d) | (!b & a), 30, 7);
    step!(b, c, (a & c) | (!a & d), 31, 12);
    // Round 3.
    step!(a, b, b ^ c ^ d, 32, 5);
    step!(d, a, a ^ b ^ c, 33, 8);
    step!(c, d, d ^ a ^ b, 34, 11);
    step!(b, c, c ^ d ^ a, 35, 14);
    step!(a, b, b ^ c ^ d, 36, 1);
    step!(d, a, a ^ b ^ c, 37, 4);
    step!(c, d, d ^ a ^ b, 38, 7);
    step!(b, c, c ^ d ^ a, 39, 10);
    step!(a, b, b ^ c ^ d, 40, 13);
    step!(d, a, a ^ b ^ c, 41, 0);
    step!(c, d, d ^ a ^ b, 42, 3);
    step!(b, c, c ^ d ^ a, 43, 6);
    step!(a, b, b ^ c ^ d, 44, 9);
    step!(d, a, a ^ b ^ c, 45, 12);
    step!(c, d, d ^ a ^ b, 46, 15);
    step!(b, c, c ^ d ^ a, 47, 2);
    // Round 4.
    step!(a, b, c ^ (b | !d), 48, 0);
    step!(d, a, b ^ (a | !c), 49, 7);
    step!(c, d, a ^ (d | !b), 50, 14);
    step!(b, c, d ^ (c | !a), 51, 5);
    step!(a, b, c ^ (b | !d), 52, 12);
    step!(d, a, b ^ (a | !c), 53, 3);
    step!(c, d, a ^ (d | !b), 54, 10);
    step!(b, c, d ^ (c | !a), 55, 1);
    step!(a, b, c ^ (b | !d), 56, 8);
    step!(d, a, b ^ (a | !c), 57, 15);
    step!(c, d, a ^ (d | !b), 58, 6);
    step!(b, c, d ^ (c | !a), 59, 13);
    step!(a, b, c ^ (b | !d), 60, 4);
    step!(d, a, b ^ (a | !c), 61, 11);
    step!(c, d, a ^ (d | !b), 62, 2);
    step!(b, c, d ^ (c | !a), 63, 9);
    state[0] = state[0].wrapping_add(a);
    state[1] = state[1].wrapping_add(b);
    state[2] = state[2].wrapping_add(c);
    state[3] = state[3].wrapping_add(d);
}

/// MD5Init: the initial state, no bytes counted.
pub fn init(ctx: &mut [u8]) {
    for (i, w) in [0x67452301u32, 0xefcdab89, 0x98badcfe, 0x10325476].iter().enumerate() {
        put_word(ctx, 4 * i, *w);
    }
    put_word(ctx, 16, 0);
    put_word(ctx, 20, 0);
}

/// MD5Update: absorbs bytes into the context.
pub fn update(ctx: &mut [u8], data: &[u8]) {
    let count = word(ctx, 16) as u64 | ((word(ctx, 20) as u64) << 32);
    let mut buffered = (count % 64) as usize;
    let total = count.wrapping_add(data.len() as u64);
    put_word(ctx, 16, total as u32);
    put_word(ctx, 20, (total >> 32) as u32);
    let mut state = [word(ctx, 0), word(ctx, 4), word(ctx, 8), word(ctx, 12)];
    let mut input = data;
    if buffered > 0 {
        let need = 64 - buffered;
        if input.len() < need {
            ctx[24 + buffered..24 + buffered + input.len()].copy_from_slice(input);
            store_state(ctx, &state);
            return;
        }
        ctx[24 + buffered..24 + 64].copy_from_slice(&input[..need]);
        let mut blk = [0u8; 64];
        blk.copy_from_slice(&ctx[24..88]);
        block(&mut state, &blk);
        input = &input[need..];
        buffered = 0;
    }
    while input.len() >= 64 {
        block(&mut state, &input[..64]);
        input = &input[64..];
    }
    let _ = buffered;
    ctx[24..24 + input.len()].copy_from_slice(input);
    store_state(ctx, &state);
}

fn store_state(ctx: &mut [u8], state: &[u32; 4]) {
    for (i, w) in state.iter().enumerate() {
        put_word(ctx, 4 * i, *w);
    }
}

/// MD5Final: the 16-byte digest, from the context after padding.
pub fn finish(ctx: &mut [u8]) -> [u8; 16] {
    let count = word(ctx, 16) as u64 | ((word(ctx, 20) as u64) << 32);
    let bits = count.wrapping_mul(8);
    let pad_len = if count % 64 < 56 { 56 - (count % 64) } else { 120 - (count % 64) } as usize;
    let mut pad = vec![0u8; pad_len + 8];
    pad[0] = 0x80;
    pad[pad_len..].copy_from_slice(&bits.to_le_bytes());
    update(ctx, &pad);
    let mut digest = [0u8; 16];
    for i in 0..4 {
        digest[4 * i..4 * i + 4].copy_from_slice(&word(ctx, 4 * i).to_le_bytes());
    }
    digest
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_constants_and_steps_match_the_reference_loop() {
        let reference_k: [u32; 64] = std::array::from_fn(|i| ((i as f64 + 1.0).sin().abs() * 4294967296.0) as u32);
        assert_eq!(K, reference_k);
        let mut seed = 0x73ab9d21u32;
        let mut next = || { seed = seed.wrapping_mul(1664525).wrapping_add(1013904223); seed };
        for _ in 0..128 {
            let initial = std::array::from_fn(|_| next());
            let words: [u32; 16] = std::array::from_fn(|_| next());
            let mut input = [0u8; 64];
            for (bytes, word) in input.chunks_exact_mut(4).zip(words) { bytes.copy_from_slice(&word.to_le_bytes()); }
            let [mut a, mut b, mut c, mut d] = initial;
            for i in 0..64 {
                let (f, g) = match i / 16 {
                    0 => ((b & c) | (!b & d), i),
                    1 => ((d & b) | (!d & c), (5 * i + 1) % 16),
                    2 => (b ^ c ^ d, (3 * i + 5) % 16),
                    _ => (c ^ (b | !d), (7 * i) % 16),
                };
                let mixed = f.wrapping_add(a).wrapping_add(reference_k[i]).wrapping_add(words[g]);
                a = d; d = c; c = b;
                b = b.wrapping_add(mixed.rotate_left(S[i]));
            }
            let expected = [initial[0].wrapping_add(a), initial[1].wrapping_add(b),
                            initial[2].wrapping_add(c), initial[3].wrapping_add(d)];
            let mut actual = initial;
            block(&mut actual, &input);
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn streaming_matches_single_update_across_padding_boundaries() {
        for len in [0, 1, 55, 56, 57, 63, 64, 65, 119, 120, 127, 128, 129, 1000] {
            let data: Vec<u8> = (0..len).map(|i| (i as u8).wrapping_mul(17)).collect();
            let mut ctx = [0u8; CONTEXT_SIZE];
            init(&mut ctx); update(&mut ctx, &data);
            let expected = finish(&mut ctx);
            for chunk in [1, 2, 7, 55, 56, 63, 64, 65, 128] {
                init(&mut ctx);
                for part in data.chunks(chunk) { update(&mut ctx, &[]); update(&mut ctx, part); }
                assert_eq!(finish(&mut ctx), expected, "len={len}, chunk={chunk}");
            }
        }
    }

    fn hex(d: &[u8; 16]) -> String {
        d.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn rfc_vectors() {
        let mut ctx = [0u8; CONTEXT_SIZE];
        init(&mut ctx);
        assert_eq!(hex(&finish(&mut ctx)), "d41d8cd98f00b204e9800998ecf8427e");
        let mut ctx = [0u8; CONTEXT_SIZE];
        init(&mut ctx);
        update(&mut ctx, b"The quick brown fox jumps over the lazy dog");
        assert_eq!(hex(&finish(&mut ctx)), "9e107d9d372bb6826bd81d3542a419d6");
        let mut ctx = [0u8; CONTEXT_SIZE];
        init(&mut ctx);
        let long = vec![b'a'; 1000];
        update(&mut ctx, &long[..3]);
        update(&mut ctx, &long[3..]);
        assert_eq!(hex(&finish(&mut ctx)), hex(&{
            let mut c = [0u8; CONTEXT_SIZE];
            init(&mut c);
            update(&mut c, &long);
            finish(&mut c)
        }));
    }
}
