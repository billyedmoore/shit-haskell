//! `Integer` primitives of the bignum stand-in. An `Integer` is an `IS` box (a machine word), or
//! `IP`/`IN` with a `BYTES` object of little-endian 64-bit magnitude limbs.
//!
//! Hot arithmetic and comparison operations execute directly on `i64` (for `IS`) or base-2^64
//! `u64` limbs (reading straight from the Haskell heap without allocating intermediate `Vec<u32>`
//! or `String` values).

use std::cmp::Ordering;

use super::heap::{self, kind, Word};
use super::prims::{
    bool_value, con1_id, force_arg, int_of, int_value, integer_of, integer_value, CON_IN, CON_IP,
    CON_IS,
};
use super::Machine;
use crate::interp::bigint::{
    bit_length, bitwise, limb64, limb64_count, popcount, shift_left, shift_right, test_bit, BigInt,
};

/// Pre-classified hot `Integer` primitives dispatched directly from `hsc_prim`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntegerOp {
    Add,
    Sub,
    Mul,
    Quot,
    Rem,
    Div,
    Mod,
    Negate,
    Abs,
    Signum,
    Eq,
    Lt,
    Le,
    ToInt,
    FromInt,
}

impl IntegerOp {
    pub fn classify(name: &str) -> Option<Self> {
        Some(match name {
            "integerAdd" => Self::Add,
            "integerSub" => Self::Sub,
            "integerMul" => Self::Mul,
            "integerQuot" => Self::Quot,
            "integerRem" => Self::Rem,
            "integerDiv" => Self::Div,
            "integerMod" => Self::Mod,
            "integerNegate" => Self::Negate,
            "integerAbs" => Self::Abs,
            "integerSignum" => Self::Signum,
            "integerEq" => Self::Eq,
            "integerLt" => Self::Lt,
            "integerLe" => Self::Le,
            "integerToInt" => Self::ToInt,
            "intToInteger" => Self::FromInt,
            _ => return None,
        })
    }

    #[inline]
    pub fn run(self, m: &mut Machine) -> Word {
        match self {
            Self::Add => op_add(m),
            Self::Sub => op_sub(m),
            Self::Mul => op_mul(m),
            Self::Quot => op_divop(m, DivKind::Quot),
            Self::Rem => op_divop(m, DivKind::Rem),
            Self::Div => op_divop(m, DivKind::Div),
            Self::Mod => op_divop(m, DivKind::Mod),
            Self::Negate => op_negate(m),
            Self::Abs => op_abs(m),
            Self::Signum => op_signum(m),
            Self::Eq => op_cmp(m, CmpKind::Eq),
            Self::Lt => op_cmp(m, CmpKind::Lt),
            Self::Le => op_cmp(m, CmpKind::Le),
            Self::ToInt => op_to_int(m),
            Self::FromInt => op_from_int(m),
        }
        .unwrap_or(0)
    }
}

#[derive(Clone, Copy)]
enum DivKind {
    Quot,
    Rem,
    Div,
    Mod,
}

#[derive(Clone, Copy)]
enum CmpKind {
    Eq,
    Lt,
    Le,
}

// ----- Fast forcing & heap decoding -------------------------------------------------------------

/// Forces `m.args[k]` and its payload field (`INT` for `IS`, `BYTES` for `IP`/`IN`).
#[inline(always)]
fn force_integer_arg(m: &mut Machine, k: usize) -> Option<Word> {
    let mut v = m.args[k];
    if matches!(heap::kind_of(m.heap_get(v, 0)), kind::THUNK | kind::IND) {
        v = force_arg(m, k)?;
    }
    let field = m.heap_get(v, 1);
    if matches!(heap::kind_of(m.heap_get(field, 0)), kind::THUNK | kind::IND) {
        // Forcing the inner thunk updates it in place with an IND (and may move `v` via GC).
        let _ = m.force(field)?;
        v = m.args[k];
    }
    Some(v)
}

#[inline(always)]
fn follow_ind(m: &Machine, mut p: Word) -> Word {
    while heap::kind_of(m.heap_get(p, 0)) == kind::IND {
        p = m.heap_get(p, 1);
    }
    p
}

/// Decoded view of an already-forced `Integer` on the heap (valid until the next GC).
#[derive(Clone, Copy)]
enum IntView {
    Small(i64),
    Big { neg: bool, bp: Word, len: usize },
}

#[inline(always)]
fn view_forced(m: &Machine, v: Word) -> IntView {
    let id = heap::aux_of(m.heap_get(v, 0));
    let field = follow_ind(m, m.heap_get(v, 1));
    if id == CON_IS {
        return IntView::Small(int_of(m, field));
    }
    let neg = id == CON_IN;
    let mut len = heap::size_of(m.heap_get(field, 0)) as usize;
    // SAFETY: `field` is a forced `BYTES` object with `len` 64-bit limb words.
    let ptr = unsafe { (field as *const u64).add(1) };
    while len > 0 && unsafe { *ptr.add(len - 1) } == 0 {
        len -= 1;
    }
    if len == 0 {
        return IntView::Small(0);
    }
    if len == 1 {
        let w = unsafe { *ptr };
        if !neg && w <= i64::MAX as u64 {
            return IntView::Small(w as i64);
        }
        if neg && w <= (i64::MAX as u64) + 1 {
            return IntView::Small(0i64.wrapping_sub(w as i64));
        }
    }
    IntView::Big { neg, bp: field, len }
}

