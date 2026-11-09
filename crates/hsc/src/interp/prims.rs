//! Primitive operations. Each primitive receives its arguments as cells and forces the
//! ones it needs; the Prelude wraps them with Haskell types.

use std::io::{BufRead, Read, Write};
use std::rc::Rc;

use super::bigint::BigInt;
use super::{bool_value, con_value, string_value, unit_value, Cell, RR, RtError, Runtime, Thunk, Value};
use std::cell::RefCell;

fn int(rt: &Runtime, c: &Cell) -> Result<i64, RtError> {
    match rt.force(c)? {
        Value::Int(i) => Ok(i),
        _ => Err(RtError::Fail("internal: expected Int".into())),
    }
}

fn integer(rt: &Runtime, c: &Cell) -> Result<BigInt, RtError> {
    match rt.force(c)? {
        Value::Integer(i) => Ok((*i).clone()),
        _ => Err(RtError::Fail("internal: expected Integer".into())),
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
        Value::Char(ch) => Ok(ch),
        _ => Err(RtError::Fail("internal: expected Char".into())),
    }
}

fn int_v(i: i64) -> RR {
    Ok(Value::Int(i))
}

fn integer_v(b: BigInt) -> RR {
    Ok(Value::Integer(Rc::new(b)))
}

fn divide_by_zero<T>() -> Result<T, RtError> {
    Err(RtError::Error("divide by zero".into()))
}

/// Calls a primitive by name with its (unevaluated) arguments.
pub fn call(rt: &Runtime, name: &str, args: &[Cell]) -> RR {
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
        "integerAdd" => integer_v(integer(rt, &args[0])?.add(&integer(rt, &args[1])?)),
        "integerSub" => integer_v(integer(rt, &args[0])?.sub(&integer(rt, &args[1])?)),
        "integerMul" => integer_v(integer(rt, &args[0])?.mul(&integer(rt, &args[1])?)),
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
        "integerEq" => Ok(bool_value(integer(rt, &args[0])? == integer(rt, &args[1])?)),
        "integerLt" => Ok(bool_value(integer(rt, &args[0])?.cmp_value(&integer(rt, &args[1])?) == std::cmp::Ordering::Less)),
        "integerLe" => Ok(bool_value(integer(rt, &args[0])?.cmp_value(&integer(rt, &args[1])?) != std::cmp::Ordering::Greater)),
        "integerToInt" => int_v(integer(rt, &args[0])?.to_i64_wrapping()),
        "integerToDouble" => Ok(Value::Double(integer(rt, &args[0])?.to_f64())),
        "integerShow" => Ok(string_value(&integer(rt, &args[0])?.to_decimal())),
        "integerShiftL" => {
            let n = int(rt, &args[1])?;
            integer_v(super::bigint::shift_left(&integer(rt, &args[0])?, n as usize))
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
        "doubleRead" => {
            let s = rt.force_string(&args[0])?;
            Ok(Value::Double(s.parse::<f64>().unwrap_or(0.0)))
        }
        "doubleToRatio" => Ok(double_to_ratio(double(rt, &args[0])?)),
        "doubleIsNaN" => Ok(bool_value(double(rt, &args[0])?.is_nan())),
        "doubleIsInfinite" => Ok(bool_value(double(rt, &args[0])?.is_infinite())),

        // ----- Char -----
        "charOrd" => int_v(chr(rt, &args[0])? as i64),
        "charChr" => {
            let n = int(rt, &args[0])?;
            match char::from_u32(n as u32) {
                Some(c) if (0..=0x10FFFF).contains(&n) => Ok(Value::Char(c)),
                _ => Err(RtError::Error(format!("Base.chr: bad argument: {n}"))),
            }
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

        _ => Err(RtError::Fail(format!("internal: unknown primitive {name}"))),
    }
}

/// `IORes a`: the result of running an IO action.
pub fn io_result(v: Value) -> Value {
    con_value("Base.IORes", vec![v])
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
pub fn double_to_ratio(x: f64) -> Value {
    if !x.is_finite() {
        return con_value("Base.:%", vec![Value::Integer(Rc::new(BigInt::zero())), Value::Integer(Rc::new(BigInt::from_i64(1)))]);
    }
    let bits = x.to_bits();
    let exp_bits = ((bits >> 52) & 0x7ff) as i64;
    let frac = bits & ((1u64 << 52) - 1);
    let (mant, exp) = if exp_bits == 0 { (frac, -1074) } else { (frac | (1u64 << 52), exp_bits - 1075) };
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
    con_value("Base.:%", vec![Value::Integer(Rc::new(num)), Value::Integer(Rc::new(den))])
}

/// `ratioLit` for a fractional literal: the exact value as a reduced `Ratio Integer`.
pub fn ratio_literal(text: &str) -> Value {
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
    con_value(
        "Base.:%",
        vec![Value::Integer(Rc::new(num)), Value::Integer(Rc::new(den))],
    )
}

#[allow(dead_code)]
fn unused(_: &RefCell<Thunk>) {}
