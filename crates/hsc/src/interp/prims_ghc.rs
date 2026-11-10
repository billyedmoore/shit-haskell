//! The GHC primitive operations on unboxed numbers and characters (GHC.Prim), by their GHC names.
//! `prims::call` falls back to this table. Int#, Word# and Char# values are `Value::Int` and
//! `Value::Char`; Double# and Float# are `Value::Double` (a Float# is held at single precision).
//! Comparisons answer Int# (1 or 0), and an unboxed tuple result is the tuple constructor.

use std::cell::RefCell;
use std::rc::Rc;

use super::prims::{chr_of, double_of, int_of, io_result_of};
use super::{con_value, new_cell, string_value, unit_value, Cell, ConVal, RR, Runtime, RtError, Thunk, Value};

/// A state-passing result `(# State# s, v #)`.
fn state_result(v: Value) -> RR {
    Ok(io_result_of(v))
}

/// The unboxed pair `(# a, b #)` of cells, the second component left unevaluated.
fn pair_cells(a: Cell, b: Cell) -> Value {
    Value::Con(Rc::new(ConVal { name: "Builtin.(,)", args: vec![a, b] }))
}

fn unit_cell() -> Cell {
    new_cell(Thunk::Done(unit_value()))
}

fn cell_of(v: Value) -> Cell {
    new_cell(Thunk::Done(v))
}

/// Runs an IO action (a state-passing function) on the state token.
fn run_io(rt: &Runtime, io: &Cell, state: &Cell) -> RR {
    let f = rt.force(io)?;
    rt.apply(f, state.clone())
}

/// The `ErrorCall` exception for a runtime error message, built by the library's own function.
fn error_call(rt: &Runtime, msg: &str) -> Option<Value> {
    // A computation that depends on itself is NonTermination, as in GHC.
    if msg == "<<loop>>" {
        if let Some(c) = rt.global("GHC.Internal.Control.Exception.Base.nonTerminationException") {
            if let Ok(v) = rt.force(&c) {
                return Some(v);
            }
        }
    }
    let f = rt.global("GHC.Internal.Exception.errorCallException")?;
    let f = rt.force(&f).ok()?;
    rt.apply(f, cell_of(string_value(msg))).ok()
}

fn b(x: bool) -> Value {
    Value::Int(x as i64)
}

fn pair(a: Value, c: Value) -> Value {
    con_value("Builtin.(,)", vec![a, c])
}

fn int(rt: &Runtime, c: &Cell) -> Result<i64, RtError> {
    int_of(rt, c)
}

fn word(rt: &Runtime, c: &Cell) -> Result<u64, RtError> {
    int_of(rt, c).map(|i| i as u64)
}

fn dbl(rt: &Runtime, c: &Cell) -> Result<f64, RtError> {
    double_of(rt, c)
}

fn flt(rt: &Runtime, c: &Cell) -> Result<f32, RtError> {
    double_of(rt, c).map(|d| d as f32)
}

fn f32v(x: f32) -> Value {
    Value::Double(x as f64)
}

/// The exponent and mantissa of a double: `x = m * 2^e`, with the mantissa of 53 bits.
fn decode_double(x: f64) -> (i64, i64) {
    if x == 0.0 || !x.is_finite() {
        return (0, 0);
    }
    let bits = x.to_bits();
    let sign = if bits >> 63 == 1 { -1i64 } else { 1 };
    let exp = ((bits >> 52) & 0x7ff) as i64;
    let frac = (bits & ((1u64 << 52) - 1)) as i64;
    let (m, e) = if exp == 0 { (frac, -1074) } else { (frac | (1 << 52), exp - 1075) };
    (sign * m, e)
}