/// SAFETY: `bp` must be a valid `BYTES` object with at least `len` limb words, and no GC may occur
/// while the returned slice is alive.
#[inline(always)]
unsafe fn limbs_of<'a>(bp: Word, len: usize) -> &'a [u64] {
    std::slice::from_raw_parts((bp as *const u64).add(1), len)
}

#[inline(always)]
fn is_value(m: &mut Machine, n: i64) -> Word {
    m.reserve(4);
    let iv = int_value(m, n);
    con1_id(m, CON_IS, iv)
}

#[inline(always)]
fn small_sign_mag(n: i64) -> (bool, u64) {
    (n < 0, n.unsigned_abs())
}

/// Encodes a signed 128-bit value (magnitude + sign) as `IS`, `IP`, or `IN`.
#[inline]
fn encode_u128(m: &mut Machine, neg: bool, mag: u128) -> Word {
    if mag == 0 {
        return is_value(m, 0);
    }
    let lo = mag as u64;
    let hi = (mag >> 64) as u64;
    if hi == 0 {
        if !neg && lo <= i64::MAX as u64 {
            return is_value(m, lo as i64);
        }
        if neg && lo <= (i64::MAX as u64) + 1 {
            return is_value(m, 0i64.wrapping_sub(lo as i64));
        }
        m.reserve(6);
        let bp = m.alloc(2);
        m.heap_set(bp, heap::header(kind::BYTES, 1, 8));
        m.heap_set(bp + 8, lo);
        return con1_id(m, if neg { CON_IN } else { CON_IP }, bp);
    }
    m.reserve(6);
    let bp = m.alloc(3);
    m.heap_set(bp, heap::header(kind::BYTES, 2, 16));
    m.heap_set(bp + 8, lo);
    m.heap_set(bp + 16, hi);
    con1_id(m, if neg { CON_IN } else { CON_IP }, bp)
}

/// Encodes normalized base-2^64 magnitude limbs with sign `neg` on the Haskell heap.
fn encode_u64_limbs(m: &mut Machine, neg: bool, mut limbs: &[u64]) -> Word {
    while limbs.last() == Some(&0) {
        limbs = &limbs[..limbs.len() - 1];
    }
    if limbs.is_empty() {
        return is_value(m, 0);
    }
    if limbs.len() == 1 {
        let w = limbs[0];
        if !neg && w <= i64::MAX as u64 {
            return is_value(m, w as i64);
        }
        if neg && w <= (i64::MAX as u64) + 1 {
            return is_value(m, 0i64.wrapping_sub(w as i64));
        }
    }
    let words = limbs.len();
    m.reserve(words + 5);
    let bp = m.alloc(1 + words);
    m.heap_set(bp, heap::header(kind::BYTES, words as u32, 8 * words as u32));
    // SAFETY: `bp` was just allocated with `1 + words` words.
    unsafe {
        std::ptr::copy_nonoverlapping(limbs.as_ptr(), (bp as *mut u64).add(1), words);
    }
    con1_id(m, if neg { CON_IN } else { CON_IP }, bp)
}

/// Forces both binary integer arguments; GC-safe because both are stored in `m.args[0..2]`.
#[inline(always)]
fn force_two(m: &mut Machine) -> Option<(IntView, IntView)> {
    let _ = force_integer_arg(m, 0)?;
    let vb = force_integer_arg(m, 1)?;
    let va = m.args[0];
    Some((view_forced(m, va), view_forced(m, vb)))
}

// ----- Base-2^64 magnitude arithmetic -----------------------------------------------------------

#[inline]
fn cmp_mag64(a: &[u64], b: &[u64]) -> Ordering {
    if a.len() != b.len() {
        return a.len().cmp(&b.len());
    }
    for i in (0..a.len()).rev() {
        if a[i] != b[i] {
            return a[i].cmp(&b[i]);
        }
    }
    Ordering::Equal
}

fn add_mag64(a: &[u64], b: &[u64]) -> Vec<u64> {
    let (long, short) = if a.len() >= b.len() { (a, b) } else { (b, a) };
    let mut out = Vec::with_capacity(long.len() + 1);
    let mut carry = 0u64;
    for i in 0..short.len() {
        let (s1, c1) = long[i].overflowing_add(short[i]);
        let (s2, c2) = s1.overflowing_add(carry);
        out.push(s2);
        carry = (c1 as u64) + (c2 as u64);
    }
    for &x in &long[short.len()..] {
        let (s, c) = x.overflowing_add(carry);
        out.push(s);
        carry = c as u64;
    }
    if carry != 0 {
        out.push(carry);
    }
    out
}

/// `a - b` where `a >= b` in magnitude.
fn sub_mag64(a: &[u64], b: &[u64]) -> Vec<u64> {
    let mut out = Vec::with_capacity(a.len());
    let mut borrow = 0u64;
    for i in 0..b.len() {
        let (d1, b1) = a[i].overflowing_sub(b[i]);
        let (d2, b2) = d1.overflowing_sub(borrow);
        out.push(d2);
        borrow = (b1 as u64) + (b2 as u64);
    }
    for &x in &a[b.len()..] {
        let (d, b1) = x.overflowing_sub(borrow);
        out.push(d);
        borrow = b1 as u64;
    }
    while out.last() == Some(&0) {
        out.pop();
    }
    out
}

