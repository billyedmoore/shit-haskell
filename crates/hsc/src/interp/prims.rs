//! Primitive operations. Each primitive receives its arguments as cells and forces the
//! ones it needs; the Prelude wraps them with Haskell types.

use std::io::{BufRead, Read, Write};
use std::rc::Rc;

use super::bigint::{bit_length, bitwise, popcount, shift_right, test_bit, BigInt};
use super::{bool_value, con_value, string_value, unit_value, Cell, RR, RtError, Runtime, Thunk, Value};
use std::cell::RefCell;

fn int(rt: &Runtime, c: &Cell) -> Result<i64, RtError> {
    match rt.force(c)? {
        Value::Int(i) => Ok(i),
        _ => Err(RtError::Fail("internal: expected Int".into())),
    }
}

fn integer(rt: &Runtime, c: &Cell) -> Result<Rc<BigInt>, RtError> {
    let v = rt.force(c)?;
    match integer_from_value(rt, &v)? {
        Some(b) => Ok(Rc::new(b)),
        None => Err(RtError::Fail("internal: expected Integer".into())),
    }
}

/// The value of an `Integer`, in either representation: the constructors of GHC's
/// `data Integer = IS Int# | IP BigNat# | IN BigNat#` (BigNat# being a native bignum), or a
/// native bignum directly (literals before they are built).
pub fn integer_from_value(rt: &Runtime, v: &Value) -> Result<Option<BigInt>, RtError> {
    Ok(match v {
        Value::Integer(i) => Some((**i).clone()),
        Value::Con(c) => match (c.name, c.args.first()) {
            ("GHC.Num.Integer.IS", Some(cell)) => match rt.force(cell)? {
                Value::Int(i) => Some(BigInt::from_i64(i)),
                _ => None,
            },
            ("GHC.Num.Integer.IP" | "GHC.Num.Integer.IN", Some(cell)) => match rt.force(cell)? {
                Value::Integer(b) => Some(if c.name == "GHC.Num.Integer.IN" { b.neg() } else { (*b).clone() }),
                Value::Bytes(b) => Some(bignat_value(&b.borrow(), c.name == "GHC.Num.Integer.IN")),
                _ => None,
            },
            _ => None,
        },
        _ => None,
    })
}

/// An `Integer` value in GHC's representation: `IS` when it fits an `Int`, else `IP`/`IN`
/// with its magnitude as a `BigNat#`.
pub fn integer_con(b: BigInt) -> Value {
    match b.to_i64() {
        Some(i) => con_value("GHC.Num.Integer.IS", vec![Value::Int(i)]),
        None if b.is_neg() => con_value("GHC.Num.Integer.IN", vec![bignat_bytes(&b.abs())]),
        None => con_value("GHC.Num.Integer.IP", vec![bignat_bytes(&b)]),
    }
}

/// A `BigNat#` (as in GHC, a `ByteArray#` of 64-bit limbs, least significant first) of a
/// magnitude.
pub fn bignat_bytes(b: &BigInt) -> Value {
    let n = super::bigint::limb64_count(b);
    let mut bytes = Vec::with_capacity(8 * n);
    for i in 0..n {
        bytes.extend_from_slice(&super::bigint::limb64(b, i).to_le_bytes());
    }
    Value::Bytes(Rc::new(RefCell::new(bytes)))
}

/// The number a `BigNat#`'s limbs are, negated for an `IN`.
pub fn bignat_value(bytes: &[u8], neg: bool) -> BigInt {
    let limbs: Vec<u64> = bytes.chunks(8).map(|c| {
        let mut w = [0u8; 8];
        w[..c.len()].copy_from_slice(c);
        u64::from_le_bytes(w)
    }).collect();
    BigInt::from_u64_limbs(neg, &limbs)
}

/// A `BigNat#` argument's number (a native bignum is accepted too).
fn bignat(rt: &Runtime, c: &Cell) -> Result<BigInt, RtError> {
    match rt.force(c)? {
        Value::Bytes(b) => Ok(bignat_value(&b.borrow(), false)),
        Value::Integer(b) => Ok((*b).clone()),
        _ => Err(RtError::Fail("internal: expected a BigNat".into())),
    }
}

fn double(rt: &Runtime, c: &Cell) -> Result<f64, RtError> {
    match rt.force(c)? {
        Value::Double(d) => Ok(d),
        _ => Err(RtError::Fail("internal: expected Double".into())),
    }
}

fn chr(rt: &Runtime, c: &Cell) -> Result<char, RtError> {
    match rt.force(c)? {
        Value::Char(ch) => Ok(char::from_u32(ch).unwrap_or('\u{fffd}')),
        _ => Err(RtError::Fail("internal: expected Char".into())),
    }
}

fn int_v(i: i64) -> RR {
    Ok(Value::Int(i))
}

fn integer_v(b: BigInt) -> RR {
    Ok(integer_con(b))
}

fn divide_by_zero<T>() -> Result<T, RtError> {
    Err(RtError::Error("divide by zero".into()))
}

/// Calls a primitive by name with its (unevaluated) arguments.
pub fn call(rt: &Runtime, name: &str, args: &[Cell]) -> RR {
    match call_named(rt, name, args) {
        // An argument of the wrong kind: the message names the primitive.
        Err(RtError::Fail(msg)) if msg.starts_with("internal: expected") && !msg.contains(" in ") => {
            Err(RtError::Fail(format!("{msg} in {name}")))
        }
        r => r,
    }
}