/// The Int64#, Word64# and sized (Int8# .. Word32#) operations: each is an Int# or Word# operation, with
/// the result narrowed to the width where that matters. Returns the base operation and the narrowing.
fn sized_alias(name: &str) -> Option<(&'static str, Option<&'static str>)> {
    let fixed: Option<&'static str> = match name {
        "and64#" => Some("and#"),
        "or64#" => Some("or#"),
        "xor64#" => Some("xor#"),
        "not64#" => Some("not#"),
        "plusInt64#" => Some("+#"),
        "subInt64#" => Some("-#"),
        "timesInt64#" => Some("*#"),
        "quotInt64#" => Some("quotInt#"),
        "remInt64#" => Some("remInt#"),
        "negateInt64#" => Some("negateInt#"),
        "eqInt64#" => Some("==#"),
        "neInt64#" => Some("/=#"),
        "ltInt64#" => Some("<#"),
        "leInt64#" => Some("<=#"),
        "gtInt64#" => Some(">#"),
        "geInt64#" => Some(">=#"),
        "uncheckedIShiftL64#" => Some("uncheckedIShiftL#"),
        "uncheckedIShiftRA64#" => Some("uncheckedIShiftRA#"),
        "uncheckedIShiftRL64#" => Some("uncheckedIShiftRL#"),
        "plusWord64#" => Some("plusWord#"),
        "subWord64#" => Some("minusWord#"),
        "timesWord64#" => Some("timesWord#"),
        "quotWord64#" => Some("quotWord#"),
        "remWord64#" => Some("remWord#"),
        "eqWord64#" => Some("eqWord#"),
        "neWord64#" => Some("neWord#"),
        "ltWord64#" => Some("ltWord#"),
        "leWord64#" => Some("leWord#"),
        "gtWord64#" => Some("gtWord#"),
        "geWord64#" => Some("geWord#"),
        "uncheckedShiftL64#" => Some("uncheckedShiftL#"),
        "uncheckedShiftRL64#" => Some("uncheckedShiftRL#"),
        "int64ToInt#" | "intToInt64#" | "int64ToWord64#" | "word64ToInt64#" => Some("word2Int#"),
        "word64ToWord#" | "wordToWord64#" => Some("int2Word#"),
        _ => None,
    };
    if fixed.is_some() {
        return fixed.map(|b| (b, None));
    }
    // Sized Int/Word operations: `plusInt8#` and friends narrow to their width.
    let width = |w: &str| match w {
        "8" => Some("narrow8Int#"),
        "16" => Some("narrow16Int#"),
        "32" => Some("narrow32Int#"),
        _ => None,
    };
    let wwidth = |w: &str| match w {
        "8" => Some("narrow8Word#"),
        "16" => Some("narrow16Word#"),
        "32" => Some("narrow32Word#"),
        _ => None,
    };
    let stem = name.strip_suffix('#')?;
    for (op, base) in [("plus", "+#"), ("sub", "-#"), ("times", "*#"), ("quot", "quotInt#"), ("rem", "remInt#")] {
        for (ty, w_base) in [("Int", base), ("Word", match base { "+#" => "plusWord#", "-#" => "minusWord#", "*#" => "timesWord#", "quotInt#" => "quotWord#", _ => "remWord#" })] {
            for w in ["8", "16", "32"] {
                if stem == format!("{op}{ty}{w}") {
                    let narrow = if ty == "Int" { width(w) } else { wwidth(w) };
                    return Some((if ty == "Int" { base } else { w_base }, narrow));
                }
            }
        }
    }
    for (op, base) in [("eq", "==#"), ("ne", "/=#"), ("lt", "<#"), ("le", "<=#"), ("gt", ">#"), ("ge", ">=#")] {
        for ty in ["Int", "Word"] {
            for w in ["8", "16", "32"] {
                if stem == format!("{op}{ty}{w}") {
                    let b = if ty == "Int" { base } else { match base { "==#" => "eqWord#", "/=#" => "neWord#", "<#" => "ltWord#", "<=#" => "leWord#", ">#" => "gtWord#", _ => "geWord#" } };
                    return Some((b, None));
                }
            }
        }
    }
    if stem == "negateInt8" || stem == "negateInt16" || stem == "negateInt32" {
        return Some(("negateInt#", width(&stem["negateInt".len()..])));
    }
    None
}

