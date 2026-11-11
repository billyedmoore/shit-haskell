//! Numeric primitive operations: `Int#`, `Word#`, `Char#`, `Double#` and `Float#` (the GHC primop
//! names), the sized conversions, and the floating classifiers base imports. Each answers a new
//! object; a comparison answers an `Int#` 0 or 1.

use super::heap::{self, kind, Word};
use super::prims::{char_value, force_arg, int_value};
use super::Machine;
use crate::native::codegen::DOUBLE;

/// An `Int#` argument (forced).
fn i(m: &mut Machine, k: usize) -> Option<i64> {
    let v = force_arg(m, k)?;
    Some(m.heap_get(v, 1) as i64)
}

/// A `Word#` argument (forced): the same bits as its `Int#` representation.
fn w(m: &mut Machine, k: usize) -> Option<u64> {
    i(m, k).map(|x| x as u64)
}

/// A `Char#` argument (forced), as its code point.
fn c(m: &mut Machine, k: usize) -> Option<u32> {
    let v = force_arg(m, k)?;
    Some(m.heap_get(v, 1) as u32)
}

/// A `Double#` (or `Float#`) argument (forced).
fn d(m: &mut Machine, k: usize) -> Option<f64> {
    let v = force_arg(m, k)?;
    Some(f64::from_bits(m.heap_get(v, 1)))
}

/// A new `Double#` object.
pub fn double_value(m: &mut Machine, x: f64) -> Word {
    let p = m.alloc(2);
    m.heap_set(p, heap::header(DOUBLE, 1, 0));
    m.heap_set(p + 8, x.to_bits());
    p
}

fn int(m: &mut Machine, x: i64) -> Word {
    int_value(m, x)
}

fn bool_int(m: &mut Machine, b: bool) -> Word {
    int_value(m, b as i64)
}