fn mul_mag64(a: &[u64], b: &[u64]) -> Vec<u64> {
    if a.is_empty() || b.is_empty() {
        return Vec::new();
    }
    let (long, short) = if a.len() >= b.len() { (a, b) } else { (b, a) };
    let mut out = vec![0u64; a.len() + b.len()];
    for (i, &x) in short.iter().enumerate() {
        let x = x as u128;
        let mut carry = 0u128;
        let row = &mut out[i..i + long.len()];
        for (o, &y) in row.iter_mut().zip(long) {
            let t = (*o as u128) + x * (y as u128) + carry;
            *o = t as u64;
            carry = t >> 64;
        }
        let mut k = i + long.len();
        while carry != 0 {
            let t = (out[k] as u128) + carry;
            out[k] = t as u64;
            carry = t >> 64;
            k += 1;
        }
    }
    while out.last() == Some(&0) {
        out.pop();
    }
    out
}

/// Truncated division of non-empty base-2^64 magnitudes: Knuth's Algorithm D.
fn divrem_mag64(a: &[u64], b: &[u64]) -> (Vec<u64>, Vec<u64>) {
    debug_assert!(!b.is_empty() && *b.last().unwrap() != 0);
    if cmp_mag64(a, b) == Ordering::Less {
        return (Vec::new(), a.to_vec());
    }
    if b.len() == 1 {
        let d = b[0] as u128;
        let mut q = vec![0u64; a.len()];
        let mut rem = 0u128;
        for i in (0..a.len()).rev() {
            let cur = (rem << 64) | (a[i] as u128);
            q[i] = (cur / d) as u64;
            rem = cur % d;
        }
        while q.last() == Some(&0) {
            q.pop();
        }
        let r = if rem == 0 { Vec::new() } else { vec![rem as u64] };
        return (q, r);
    }
    let n = b.len();
    let m = a.len() - n;
    let s = b[n - 1].leading_zeros();
    let shl = |x: &[u64], extra: bool| -> Vec<u64> {
        let mut out = Vec::with_capacity(x.len() + usize::from(extra));
        let mut carry = 0u64;
        for &w in x {
            if s == 0 {
                out.push(w);
            } else {
                out.push((w << s) | carry);
                carry = w >> (64 - s);
            }
        }
        if extra {
            out.push(carry);
        }
        out
    };
    let vn = shl(b, false);
    let mut un = shl(a, true);
    let mut q = vec![0u64; m + 1];
    let top = vn[n - 1] as u128;
    let next = vn[n - 2] as u128;
    for j in (0..=m).rev() {
        let u_hi = un[j + n] as u128;
        let u_lo = un[j + n - 1] as u128;
        let (mut qhat, mut rhat) = if u_hi == top {
            (u64::MAX as u128, u_lo + top)
        } else {
            let num = (u_hi << 64) | u_lo;
            (num / top, num % top)
        };
        let u_next = un[j + n - 2] as u128;
        while rhat < (1u128 << 64) && qhat * next > ((rhat << 64) | u_next) {
            qhat -= 1;
            rhat += top;
        }
        // Multiply and subtract qhat * vn from un[j..=j+n].
        let mut borrow = 0u64;
        for i in 0..n {
            // qhat <= 2^64-1, vn[i] <= 2^64-1, borrow <= 2^64-1 => p <= 2^128 - 2^64 (never overflows).
            let p = qhat * (vn[i] as u128) + (borrow as u128);
            let (d, b1) = un[j + i].overflowing_sub(p as u64);
            un[j + i] = d;
            borrow = (p >> 64) as u64 + (b1 as u64);
        }
        let (d, underflow) = un[j + n].overflowing_sub(borrow);
        un[j + n] = d;
        if underflow {
            qhat -= 1;
            let mut carry = 0u64;
            for i in 0..n {
                let (s1, c1) = un[j + i].overflowing_add(vn[i]);
                let (s2, c2) = s1.overflowing_add(carry);
                un[j + i] = s2;
                carry = (c1 as u64) + (c2 as u64);
            }
            un[j + n] = un[j + n].wrapping_add(carry);
        }
        q[j] = qhat as u64;
    }
    let mut r: Vec<u64> = (0..n)
        .map(|i| if s == 0 { un[i] } else { (un[i] >> s) | (un[i + 1] << (64 - s)) })
        .collect();
    while q.last() == Some(&0) {
        q.pop();
    }
    while r.last() == Some(&0) {
        r.pop();
    }
    (q, r)
}

// ----- Direct heap-writing fast paths (zero Rust Vec allocation) --------------------------------