fn call_named(rt: &Runtime, name: &str, args: &[Cell]) -> RR {
    // A foreign C call: the runtime's own version of the function, else the C function itself.
    if let Some((sig, ret, sym)) = crate::ccall::parse(name) {
        // A safe call of a C function goes to C (other threads run meanwhile).
        if crate::ccall::is_safe(name) {
            if let Some(f) = crate::ccall::resolve(name) {
                return super::ffi::generic_call(rt, f, sig, ret, args, true, crate::ccall::is_interruptible(name));
            }
        }
        if let Some(r) = super::ffi::call(rt, sym, args) {
            return r;
        }
        if let Some(a) = crate::ccall::address_import(name) {
            return Ok(io_result_of(Value::Int(a as i64)));
        }
        if let Some(f) = crate::ccall::resolve(name) {
            let r = super::ffi::generic(rt, f, sig, ret, args, crate::ccall::is_safe(name));
            // HSC_TRACE_CCALL: each C call made by name, with its answer.
            if std::env::var_os("HSC_TRACE_CCALL").is_some() {
                let shown = match &r {
                    Ok(Value::Con(c)) if c.args.len() == 2 => match &*c.args[1].borrow() {
                        super::Thunk::Done(Value::Int(i)) => format!("{i} ({i:#x})"),
                        super::Thunk::Done(Value::Addr(_, o)) => format!("addr+{o}"),
                        _ => "value".into(),
                    },
                    Ok(_) => "value".into(),
                    Err(_) => "error".into(),
                };
                eprintln!("hsc: ccall {sym} -> {shown}");
            }
            return r;
        }
        return call(rt, sym, args);
    }
    match name {
        // ----- Int -----
        "intAdd" => int_v(int(rt, &args[0])?.wrapping_add(int(rt, &args[1])?)),
        "intSub" => int_v(int(rt, &args[0])?.wrapping_sub(int(rt, &args[1])?)),
        "intMul" => int_v(int(rt, &args[0])?.wrapping_mul(int(rt, &args[1])?)),
        "intNegate" => int_v(int(rt, &args[0])?.wrapping_neg()),
        "intQuot" | "intRem" | "intDiv" | "intMod" => {
            let a = int(rt, &args[0])?;
            let b = int(rt, &args[1])?;
            if b == 0 {
                return divide_by_zero();
            }
            int_v(match name {
                "intQuot" => a.wrapping_div(b),
                "intRem" => a.wrapping_rem(b),
                "intDiv" => {
                    let q = a.wrapping_div(b);
                    if (a % b != 0) && ((a < 0) != (b < 0)) {
                        q - 1
                    } else {
                        q
                    }
                }
                _ => {
                    let r = a.wrapping_rem(b);
                    if r != 0 && ((r < 0) != (b < 0)) {
                        r + b
                    } else {
                        r
                    }
                }
            })
        }
        "intEq" => Ok(bool_value(int(rt, &args[0])? == int(rt, &args[1])?)),
        "intLt" => Ok(bool_value(int(rt, &args[0])? < int(rt, &args[1])?)),
        "intLe" => Ok(bool_value(int(rt, &args[0])? <= int(rt, &args[1])?)),
        "intToInteger" => integer_v(BigInt::from_i64(int(rt, &args[0])?)),
        "intToDouble" => Ok(Value::Double(int(rt, &args[0])? as f64)),
        "intShow" => Ok(string_value(&int(rt, &args[0])?.to_string())),

        // ----- Integer -----
        "integerAdd" => {
            let (a, b) = (integer(rt, &args[0])?, integer(rt, &args[1])?);
            integer_v(a.add(&b))
        }
        "integerSub" => {
            let (a, b) = (integer(rt, &args[0])?, integer(rt, &args[1])?);
            integer_v(a.sub(&b))
        }
        "integerMul" => {
            let (a, b) = (integer(rt, &args[0])?, integer(rt, &args[1])?);
            integer_v(a.mul(&b))
        }
        "integerNegate" => integer_v(integer(rt, &args[0])?.neg()),
        "integerAbs" => integer_v(integer(rt, &args[0])?.abs()),
        "integerSignum" => {
            let a = integer(rt, &args[0])?;
            integer_v(BigInt::from_i64(if a.is_zero() {
                0
            } else if a.is_neg() {
                -1
            } else {
                1
            }))
        }
        "integerQuot" | "integerRem" | "integerDiv" | "integerMod" => {
            let a = integer(rt, &args[0])?;
            let b = integer(rt, &args[1])?;
            if b.is_zero() {
                return divide_by_zero();
            }
            integer_v(match name {
                "integerQuot" => a.quot_rem(&b).0,
                "integerRem" => a.quot_rem(&b).1,
                "integerDiv" => a.div_mod(&b).0,
                _ => a.div_mod(&b).1,
            })
        }
        "integerEq" => {
            let (a, b) = (integer(rt, &args[0])?, integer(rt, &args[1])?);
            Ok(bool_value(*a == *b))
        }
        "integerLt" => {
            let (a, b) = (integer(rt, &args[0])?, integer(rt, &args[1])?);
            Ok(bool_value(a.cmp_value(&b) == std::cmp::Ordering::Less))
        }
        "integerLe" => {
            let (a, b) = (integer(rt, &args[0])?, integer(rt, &args[1])?);
            Ok(bool_value(a.cmp_value(&b) != std::cmp::Ordering::Greater))
        }
        "integerToInt" => int_v(integer(rt, &args[0])?.to_i64_wrapping()),
        "integerToDouble" => Ok(Value::Double(integer(rt, &args[0])?.to_f64())),
        "integerShow" => Ok(string_value(&integer(rt, &args[0])?.to_decimal())),
        "integerShiftL" => {
            let n = int(rt, &args[1])?;
            integer_v(super::bigint::shift_left(&*integer(rt, &args[0])?, n as usize))
        }

        // ----- Double -----
        "doubleAdd" => Ok(Value::Double(double(rt, &args[0])? + double(rt, &args[1])?)),
        "doubleSub" => Ok(Value::Double(double(rt, &args[0])? - double(rt, &args[1])?)),
        "doubleMul" => Ok(Value::Double(double(rt, &args[0])? * double(rt, &args[1])?)),
        "doubleDiv" => Ok(Value::Double(double(rt, &args[0])? / double(rt, &args[1])?)),
        "doubleNegate" => Ok(Value::Double(-double(rt, &args[0])?)),
        "doubleEq" => Ok(bool_value(double(rt, &args[0])? == double(rt, &args[1])?)),
        "doubleLt" => Ok(bool_value(double(rt, &args[0])? < double(rt, &args[1])?)),
        "doubleLe" => Ok(bool_value(double(rt, &args[0])? <= double(rt, &args[1])?)),
        "doubleFromInteger" => Ok(Value::Double(integer(rt, &args[0])?.to_f64())),
        "doubleShow" => Ok(string_value(&show_double(double(rt, &args[0])?))),
        "doubleSqrt" => Ok(Value::Double(double(rt, &args[0])?.sqrt())),
        "doubleExp" => Ok(Value::Double(double(rt, &args[0])?.exp())),
        "doubleLog" => Ok(Value::Double(double(rt, &args[0])?.ln())),
        "doublePow" => Ok(Value::Double(double(rt, &args[0])?.powf(double(rt, &args[1])?))),
        "doubleSin" => Ok(Value::Double(double(rt, &args[0])?.sin())),
        "doubleCos" => Ok(Value::Double(double(rt, &args[0])?.cos())),
        "doubleTan" => Ok(Value::Double(double(rt, &args[0])?.tan())),
        "doubleAtan2" => Ok(Value::Double(double(rt, &args[0])?.atan2(double(rt, &args[1])?))),
        "doubleTruncate" => integer_v(bigint_from_f64(double(rt, &args[0])?.trunc())),
        "doubleFloor" => integer_v(bigint_from_f64(double(rt, &args[0])?.floor())),
        "doubleCeiling" => integer_v(bigint_from_f64(double(rt, &args[0])?.ceil())),
        "intAnd" => int_v(int(rt, &args[0])? & int(rt, &args[1])?),
        "intOr" => int_v(int(rt, &args[0])? | int(rt, &args[1])?),
        "intXor" => int_v(int(rt, &args[0])? ^ int(rt, &args[1])?),
        "intShiftL" => {
            let n = int(rt, &args[1])?;
            int_v(if n >= 64 { 0 } else { int(rt, &args[0])?.wrapping_shl(n as u32) })
        }
        "intShiftR" => {
            let n = int(rt, &args[1])?;
            int_v(int(rt, &args[0])? >> n.min(63))
        }
        "intPopCount" => int_v(int(rt, &args[0])?.count_ones() as i64),
        "doubleFixed" => {
            let digits = int(rt, &args[0])?.max(0) as usize;
            Ok(string_value(&format!("{:.*}", digits, double(rt, &args[1])?)))
        }
        "doubleExpFmt" => {
            let digits = int(rt, &args[0])?.max(0) as usize;
            let x = double(rt, &args[1])?;
            let s = format!("{:.*e}", digits, x);
            // Haskell-style exponent: e+05 is printed as e5 by GHC's printf? Keep C-like e+XX.
            let (m, e) = s.split_once('e').unwrap_or((&s, "0"));
            let ev: i32 = e.parse().unwrap_or(0);
            let sign = if ev < 0 { "-" } else { "+" };
            Ok(string_value(&format!("{m}e{sign}{:02}", ev.abs())))
        }
        "unsafeCoerce" => rt.force(&args[0]),
        "throwExc" => Err(RtError::Thrown(rt.force(&args[0])?)),
        "throwIOExc" => Err(RtError::Thrown(rt.force(&args[0])?)),
        "catchExc" => {
            // catch#: args are the action, the converter for runtime errors, the handler, the world.
            let run_io = |cell: &Cell| -> RR {
                let io = rt.force(cell)?;
                let run = match io {
                    other_io @ Value::Con(_) | other_io @ Value::Fun(_) => other_io,
                    _ => return Err(RtError::Fail("internal: catch of a non-IO action".into())),
                };
                rt.apply(run, args[3].clone())
            };
            let outcome = run_io(&args[0]);
            let exc = match outcome {
                Ok(v) => return Ok(v),
                Err(RtError::Thrown(v)) | Err(RtError::Async(v)) => v,
                Err(RtError::Error(msg)) | Err(RtError::Fail(msg)) | Err(RtError::ErrorLoc(msg, _)) => {
                    let conv = rt.force(&args[1])?;
                    let cell = Rc::new(RefCell::new(super::Thunk::Done(string_value(&msg))));
                    rt.apply(conv, cell)?
                }
                Err(other) => return Err(other),
            };
            let handler = rt.force(&args[2])?;
            let exc_cell = Rc::new(RefCell::new(super::Thunk::Done(exc)));
            let io = rt.apply(handler, exc_cell)?;
            let run = match io {
                other_io @ Value::Con(_) | other_io @ Value::Fun(_) => other_io,
                _ => return Err(RtError::Fail("internal: handler is not an IO action".into())),
            };
            rt.apply(run, args[3].clone())
        }
        "vecFromList" => {
            let n = int(rt, &args[0])?.max(0) as usize;
            let mut cells = Vec::with_capacity(n);
            let mut cur = rt.force(&args[1])?;
            while cells.len() < n {
                match cur {
                    Value::Con(c) if c.name == "Builtin.:" => {
                        cells.push(c.args[0].clone());
                        cur = rt.force(&c.args[1])?;
                    }
                    _ => break,
                }
            }
            Ok(Value::Vec(Rc::new(RefCell::new(cells))))
        }
        "vecIndex" => match rt.force(&args[0])? {
            Value::Vec(v) => {
                let i = int(rt, &args[1])?;
                let cell = v.borrow().get(i as usize).cloned();
                match cell {
                    Some(c) => rt.force(&c),
                    None => Err(RtError::Error("Ix: index out of range".into())),
                }
            }
            _ => Err(RtError::Fail("internal: vecIndex of a non-vector".into())),
        },
        "vecToList" => match rt.force(&args[0])? {
            Value::Vec(v) => {
                let items: Vec<Cell> = v.borrow().clone();
                let mut list = Value::Con(Rc::new(super::ConVal { name: "Builtin.[]", args: vec![] }));
                for c in items.into_iter().rev() {
                    list = Value::Con(Rc::new(super::ConVal {
                        name: "Builtin.:",
                        args: vec![c, Rc::new(RefCell::new(super::Thunk::Done(list)))],
                    }));
                }
                Ok(list)
            }
            _ => Err(RtError::Fail("internal: vecToList of a non-vector".into())),
        },
        "vecUpdate" => match rt.force(&args[0])? {
            Value::Vec(v) => {
                let mut cells: Vec<Cell> = v.borrow().clone();
                let mut cur = rt.force(&args[1])?;
                loop {
                    match cur {
                        Value::Con(c) if c.name == "Builtin.:" => {
                            let pair = rt.force(&c.args[0])?;
                            if let Value::Con(p) = pair {
                                let i = int(rt, &p.args[0])? as usize;
                                if i < cells.len() {
                                    cells[i] = p.args[1].clone();
                                }
                            }
                            cur = rt.force(&c.args[1])?;
                        }
                        _ => break,
                    }
                }
                Ok(Value::Vec(Rc::new(RefCell::new(cells))))
            }
            _ => Err(RtError::Fail("internal: vecUpdate of a non-vector".into())),
        },
        // The monotonic clock (GHC.Internal.Clock): nanoseconds since the first reading.
        "getMonotonicNSec" => {
            use std::sync::OnceLock;
            static START: OnceLock<std::time::Instant> = OnceLock::new();
            let start = START.get_or_init(std::time::Instant::now);
            Ok(io_result(int_v(start.elapsed().as_nanos() as i64)?))
        }
        "traceErr" => {
            let s = rt.force_string(&args[0])?;
            let _ = std::io::stderr().write_all(format!("{s}\n").as_bytes());
            rt.force(&args[1])
        }
        "getArgs" => Ok(io_result(list_value(Vec::new()))),
        "getProgName" => Ok(io_result(string_value(&rt.prog_name))),
        "unsafeRunIO" => {
            // `unsafePerformIO io`: run the IO action (a function of the world) and take its result.
            let io = rt.force(&args[0])?;
            let run = match io {
                other_io @ Value::Con(_) | other_io @ Value::Fun(_) => other_io,
                _ => return Err(RtError::Fail("internal: unsafePerformIO of a non-IO value".into())),
            };
            let res = rt.apply(run, Rc::new(RefCell::new(super::Thunk::Done(unit_value()))))?;
            match res {
                Value::Con(c) if c.name == "Builtin.(,)" => rt.force(&c.args[1]),
                _ => Err(RtError::Fail("internal: IO result".into())),
            }
        }
        "floatFromDouble" => Ok(Value::Double((double(rt, &args[0])? as f32) as f64)),
        "floatToDouble" => Ok(Value::Double(double(rt, &args[0])?)),
        "floatShow" => Ok(string_value(&show_float(double(rt, &args[0])? as f32))),
        "doubleRead" => {
            let s = rt.force_string(&args[0])?;
            Ok(Value::Double(s.parse::<f64>().unwrap_or(0.0)))
        }
        "doubleToRatio" => Ok(double_to_ratio(double(rt, &args[0])?, rt.ratio_con)),
        "doubleIsNaN" => Ok(bool_value(double(rt, &args[0])?.is_nan())),
        "doubleIsInfinite" => Ok(bool_value(double(rt, &args[0])?.is_infinite())),

        // ----- Char -----
        "charOrd" => match rt.force(&args[0])? {
            Value::Char(ch) => int_v(ch as i64),
            _ => Err(RtError::Fail("internal: expected Char".into())),
        },
        "charChr" => {
            let n = int(rt, &args[0])?;
            match n {
                0..=0x10FFFF => Ok(Value::Char(n as u32)),
                _ => Err(RtError::Error(format!("Base.chr: bad argument: {n}"))),
            }
        }
        "charCategory" => {
            // Index into Data.Char.GeneralCategory's constructors, approximated from the
            // character's properties (exact for ASCII and common Latin-1 letters).
            let c = chr(rt, &args[0])?;
            let idx = general_category_index(c);
            int_v(idx)
        }
        "charEq" => Ok(bool_value(chr(rt, &args[0])? == chr(rt, &args[1])?)),
        "charLt" => Ok(bool_value(chr(rt, &args[0])? < chr(rt, &args[1])?)),
        "charLe" => Ok(bool_value(chr(rt, &args[0])? <= chr(rt, &args[1])?)),

        // ----- control -----
        "seq" => {
            rt.force(&args[0])?;
            rt.force(&args[1])
        }
        "error" => Err(RtError::Error(rt.force_string(&args[0])?)),
        "errorLoc" => {
            let msg = rt.force_string(&args[0])?;
            let site = rt.force_string(&args[1])?;
            Err(RtError::ErrorLoc(msg, site))
        }
        "exitWith" => {
            let code = int(rt, &args[0])?;
            Err(RtError::Exit(code as i32))
        }

        // ----- IO; the last argument is the world token -----
        "putStr" => {
            let s = rt.force_string(&args[0])?;
            let mut out = std::io::stdout().lock();
            let _ = out.write_all(s.as_bytes());
            Ok(io_result(unit_value()))
        }
        "putErr" => {
            let s = rt.force_string(&args[0])?;
            let _ = std::io::stderr().write_all(s.as_bytes());
            Ok(io_result(unit_value()))
        }
        "newRef" => Ok(io_result(Value::Ref(Rc::new(RefCell::new(args[0].clone()))))),
        "readRef" => match rt.force(&args[0])? {
            Value::Ref(r) => {
                let cell = r.borrow().clone();
                Ok(io_result(rt.force(&cell)?))
            }
            _ => Err(RtError::Fail("internal: readRef of a non-reference".into())),
        },
        "writeRef" => match rt.force(&args[0])? {
            Value::Ref(r) => {
                *r.borrow_mut() = args[1].clone();
                Ok(io_result(unit_value()))
            }
            _ => Err(RtError::Fail("internal: writeRef of a non-reference".into())),
        },
        "vecNewIO" => {
            // A mutable vector of `n` slots, all holding the same (lazy) element.
            let n = int(rt, &args[0])?.max(0) as usize;
            Ok(io_result(Value::Vec(Rc::new(RefCell::new(vec![args[1].clone(); n])))))
        }
        "vecReadIO" => match rt.force(&args[0])? {
            Value::Vec(v) => {
                let i = int(rt, &args[1])?;
                let cell = v.borrow().get(i as usize).cloned();
                match cell {
                    Some(c) => Ok(io_result(rt.force(&c)?)),
                    None => Err(RtError::Error("array index out of range".into())),
                }
            }
            _ => Err(RtError::Fail("internal: readArray of a non-array".into())),
        },
        "vecWriteIO" => match rt.force(&args[0])? {
            Value::Vec(v) => {
                let i = int(rt, &args[1])?;
                let mut slots = v.borrow_mut();
                if i < 0 || i as usize >= slots.len() {
                    return Err(RtError::Error("array index out of range".into()));
                }
                slots[i as usize] = args[2].clone();
                Ok(io_result(unit_value()))
            }
            _ => Err(RtError::Fail("internal: writeArray of a non-array".into())),
        },
        "vecCopyIO" => match rt.force(&args[0])? {
            Value::Vec(v) => {
                let copy = v.borrow().clone();
                Ok(io_result(Value::Vec(Rc::new(RefCell::new(copy)))))
            }
            _ => Err(RtError::Fail("internal: copy of a non-array".into())),
        },
        "flushStdout" => {
            flush_stdout();
            Ok(io_result(unit_value()))
        }
        "getLine" => {
            let mut line = String::new();
            let n = std::io::stdin().lock().read_line(&mut line).unwrap_or(0);
            if n == 0 {
                return Err(RtError::Error("Base.getLine: end of file".into()));
            }
            if line.ends_with('\n') {
                line.pop();
            }
            Ok(io_result(string_value(&line)))
        }
        "getContents" => {
            let mut s = String::new();
            let _ = std::io::stdin().lock().read_to_string(&mut s);
            Ok(io_result(string_value(&s)))
        }
        "isEOF" => {
            let eof = std::io::stdin().lock().fill_buf().map(|b| b.is_empty()).unwrap_or(true);
            Ok(io_result(bool_value(eof)))
        }
        "readFile" => {
            let path = rt.force_string(&args[0])?;
            match std::fs::read_to_string(&path) {
                Ok(s) => Ok(io_result(string_value(&s))),
                Err(e) => Err(RtError::Error(format!("{path}: openFile: {}", io_message(&e)))),
            }
        }

        "integerAnd" => integer_v(bitwise(&*integer(rt, &args[0])?, &*integer(rt, &args[1])?, |p, q| p & q)),
        "integerOr" => integer_v(bitwise(&*integer(rt, &args[0])?, &*integer(rt, &args[1])?, |p, q| p | q)),
        "integerXor" => integer_v(bitwise(&*integer(rt, &args[0])?, &*integer(rt, &args[1])?, |p, q| p ^ q)),
        "integerShiftR" => {
            let n = int(rt, &args[1])?.max(0) as usize;
            integer_v(shift_right(&*integer(rt, &args[0])?, n))
        }
        "integerTestBit" => {
            let n = int(rt, &args[1])?.max(0) as usize;
            Ok(bool_value(test_bit(&*integer(rt, &args[0])?, n)))
        }
        "integerPopCount" => int_v(popcount(&*integer(rt, &args[0])?) as i64),
        "integerLog2" => int_v(bit_length(&*integer(rt, &args[0])?) as i64 - 1),
        "integerGcd" => integer_v(integer(rt, &args[0])?.gcd(&*integer(rt, &args[1])?)),
        // (mantissa, exponent) with value mantissa * 2^exponent, the mantissa having 53 bits.
        "integerDecodeDouble" => {
            let d = double(rt, &args[0])?;
            let (m, e) = if d == 0.0 || !d.is_finite() {
                (0i64, 0i64)
            } else {
                let bits = d.to_bits();
                let sign = if bits >> 63 == 1 { -1 } else { 1 };
                let exp = ((bits >> 52) & 0x7ff) as i64;
                let frac = (bits & ((1u64 << 52) - 1)) as i64;
                let (mant, e) = if exp == 0 { { let z = (frac as u64).leading_zeros() as i64 - 11; ((frac << z), -1074 - z) } } else { (frac | (1 << 52), exp - 1075) };
                (sign * mant, e)
            };
            Ok(con_value("Builtin.(,)", vec![integer_con(BigInt::from_i64(m)), Value::Int(e)]))
        }
        "integerEncodeDouble" => {
            let m = integer(rt, &args[0])?.to_f64();
            let e = int(rt, &args[1])?;
            Ok(Value::Double(super::bigint::ldexp(m, e)))
        }
        "bigNatIndex" => {
            let b = bignat(rt, &args[0])?;
            let i = int(rt, &args[1])?.max(0) as usize;
            int_v(super::bigint::limb64(&b, i) as i64)
        }
        "bigNatSize" => {
            let b = bignat(rt, &args[0])?;
            int_v(super::bigint::limb64_count(&b) as i64)
        }
        // A BigNat# is the magnitude of an Integer, as its limbs.
        "integerBigNat" => Ok(bignat_bytes(&integer(rt, &args[0])?.abs())),
        "integerFromBigNat" => integer_v(bignat(rt, &args[0])?),
        // GHC's primops over unboxed types. Unboxed values share the boxed representation, so
        // these are the same operations as the boxed ones, with Int# results for comparisons.
        "+#" | "plusWord#" => int_v(int(rt, &args[0])?.wrapping_add(int(rt, &args[1])?)),
        "-#" | "minusWord#" => int_v(int(rt, &args[0])?.wrapping_sub(int(rt, &args[1])?)),
        "*#" | "timesWord#" => int_v(int(rt, &args[0])?.wrapping_mul(int(rt, &args[1])?)),
        "negateInt#" => int_v(int(rt, &args[0])?.wrapping_neg()),
        "quotInt#" => {
            let d = int(rt, &args[1])?;
            if d == 0 {
                return Err(RtError::Error("divide by zero".into()));
            }
            int_v(int(rt, &args[0])?.wrapping_div(d))
        }
        "remInt#" => {
            let d = int(rt, &args[1])?;
            if d == 0 {
                return Err(RtError::Error("divide by zero".into()));
            }
            int_v(int(rt, &args[0])?.wrapping_rem(d))
        }
        "==#" | "eqWord#" | "eqChar#" => int_v((int_or_char(rt, &args[0])? == int_or_char(rt, &args[1])?) as i64),
        "/=#" | "neWord#" | "neChar#" => int_v((int_or_char(rt, &args[0])? != int_or_char(rt, &args[1])?) as i64),
        "<#" | "ltChar#" => int_v((int_or_char(rt, &args[0])? < int_or_char(rt, &args[1])?) as i64),
        "<=#" | "leChar#" => int_v((int_or_char(rt, &args[0])? <= int_or_char(rt, &args[1])?) as i64),
        ">#" | "gtChar#" => int_v((int_or_char(rt, &args[0])? > int_or_char(rt, &args[1])?) as i64),
        ">=#" | "geChar#" => int_v((int_or_char(rt, &args[0])? >= int_or_char(rt, &args[1])?) as i64),
        // Word# values are machine words: their order is unsigned.
        "ltWord#" => int_v((word_of(rt, &args[0])? < word_of(rt, &args[1])?) as i64),
        "leWord#" => int_v((word_of(rt, &args[0])? <= word_of(rt, &args[1])?) as i64),
        "gtWord#" => int_v((word_of(rt, &args[0])? > word_of(rt, &args[1])?) as i64),
        "geWord#" => int_v((word_of(rt, &args[0])? >= word_of(rt, &args[1])?) as i64),
        "andI#" => int_v(int(rt, &args[0])? & int(rt, &args[1])?),
        "orI#" => int_v(int(rt, &args[0])? | int(rt, &args[1])?),
        "xorI#" => int_v(int(rt, &args[0])? ^ int(rt, &args[1])?),
        "notI#" => int_v(!int(rt, &args[0])?),
        "uncheckedIShiftL#" => int_v(int(rt, &args[0])?.wrapping_shl(int(rt, &args[1])? as u32)),
        "uncheckedIShiftRA#" => int_v(int(rt, &args[0])?.wrapping_shr(int(rt, &args[1])? as u32)),
        "uncheckedIShiftRL#" => int_v(((int(rt, &args[0])? as u64) >> (int(rt, &args[1])? as u32 & 63)) as i64),
        "int2Word#" | "word2Int#" => int_v(int(rt, &args[0])?),
        "int2Double#" => Ok(Value::Double(int(rt, &args[0])? as f64)),
        "double2Int#" => int_v(double(rt, &args[0])? as i64),
        "chr#" => Ok(Value::Char(int(rt, &args[0])? as u32)),
        "ord#" => int_v(match rt.force(&args[0])? {
            Value::Char(c) => c as i64,
            _ => return Err(RtError::Fail("internal: ord# of a non-character".into())),
        }),
        "+##" => Ok(Value::Double(double(rt, &args[0])? + double(rt, &args[1])?)),
        "-##" => Ok(Value::Double(double(rt, &args[0])? - double(rt, &args[1])?)),
        "*##" => Ok(Value::Double(double(rt, &args[0])? * double(rt, &args[1])?)),
        "/##" => Ok(Value::Double(double(rt, &args[0])? / double(rt, &args[1])?)),
        "==##" => int_v((double(rt, &args[0])? == double(rt, &args[1])?) as i64),
        "<##" => int_v((double(rt, &args[0])? < double(rt, &args[1])?) as i64),
        "<=##" => int_v((double(rt, &args[0])? <= double(rt, &args[1])?) as i64),
        ">##" => int_v((double(rt, &args[0])? > double(rt, &args[1])?) as i64),
        ">=##" => int_v((double(rt, &args[0])? >= double(rt, &args[1])?) as i64),
        "negateDouble#" => Ok(Value::Double(-double(rt, &args[0])?)),
        "sqrtDouble#" => Ok(Value::Double(double(rt, &args[0])?.sqrt())),
        // tagToEnum# is type-directed in GHC. The libraries use it at Bool only (isTrue#).
        "tagToEnum#" => Ok(bool_value(int(rt, &args[0])? != 0)),
        "dataToTag#" => match rt.force(&args[0])? {
            Value::Con(c) => int_v(con_tag(&c.name)),
            _ => int_v(0),
        },
        // Conversions between the sized integer types and Int#/Word# (GHC.Prim). Each sized
        // value is held as an Int, narrowed to its width where the conversion narrows.
        "indexCharOffAddr#" | "indexWideCharOffAddr#" | "indexWord8OffAddr#" | "indexInt8OffAddr#"
        | "indexIntOffAddr#" | "indexWordOffAddr#" => {
            let (bytes, off) = match rt.force(&args[0])? {
                Value::Addr(b, o) => (b, o),
                _ => return Err(RtError::Fail("internal: indexing a non-address".into())),
            };
            let bytes = bytes.borrow();
            let i = int(rt, &args[1])?;
            let at = |k: usize| bytes.get(k).copied().unwrap_or(0) as i64;
            let base = off as i64 + i;
            match name {
                "indexCharOffAddr#" | "indexWideCharOffAddr#" => Ok(Value::Char(at(base as usize) as u32)),
                "indexWord8OffAddr#" => int_v(at(base as usize)),
                "indexInt8OffAddr#" => int_v(at(base as usize) as u8 as i8 as i64),
                _ => {
                    let start = base as usize;
                    let mut v: u64 = 0;
                    for k in (0..8).rev() {
                        v = (v << 8) | (bytes.get(start + k).copied().unwrap_or(0) as u64);
                    }
                    int_v(v as i64)
                }
            }
        }
        "newByteArray#" | "newPinnedByteArray#" => {
            let n = int(rt, &args[0])?.max(0) as usize;
            Ok(io_result(Value::Bytes(Rc::new(RefCell::new(vec![0u8; n])))))
        }
        "unsafeFreezeByteArray#" | "unsafeThawByteArray#" => Ok(io_result(rt.force(&args[0])?)),
        "sizeofByteArray#" | "sizeofMutableByteArray#" => match rt.force(&args[0])? {
            Value::Bytes(b) => int_v(b.borrow().len() as i64),
            _ => Err(RtError::Fail("internal: size of a non-array".into())),
        },
        "getSizeofMutableByteArray#" => match rt.force(&args[0])? {
            Value::Bytes(b) => Ok(io_result(Value::Int(b.borrow().len() as i64))),
            _ => Err(RtError::Fail("internal: size of a non-array".into())),
        },
        _ if element_access(name).is_some() => {
            let (access, elem) = element_access(name).unwrap_or(("", ""));
            let (size, kind) = element_spec(elem).unwrap_or((8, "int"));
            let at = int(rt, &args[1])?;
            let scale = if element_unaligned(name) { 1 } else { size as i64 };
            let off = (at * scale).max(0) as usize;
            match (access, rt.force(&args[0])?) {
                ("index", Value::Addr(bytes, o)) => Ok(read_element(&bytes.borrow(), o + off, size, kind)),
                ("read", Value::Addr(bytes, o)) => Ok(io_result(read_element(&bytes.borrow(), o + off, size, kind))),
                ("index", Value::Bytes(b)) => Ok(read_element(&b.borrow(), off, size, kind)),
                ("read", Value::Bytes(b)) => Ok(io_result(read_element(&b.borrow(), off, size, kind))),
                ("write", Value::Bytes(b)) => {
                    let v = unbox_element(rt, rt.force(&args[2])?)?;
                    write_element(&mut b.borrow_mut(), off, size, kind, &v);
                    Ok(unit_value())
                }
                ("write", Value::Addr(bytes, o)) => {
                    let v = unbox_element(rt, rt.force(&args[2])?)?;
                    write_element(&mut bytes.borrow_mut(), o + off, size, kind, &v);
                    Ok(unit_value())
                }
                // A raw machine address (memory a C function answered): read and written in place.
                (acc, Value::Int(p)) if p != 0 => {
                    let at = (p as usize).wrapping_add(off) as *mut u8;
                    if acc == "write" {
                        let v = unbox_element(rt, rt.force(&args[2])?)?;
                        let mut tmp = vec![0u8; size];
                        write_element(&mut tmp, 0, size, kind, &v);
                        // SAFETY: the program writes memory it was given by C code.
                        unsafe { std::ptr::copy_nonoverlapping(tmp.as_ptr(), at, size) };
                        return Ok(unit_value());
                    }
                    // SAFETY: the program reads memory it was given by C code.
                    let bytes = unsafe { std::slice::from_raw_parts(at, size) };
                    let v = read_element(bytes, 0, size, kind);
                    Ok(if acc == "read" { io_result(v) } else { v })
                }
                (_, other) => Err(RtError::Fail(format!("internal: {name} on an unsupported value {}", match other { Value::Con(c) => format!("con {}/{}", c.name, c.args.len()), Value::Int(i) => format!("int {i}"), Value::Addr(..) => "addr".into(), Value::Bytes(_) => "bytes".into(), _ => "other".into() }))),
            }
        }
        "plusAddr#" => match rt.force(&args[0])? {
            Value::Addr(b, o) => Ok(Value::Addr(b, (o as i64 + int(rt, &args[1])?) as usize)),
            Value::Int(p) => int_v(p.wrapping_add(int(rt, &args[1])?)),
            _ => Err(RtError::Fail("internal: plusAddr# of a non-address".into())),
        },
        "minusAddr#" => match (rt.force(&args[0])?, rt.force(&args[1])?) {
            (Value::Addr(a, x), Value::Addr(b, y)) if Rc::ptr_eq(&a, &b) => int_v(x as i64 - y as i64),
            (Value::Int(x), Value::Int(y)) => int_v(x.wrapping_sub(y)),
            _ => Err(RtError::Fail("internal: minusAddr# of non-addresses".into())),
        },
        // Addresses compare within a buffer by offset, different buffers by their place, and raw
        // machine addresses (and the null address, however it was made) by value.
        "ltAddr#" | "leAddr#" | "gtAddr#" | "geAddr#" | "neAddr#" | "eqAddr#" => {
            let key = |v: Value| -> (usize, i64) {
                match v {
                    Value::Addr(b, o) if b.borrow().is_empty() => (0, o as i64),
                    Value::Addr(b, o) => (Rc::as_ptr(&b) as *const u8 as usize, o as i64),
                    Value::Int(x) => (0, x),
                    _ => (usize::MAX, 0),
                }
            };
            let (a, b) = (key(rt.force(&args[0])?), key(rt.force(&args[1])?));
            let r = match name {
                "eqAddr#" => a == b,
                "neAddr#" => a != b,
                "ltAddr#" => a < b,
                "leAddr#" => a <= b,
                "gtAddr#" => a > b,
                _ => a >= b,
            };
            int_v(r as i64)
        }
        _ if sized_conversion(name).is_some() => {
            let f = sized_conversion(name).unwrap_or(|x| x);
            int_v(f(int(rt, &args[0])?))
        }
        _ => match super::prims_ghc::call(rt, name, args) {
            Some(r) => r,
            None => Err(RtError::Fail(format!("internal: unknown primitive {name}"))),
        },
    }
}