/// Runs a numeric primitive, if `name` is one. The arguments are in the record.
pub fn run(m: &mut Machine, name: &str) -> Option<Word> {
    let r = match name {
        // ----- Int# -----
        "+#" | "-#" | "*#" | "quotInt#" | "remInt#" | "andI#" | "orI#" | "xorI#" | "==#" | "/=#" | "<#"
        | "<=#" | ">#" | ">=#" | "uncheckedIShiftL#" | "uncheckedIShiftRA#" | "uncheckedIShiftRL#" => {
            let (Some(a), Some(b)) = (i(m, 0), i(m, 1)) else { return Some(0) };
            match name {
                "+#" => int(m, a.wrapping_add(b)),
                "-#" => int(m, a.wrapping_sub(b)),
                "*#" => int(m, a.wrapping_mul(b)),
                "quotInt#" => {
                    if b == 0 {
                        return Some(0);
                    }
                    int(m, a.wrapping_div(b))
                }
                "remInt#" => {
                    if b == 0 {
                        return Some(0);
                    }
                    int(m, a.wrapping_rem(b))
                }
                "andI#" => int(m, a & b),
                "orI#" => int(m, a | b),
                "xorI#" => int(m, a ^ b),
                "==#" => bool_int(m, a == b),
                "/=#" => bool_int(m, a != b),
                "<#" => bool_int(m, a < b),
                "<=#" => bool_int(m, a <= b),
                ">#" => bool_int(m, a > b),
                ">=#" => bool_int(m, a >= b),
                "uncheckedIShiftL#" => int(m, a.wrapping_shl(b as u32)),
                "uncheckedIShiftRA#" => int(m, a.wrapping_shr(b as u32)),
                _ => int(m, ((a as u64).wrapping_shr(b as u32)) as i64),
            }
        }
        "quotRemInt#" | "quotRemWord#" => {
            let (Some(a), Some(b)) = (i(m, 0), i(m, 1)) else { return Some(0) };
            if b == 0 {
                return Some(0);
            }
            let (q, r) = if name == "quotRemInt#" {
                (a.wrapping_div(b), a.wrapping_rem(b))
            } else {
                (((a as u64) / (b as u64)) as i64, ((a as u64) % (b as u64)) as i64)
            };
            let qv = int(m, q);
            let rv = int(m, r);
            con_pair(m, qv, rv)
        }
        "addIntC#" | "subIntC#" => {
            let (Some(a), Some(b)) = (i(m, 0), i(m, 1)) else { return Some(0) };
            let (r, o) = if name == "addIntC#" { a.overflowing_add(b) } else { a.overflowing_sub(b) };
            let rv = int(m, r);
            let ov = int(m, o as i64);
            con_pair(m, rv, ov)
        }
        "mulIntMayOflo#" => {
            let (Some(a), Some(b)) = (i(m, 0), i(m, 1)) else { return Some(0) };
            let r = (a as i128) * (b as i128);
            let flag = (r > i64::MAX as i128 || r < i64::MIN as i128) as i64;
            int(m, flag)
        }
        "negateInt#" | "notI#" | "not#" | "int2Word#" | "word2Int#" | "chr#" | "ord#" => {
            let Some(a) = i(m, 0) else { return Some(0) };
            match name {
                "negateInt#" => int(m, a.wrapping_neg()),
                "notI#" | "not#" => int(m, !a),
                "ord#" => int(m, a),
                _ => int(m, a),
            }
        }
        "chr#" => {
            let Some(a) = i(m, 0) else { return Some(0) };
            char_value(m, char::from_u32(a as u32).unwrap_or('\u{fffd}'))
        }
        // ----- Word# -----
        "plusWord#" | "minusWord#" | "timesWord#" | "quotWord#" | "remWord#" | "and#" | "or#" | "xor#"
        | "eqWord#" | "neWord#" | "ltWord#" | "leWord#" | "gtWord#" | "geWord#" | "uncheckedShiftL#"
        | "uncheckedShiftRL#" => {
            let (Some(a), Some(b)) = (w(m, 0), w(m, 1)) else { return Some(0) };
            match name {
                "plusWord#" => int(m, a.wrapping_add(b) as i64),
                "minusWord#" => int(m, a.wrapping_sub(b) as i64),
                "timesWord#" => int(m, a.wrapping_mul(b) as i64),
                "quotWord#" | "remWord#" => {
                    if b == 0 {
                        return Some(0);
                    }
                    int(m, if name == "quotWord#" { a / b } else { a % b } as i64)
                }
                "and#" => int(m, (a & b) as i64),
                "or#" => int(m, (a | b) as i64),
                "xor#" => int(m, (a ^ b) as i64),
                "eqWord#" => bool_int(m, a == b),
                "neWord#" => bool_int(m, a != b),
                "ltWord#" => bool_int(m, a < b),
                "leWord#" => bool_int(m, a <= b),
                "gtWord#" => bool_int(m, a > b),
                "geWord#" => bool_int(m, a >= b),
                "uncheckedShiftL#" => int(m, a.wrapping_shl(b as u32) as i64),
                _ => int(m, a.wrapping_shr(b as u32) as i64),
            }
        }
        "not#w" => {
            let Some(a) = w(m, 0) else { return Some(0) };
            int(m, !a as i64)
        }
        "word2Double#" => {
            let Some(a) = w(m, 0) else { return Some(0) };
            double_value(m, a as f64)
        }
        "int2Double#" | "int2Float#" => {
            let Some(a) = i(m, 0) else { return Some(0) };
            let x = if name == "int2Float#" { a as f32 as f64 } else { a as f64 };
            double_value(m, x)
        }
        "double2Int#" | "float2Int#" => {
            let Some(x) = d(m, 0) else { return Some(0) };
            int(m, x as i64)
        }
        n if crate::interp::prims_ghc::bit_op(n, 0).is_some() => {
            let Some(a) = w(m, 0) else { return Some(0) };
            int(m, crate::interp::prims_ghc::bit_op(n, a).unwrap_or(0))
        }
        "bitReverse#" => {
            let Some(a) = w(m, 0) else { return Some(0) };
            int(m, a.reverse_bits() as i64)
        }
        "byteSwap#" | "byteSwap64#" => {
            let Some(a) = w(m, 0) else { return Some(0) };
            int(m, a.swap_bytes() as i64)
        }
        "popCnt#" | "popCnt64#" => {
            let Some(a) = w(m, 0) else { return Some(0) };
            int(m, a.count_ones() as i64)
        }
        "clz#" | "clz64#" => {
            let Some(a) = w(m, 0) else { return Some(0) };
            int(m, a.leading_zeros() as i64)
        }
        "ctz#" | "ctz64#" => {
            let Some(a) = w(m, 0) else { return Some(0) };
            int(m, a.trailing_zeros() as i64)
        }
        // ----- Char# -----
        "eqChar#" | "neChar#" | "ltChar#" | "leChar#" | "gtChar#" | "geChar#" => {
            let (Some(a), Some(b)) = (c(m, 0), c(m, 1)) else { return Some(0) };
            match name {
                "eqChar#" => bool_int(m, a == b),
                "neChar#" => bool_int(m, a != b),
                "ltChar#" => bool_int(m, a < b),
                "leChar#" => bool_int(m, a <= b),
                "gtChar#" => bool_int(m, a > b),
                _ => bool_int(m, a >= b),
            }
        }
        // ----- Float# (the named primops; each result is rounded to single precision) -----
        "plusFloat#" | "minusFloat#" | "timesFloat#" | "divideFloat#" | "powerFloat#" => {
            let (Some(a), Some(b)) = (d(m, 0), d(m, 1)) else { return Some(0) };
            let x = match name {
                "plusFloat#" => a + b,
                "minusFloat#" => a - b,
                "timesFloat#" => a * b,
                "divideFloat#" => a / b,
                _ => a.powf(b),
            };
            double_value(m, x as f32 as f64)
        }
        "eqFloat#" | "neFloat#" | "ltFloat#" | "leFloat#" | "gtFloat#" | "geFloat#" => {
            let (Some(a), Some(b)) = (d(m, 0), d(m, 1)) else { return Some(0) };
            let t = match name {
                "eqFloat#" => a == b,
                "neFloat#" => a != b,
                "ltFloat#" => a < b,
                "leFloat#" => a <= b,
                "gtFloat#" => a > b,
                _ => a >= b,
            };
            bool_int(m, t)
        }
        "word2Float#" => {
            let Some(v) = force_arg(m, 0) else { return Some(0) };
            let w = m.heap_get(v, 1);
            double_value(m, w as f32 as f64)
        }
        // ----- Double# and Float# -----
        "+##" | "-##" | "*##" | "/##" | "**##" | "==##" | "/=##" | "<##" | "<=##" | ">##" | ">=##" | "+#f"
        | "-#f" | "*#f" | "/#f" | "==#f" | "/=#f" | "<#f" | "<=#f" | ">#f" | ">=#f" => {
            let (Some(a), Some(b)) = (d(m, 0), d(m, 1)) else { return Some(0) };
            let single = name.ends_with("#f");
            let base = name.trim_end_matches("#f").trim_end_matches("##");
            let r = match base {
                "+" => Some(a + b),
                "-" => Some(a - b),
                "*" => Some(a * b),
                "/" => Some(a / b),
                "**" => Some(a.powf(b)),
                _ => None,
            };
            match r {
                Some(x) => {
                    let x = if single { x as f32 as f64 } else { x };
                    double_value(m, x)
                }
                None => {
                    let t = match base {
                        "==" => a == b,
                        "/=" => a != b,
                        "<" => a < b,
                        "<=" => a <= b,
                        ">" => a > b,
                        _ => a >= b,
                    };
                    bool_int(m, t)
                }
            }
        }
        "negateDouble#" | "negateFloat#" | "fabsDouble#" | "fabsFloat#" | "sqrtDouble#" | "sqrtFloat#"
        | "expDouble#" | "expFloat#" | "logDouble#" | "logFloat#" | "sinDouble#" | "sinFloat#"
        | "cosDouble#" | "cosFloat#" | "tanDouble#" | "tanFloat#" | "asinDouble#" | "asinFloat#"
        | "acosDouble#" | "acosFloat#" | "atanDouble#" | "atanFloat#" | "sinhDouble#" | "sinhFloat#"
        | "coshDouble#" | "coshFloat#" | "tanhDouble#" | "tanhFloat#" | "asinhDouble#" | "asinhFloat#"
        | "acoshDouble#" | "acoshFloat#" | "atanhDouble#" | "atanhFloat#" | "float2Double#"
        | "double2Float#" => {
            let Some(x) = d(m, 0) else { return Some(0) };
            let op = match name {
                "float2Double#" => "float2Double",
                "double2Float#" => "double2Float",
                _ => name.trim_end_matches("Double#").trim_end_matches("Float#").trim_end_matches('#'),
            };
            let r = match op {
                "negate" => -x,
                "fabs" => x.abs(),
                "sqrt" => x.sqrt(),
                "exp" => x.exp(),
                "log" => x.ln(),
                "sin" => x.sin(),
                "cos" => x.cos(),
                "tan" => x.tan(),
                "asin" => x.asin(),
                "acos" => x.acos(),
                "atan" => x.atan(),
                "sinh" => x.sinh(),
                "cosh" => x.cosh(),
                "tanh" => x.tanh(),
                "asinh" => crate::interp::prims_ghc::libm("asinh", x),
                "acosh" => crate::interp::prims_ghc::libm("acosh", x),
                "atanh" => crate::interp::prims_ghc::libm("atanh", x),
                "float2Double" => x,
                "double2Float" => x as f32 as f64,
                _ => x,
            };
            double_value(m, r)
        }
        "decodeDouble_Int64#" => {
            let Some(x) = d(m, 0) else { return Some(0) };
            let bits = x.to_bits();
            let (mant, exp) = if x == 0.0 || !x.is_finite() {
                (0i64, 0i64)
            } else {
                let sign = if bits >> 63 == 1 { -1 } else { 1 };
                let e = ((bits >> 52) & 0x7ff) as i64;
                let frac = (bits & ((1u64 << 52) - 1)) as i64;
                let (mt, ex) = if e == 0 { { let z = (frac as u64).leading_zeros() as i64 - 11; ((frac << z), -1074 - z) } } else { (frac | (1 << 52), e - 1075) };
                (sign * mt, ex)
            };
            m.reserve(7);
            let mv = int(m, mant);
            let ev = int(m, exp);
            con_pair(m, mv, ev)
        }
        "decodeFloat_Int#" => {
            let Some(x) = d(m, 0) else { return Some(0) };
            let x = x as f32;
            let (mant, exp) = if x == 0.0 || !x.is_finite() {
                (0i64, 0i64)
            } else {
                let bits = x.to_bits();
                let sign: i64 = if bits >> 31 == 1 { -1 } else { 1 };
                let e = ((bits >> 23) & 0xff) as i64;
                let frac = (bits & ((1u32 << 23) - 1)) as i64;
                let (mt, ex) = if e == 0 { { let z = (frac as u32).leading_zeros() as i64 - 8; (frac << z, -149 - z) } } else { (frac | (1 << 23), e - 150) };
                (sign * mt, ex)
            };
            m.reserve(7);
            let mv = int(m, mant);
            let ev = int(m, exp);
            con_pair(m, mv, ev)
        }
        _ => return narrow(m, name),
    };
    Some(r)
}