/// Runs a GHC primitive by name, or answers None when it is not one of them.
pub fn call(rt: &Runtime, name: &str, args: &[Cell]) -> Option<RR> {
    if let Some((base, narrow)) = sized_alias(name) {
        return Some(match super::prims::call(rt, base, args) {
            Ok(v) => match narrow {
                None => Ok(v),
                Some(n) => super::prims::call(rt, n, &[cell_of(v)]),
            },
            Err(e) => Err(e),
        });
    }
    if let Some(r) = call_state(rt, name, args) {
        return Some(r);
    }
    if let Some(r) = ffi(rt, name, args) {
        return Some(r);
    }
    let r: RR = match name {
        // ----- Int# -----
        "+#" => int(rt, &args[0]).and_then(|a| Ok(Value::Int(a.wrapping_add(int(rt, &args[1])?)))),
        "-#" => int(rt, &args[0]).and_then(|a| Ok(Value::Int(a.wrapping_sub(int(rt, &args[1])?)))),
        "*#" => int(rt, &args[0]).and_then(|a| Ok(Value::Int(a.wrapping_mul(int(rt, &args[1])?)))),
        "quotInt#" | "remInt#" | "quotRemInt#" => (|| {
            let a = int(rt, &args[0])?;
            let d = int(rt, &args[1])?;
            if d == 0 {
                return Err(RtError::Error("divide by zero".into()));
            }
            let (q, r) = (a.wrapping_div(d), a.wrapping_rem(d));
            Ok(match name {
                "quotInt#" => Value::Int(q),
                "remInt#" => Value::Int(r),
                _ => pair(Value::Int(q), Value::Int(r)),
            })
        })(),
        "negateInt#" => int(rt, &args[0]).map(|a| Value::Int(a.wrapping_neg())),
        "==#" => int(rt, &args[0]).and_then(|a| Ok(b(a == int(rt, &args[1])?))),
        "/=#" => int(rt, &args[0]).and_then(|a| Ok(b(a != int(rt, &args[1])?))),
        "<#" => int(rt, &args[0]).and_then(|a| Ok(b(a < int(rt, &args[1])?))),
        "<=#" => int(rt, &args[0]).and_then(|a| Ok(b(a <= int(rt, &args[1])?))),
        ">#" => int(rt, &args[0]).and_then(|a| Ok(b(a > int(rt, &args[1])?))),
        ">=#" => int(rt, &args[0]).and_then(|a| Ok(b(a >= int(rt, &args[1])?))),
        "andI#" => int(rt, &args[0]).and_then(|a| Ok(Value::Int(a & int(rt, &args[1])?))),
        "orI#" => int(rt, &args[0]).and_then(|a| Ok(Value::Int(a | int(rt, &args[1])?))),
        "xorI#" => int(rt, &args[0]).and_then(|a| Ok(Value::Int(a ^ int(rt, &args[1])?))),
        "notI#" => int(rt, &args[0]).map(|a| Value::Int(!a)),
        "uncheckedIShiftL#" => int(rt, &args[0]).and_then(|a| Ok(Value::Int(a.wrapping_shl(int(rt, &args[1])? as u32)))),
        "uncheckedIShiftRA#" => int(rt, &args[0]).and_then(|a| Ok(Value::Int(a.wrapping_shr(int(rt, &args[1])? as u32)))),
        "uncheckedIShiftRL#" => int(rt, &args[0]).and_then(|a| Ok(Value::Int(((a as u64).wrapping_shr(int(rt, &args[1])? as u32)) as i64))),
        "addIntC#" | "subIntC#" => (|| {
            let a = int(rt, &args[0])?;
            let c = int(rt, &args[1])?;
            let (r, o) = if name == "addIntC#" { a.overflowing_add(c) } else { a.overflowing_sub(c) };
            Ok(pair(Value::Int(r), Value::Int(o as i64)))
        })(),
        "mulIntMayOflo#" => (|| {
            let a = int(rt, &args[0])?;
            let c = int(rt, &args[1])?;
            Ok(Value::Int(a.checked_mul(c).is_none() as i64))
        })(),
        "int2Word#" | "word2Int#" => int(rt, &args[0]).map(Value::Int),
        "int2Double#" => int(rt, &args[0]).map(|a| Value::Double(a as f64)),
        "int2Float#" => int(rt, &args[0]).map(|a| f32v(a as f32)),
        "word2Double#" => word(rt, &args[0]).map(|a| Value::Double(a as f64)),
        "word2Float#" => word(rt, &args[0]).map(|a| f32v(a as f32)),
        "double2Int#" | "float2Int#" => dbl(rt, &args[0]).map(|d| Value::Int(d as i64)),
        "chr#" => int(rt, &args[0]).map(|a| Value::Char(char::from_u32(a as u32).unwrap_or('\u{fffd}'))),
        "ord#" => chr_of(rt, &args[0]).map(|c| Value::Int(c as i64)),

        // ----- Word# -----
        "plusWord#" => word(rt, &args[0]).and_then(|a| Ok(Value::Int(a.wrapping_add(word(rt, &args[1])?) as i64))),
        "minusWord#" => word(rt, &args[0]).and_then(|a| Ok(Value::Int(a.wrapping_sub(word(rt, &args[1])?) as i64))),
        "timesWord#" => word(rt, &args[0]).and_then(|a| Ok(Value::Int(a.wrapping_mul(word(rt, &args[1])?) as i64))),
        "quotWord#" | "remWord#" | "quotRemWord#" => (|| {
            let a = word(rt, &args[0])?;
            let d = word(rt, &args[1])?;
            if d == 0 {
                return Err(RtError::Error("divide by zero".into()));
            }
            let (q, r) = (a / d, a % d);
            Ok(match name {
                "quotWord#" => Value::Int(q as i64),
                "remWord#" => Value::Int(r as i64),
                _ => pair(Value::Int(q as i64), Value::Int(r as i64)),
            })
        })(),
        "and#" => int(rt, &args[0]).and_then(|a| Ok(Value::Int(a & int(rt, &args[1])?))),
        "or#" => int(rt, &args[0]).and_then(|a| Ok(Value::Int(a | int(rt, &args[1])?))),
        "xor#" => int(rt, &args[0]).and_then(|a| Ok(Value::Int(a ^ int(rt, &args[1])?))),
        "not#" => int(rt, &args[0]).map(|a| Value::Int(!a)),
        "uncheckedShiftL#" => word(rt, &args[0]).and_then(|a| Ok(Value::Int(a.wrapping_shl(int(rt, &args[1])? as u32) as i64))),
        "uncheckedShiftRL#" => word(rt, &args[0]).and_then(|a| Ok(Value::Int(a.wrapping_shr(int(rt, &args[1])? as u32) as i64))),
        "eqWord#" => word(rt, &args[0]).and_then(|a| Ok(b(a == word(rt, &args[1])?))),
        "neWord#" => word(rt, &args[0]).and_then(|a| Ok(b(a != word(rt, &args[1])?))),
        "ltWord#" => word(rt, &args[0]).and_then(|a| Ok(b(a < word(rt, &args[1])?))),
        "leWord#" => word(rt, &args[0]).and_then(|a| Ok(b(a <= word(rt, &args[1])?))),
        "gtWord#" => word(rt, &args[0]).and_then(|a| Ok(b(a > word(rt, &args[1])?))),
        "geWord#" => word(rt, &args[0]).and_then(|a| Ok(b(a >= word(rt, &args[1])?))),
        "popCnt#" | "popCnt64#" => word(rt, &args[0]).map(|a| Value::Int(a.count_ones() as i64)),
        "clz#" | "clz64#" => word(rt, &args[0]).map(|a| Value::Int(a.leading_zeros() as i64)),
        "ctz#" | "ctz64#" => word(rt, &args[0]).map(|a| Value::Int(a.trailing_zeros() as i64)),
        "byteSwap#" | "byteSwap64#" => word(rt, &args[0]).map(|a| Value::Int(a.swap_bytes() as i64)),

        // ----- Char# -----
        "eqChar#" => chr_of(rt, &args[0]).and_then(|a| Ok(b(a == chr_of(rt, &args[1])?))),
        "neChar#" => chr_of(rt, &args[0]).and_then(|a| Ok(b(a != chr_of(rt, &args[1])?))),
        "ltChar#" => chr_of(rt, &args[0]).and_then(|a| Ok(b(a < chr_of(rt, &args[1])?))),
        "leChar#" => chr_of(rt, &args[0]).and_then(|a| Ok(b(a <= chr_of(rt, &args[1])?))),
        "gtChar#" => chr_of(rt, &args[0]).and_then(|a| Ok(b(a > chr_of(rt, &args[1])?))),
        "geChar#" => chr_of(rt, &args[0]).and_then(|a| Ok(b(a >= chr_of(rt, &args[1])?))),

        // ----- narrowing -----
        "narrow8Int#" => int(rt, &args[0]).map(|a| Value::Int(a as i8 as i64)),
        "narrow16Int#" => int(rt, &args[0]).map(|a| Value::Int(a as i16 as i64)),
        "narrow32Int#" => int(rt, &args[0]).map(|a| Value::Int(a as i32 as i64)),
        "narrow8Word#" => int(rt, &args[0]).map(|a| Value::Int(a as u8 as i64)),
        "narrow16Word#" => int(rt, &args[0]).map(|a| Value::Int(a as u16 as i64)),
        "narrow32Word#" => int(rt, &args[0]).map(|a| Value::Int(a as u32 as i64)),

        // ----- Double# -----
        "+##" => dbl(rt, &args[0]).and_then(|a| Ok(Value::Double(a + dbl(rt, &args[1])?))),
        "-##" => dbl(rt, &args[0]).and_then(|a| Ok(Value::Double(a - dbl(rt, &args[1])?))),
        "*##" => dbl(rt, &args[0]).and_then(|a| Ok(Value::Double(a * dbl(rt, &args[1])?))),
        "/##" => dbl(rt, &args[0]).and_then(|a| Ok(Value::Double(a / dbl(rt, &args[1])?))),
        "**##" => dbl(rt, &args[0]).and_then(|a| Ok(Value::Double(a.powf(dbl(rt, &args[1])?)))),
        "==##" => dbl(rt, &args[0]).and_then(|a| Ok(b(a == dbl(rt, &args[1])?))),
        "/=##" => dbl(rt, &args[0]).and_then(|a| Ok(b(a != dbl(rt, &args[1])?))),
        "<##" => dbl(rt, &args[0]).and_then(|a| Ok(b(a < dbl(rt, &args[1])?))),
        "<=##" => dbl(rt, &args[0]).and_then(|a| Ok(b(a <= dbl(rt, &args[1])?))),
        ">##" => dbl(rt, &args[0]).and_then(|a| Ok(b(a > dbl(rt, &args[1])?))),
        ">=##" => dbl(rt, &args[0]).and_then(|a| Ok(b(a >= dbl(rt, &args[1])?))),
        "negateDouble#" => dbl(rt, &args[0]).map(|a| Value::Double(-a)),
        "fabsDouble#" => dbl(rt, &args[0]).map(|a| Value::Double(a.abs())),
        "sqrtDouble#" => dbl(rt, &args[0]).map(|a| Value::Double(a.sqrt())),
        "expDouble#" => dbl(rt, &args[0]).map(|a| Value::Double(a.exp())),
        "expm1Double#" => dbl(rt, &args[0]).map(|a| Value::Double(a.exp_m1())),
        "logDouble#" => dbl(rt, &args[0]).map(|a| Value::Double(a.ln())),
        "log1pDouble#" => dbl(rt, &args[0]).map(|a| Value::Double(a.ln_1p())),
        "sinDouble#" => dbl(rt, &args[0]).map(|a| Value::Double(a.sin())),
        "cosDouble#" => dbl(rt, &args[0]).map(|a| Value::Double(a.cos())),
        "tanDouble#" => dbl(rt, &args[0]).map(|a| Value::Double(a.tan())),
        "asinDouble#" => dbl(rt, &args[0]).map(|a| Value::Double(a.asin())),
        "acosDouble#" => dbl(rt, &args[0]).map(|a| Value::Double(a.acos())),
        "atanDouble#" => dbl(rt, &args[0]).map(|a| Value::Double(a.atan())),
        "sinhDouble#" => dbl(rt, &args[0]).map(|a| Value::Double(a.sinh())),
        "coshDouble#" => dbl(rt, &args[0]).map(|a| Value::Double(a.cosh())),
        "tanhDouble#" => dbl(rt, &args[0]).map(|a| Value::Double(a.tanh())),
        "asinhDouble#" => dbl(rt, &args[0]).map(|a| Value::Double(a.asinh())),
        "acoshDouble#" => dbl(rt, &args[0]).map(|a| Value::Double(a.acosh())),
        "atanhDouble#" => dbl(rt, &args[0]).map(|a| Value::Double(a.atanh())),
        "double2Float#" => dbl(rt, &args[0]).map(|a| f32v(a as f32)),
        "float2Double#" => dbl(rt, &args[0]).map(Value::Double),
        "decodeDouble_Int64#" => dbl(rt, &args[0]).map(|a| {
            let (m, e) = decode_double(a);
            pair(Value::Int(m), Value::Int(e))
        }),

        // ----- Float# (held at single precision) -----
        "plusFloat#" => flt(rt, &args[0]).and_then(|a| Ok(f32v(a + flt(rt, &args[1])?))),
        "minusFloat#" => flt(rt, &args[0]).and_then(|a| Ok(f32v(a - flt(rt, &args[1])?))),
        "timesFloat#" => flt(rt, &args[0]).and_then(|a| Ok(f32v(a * flt(rt, &args[1])?))),
        "divideFloat#" => flt(rt, &args[0]).and_then(|a| Ok(f32v(a / flt(rt, &args[1])?))),
        "powerFloat#" => flt(rt, &args[0]).and_then(|a| Ok(f32v(a.powf(flt(rt, &args[1])?)))),
        "eqFloat#" => flt(rt, &args[0]).and_then(|a| Ok(b(a == flt(rt, &args[1])?))),
        "neFloat#" => flt(rt, &args[0]).and_then(|a| Ok(b(a != flt(rt, &args[1])?))),
        "ltFloat#" => flt(rt, &args[0]).and_then(|a| Ok(b(a < flt(rt, &args[1])?))),
        "leFloat#" => flt(rt, &args[0]).and_then(|a| Ok(b(a <= flt(rt, &args[1])?))),
        "gtFloat#" => flt(rt, &args[0]).and_then(|a| Ok(b(a > flt(rt, &args[1])?))),
        "geFloat#" => flt(rt, &args[0]).and_then(|a| Ok(b(a >= flt(rt, &args[1])?))),
        "negateFloat#" => flt(rt, &args[0]).map(|a| f32v(-a)),
        "fabsFloat#" => flt(rt, &args[0]).map(|a| f32v(a.abs())),
        "sqrtFloat#" => flt(rt, &args[0]).map(|a| f32v(a.sqrt())),
        "expFloat#" => flt(rt, &args[0]).map(|a| f32v(a.exp())),
        "logFloat#" => flt(rt, &args[0]).map(|a| f32v(a.ln())),
        "sinFloat#" => flt(rt, &args[0]).map(|a| f32v(a.sin())),
        "cosFloat#" => flt(rt, &args[0]).map(|a| f32v(a.cos())),
        "tanFloat#" => flt(rt, &args[0]).map(|a| f32v(a.tan())),

        _ => return None,
    };
    Some(r)
}