/// Access to the unboxed-value helpers, for the GHC primitive table.
pub(super) fn int_of(rt: &Runtime, c: &Cell) -> Result<i64, RtError> {
    int(rt, c)
}

pub(super) fn double_of(rt: &Runtime, c: &Cell) -> Result<f64, RtError> {
    double(rt, c)
}

pub(super) fn chr_of(rt: &Runtime, c: &Cell) -> Result<char, RtError> {
    chr(rt, c)
}

pub(super) fn io_result_of(v: Value) -> Value {
    io_result(v)
}

/// A Haskell list of values.
pub fn list_value(items: Vec<Value>) -> Value {
    items.into_iter().rev().fold(
        Value::Con(Rc::new(super::ConVal {
            name: "Builtin.[]",
            args: vec![],
        })),
        |tail, head| {
            Value::Con(Rc::new(super::ConVal {
                name: "Builtin.:",
                args: vec![
                    Rc::new(RefCell::new(super::Thunk::Done(head))),
                    Rc::new(RefCell::new(super::Thunk::Done(tail))),
                ],
            }))
        },
    )
}

fn general_category_index(c: char) -> i64 {
    const LU: i64 = 0; const LL: i64 = 1; const LO: i64 = 4;
    const MN: i64 = 5; const ND: i64 = 8; const NL: i64 = 9; const NO: i64 = 10;
    const PC: i64 = 11; const PD: i64 = 12; const PS: i64 = 13; const PE: i64 = 14;
    const PI: i64 = 15; const PF: i64 = 16; const PO: i64 = 17;
    const SM: i64 = 18; const SC: i64 = 19; const SK: i64 = 20; const SO: i64 = 21;
    const ZS: i64 = 22; const ZL: i64 = 23; const ZP: i64 = 24;
    const CC: i64 = 25; const CN: i64 = 29; const CO: i64 = 28;
    if c.is_ascii() {
        return match c {
            'A'..='Z' => LU,
            'a'..='z' => LL,
            '0'..='9' => ND,
            ' ' => ZS,
            '\x00'..='\x1f' | '\x7f' => CC,
            '_' => PC,
            '-' => PD,
            '(' | '[' | '{' => PS,
            ')' | ']' | '}' => PE,
            '+' | '<' | '=' | '>' | '|' | '~' => SM,
            '$' => SC,
            '^' | '`' => SK,
            _ => PO,
        };
    }
    if c.is_uppercase() {
        LU
    } else if c.is_lowercase() {
        LL
    } else if c.is_alphabetic() {
        LO
    } else if c.is_numeric() {
        NO
    } else if c.is_whitespace() {
        ZS
    } else if c.is_control() {
        CC
    } else if c == '\u{ffff}' || c == '\u{10ffff}' {
        CN
    } else {
        let _ = (MN, NL, PC, PD, PS, PE, PI, PF, PO, SM, SC, SK, SO, ZL, ZP, CO);
        SO
    }
}