/// Multiplies a forced `Big` integer in `m.args[big_arg]` by a nonzero `u64` scalar, writing the
/// result directly onto the Haskell heap.
fn mul_big_by_u64(m: &mut Machine, big_arg: usize, out_neg: bool, big_len: usize, k: u64) -> Word {
    if k == 0 {
        return is_value(m, 0);
    }
    let max_words = big_len + 1;
    m.reserve(max_words + 6);
    // Re-read after possible GC in `reserve`.
    let v = m.args[big_arg];
    let src_bp = follow_ind(m, m.heap_get(v, 1));
    let saved_hp = m.hp;
    let dst_bp = m.alloc(1 + max_words);
    let k128 = k as u128;
    let mut carry = 0u128;
    // SAFETY: `src_bp` has `big_len` limbs; `dst_bp` was just allocated with `1 + max_words` words.
    unsafe {
        let src = (src_bp as *const u64).add(1);
        let dst = (dst_bp as *mut u64).add(1);
        for i in 0..big_len {
            let t = (*src.add(i) as u128) * k128 + carry;
            *dst.add(i) = t as u64;
            carry = t >> 64;
        }
        let words = if carry != 0 {
            *dst.add(big_len) = carry as u64;
            big_len + 1
        } else {
            // Reclaim the unused top limb word immediately (no other allocation intervened).
            m.hp -= 8;
            big_len
        };
        if words == 1 {
            let w = *dst;
            if !out_neg && w <= i64::MAX as u64 {
                m.hp = saved_hp;
                return is_value(m, w as i64);
            }
            if out_neg && w <= (i64::MAX as u64) + 1 {
                m.hp = saved_hp;
                return is_value(m, 0i64.wrapping_sub(w as i64));
            }
        }
        m.heap_set(dst_bp, heap::header(kind::BYTES, words as u32, 8 * words as u32));
    }
    con1_id(m, if out_neg { CON_IN } else { CON_IP }, dst_bp)
}

/// Adds or subtracts a nonzero `u64` magnitude to/from a forced `Big` integer in `m.args[big_arg]`.
fn add_sub_big_u64(
    m: &mut Machine,
    big_arg: usize,
    big_neg: bool,
    big_len: usize,
    add_mag: bool,
    k: u64,
) -> Word {
    if k == 0 {
        // Result has same magnitude as `big_arg`, possibly negated.
        let v = m.args[big_arg];
        let bp = follow_ind(m, m.heap_get(v, 1));
        if big_neg == (heap::aux_of(m.heap_get(v, 0)) == CON_IN) {
            return v;
        }
        if big_len == 1 {
            let w = m.heap_get(bp, 1);
            if !big_neg && w <= i64::MAX as u64 {
                return is_value(m, w as i64);
            }
            if big_neg && w <= (i64::MAX as u64) + 1 {
                return is_value(m, 0i64.wrapping_sub(w as i64));
            }
        }
        m.reserve(4);
        let bp = follow_ind(m, m.heap_get(m.args[big_arg], 1));
        return con1_id(m, if big_neg { CON_IN } else { CON_IP }, bp);
    }
    let max_words = big_len + usize::from(add_mag);
    m.reserve(max_words + 6);
    let src_bp = follow_ind(m, m.heap_get(m.args[big_arg], 1));
    let saved_hp = m.hp;
    let dst_bp = m.alloc(1 + max_words);
    // SAFETY: `src_bp` has `big_len >= 1` limbs (`|big| > i64::MAX` or `|big| > 2^63`); `dst_bp` has `max_words`.
    let words = unsafe {
        let src = (src_bp as *const u64).add(1);
        let dst = (dst_bp as *mut u64).add(1);
        if add_mag {
            let (s0, c0) = (*src).overflowing_add(k);
            *dst = s0;
            let mut carry = c0 as u64;
            for i in 1..big_len {
                let (s, c) = (*src.add(i)).overflowing_add(carry);
                *dst.add(i) = s;
                carry = c as u64;
            }
            if carry != 0 {
                *dst.add(big_len) = carry;
                big_len + 1
            } else {
                m.hp -= 8;
                big_len
            }
        } else {
            // Since `big` is normalized `Big`, `|big| > k` strictly when `k <= 2^63` unless
            // `big_len == 1 && src[0] == 2^63 && k == 2^63` (which produces 0!).
            let (d0, b0) = (*src).overflowing_sub(k);
            *dst = d0;
            let mut borrow = b0 as u64;
            for i in 1..big_len {
                let (d, b) = (*src.add(i)).overflowing_sub(borrow);
                *dst.add(i) = d;
                borrow = b as u64;
            }
            let mut w = big_len;
            while w > 0 && *dst.add(w - 1) == 0 {
                w -= 1;
            }
            if w < big_len {
                m.hp -= 8 * (big_len - w) as Word;
            }
            w
        }
    };
    if words == 0 {
        m.hp = saved_hp;
        return is_value(m, 0);
    }
    if words == 1 {
        let w = m.heap_get(dst_bp, 1);
        if !big_neg && w <= i64::MAX as u64 {
            m.hp = saved_hp;
            return is_value(m, w as i64);
        }
        if big_neg && w <= (i64::MAX as u64) + 1 {
            m.hp = saved_hp;
            return is_value(m, 0i64.wrapping_sub(w as i64));
        }
    }
    m.heap_set(dst_bp, heap::header(kind::BYTES, words as u32, 8 * words as u32));
    con1_id(m, if big_neg { CON_IN } else { CON_IP }, dst_bp)
}