/// A `Ptr` value: the constructor around an address (a C function's `Ptr` result).
/// The C functions that base imports with `foreign import ccall`, where the answer does not depend on
/// the host's files or processes: the locale's codeset is UTF-8.
fn ffi(rt: &Runtime, name: &str, args: &[Cell]) -> Option<RR> {
    if let Some(r) = super::ffi::call(rt, name, args) {
        return Some(r);
    }
    let r: RR = match name {
        "localeEncoding" => Ok(io_result_of(Value::Addr(Rc::new(RefCell::new(b"UTF-8\0".to_vec())), 0))),
        _ => return None,
    };
    Some(r)
}

/// The primitives that act on mutable state, arrays, exceptions and masking. Each takes the
/// state token last and answers the `(# State#, _ #)` pair, or just the state where it has no result.
fn call_state(rt: &Runtime, name: &str, args: &[Cell]) -> Option<RR> {
    let r: RR = match name {
        // ----- MutVar# -----
        "newMutVar#" => state_result(Value::Ref(Rc::new(RefCell::new(args[0].clone())))),
        "readMutVar#" => match rt.force(&args[0]) {
            Ok(Value::Ref(r)) => {
                let contents = r.borrow().clone();
                Ok(pair_cells_value(unit_cell(), contents))
            }
            Ok(_) => Err(RtError::Fail("internal: readMutVar# of a non-reference".into())),
            Err(e) => Err(e),
        },
        "writeMutVar#" => match rt.force(&args[0]) {
            Ok(Value::Ref(r)) => {
                *r.borrow_mut() = args[1].clone();
                Ok(unit_value())
            }
            Ok(_) => Err(RtError::Fail("internal: writeMutVar# of a non-reference".into())),
            Err(e) => Err(e),
        },

        // ----- Array# / MutableArray# -----
        "newArray#" => rt.force(&args[0]).and_then(|n| {
            let n = match n {
                Value::Int(n) => n.max(0) as usize,
                _ => return Err(RtError::Fail("internal: newArray# size".into())),
            };
            let cells: Vec<Cell> = (0..n).map(|_| args[1].clone()).collect();
            state_result(Value::Vec(Rc::new(RefCell::new(cells))))
        }),
        "readArray#" => array_index(rt, &args[0], &args[1]).map(|c| pair_cells_value(unit_cell(), c)),
        "indexArray#" => array_index(rt, &args[0], &args[1]).and_then(|c| rt.force(&c)).map(|v| v),
        "writeArray#" => array_slot(rt, &args[0], &args[1]).map(|(vec, i)| {
            vec.borrow_mut()[i] = args[2].clone();
            unit_value()
        }),
        "sizeofArray#" | "sizeofMutableArray#" => array_len(rt, &args[0]).map(|n| Value::Int(n as i64)),
        "unsafeFreezeArray#" | "unsafeThawArray#" => rt.force(&args[0]).and_then(state_result),
        "freezeArray#" | "thawArray#" => copy_array(rt, &args[0], &args[1], &args[2]).and_then(state_result),
        "copyArray#" | "copyMutableArray#" => copy_into(rt, args).map(|_| unit_value()),

        // ----- evaluation, masking, exceptions -----
        "seq#" => rt.force(&args[0]).and_then(state_result),
        "touch#" | "noDuplicate#" => Ok(unit_value()),
        // keepAlive# x s k: the continuation runs on the state; x only has to stay alive meanwhile.
        "keepAlive#" => rt.force(&args[2]).and_then(|k| rt.apply(k, args[1].clone())),
        "maskAsyncExceptions#" | "unmaskAsyncExceptions#" | "maskUninterruptible#" => {
            rt.force(&args[0]).and_then(|f| rt.apply(f, args[1].clone()))
        }
        "getMaskingState#" => state_result(Value::Int(0)),
        "raise#" => rt.force(&args[0]).and_then(|e| Err(RtError::Thrown(e))),
        "raiseIO#" => rt.force(&args[0]).and_then(|e| Err(RtError::Thrown(e))),
        "catch#" => catch_prim(rt, args),

        // ----- byte arrays (the element accessors are in prims.rs) -----
        "newAlignedPinnedByteArray#" | "newByteArray#" | "newPinnedByteArray#" => match rt.force(&args[0]) {
            Ok(Value::Int(n)) => state_result(Value::Bytes(Rc::new(RefCell::new(vec![0u8; n.max(0) as usize])))),
            Ok(_) => Err(RtError::Fail("internal: byte array size".into())),
            Err(e) => Err(e),
        },
        "copyByteArray#" | "copyMutableByteArray#" | "copyMutableByteArrayNonOverlapping#" => {
            copy_bytes(rt, args).map(|_| unit_value())
        }
        "setByteArray#" => set_bytes(rt, args).map(|_| unit_value()),
        "shrinkMutableByteArray#" => match (rt.force(&args[0]), rt.force(&args[1])) {
            (Ok(Value::Bytes(b)), Ok(Value::Int(n))) => {
                b.borrow_mut().truncate(n.max(0) as usize);
                Ok(unit_value())
            }
            (Err(e), _) | (_, Err(e)) => Err(e),
            _ => Err(RtError::Fail("internal: shrinkMutableByteArray#".into())),
        },
        "resizeMutableByteArray#" => match (rt.force(&args[0]), rt.force(&args[1])) {
            (Ok(Value::Bytes(b)), Ok(Value::Int(n))) => {
                let mut data = b.borrow().clone();
                data.resize(n.max(0) as usize, 0);
                state_result(Value::Bytes(Rc::new(RefCell::new(data))))
            }
            (Err(e), _) | (_, Err(e)) => Err(e),
            _ => Err(RtError::Fail("internal: resizeMutableByteArray#".into())),
        },
        "compareByteArrays#" => compare_bytes(rt, args).map(Value::Int),
        "isMutableByteArrayPinned#" | "isByteArrayPinned#" => Ok(Value::Int(1)),

        // ----- weak pointers: the runtime has no garbage collector, so a weak pointer keeps its value and
        // its finalizer never runs -----
        "mkWeak#" => Ok(state_result_value(con_value("GHC.Weak.Weak", vec![cell_value_of(&args[1])]))),
        "mkWeakNoFinalizer#" => Ok(state_result_value(con_value("GHC.Weak.Weak", vec![cell_value_of(&args[1])]))),
        "deRefWeak#" => match rt.force(&args[0]) {
            Ok(Value::Con(w)) => {
                let v = w.args.first().cloned().unwrap_or_else(unit_cell);
                rt.force(&v).map(|val| con_value("Builtin.(,,)", vec![unit_value(), Value::Int(1), val]))
            }
            Ok(_) => Err(RtError::Fail("internal: deRefWeak# of a non-weak".into())),
            Err(e) => Err(e),
        },
        "finalizeWeak#" => Ok(con_value("Builtin.(,,)", vec![unit_value(), Value::Int(0), unit_value()])),
        "addCFinalizerToWeak#" => Ok(state_result_value(Value::Int(1))),

        // ----- MVar# (the scheduler's primitives, under their GHC names) -----
        "newMVar#" => super::prims::call(rt, "newMVar", args),
        "takeMVar#" => super::prims::call(rt, "takeMVar", args),
        "readMVar#" => super::prims::call(rt, "readMVar", args),
        // putMVar# answers the state token alone, not a pair.
        "putMVar#" => super::prims::call(rt, "putMVar", args).map(|_| unit_value()),

        // ----- threads: the primops over the runtime's threads (a ThreadId# is the thread number) -----
        "myThreadId#" => super::prims::call(rt, "myThreadId", args),
        "fork#" => super::prims::call(rt, "forkThread", args),
        "forkOn#" => super::prims::call(rt, "forkThread", &args[1..]),
        "yield#" => super::prims::call(rt, "yieldThread", args).map(|_| unit_value()),
        "delay#" => super::prims::call(rt, "threadDelay", args).map(|_| unit_value()),
        "killThread#" => super::prims::call(rt, "throwTo", args).map(|_| unit_value()),
        "isCurrentThreadBound#" => Ok(io_result_of(Value::Int(0))),
        "labelThread#" => Ok(unit_value()),

        // ----- stable pointers: an index into the runtime's table -----
        "makeStablePtr#" => {
            let mut table = rt.stable.borrow_mut();
            table.push(Some(args[0].clone()));
            state_result(Value::Int(table.len() as i64 - 1))
        }
        "deRefStablePtr#" => (|| {
            let i = int(rt, &args[0])?;
            let cell = rt.stable.borrow().get(i as usize).cloned().flatten();
            match cell {
                Some(c) => state_result(rt.force(&c)?),
                None => Err(RtError::Fail("internal: dereferenced a freed stable pointer".into())),
            }
        })(),
        "eqStablePtr#" => match (int(rt, &args[0]), int(rt, &args[1])) {
            (Ok(x), Ok(y)) => Ok(b(x == y)),
            (Err(e), _) | (_, Err(e)) => Err(e),
        },

        // ----- byte arrays and addresses: a pointer is a view of its array's buffer -----
        "mutableByteArrayContents#" | "byteArrayContents#" => match rt.force(&args[0]) {
            Ok(Value::Bytes(b)) => Ok(Value::Addr(b, 0)),
            Ok(_) => Err(RtError::Fail("internal: contents of a non-array".into())),
            Err(e) => Err(e),
        },
        "nullAddr#" => Ok(Value::Addr(Rc::new(RefCell::new(Vec::new())), 0)),
        "addr2Int#" => match rt.force(&args[0]) {
            Ok(Value::Addr(b, o)) => Ok(Value::Int(Rc::as_ptr(&b) as i64 + o as i64)),
            Ok(_) => Err(RtError::Fail("internal: addr2Int# of a non-address".into())),
            Err(e) => Err(e),
        },
        _ => return None,
    };
    Some(r)
}

