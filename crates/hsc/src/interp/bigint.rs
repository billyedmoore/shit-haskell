//! Arbitrary-precision signed integers for `Integer`, in sign-magnitude form over
//! base-2^32 limbs. Only what the Prelude primitives need.

use std::cmp::Ordering;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BigInt {
    /// True for negative values; zero is never negative.
    neg: bool,
    /// Little-endian limbs, with no trailing zero limbs.
    mag: Vec<u32>,
}

fn trim(v: &mut Vec<u32>) {
    while v.last() == Some(&0) {
        v.pop();
    }
}

fn cmp_mag(a: &[u32], b: &[u32]) -> Ordering {
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

fn add_mag(a: &[u32], b: &[u32]) -> Vec<u32> {
    let mut out = Vec::with_capacity(a.len().max(b.len()) + 1);
    let mut carry = 0u64;
    for i in 0..a.len().max(b.len()) {
        let s = *a.get(i).unwrap_or(&0) as u64 + *b.get(i).unwrap_or(&0) as u64 + carry;
        out.push(s as u32);
        carry = s >> 32;
    }
    if carry > 0 {
        out.push(carry as u32);
    }
    out
}

/// `a - b` where `a >= b` in magnitude.
fn sub_mag(a: &[u32], b: &[u32]) -> Vec<u32> {
    let mut out = Vec::with_capacity(a.len());
    let mut borrow = 0i64;
    for i in 0..a.len() {
        let mut d = a[i] as i64 - *b.get(i).unwrap_or(&0) as i64 - borrow;
        if d < 0 {
            d += 1 << 32;
            borrow = 1;
        } else {
            borrow = 0;
        }
        out.push(d as u32);
    }
    trim(&mut out);
    out
}

fn mul_mag(a: &[u32], b: &[u32]) -> Vec<u32> {
    if a.is_empty() || b.is_empty() {
        return Vec::new();
    }
    let mut out = vec![0u32; a.len() + b.len()];
    for (i, &x) in a.iter().enumerate() {
        let mut carry = 0u64;
        for (j, &y) in b.iter().enumerate() {
            let t = out[i + j] as u64 + x as u64 * y as u64 + carry;
            out[i + j] = t as u32;
            carry = t >> 32;
        }
        let mut k = i + b.len();
        while carry > 0 {
            let t = out[k] as u64 + carry;
            out[k] = t as u32;
            carry = t >> 32;
            k += 1;
        }
    }
    trim(&mut out);
    out
}

fn bit_len(a: &[u32]) -> usize {
    match a.last() {
        None => 0,
        Some(&top) => (a.len() - 1) * 32 + (32 - top.leading_zeros() as usize),
    }
}

fn shl_mag(a: &[u32], n: usize) -> Vec<u32> {
    if a.is_empty() {
        return Vec::new();
    }
    let (limbs, bits) = (n / 32, n % 32);
    let mut out = vec![0u32; limbs];
    let mut carry = 0u32;
    for &x in a {
        if bits == 0 {
            out.push(x);
        } else {
            out.push((x << bits) | carry);
            carry = x >> (32 - bits);
        }
    }
    if carry > 0 {
        out.push(carry);
    }
    trim(&mut out);
    out
}

fn shr1_mag(a: &mut Vec<u32>) {
    let mut carry = 0u32;
    for x in a.iter_mut().rev() {
        let new_carry = *x & 1;
        *x = (*x >> 1) | (carry << 31);
        carry = new_carry;
    }
    trim(a);
}

/// Truncated division of magnitudes by binary long division.
fn divrem_mag(a: &[u32], b: &[u32]) -> (Vec<u32>, Vec<u32>) {
    if cmp_mag(a, b) == Ordering::Less {
        return (Vec::new(), a.to_vec());
    }
    if b.len() == 1 {
        let d = b[0] as u64;
        let mut q = vec![0u32; a.len()];
        let mut rem = 0u64;
        for i in (0..a.len()).rev() {
            let cur = (rem << 32) | a[i] as u64;
            q[i] = (cur / d) as u32;
            rem = cur % d;
        }
        trim(&mut q);
        let mut r = vec![rem as u32];
        trim(&mut r);
        return (q, r);
    }
    let shift = bit_len(a) - bit_len(b);
    let mut divisor = shl_mag(b, shift);
    let mut rem = a.to_vec();
    let mut quot = vec![0u32; shift / 32 + 1];
    for s in (0..=shift).rev() {
        if cmp_mag(&rem, &divisor) != Ordering::Less {
            rem = sub_mag(&rem, &divisor);
            quot[s / 32] |= 1 << (s % 32);
        }
        shr1_mag(&mut divisor);
    }
    trim(&mut quot);
    (quot, rem)
}

impl BigInt {
    pub fn zero() -> BigInt {
        BigInt {
            neg: false,
            mag: Vec::new(),
        }
    }

    fn from_parts(neg: bool, mut mag: Vec<u32>) -> BigInt {
        trim(&mut mag);
        let neg = neg && !mag.is_empty();
        BigInt { neg, mag }
    }

    pub fn from_i64(v: i64) -> BigInt {
        let neg = v < 0;
        let m = v.unsigned_abs();
        BigInt::from_parts(neg, vec![m as u32, (m >> 32) as u32])
    }

    pub fn from_u64(v: u64) -> BigInt {
        BigInt::from_parts(false, vec![v as u32, (v >> 32) as u32])
    }

    /// The value as an `i64`, wrapping modulo 2^64 like `fromInteger` for `Int`.
    pub fn to_i64_wrapping(&self) -> i64 {
        let lo = *self.mag.first().unwrap_or(&0) as u64;
        let hi = *self.mag.get(1).unwrap_or(&0) as u64;
        let m = lo | (hi << 32);
        if self.neg {
            (m as i64).wrapping_neg()
        } else {
            m as i64
        }
    }

    pub fn to_i64(&self) -> Option<i64> {
        if self.mag.len() > 2 {
            return None;
        }
        let m = self.to_i64_wrapping();
        let back = BigInt::from_i64(m);
        if back == *self {
            Some(m)
        } else {
            None
        }
    }

    pub fn is_neg(&self) -> bool {
        self.neg
    }

    pub fn is_zero(&self) -> bool {
        self.mag.is_empty()
    }

    /// Parses an optionally negative decimal literal.
    pub fn parse(s: &str) -> Option<BigInt> {
        let (neg, digits) = match s.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, s),
        };
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let mut mag: Vec<u32> = Vec::new();
        for chunk in digits.as_bytes().chunks(9) {
            let v: u64 = std::str::from_utf8(chunk).ok()?.parse().ok()?;
            let scale = 10u64.pow(chunk.len() as u32);
            // mag = mag * scale + v
            let mut carry = v;
            for limb in mag.iter_mut() {
                let t = *limb as u64 * scale + carry;
                *limb = t as u32;
                carry = t >> 32;
            }
            while carry > 0 {
                mag.push(carry as u32);
                carry >>= 32;
            }
        }
        Some(BigInt::from_parts(neg, mag))
    }

    pub fn to_decimal(&self) -> String {
        if self.mag.is_empty() {
            return "0".into();
        }
        let mut chunks = Vec::new();
        let mut cur = self.mag.clone();
        while !cur.is_empty() {
            let (q, r) = divrem_mag(&cur, &[1_000_000_000]);
            chunks.push(r.first().copied().unwrap_or(0));
            cur = q;
        }
        let mut s = String::new();
        if self.neg {
            s.push('-');
        }
        s.push_str(&chunks.last().expect("nonzero").to_string());
        for c in chunks.iter().rev().skip(1) {
            s.push_str(&format!("{c:09}"));
        }
        s
    }

    pub fn neg(&self) -> BigInt {
        BigInt::from_parts(!self.neg, self.mag.clone())
    }

    pub fn abs(&self) -> BigInt {
        BigInt::from_parts(false, self.mag.clone())
    }

    pub fn add(&self, o: &BigInt) -> BigInt {
        if self.neg == o.neg {
            return BigInt::from_parts(self.neg, add_mag(&self.mag, &o.mag));
        }
        match cmp_mag(&self.mag, &o.mag) {
            Ordering::Less => BigInt::from_parts(o.neg, sub_mag(&o.mag, &self.mag)),
            _ => BigInt::from_parts(self.neg, sub_mag(&self.mag, &o.mag)),
        }
    }

    pub fn sub(&self, o: &BigInt) -> BigInt {
        self.add(&o.neg())
    }

    pub fn mul(&self, o: &BigInt) -> BigInt {
        BigInt::from_parts(self.neg != o.neg, mul_mag(&self.mag, &o.mag))
    }

    /// Truncated division: quotient rounds toward zero, remainder has the dividend's sign.
    /// Panics on division by zero; callers check first.
    pub fn quot_rem(&self, o: &BigInt) -> (BigInt, BigInt) {
        let (q, r) = divrem_mag(&self.mag, &o.mag);
        (
            BigInt::from_parts(self.neg != o.neg, q),
            BigInt::from_parts(self.neg, r),
        )
    }

    /// Floored division, as `div` and `mod` in Haskell.
    pub fn div_mod(&self, o: &BigInt) -> (BigInt, BigInt) {
        let (q, r) = self.quot_rem(o);
        if !r.is_zero() && (r.neg != o.neg) {
            (q.sub(&BigInt::from_i64(1)), r.add(o))
        } else {
            (q, r)
        }
    }

    pub fn gcd(&self, o: &BigInt) -> BigInt {
        let mut a = self.abs();
        let mut b = o.abs();
        while !b.is_zero() {
            let (_, r) = a.quot_rem(&b);
            a = b;
            b = r;
        }
        a
    }

    pub fn pow10(n: u32) -> BigInt {
        let mut r = BigInt::from_i64(1);
        let ten = BigInt::from_i64(10);
        for _ in 0..n {
            r = r.mul(&ten);
        }
        r
    }

    /// Nearest double, by way of the top 64 significant bits.
    pub fn to_f64(&self) -> f64 {
        let bits = bit_len(&self.mag);
        let v = if bits <= 64 {
            let lo = *self.mag.first().unwrap_or(&0) as u64;
            let hi = *self.mag.get(1).unwrap_or(&0) as u64;
            (lo | (hi << 32)) as f64
        } else {
            let shift = bits - 64;
            let top = shl_mag_right(&self.mag, shift);
            let lo = *top.first().unwrap_or(&0) as u64;
            let hi = *top.get(1).unwrap_or(&0) as u64;
            ((lo | (hi << 32)) as f64) * 2f64.powi(shift as i32)
        };
        if self.neg {
            -v
        } else {
            v
        }
    }

    pub fn cmp_value(&self, o: &BigInt) -> Ordering {
        match (self.neg, o.neg) {
            (false, true) => Ordering::Greater,
            (true, false) => Ordering::Less,
            (false, false) => cmp_mag(&self.mag, &o.mag),
            (true, true) => cmp_mag(&o.mag, &self.mag),
        }
    }
}

