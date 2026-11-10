//! MD5 (RFC 1321), for the RTS's `__hsbase_MD5*` entry points that `GHC.Internal.Fingerprint` uses.
//! The context is the RTS's `MD5Context` layout in the 88 bytes an address points at: four state
//! words, the byte count as two words, and a 64-byte block buffer, all little-endian.

const S: [u32; 64] = [
    7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20,
    4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
];

fn k_table() -> [u32; 64] {
    let mut k = [0u32; 64];
    for (i, slot) in k.iter_mut().enumerate() {
        *slot = ((i as f64 + 1.0).sin().abs() * 4294967296.0) as u32;
    }
    k
}

pub const CONTEXT_SIZE: usize = 88;

fn word(ctx: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([ctx[at], ctx[at + 1], ctx[at + 2], ctx[at + 3]])
}

fn put_word(ctx: &mut [u8], at: usize, w: u32) {
    ctx[at..at + 4].copy_from_slice(&w.to_le_bytes());
}

fn block(state: &mut [u32; 4], m: &[u8], k: &[u32; 64]) {
    let mut w = [0u32; 16];
    for (i, slot) in w.iter_mut().enumerate() {
        *slot = u32::from_le_bytes([m[4 * i], m[4 * i + 1], m[4 * i + 2], m[4 * i + 3]]);
    }
    let [mut a, mut b, mut c, mut d] = *state;
    for i in 0..64 {
        let (f, g) = match i / 16 {
            0 => ((b & c) | (!b & d), i),
            1 => ((d & b) | (!d & c), (5 * i + 1) % 16),
            2 => (b ^ c ^ d, (3 * i + 5) % 16),
            _ => (c ^ (b | !d), (7 * i) % 16),
        };
        let f = f.wrapping_add(a).wrapping_add(k[i]).wrapping_add(w[g]);
        a = d;
        d = c;
        c = b;
        b = b.wrapping_add(f.rotate_left(S[i]));
    }
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
    let k = k_table();
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
        block(&mut state, &blk, &k);
        input = &input[need..];
        buffered = 0;
    }
    while input.len() >= 64 {
        block(&mut state, &input[..64], &k);
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