/// The bytes of a byte array argument (or an address's buffer) as a shared buffer and an offset.
fn bytes_of(rt: &Runtime, c: &Cell) -> Result<(Rc<RefCell<Vec<u8>>>, usize), RtError> {
    match rt.force(c)? {
        Value::Bytes(b) => Ok((b, 0)),
        Value::Addr(b, o) => Ok((b, o)),
        _ => Err(RtError::Fail("internal: expected a byte array".into())),
    }
}

fn int_arg(rt: &Runtime, c: &Cell) -> Result<i64, RtError> {
    match rt.force(c)? {
        Value::Int(n) => Ok(n),
        _ => Err(RtError::Fail("internal: expected an Int#".into())),
    }
}

/// copyByteArray# src srcOff dst dstOff n: the bytes are copied, overlapping ranges included.
fn copy_bytes(rt: &Runtime, args: &[Cell]) -> Result<(), RtError> {
    let (src, so) = bytes_of(rt, &args[0])?;
    let so = so + int_arg(rt, &args[1])? as usize;
    let (dst, dof) = bytes_of(rt, &args[2])?;
    let dof = dof + int_arg(rt, &args[3])? as usize;
    let n = int_arg(rt, &args[4])? as usize;
    let chunk: Vec<u8> = src.borrow()[so..so + n].to_vec();
    dst.borrow_mut()[dof..dof + n].copy_from_slice(&chunk);
    Ok(())
}