fn shl_mag_right(a: &[u32], n: usize) -> Vec<u32> {
    let (limbs, bits) = (n / 32, n % 32);
    let mut out: Vec<u32> = a.iter().skip(limbs).copied().collect();
    if bits > 0 {
        for i in 0..out.len() {
            let hi = out.get(i + 1).copied().unwrap_or(0);
            out[i] = (out[i] >> bits) | (hi << (32 - bits));
        }
    }
    trim(&mut out);
    out
}

/// Exposed for the shifting primitives.
pub fn shift_left(a: &BigInt, n: usize) -> BigInt {
    BigInt::from_parts(a.neg, shl_mag(&a.mag, n))
}

/// Two's-complement limbs of `a`, `len` limbs long (sign-extended).
fn twos(a: &BigInt, len: usize) -> Vec<u32> {
    let mut v: Vec<u32> = a.mag.clone();
    v.resize(len, 0);
    if a.neg {
        // -x = !(x - 1) for the magnitude x.
        let mut borrow = true;
        for limb in v.iter_mut() {
            if borrow {
                let (r, b) = limb.overflowing_sub(1);
                *limb = r;
                borrow = b;
            }
        }
        for limb in v.iter_mut() {
            *limb = !*limb;
        }
    }
    v
}

/// The value of two's-complement limbs.
fn from_twos(mut v: Vec<u32>) -> BigInt {
    let neg = v.last().is_some_and(|top| top & 0x8000_0000 != 0);
    if neg {
        for limb in v.iter_mut() {
            *limb = !*limb;
        }
        let mut carry = true;
        for limb in v.iter_mut() {
            if carry {
                let (r, c) = limb.overflowing_add(1);
                *limb = r;
                carry = c;
            }
        }
    }
    trim(&mut v);
    BigInt::from_parts(neg && !v.is_empty(), v)
}