/// Fast single-limb `Big / Small` and `Big % Small` without allocating a Rust `Vec`.
fn divop_big_by_small(
    m: &mut Machine,
    a_neg: bool,
    a_len: usize,
    b: i64,
    kind: DivKind,
) -> Word {
    if b == 0 {
        m.exc = m.error_value("divide by zero");
        return 0;
    }
    let (b_neg, d) = small_sign_mag(b);
    let d128 = d as u128;
    let q_neg = a_neg != b_neg;
    match kind {
        DivKind::Rem | DivKind::Mod => {
            let src_bp = follow_ind(m, m.heap_get(m.args[0], 1));
            let mut rem = 0u128;
            // SAFETY: `src_bp` has `a_len` limbs; no GC occurs in this loop.
            unsafe {
                let src = (src_bp as *const u64).add(1);
                for i in (0..a_len).rev() {
                    let cur = (rem << 64) | (*src.add(i) as u128);
                    rem = cur % d128;
                }
            }
            let r_u64 = rem as u64;
            if r_u64 == 0 {
                return is_value(m, 0);
            }
            if matches!(kind, DivKind::Mod) && q_neg {
                // `mod` adjusts remainder to `|b| - |r|` with sign `b_neg`.
                encode_u128(m, b_neg, (d - r_u64) as u128)
            } else {
                encode_u128(m, a_neg, r_u64 as u128)
            }
        }
        DivKind::Quot | DivKind::Div => {
            m.reserve(a_len + 6);
            let src_bp = follow_ind(m, m.heap_get(m.args[0], 1));
            let saved_hp = m.hp;
            let dst_bp = m.alloc(1 + a_len);
            let mut rem = 0u128;
            // SAFETY: `src_bp` has `a_len` limbs; `dst_bp` has `1 + a_len` words.
            let mut words = unsafe {
                let src = (src_bp as *const u64).add(1);
                let dst = (dst_bp as *mut u64).add(1);
                for i in (0..a_len).rev() {
                    let cur = (rem << 64) | (*src.add(i) as u128);
                    *dst.add(i) = (cur / d128) as u64;
                    rem = cur % d128;
                }
                let mut w = a_len;
                while w > 0 && *dst.add(w - 1) == 0 {
                    w -= 1;
                }
                w
            };
            // `div` rounds toward -inf: if `rem != 0 && q_neg`, add 1 to quotient magnitude.
            if matches!(kind, DivKind::Div) && rem != 0 && q_neg {
                // Note: since `|q| = floor(|a| / d) <= |a|`, adding 1 cannot exceed `a_len` limbs
                // unless `d == 1`, in which case `rem == 0`!
                unsafe {
                    let dst = (dst_bp as *mut u64).add(1);
                    let mut carry = 1u64;
                    for i in 0..words {
                        let (s, c) = (*dst.add(i)).overflowing_add(carry);
                        *dst.add(i) = s;
                        carry = c as u64;
                        if carry == 0 {
                            break;
                        }
                    }
                    if carry != 0 {
                        *dst.add(words) = carry;
                        words += 1;
                    }
                }
            }
            if words < a_len {
                m.hp -= 8 * (a_len - words) as Word;
            }
            if words == 0 {
                m.hp = saved_hp;
                return is_value(m, 0);
            }
            if words == 1 {
                let w = m.heap_get(dst_bp, 1);
                if !q_neg && w <= i64::MAX as u64 {
                    m.hp = saved_hp;
                    return is_value(m, w as i64);
                }
                if q_neg && w <= (i64::MAX as u64) + 1 {
                    m.hp = saved_hp;
                    return is_value(m, 0i64.wrapping_sub(w as i64));
                }
            }
            m.heap_set(dst_bp, heap::header(kind::BYTES, words as u32, 8 * words as u32));
            con1_id(m, if q_neg { CON_IN } else { CON_IP }, dst_bp)
        }
    }
}

// ----- Pre-classified operations ----------------------------------------------------------------

fn add_signed_big(m: &mut Machine, a_neg: bool, a: &[u64], b_neg: bool, b: &[u64]) -> (bool, Vec<u64>) {
    let _ = m;
    if a_neg == b_neg {
        (a_neg, add_mag64(a, b))
    } else {
        match cmp_mag64(a, b) {
            Ordering::Equal => (false, Vec::new()),
            Ordering::Greater => (a_neg, sub_mag64(a, b)),
            Ordering::Less => (b_neg, sub_mag64(b, a)),
        }
    }
}

#[inline]
fn op_add(m: &mut Machine) -> Option<Word> {
    let (a, b) = force_two(m)?;
    Some(match (a, b) {
        (IntView::Small(x), IntView::Small(y)) => match x.checked_add(y) {
            Some(r) => is_value(m, r),
            None => {
                let r = (x as i128) + (y as i128);
                encode_u128(m, r < 0, r.unsigned_abs())
            }
        },
        (IntView::Big { neg, len, .. }, IntView::Small(y)) => {
            let (yn, ym) = small_sign_mag(y);
            add_sub_big_u64(m, 0, neg, len, neg == yn, ym)
        }
        (IntView::Small(x), IntView::Big { neg, len, .. }) => {
            let (xn, xm) = small_sign_mag(x);
            add_sub_big_u64(m, 1, neg, len, neg == xn, xm)
        }
        (IntView::Big { neg: an, bp: abp, len: alen }, IntView::Big { neg: bn, bp: bbp, len: blen }) => {
            let (rn, res) = unsafe { add_signed_big(m, an, limbs_of(abp, alen), bn, limbs_of(bbp, blen)) };
            encode_u64_limbs(m, rn, &res)
        }
    })
}