/// setByteArray# arr off n c: fills n bytes from off with c.
fn set_bytes(rt: &Runtime, args: &[Cell]) -> Result<(), RtError> {
    let (arr, o) = bytes_of(rt, &args[0])?;
    let off = o + int_arg(rt, &args[1])? as usize;
    let n = int_arg(rt, &args[2])? as usize;
    let c = int_arg(rt, &args[3])? as u8;
    for b in &mut arr.borrow_mut()[off..off + n] {
        *b = c;
    }
    Ok(())
}

/// compareByteArrays# a aOff b bOff n: memcmp-style ordering of two ranges.
fn compare_bytes(rt: &Runtime, args: &[Cell]) -> Result<i64, RtError> {
    let (a, ao) = bytes_of(rt, &args[0])?;
    let ao = ao + int_arg(rt, &args[1])? as usize;
    let (b, bo) = bytes_of(rt, &args[2])?;
    let bo = bo + int_arg(rt, &args[3])? as usize;
    let n = int_arg(rt, &args[4])? as usize;
    let x = a.borrow();
    let y = b.borrow();
    Ok(match x[ao..ao + n].cmp(&y[bo..bo + n]) {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => 1,
    })
}

/// A state-passing result around a value that is already evaluated.
fn state_result_value(v: Value) -> Value {
    io_result_of(v)
}