/// Bitwise and, or, xor on arbitrary integers (two's complement, as GHC defines them).
pub fn bitwise(a: &BigInt, b: &BigInt, op: fn(u32, u32) -> u32) -> BigInt {
    let len = a.mag.len().max(b.mag.len()) + 1;
    let x = twos(a, len);
    let y = twos(b, len);
    from_twos(x.iter().zip(&y).map(|(p, q)| op(*p, *q)).collect())
}

/// Arithmetic shift right: floor division by 2^n.
pub fn shift_right(a: &BigInt, n: usize) -> BigInt {
    let divisor = shift_left(&BigInt::from_i64(1), n);
    a.div_mod(&divisor).0
}

/// The number of bits of the magnitude (0 for zero).
pub fn bit_length(a: &BigInt) -> usize {
    match a.mag.last() {
        None => 0,
        Some(top) => (a.mag.len() - 1) * 32 + (32 - top.leading_zeros() as usize),
    }
}

/// The number of 64-bit words of the magnitude.
pub fn limb64_count(a: &BigInt) -> usize {
    a.mag.len().div_ceil(2)
}

/// The i-th 64-bit word of the magnitude (0 beyond its end).
pub fn limb64(a: &BigInt, i: usize) -> u64 {
    let lo = a.mag.get(2 * i).copied().unwrap_or(0) as u64;
    let hi = a.mag.get(2 * i + 1).copied().unwrap_or(0) as u64;
    lo | (hi << 32)
}

