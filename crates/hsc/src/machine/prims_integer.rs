//! `Integer` primitives of the bignum stand-in. An `Integer` is an `IS` box (a machine word), or
//! `IP`/`IN` with the magnitude's limbs; the operations decode both operands, compute with `BigInt`,
//! and encode the result.

use super::heap::Word;
use super::prims::{force_arg, int_value, integer_of, integer_value};
use super::Machine;
use crate::interp::bigint::{bit_length, bitwise, limb64, limb64_count, popcount, shift_left, shift_right, test_bit, BigInt};

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
    let r = match name {
        "integerAdd" | "integerSub" | "integerMul" | "integerQuot" | "integerRem" | "integerDiv" | "integerMod"
        | "integerGcd" | "integerAnd" | "integerOr" | "integerXor" => {
            let a = big(m, 0)?;
            let b = big(m, 1)?;
            let r = match name {
                "integerAdd" => a.add(&b),
                "integerSub" => a.sub(&b),
                "integerMul" => a.mul(&b),
                "integerQuot" => {
                    if b.is_zero() {
                        m.exc = m.error_value("divide by zero");
                        return Some(0);
                    }
                    a.quot_rem(&b).0
                }
                "integerRem" => {
                    if b.is_zero() {
                        m.exc = m.error_value("divide by zero");
                        return Some(0);
                    }
                    a.quot_rem(&b).1
                }
                "integerDiv" => {
                    if b.is_zero() {
                        m.exc = m.error_value("divide by zero");
                        return Some(0);
                    }
                    a.div_mod(&b).0
                }
                "integerMod" => {
                    if b.is_zero() {
                        m.exc = m.error_value("divide by zero");
                        return Some(0);
                    }
                    a.div_mod(&b).1
                }
                "integerGcd" => a.gcd(&b),
                "integerAnd" => bitwise(&a, &b, |x, y| x & y),
                "integerOr" => bitwise(&a, &b, |x, y| x | y),
                _ => bitwise(&a, &b, |x, y| x ^ y),
            };
            integer_value(m, r)
        }
        "integerNegate" => {
            let a = big(m, 0)?;
            integer_value(m, a.neg())
        }
        "integerAbs" => {
            let a = big(m, 0)?;
            integer_value(m, a.abs())
        }
        "integerSignum" => {
            let a = big(m, 0)?;
            let s = if a.is_zero() { 0 } else if a.is_neg() { -1 } else { 1 };
            integer_value(m, BigInt::from_i64(s))
        }
        "integerEq" | "integerLt" | "integerLe" => {
            let a = big(m, 0)?;
            let b = big(m, 1)?;
            let ord = a.cmp_value(&b);
            let t = match name {
                "integerEq" => ord == std::cmp::Ordering::Equal,
                "integerEq" => ord == std::cmp::Ordering::Equal,
                "integerLt" => ord == std::cmp::Ordering::Less,
                _ => ord != std::cmp::Ordering::Greater,
            };
            // Bool: the constructors of GHC.Types.
            let c = if t { "GHC.Types.True" } else { "GHC.Types.False" };
            super::prims::con_value(m, c, 0, &[])
        }
        "integerToInt" => {
            let a = big(m, 0)?;
            int_value(m, a.to_i64_wrapping())
        }
        "intToInteger" => {
            let i = int_of_arg(m, 0)?;
            integer_value(m, BigInt::from_i64(i))
        }
        "integerToDouble" => {
            let a = big(m, 0)?;
            super::prims_num::double_value(m, a.to_f64())
        }
        // A BigNat# is the limbs object of an `IP`: an Integer's magnitude, least significant first.
        "integerBigNat" => {
            let a = big(m, 0)?.abs();
            let limbs: Vec<u64> = (0..limb64_count(&a)).map(|i| limb64(&a, i)).collect();
            let words = limbs.len();
            m.reserve(words + 2);
            let bp = m.alloc(1 + words);
            m.heap_set(bp, super::heap::header(super::heap::kind::BYTES, words as u32, 8 * words as u32));
            for (i, l) in limbs.iter().enumerate() {
                m.heap_set(bp + 8 + 8 * i as Word, *l);
            }
            bp
        }
        "integerFromBigNat" => {
            let bp = force_arg(m, 0)?;
            let words = super::heap::size_of(m.heap_get(bp, 0)) as usize;
            let limbs: Vec<u64> = (0..words).map(|i| m.heap_get(bp, 1 + i)).collect();
            integer_value(m, BigInt::from_u64_limbs(false, &limbs))
        }
        "bigNatSize" => {
            // The limbs of a BigNat (the byte array an `IP`/`IN` holds): their count.
            let bp = force_arg(m, 0)?;
            let words = super::heap::size_of(m.heap_get(bp, 0)) as i64;
            int_value(m, words)
        }
        "bigNatIndex" => {
            // The limb at an index, least significant first; 0 outside the number.
            let bp = force_arg(m, 0)?;
            let i = int_of_arg(m, 1)?;
            let words = super::heap::size_of(m.heap_get(bp, 0)) as i64;
            let limb = if i >= 0 && i < words { m.heap_get(bp, 1 + i as usize) as i64 } else { 0 };
            int_value(m, limb)
        }
        "integerLog2" => {
            let a = big(m, 0)?;
            int_value(m, bit_length(&a) as i64 - 1)
        }
        "integerPopCount" => {
            let a = big(m, 0)?;
            int_value(m, popcount(&a) as i64)
        }
        "integerTestBit" => {
            let a = big(m, 0)?;
            let n = int_of_arg(m, 1)?.max(0) as usize;
            // The declared result is a Bool: the constructor, not a number.
            let c = if test_bit(&a, n) { "GHC.Types.True" } else { "GHC.Types.False" };
            super::prims::con_value(m, c, 0, &[])
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
                let (mt, ex) = if e == 0 { { let z = (frac as u64).leading_zeros() as i64 - 11; ((frac << z), -1074 - z) } } else { (frac | (1 << 52), e - 1075) };
                (sign * mt, ex)
            };
            m.reserve(10);
            let mv = integer_value(m, BigInt::from_i64(mant));
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