/// A cell holding the value of an argument, shared with the argument.
fn cell_value_of(c: &Cell) -> Value {
    Value::Ref(Rc::new(RefCell::new(c.clone())))
}

/// A pair value whose second component is the given cell.
fn pair_cells_value(a: Cell, b: Cell) -> Value {
    pair_cells(a, b)
}

fn array_len(rt: &Runtime, arr: &Cell) -> Result<usize, RtError> {
    match rt.force(arr)? {
        Value::Vec(v) => Ok(v.borrow().len()),
        _ => Err(RtError::Fail("internal: array operation on a non-array".into())),
    }
}

fn array_index(rt: &Runtime, arr: &Cell, i: &Cell) -> Result<Cell, RtError> {
    let (vec, i) = array_slot(rt, arr, i)?;
    let c = vec.borrow()[i].clone();
    Ok(c)
}

fn array_slot(rt: &Runtime, arr: &Cell, i: &Cell) -> Result<(Rc<RefCell<Vec<Cell>>>, usize), RtError> {
    let idx = match rt.force(i)? {
        Value::Int(n) => n as usize,
        _ => return Err(RtError::Fail("internal: array index".into())),
    };
    match rt.force(arr)? {
        Value::Vec(v) => {
            if idx >= v.borrow().len() {
                return Err(RtError::Fail("internal: array index out of range".into()));
            }
            Ok((v, idx))
        }
        _ => Err(RtError::Fail("internal: array operation on a non-array".into())),
    }
}