#[inline]
fn op_sub(m: &mut Machine) -> Option<Word> {
    let (a, b) = force_two(m)?;
    Some(match (a, b) {
        (IntView::Small(x), IntView::Small(y)) => match x.checked_sub(y) {
            Some(r) => is_value(m, r),
            None => {
                let r = (x as i128) - (y as i128);
                encode_u128(m, r < 0, r.unsigned_abs())
            }
        },
        (IntView::Big { neg, len, .. }, IntView::Small(y)) => {
            let (yn, ym) = small_sign_mag(y);
            // a - y = a + (-y)
            add_sub_big_u64(m, 0, neg, len, neg != yn, ym)
        }
        (IntView::Small(x), IntView::Big { neg, len, .. }) => {
            let (xn, xm) = small_sign_mag(x);
            // x - b = (-b) + x
            let b_neg_flipped = !neg;
            add_sub_big_u64(m, 1, b_neg_flipped, len, b_neg_flipped == xn, xm)
        }
        (IntView::Big { neg: an, bp: abp, len: alen }, IntView::Big { neg: bn, bp: bbp, len: blen }) => {
            let (rn, res) = unsafe { add_signed_big(m, an, limbs_of(abp, alen), !bn, limbs_of(bbp, blen)) };
            encode_u64_limbs(m, rn, &res)
        }
    })
}

#[inline]
fn op_mul(m: &mut Machine) -> Option<Word> {
    let (a, b) = force_two(m)?;
    Some(match (a, b) {
        (IntView::Small(x), IntView::Small(y)) => {
            let r = (x as i128) * (y as i128);
            if r >= i64::MIN as i128 && r <= i64::MAX as i128 {
                is_value(m, r as i64)
            } else {
                encode_u128(m, r < 0, r.unsigned_abs())
            }
        }
        (IntView::Big { neg, len, .. }, IntView::Small(y)) => {
            let (yn, ym) = small_sign_mag(y);
            mul_big_by_u64(m, 0, neg != yn, len, ym)
        }
        (IntView::Small(x), IntView::Big { neg, len, .. }) => {
            let (xn, xm) = small_sign_mag(x);
            mul_big_by_u64(m, 1, xn != neg, len, xm)
        }
        (IntView::Big { neg: an, bp: abp, len: alen }, IntView::Big { neg: bn, bp: bbp, len: blen }) => {
            let prod = unsafe { mul_mag64(limbs_of(abp, alen), limbs_of(bbp, blen)) };
            encode_u64_limbs(m, an != bn, &prod)
        }
    })
}

fn op_divop(m: &mut Machine, kind: DivKind) -> Option<Word> {
    let (a, b) = force_two(m)?;
    Some(match (a, b) {
        (IntView::Small(x), IntView::Small(y)) => {
            if y == 0 {
                m.exc = m.error_value("divide by zero");
                return Some(0);
            }
            if x == i64::MIN && y == -1 {
                return Some(match kind {
                    DivKind::Quot | DivKind::Div => encode_u128(m, false, 1u128 << 63),
                    DivKind::Rem | DivKind::Mod => is_value(m, 0),
                });
            }
            let r = match kind {
                DivKind::Quot => x / y,
                DivKind::Rem => x % y,
                DivKind::Div => {
                    let q = x / y;
                    let rem = x % y;
                    if rem != 0 && (x < 0) != (y < 0) { q - 1 } else { q }
                }
                DivKind::Mod => {
                    let rem = x % y;
                    if rem != 0 && (x < 0) != (y < 0) { rem + y } else { rem }
                }
            };
            is_value(m, r)
        }
        (IntView::Big { neg: an, len: alen, .. }, IntView::Small(y)) => {
            divop_big_by_small(m, an, alen, y, kind)
        }
        (IntView::Small(x), IntView::Big { neg: bn, bp: bbp, len: blen }) => {
            // `|x| <= 2^63 <= |b|`: if `x == i64::MIN && b == +2^63`, `|x| == |b|`; otherwise `|x| < |b|`.
            if x == 0 {
                return Some(is_value(m, 0));
            }
            let (xn, xm) = small_sign_mag(x);
            let b_limbs = unsafe { limbs_of(bbp, blen) };
            let (q, r) = divrem_mag64(&[xm], b_limbs);
            finish_divop(m, xn, bn, b_limbs, q, r, kind)
        }
        (IntView::Big { neg: an, bp: abp, len: alen }, IntView::Big { neg: bn, bp: bbp, len: blen }) => {
            let (q, r) = unsafe { divrem_mag64(limbs_of(abp, alen), limbs_of(bbp, blen)) };
            // Re-read `b_limbs` safely (no GC has happened yet).
            let b_limbs = unsafe { limbs_of(bbp, blen) };
            finish_divop(m, an, bn, b_limbs, q, r, kind)
        }
    })
}

fn finish_divop(
    m: &mut Machine,
    a_neg: bool,
    b_neg: bool,
    b_limbs: &[u64],
    q: Vec<u64>,
    r: Vec<u64>,
    kind: DivKind,
) -> Word {
    let q_neg = a_neg != b_neg;
    match kind {
        DivKind::Quot => encode_u64_limbs(m, q_neg, &q),
        DivKind::Rem => encode_u64_limbs(m, a_neg, &r),
        DivKind::Div => {
            if !r.is_empty() && q_neg {
                let q1 = add_mag64(&q, &[1]);
                encode_u64_limbs(m, true, &q1)
            } else {
                encode_u64_limbs(m, q_neg, &q)
            }
        }
        DivKind::Mod => {
            if !r.is_empty() && q_neg {
                let r1 = sub_mag64(b_limbs, &r);
                encode_u64_limbs(m, b_neg, &r1)
            } else {
                encode_u64_limbs(m, a_neg, &r)
            }
        }
    }
}