/// `(# State# RealWorld, a #)`: the result of running an IO action.
pub fn io_result(v: Value) -> Value {
    con_value("Builtin.(,)", vec![unit_value(), v])
}

fn io_message(e: &std::io::Error) -> String {
    match e.kind() {
        std::io::ErrorKind::NotFound => "does not exist (No such file or directory)".into(),
        _ => e.to_string(),
    }
}

pub fn flush_stdout() {
    let _ = std::io::stdout().flush();
}

fn bigint_from_f64(x: f64) -> BigInt {
    if x.abs() < 9.0e18 {
        return BigInt::from_i64(x as i64);
    }
    // Decompose the float exactly: x = mantissa * 2^exp.
    let bits = x.to_bits();
    let exp = ((bits >> 52) & 0x7ff) as i64 - 1075;
    let mant = (bits & ((1u64 << 52) - 1)) | (1u64 << 52);
    let base = BigInt::from_u64(mant);
    let v = if exp >= 0 {
        super::bigint::shift_left(&base, exp as usize)
    } else {
        BigInt::from_i64(0)
    };
    if x < 0.0 {
        v.neg()
    } else {
        v
    }
}

/// `show` for Float: the shortest digits that identify the single-precision value.
pub fn show_float(x: f32) -> String {
    if x.is_nan() || x.is_infinite() || x == 0.0 {
        return show_double(x as f64);
    }
    // Rust prints the shortest round-trip digits for f32 in exponent form.
    let s = format!("{:e}", x.abs());
    let (mant, exp) = s.split_once('e').expect("exponent form");
    let exp: i32 = exp.parse().expect("exponent");
    let digits: String = mant.chars().filter(|c| *c != '.').collect();
    let sign = if x < 0.0 { "-" } else { "" };
    let e = exp + 1;
    let body = if (0..=7).contains(&e) && x.abs() >= 0.1 {
        if e <= 0 {
            format!("0.{}{}", "0".repeat((-e) as usize), digits)
        } else {
            let e = e as usize;
            let (ip, fp) = if digits.len() > e {
                (digits[..e].to_string(), digits[e..].to_string())
            } else {
                (format!("{}{}", digits, "0".repeat(e - digits.len())), String::new())
            };
            format!("{ip}.{}", if fp.is_empty() { "0".to_string() } else { fp })
        }
    } else {
        let rest = if digits.len() > 1 { &digits[1..] } else { "0" };
        format!("{}.{}e{}", &digits[..1], rest, e - 1)
    };
    format!("{sign}{body}")
}