/// A fresh array holding the cells `off .. off+n` of the source.
fn copy_array(rt: &Runtime, src: &Cell, off: &Cell, n: &Cell) -> Result<Value, RtError> {
    let off = match rt.force(off)? {
        Value::Int(o) => o as usize,
        _ => return Err(RtError::Fail("internal: copy offset".into())),
    };
    let n = match rt.force(n)? {
        Value::Int(k) => k as usize,
        _ => return Err(RtError::Fail("internal: copy length".into())),
    };
    match rt.force(src)? {
        Value::Vec(v) => {
            let cells: Vec<Cell> = v.borrow()[off..off + n].to_vec();
            Ok(Value::Vec(Rc::new(RefCell::new(cells))))
        }
        _ => Err(RtError::Fail("internal: copy of a non-array".into())),
    }
}

/// copyArray# src srcOff dst dstOff n: the cells are shared, as the array holds references.
fn copy_into(rt: &Runtime, args: &[Cell]) -> Result<(), RtError> {
    let num = |c: &Cell| -> Result<usize, RtError> {
        match rt.force(c)? {
            Value::Int(k) => Ok(k as usize),
            _ => Err(RtError::Fail("internal: copy argument".into())),
        }
    };
    let (so, dof, n) = (num(&args[1])?, num(&args[3])?, num(&args[4])?);
    let src: Vec<Cell> = match rt.force(&args[0])? {
        Value::Vec(v) => v.borrow()[so..so + n].to_vec(),
        _ => return Err(RtError::Fail("internal: copy from a non-array".into())),
    };
    match rt.force(&args[2])? {
        Value::Vec(v) => {
            let mut dst = v.borrow_mut();
            for (k, c) in src.into_iter().enumerate() {
                dst[dof + k] = c;
            }
            Ok(())
        }
        _ => Err(RtError::Fail("internal: copy to a non-array".into())),
    }
}

fn catch_handle(rt: &Runtime, exc: Value, args: &[Cell]) -> RR {
    let handler = rt.force(&args[1])?;
    let action = rt.apply(handler, cell_of(exc))?;
    rt.apply(action, args[2].clone())
}

/// catch# io handler s: the action runs; an exception goes to the handler, which runs in its place.
fn catch_prim(rt: &Runtime, args: &[Cell]) -> RR {
    let exc = match run_io(rt, &args[0], &args[2]) {
        Ok(v) => return Ok(v),        Err(RtError::Thrown(e)) | Err(RtError::Async(e)) => e,
        Err(RtError::Error(msg)) | Err(RtError::Fail(msg)) | Err(RtError::ErrorLoc(msg, _)) => {
            match error_call(rt, &msg) {
                Some(e) => e,
                None => return Err(RtError::Fail(msg)),
            }
        }
        Err(other) => return Err(other),
    };
    let handler = rt.force(&args[1])?;
    let action = rt.apply(handler, cell_of(exc))?;
    rt.apply(action, args[2].clone())
}