#[inline]
fn op_negate(m: &mut Machine) -> Option<Word> {
    let v = force_integer_arg(m, 0)?;
    Some(match view_forced(m, v) {
        IntView::Small(x) => match x.checked_neg() {
            Some(r) => is_value(m, r),
            None => encode_u128(m, false, 1u128 << 63),
        },
        IntView::Big { neg, bp, len } => {
            if !neg && len == 1 && m.heap_get(bp, 1) == (1u64 << 63) {
                return Some(is_value(m, i64::MIN));
            }
            m.reserve(4);
            let bp = follow_ind(m, m.heap_get(m.args[0], 1));
            con1_id(m, if !neg { CON_IN } else { CON_IP }, bp)
        }
    })
}

#[inline]
fn op_abs(m: &mut Machine) -> Option<Word> {
    let v = force_integer_arg(m, 0)?;
    Some(match view_forced(m, v) {
        IntView::Small(x) => match x.checked_abs() {
            Some(r) => is_value(m, r),
            None => encode_u128(m, false, 1u128 << 63),
        },
        IntView::Big { neg, .. } => {
            if !neg {
                v
            } else {
                m.reserve(4);
                let bp = follow_ind(m, m.heap_get(m.args[0], 1));
                con1_id(m, CON_IP, bp)
            }
        }
    })
}

#[inline]
fn op_signum(m: &mut Machine) -> Option<Word> {
    let v = force_integer_arg(m, 0)?;
    let s = match view_forced(m, v) {
        IntView::Small(x) => x.signum(),
        IntView::Big { neg, .. } => if neg { -1 } else { 1 },
    };
    Some(is_value(m, s))
}

#[inline]
fn op_cmp(m: &mut Machine, kind: CmpKind) -> Option<Word> {
    let (a, b) = force_two(m)?;
    let ord = match (a, b) {
        (IntView::Small(x), IntView::Small(y)) => x.cmp(&y),
        (IntView::Big { neg: an, .. }, IntView::Small(_)) => {
            if an { Ordering::Less } else { Ordering::Greater }
        }
        (IntView::Small(_), IntView::Big { neg: bn, .. }) => {
            if bn { Ordering::Greater } else { Ordering::Less }
        }
        (IntView::Big { neg: an, bp: abp, len: alen }, IntView::Big { neg: bn, bp: bbp, len: blen }) => {
            if an != bn {
                if an { Ordering::Less } else { Ordering::Greater }
            } else {
                let c = unsafe { cmp_mag64(limbs_of(abp, alen), limbs_of(bbp, blen)) };
                if an { c.reverse() } else { c }
            }
        }
    };
    let t = match kind {
        CmpKind::Eq => ord == Ordering::Equal,
        CmpKind::Lt => ord == Ordering::Less,
        CmpKind::Le => ord != Ordering::Greater,
    };
    Some(bool_value(t))
}

#[inline]
fn op_to_int(m: &mut Machine) -> Option<Word> {
    let v = force_integer_arg(m, 0)?;
    let n = match view_forced(m, v) {
        IntView::Small(x) => x,
        IntView::Big { neg, bp, .. } => {
            let low = m.heap_get(bp, 1) as i64;
            if neg { low.wrapping_neg() } else { low }
        }
    };
    Some(int_value(m, n))
}

#[inline]
fn op_from_int(m: &mut Machine) -> Option<Word> {
    let i = int_of_arg(m, 0)?;
    Some(is_value(m, i))
}

// ----- General fallback (and less-hot bit/conversion primitives) --------------------------------

/// An `Integer` argument, forced and decoded.
fn big(m: &mut Machine, k: usize) -> Option<BigInt> {
    let v = force_arg(m, k)?;
    integer_of(m, v)
}

fn int_of_arg(m: &mut Machine, k: usize) -> Option<i64> {
    let v = force_arg(m, k)?;
    Some(m.heap_get(v, 1) as i64)
}