/// `show` for Double, following GHC: shortest round-trip digits, fixed notation for
/// 0.1 <= |x| < 10^7 and exponent notation otherwise.
pub fn show_double(x: f64) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "Infinity".into() } else { "-Infinity".into() };
    }
    if x == 0.0 {
        return if x.is_sign_negative() { "-0.0".into() } else { "0.0".into() };
    }
    let sign = if x < 0.0 { "-" } else { "" };
    let s = format!("{:e}", x.abs());
    let (mant, exp) = s.split_once('e').expect("exponent form");
    let exp: i32 = exp.parse().expect("exponent");
    let digits: String = mant.chars().filter(|c| *c != '.').collect();
    // value = 0.d1d2... * 10^e
    let e = exp + 1;
    let body = if (0..=7).contains(&e) && x.abs() >= 0.1 {
        if e <= 0 {
            format!("0.{}{}", "0".repeat((-e) as usize), digits)
        } else {
            let e = e as usize;
            let (int_part, frac_part) = if digits.len() > e {
                (digits[..e].to_string(), digits[e..].to_string())
            } else {
                (format!("{}{}", digits, "0".repeat(e - digits.len())), String::new())
            };
            let frac = if frac_part.is_empty() { "0".to_string() } else { frac_part };
            format!("{int_part}.{frac}")
        }
    } else {
        let rest = if digits.len() > 1 { &digits[1..] } else { "0" };
        format!("{}.{}e{}", &digits[..1], rest, e - 1)
    };
    format!("{sign}{body}")
}