/// The number of one bits of the magnitude.
pub fn popcount(a: &BigInt) -> usize {
    a.mag.iter().map(|l| l.count_ones() as usize).sum()
}

/// Whether bit `n` of the two's-complement value is set.
pub fn test_bit(a: &BigInt, n: usize) -> bool {
    let len = (n / 32 + 1).max(a.mag.len()) + 1;
    let limbs = twos(a, len);
    limbs[n / 32] & (1 << (n % 32)) != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimal_roundtrip() {
        let s = "-123456789012345678901234567890";
        assert_eq!(BigInt::parse(s).unwrap().to_decimal(), s);
    }

    #[test]
    fn arithmetic() {
        let a = BigInt::parse("100000000000000000000").unwrap();
        let b = BigInt::parse("333333333").unwrap();
        assert_eq!(a.mul(&b).to_decimal(), "33333333300000000000000000000");
        let (q, r) = a.quot_rem(&b);
        assert_eq!(q.mul(&b).add(&r), a);
        let (dq, dr) = BigInt::from_i64(-7).div_mod(&BigInt::from_i64(2));
        assert_eq!((dq.to_i64(), dr.to_i64()), (Some(-4), Some(1)));
    }
}

#[cfg(test)]
mod bit_tests {
    use super::*;

    #[test]
    fn bitwise_matches_i64() {
        for &(x, y) in &[(12i64, 10i64), (-12, 10), (-7, -3), (5, -1)] {
            let a = BigInt::from_i64(x);
            let b = BigInt::from_i64(y);
            assert_eq!(bitwise(&a, &b, |p, q| p & q).to_i64(), Some(x & y));
            assert_eq!(bitwise(&a, &b, |p, q| p | q).to_i64(), Some(x | y));
            assert_eq!(bitwise(&a, &b, |p, q| p ^ q).to_i64(), Some(x ^ y));
        }
    }

    #[test]
    fn shifts_and_bits() {
        assert_eq!(shift_right(&BigInt::from_i64(-5), 1).to_i64(), Some(-3));
        assert_eq!(shift_right(&BigInt::from_i64(40), 3).to_i64(), Some(5));
        assert_eq!(bit_length(&BigInt::from_i64(1024)), 11);
        assert_eq!(popcount(&BigInt::from_i64(255)), 8);
        assert!(test_bit(&BigInt::from_i64(-1), 100));
        assert!(!test_bit(&BigInt::from_i64(4), 0));
        assert!(test_bit(&BigInt::from_i64(4), 2));
    }
}