/// Sized integer conversions: `intToInt8#` and friends, which truncate and sign- or zero-extend.
fn narrow(m: &mut Machine, name: &str) -> Option<Word> {
    let (bits, signed, from_word) = match name {
        "intToInt8#" | "int8ToInt#" | "int8ToWord8#" | "word8ToInt8#" => (8, true, false),
        "intToInt16#" | "int16ToInt#" => (16, true, false),
        "intToInt32#" | "int32ToInt#" => (32, true, false),
        "intToInt64#" | "int64ToInt#" => (64, true, false),
        "narrow8Word#" | "wordToWord8#" | "word8ToWord#" => (8, false, true),
        "narrow16Word#" => (16, false, true),
        "narrow32Word#" => (32, false, true),
        "wordToWord16#" | "word16ToWord#" => (16, false, true),
        "wordToWord32#" | "word32ToWord#" => (32, false, true),
        "wordToWord64#" | "word64ToWord#" => (64, false, true),
        _ => return None,
    };
    let v = i(m, 0)?;
    let _ = from_word;
    let r = match (bits, signed) {
        (64, _) => v,
        (b, true) => (v << (64 - b)) >> (64 - b),
        (b, false) => ((v as u64) & ((1u64 << b) - 1)) as i64,
    };
    Some(int(m, r))
}

/// A pair (# a, b #) of two objects.
pub fn con_pair(m: &mut Machine, a: Word, b: Word) -> Word {
    super::prims::con_value(m, "Builtin.(,)", 2, &[a, b])
}

#[allow(dead_code)]
fn unused_kind() -> u8 {
    kind::INT
}