/// The exact value of a finite double as a reduced `Ratio Integer` (`toRational`).
pub fn double_to_ratio(x: f64, ratio: &'static str) -> Value {
    if !x.is_finite() {
        return con_value(ratio, vec![integer_con(BigInt::zero()), integer_con(BigInt::from_i64(1))]);
    }
    let bits = x.to_bits();
    let exp_bits = ((bits >> 52) & 0x7ff) as i64;
    let frac = bits & ((1u64 << 52) - 1);
    let (mant, exp) = if exp_bits == 0 { { let z = (frac as u64).leading_zeros() as i64 - 11; ((frac << z), -1074 - z) } } else { (frac | (1u64 << 52), exp_bits - 1075) };
    let mut num = BigInt::from_u64(mant);
    let mut den = BigInt::from_i64(1);
    if exp >= 0 {
        num = super::bigint::shift_left(&num, exp as usize);
    } else {
        den = super::bigint::shift_left(&den, (-exp) as usize);
    }
    if x < 0.0 {
        num = num.neg();
    }
    let g = num.gcd(&den);
    if !g.is_zero() && g != BigInt::from_i64(1) {
        num = num.quot_rem(&g).0;
        den = den.quot_rem(&g).0;
    }
    con_value(ratio, vec![integer_con(num), integer_con(den)])
}