/// Runs an `Integer` primitive, if `name` is one.
pub fn run(m: &mut Machine, name: &str) -> Option<Word> {
    if let Some(op) = IntegerOp::classify(name) {
        return Some(op.run(m));
    }
    let r = match name {
        "integerGcd" | "integerAnd" | "integerOr" | "integerXor" => {
            let a = big(m, 0)?;
            let b = big(m, 1)?;
            let r = match name {
                "integerGcd" => a.gcd(&b),
                "integerAnd" => bitwise(&a, &b, |x, y| x & y),
                "integerOr" => bitwise(&a, &b, |x, y| x | y),
                _ => bitwise(&a, &b, |x, y| x ^ y),
            };
            integer_value(m, r)
        }
        "integerToDouble" => {
            let a = big(m, 0)?;
            super::prims_num::double_value(m, a.to_f64())
        }
        // A BigNat# is the limbs object of an `IP`: an Integer's magnitude, least significant first.
        "integerBigNat" => {
            let a = big(m, 0)?.abs();
            let words = limb64_count(&a);
            m.reserve(words + 2);
            let bp = m.alloc(1 + words);
            m.heap_set(bp, heap::header(kind::BYTES, words as u32, 8 * words as u32));
            for i in 0..words {
                m.heap_set(bp + 8 + 8 * i as Word, limb64(&a, i));
            }
            bp
        }
        "integerFromBigNat" => {
            let bp = force_arg(m, 0)?;
            let words = heap::size_of(m.heap_get(bp, 0)) as usize;
            let limbs: Vec<u64> = (0..words).map(|i| m.heap_get(bp, 1 + i)).collect();
            encode_u64_limbs(m, false, &limbs)
        }
        "bigNatSize" => {
            let bp = force_arg(m, 0)?;
            let words = heap::size_of(m.heap_get(bp, 0)) as i64;
            int_value(m, words)
        }
        "bigNatIndex" => {
            let bp = force_arg(m, 0)?;
            let i = int_of_arg(m, 1)?;
            let words = heap::size_of(m.heap_get(bp, 0)) as i64;
            let limb = if i >= 0 && i < words { m.heap_get(bp, 1 + i as usize) as i64 } else { 0 };
            int_value(m, limb)
        }
        "integerLog2" => {
            let a = big(m, 0)?;
            int_value(m, bit_length(&a) as i64 - 1)
        }
        "integerPopCount" => {
            let a = big(m, 0)?;
            int_value(m, popcount(&a))
        }
        "integerTestBit" => {
            let a = big(m, 0)?;
            let n = int_of_arg(m, 1)?.max(0) as usize;
            bool_value(test_bit(&a, n))
        }
        "integerShiftL" => {
            let a = big(m, 0)?;
            let n = int_of_arg(m, 1)?.max(0) as usize;
            integer_value(m, shift_left(&a, n))
        }
        "integerShiftR" => {
            let a = big(m, 0)?;
            let n = int_of_arg(m, 1)?.max(0) as usize;
            integer_value(m, shift_right(&a, n))
        }
        "integerDecodeDouble" => {
            let v = force_arg(m, 0)?;
            let x = f64::from_bits(m.heap_get(v, 1));
            let (mant, exp) = if x == 0.0 || !x.is_finite() {
                (0i64, 0i64)
            } else {
                let bits = x.to_bits();
                let sign = if bits >> 63 == 1 { -1 } else { 1 };
                let e = ((bits >> 52) & 0x7ff) as i64;
                let frac = (bits & ((1u64 << 52) - 1)) as i64;
                let (mt, ex) = if e == 0 {
                    let z = (frac as u64).leading_zeros() as i64 - 11;
                    (frac << z, -1074 - z)
                } else {
                    (frac | (1 << 52), e - 1075)
                };
                (sign * mt, ex)
            };
            m.reserve(10);
            let mv = is_value(m, mant);
            let ev = int_value(m, exp);
            super::prims_num::con_pair(m, mv, ev)
        }
        "integerEncodeDouble" => {
            let a = big(m, 0)?;
            let e = int_of_arg(m, 1)?;
            let x = crate::interp::bigint::ldexp(a.to_f64(), e);
            super::prims_num::double_value(m, x)
        }
        _ => return None,
    };
    Some(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn to_bigint(limbs: &[u64]) -> BigInt {
        BigInt::from_u64_limbs(false, limbs)
    }

    fn from_bigint(b: &BigInt) -> Vec<u64> {
        (0..limb64_count(b)).map(|i| limb64(b, i)).collect()
    }

    #[test]
    fn base64_arithmetic_matches_bigint() {
        let cases: &[&[u64]] = &[
            &[1],
            &[u64::MAX],
            &[0, 1],
            &[u64::MAX, u64::MAX],
            &[1, 0, u64::MAX],
            &[0xffff_ffff_ffff_fffe, 0xffff_ffff_ffff_ffff, 0x8000_0000_0000_0000],
            &[0x1234_5678_9abc_def0, 0xfedc_ba98_7654_3210, 0x0000_0000_0000_0001, 0x7fff_ffff_ffff_ffff],
            &[0, 0, 0, 0, 1],
            // Classic Knuth D add-back trigger: dividend top == divisor top with large next limb.
            &[0, 0, 0x8000_0000_0000_0000, 0x7fff_ffff_ffff_ffff],
            &[1, u64::MAX, 0x8000_0000_0000_0000],
        ];
        for &a in cases {
            let ba = to_bigint(a);
            for &b in cases {
                let bb = to_bigint(b);
                assert_eq!(add_mag64(a, b), from_bigint(&ba.add(&bb)));
                assert_eq!(mul_mag64(a, b), from_bigint(&ba.mul(&bb)));
                if cmp_mag64(a, b) != Ordering::Less {
                    assert_eq!(sub_mag64(a, b), from_bigint(&ba.sub(&bb)));
                }
                let (q64, r64) = divrem_mag64(a, b);
                let (qb, rb) = ba.quot_rem(&bb);
                assert_eq!(q64, from_bigint(&qb), "quot mismatch for {a:?} / {b:?}");
                assert_eq!(r64, from_bigint(&rb), "rem mismatch for {a:?} / {b:?}");
            }
        }
    }

    #[test]
    fn base64_knuth_d_handles_factorial_and_powers_of_ten() {
        let mut fact = vec![1u64];
        for k in 2..=300u64 {
            fact = mul_mag64(&fact, &[k]);
        }
        let bfact = to_bigint(&fact);
        let mut p10 = vec![1_000_000_000u64];
        for _ in 0..5 {
            p10 = mul_mag64(&p10, &p10);
            let bp10 = to_bigint(&p10);
            let (q64, r64) = divrem_mag64(&fact, &p10);
            let (qb, rb) = bfact.quot_rem(&bp10);
            assert_eq!(q64, from_bigint(&qb));
            assert_eq!(r64, from_bigint(&rb));
        }
    }
}