/// `ratioLit` for a fractional literal: the exact value as a reduced `Ratio Integer`.
pub fn ratio_literal(text: &str, ratio: &'static str) -> Value {
    let (num, den) = ratio_parts(text);
    con_value(ratio, vec![integer_con(num), integer_con(den)])
}

/// The numerator and denominator of a fractional literal's text, in lowest terms.
pub fn ratio_parts(text: &str) -> (BigInt, BigInt) {
    let (neg, body) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    let (mantissa, exp) = match body.split_once(['e', 'E']) {
        Some((m, e)) => (m, e.parse::<i64>().unwrap_or(0)),
        None => (body, 0),
    };
    let (int_digits, frac_digits) = match mantissa.split_once('.') {
        Some((i, f)) => (i, f),
        None => (mantissa, ""),
    };
    let digits = format!("{int_digits}{frac_digits}");
    let mut num = BigInt::parse(&digits).unwrap_or_else(BigInt::zero);
    let scale = exp - frac_digits.len() as i64;
    let mut den = BigInt::from_i64(1);
    if scale >= 0 {
        num = num.mul(&BigInt::pow10(scale as u32));
    } else {
        den = BigInt::pow10((-scale) as u32);
    }
    if neg {
        num = num.neg();
    }
    let g = num.gcd(&den);
    if !g.is_zero() && g != BigInt::from_i64(1) {
        num = num.quot_rem(&g).0;
        den = den.quot_rem(&g).0;
    }
    (num, den)
}

#[allow(dead_code)]
fn unused(_: &RefCell<Thunk>) {}

/// A `Word#`: the machine word, read as unsigned.
fn word_of(rt: &Runtime, c: &Cell) -> Result<u64, RtError> {
    int_or_char(rt, c).map(|i| i as u64)
}

/// An `Int#`, or a `Char#` as its code point (comparisons accept both).
fn int_or_char(rt: &Runtime, c: &Cell) -> Result<i64, RtError> {
    match rt.force(c)? {
        Value::Char(ch) => Ok(ch as i64),
        Value::Int(i) => Ok(i),
        _ => Err(RtError::Fail("internal: comparison of a non-integer".into())),
    }
}

/// The position of a constructor in its type, for `dataToTag#` (Bool only for now).
pub fn con_tag(name: &str) -> i64 {
    match name {
        "GHC.Types.False" | "Base.False" => 0,
        "GHC.Types.True" | "Base.True" => 1,
        _ => 0,
    }
}


/// The value conversion of a sized-integer primop such as `wordToWord8#` or `int8ToInt#`.
fn sized_conversion(name: &str) -> Option<fn(i64) -> i64> {
    fn w8(x: i64) -> i64 { x as u8 as i64 }
    fn w16(x: i64) -> i64 { x as u16 as i64 }
    fn w32(x: i64) -> i64 { x as u32 as i64 }
    fn i8_(x: i64) -> i64 { x as i8 as i64 }
    fn i16_(x: i64) -> i64 { x as i16 as i64 }
    fn i32_(x: i64) -> i64 { x as i32 as i64 }
    fn id(x: i64) -> i64 { x }
    Some(match name {
        "wordToWord8#" | "int8ToWord8#" | "word8ToWord8#" => w8,
        "wordToWord16#" | "int16ToWord16#" | "word16ToWord16#" => w16,
        "wordToWord32#" | "int32ToWord32#" | "word32ToWord32#" => w32,
        "intToInt8#" | "word8ToInt8#" | "int8ToInt8#" => i8_,
        "intToInt16#" | "word16ToInt16#" | "int16ToInt16#" => i16_,
        "intToInt32#" | "word32ToInt32#" | "int32ToInt32#" => i32_,
        "wordToWord64#" | "word64ToWord#" | "int64ToWord64#" | "word64ToInt64#" | "intToInt64#"
        | "int64ToInt#" | "word8ToWord#" | "word16ToWord#" | "word32ToWord#" | "int8ToInt#"
        | "int16ToInt#" | "int32ToInt#" | "word8ToInt#" | "word16ToInt#" | "word32ToInt#"
        | "word64ToInt#" | "int64ToWord#" | "word64ToWord64#" | "int64ToInt64#" => id,
        _ => return None,
    })
}

/// The access (index, read or write) and element type named by an element primop such as
/// `readWord8Array#` or `indexIntOffAddr#`.
fn element_access(name: &str) -> Option<(&str, &str)> {
    let access = ["index", "read", "write"].into_iter().find(|a| name.starts_with(a))?;
    let rest = &name[access.len()..];
    let elem = rest
        .strip_prefix("Word8ArrayAs")
        .or_else(|| rest.strip_prefix("Word8OffAddrAs"))
        .and_then(|r| r.strip_suffix('#'))
        .or_else(|| rest.strip_suffix("Array#").or_else(|| rest.strip_suffix("OffAddr#")))?;
    element_spec(elem).map(|_| (access, elem))
}

/// Whether an element primop takes a byte offset (`readWord8ArrayAsInt32#`) rather than an index.
fn element_unaligned(name: &str) -> bool {
    name.contains("Word8ArrayAs") || name.contains("Word8OffAddrAs")
}

/// The width in bytes and the representation kind of an element type.
fn element_spec(elem: &str) -> Option<(usize, &'static str)> {
    Some(match elem {
        "Char" => (1, "char8"),
        "WideChar" => (4, "char32"),
        "Int" | "Int64" => (8, "int"),
        "Int8" => (1, "int"),
        "Int16" => (2, "int"),
        "Int32" => (4, "int"),
        "Word" | "Word64" => (8, "word"),
        "Word8" => (1, "word"),
        "Word16" => (2, "word"),
        "Word32" => (4, "word"),
        "Double" => (8, "double"),
        "Float" => (4, "float"),
        // Addresses stored in memory: no memory is modelled yet, so a stored address reads back
        // as the empty address.
        "Addr" | "StablePtr" => (8, "addr"),
        _ => return None,
    })
}

/// The element at byte offset `at` of a byte buffer.
fn read_element(bytes: &[u8], at: usize, size: usize, kind: &str) -> Value {
    let mut v: u64 = 0;
    for k in (0..size).rev() {
        v = (v << 8) | bytes.get(at + k).copied().unwrap_or(0) as u64;
    }
    match kind {
        "char8" => Value::Char(v as u8 as u32),
        "char32" => Value::Char(v as u32),
        "int" => {
            let shift = 64 - 8 * size as u32;
            Value::Int(((v << shift) as i64) >> shift)
        }
        "word" => Value::Int(v as i64),
        "double" => Value::Double(f64::from_bits(v)),
        // An address read from memory is a raw machine address (or the null address).
        "addr" if v == 0 => Value::Addr(Rc::new(RefCell::new(Vec::new())), 0),
        "addr" => Value::Int(v as i64),
        _ => Value::Double(f32::from_bits(v as u32) as f64),
    }
}

/// Stores an element at byte offset `at` of a byte buffer.
/// The raw element of a sized box (`W8#` and the like around one machine value).
fn unbox_element(rt: &Runtime, v: Value) -> Result<Value, RtError> {
    match v {
        Value::Con(c) if c.args.len() == 1 => {
            let inner = rt.force(&c.args[0])?;
            unbox_element(rt, inner)
        }
        other => Ok(other),
    }
}

fn write_element(bytes: &mut [u8], at: usize, size: usize, kind: &str, v: &Value) {
    let bits: u64 = match (kind, v) {
        ("double", Value::Double(d)) => d.to_bits(),
        ("float", Value::Double(d)) => (*d as f32).to_bits() as u64,
        // An address stored in memory is the machine address of its bytes (C code may read it).
        (_, Value::Addr(b, o)) => {
            // SAFETY: only the address is taken.
            let vec = unsafe { &mut *b.as_ptr() };
            if vec.is_empty() && *o == 0 { 0 } else { vec.as_mut_ptr() as u64 + *o as u64 }
        }
        (_, Value::Bytes(b)) => {
            // SAFETY: as above.
            unsafe { (*b.as_ptr()).as_mut_ptr() as u64 }
        }
        (_, Value::Char(c)) => *c as u64,
        (_, Value::Int(i)) => *i as u64,
        (_, Value::Double(d)) => *d as i64 as u64,
        _ => 0,
    };
    for k in 0..size {
        if let Some(slot) = bytes.get_mut(at + k) {
            *slot = (bits >> (8 * k)) as u8;
        }
    }
}
