//! The primitive operations of the native machine, by name. Each takes its arguments (object
//! addresses, possibly unevaluated) and answers an object address, or 0 with the exception raised.
//! `Int#`, `Word#` and `Char#` values are INT and CHAR objects holding the number in their payload.

use super::heap::{self, kind, Word};
use super::Machine;
use crate::interp::bigint::{limb64, limb64_count, BigInt};
use crate::native::codegen::{CHAR, INT};

/// A new `Int#` object.
pub fn int_value(m: &mut Machine, n: i64) -> Word {
    let p = m.alloc(2);
    m.heap_set(p, heap::header(INT_KIND, 1, 0));
    m.heap_set(p + 8, n as Word);
    p
}

const INT_KIND: u8 = INT;

/// The number in an `Int#` (or `Char#`, or `Word#`) object, after it is evaluated.
pub fn int_of(m: &Machine, p: Word) -> i64 {
    m.heap_get(p, 1) as i64
}

/// Forces argument `i` (in the record's argument slots, which the collector updates) and returns
/// its value. None when forcing raised.
pub fn force_arg(m: &mut Machine, i: usize) -> Option<Word> {
    let v = m.force(m.args[i])?;
    m.args[i] = v;
    Some(v)
}

/// The value of an `Int#`-like argument.
fn int_arg(m: &mut Machine, i: usize) -> Option<i64> {
    let v = force_arg(m, i)?;
    Some(int_of(m, v))
}

/// Runs a primitive operation on the arguments in the record. Answers the result object, or 0 with
/// the exception raised.
pub fn run(m: &mut Machine, name: &str, nargs: usize) -> Word {
    // A fractional literal's value is built with the literal (see `literal`, kind 2): the primitive
    // answers the `Ratio Integer` it is given.
    if name == "ratioLit" {
        return force_arg(m, 0).unwrap_or(0);
    }
    // The flag records a failed evaluation inside this primitive only: an earlier primitive that
    // failed (and answered 0 itself) must not make this one look failed.
    m.prim_failed = false;
    let result = super::prims_num::run(m, name)
        .or_else(|| super::prims_mem::run(m, name))
        .or_else(|| super::prims_ffi::run(m, name, nargs))
        .or_else(|| super::prims_misc::run(m, name, nargs))
        .or_else(|| super::conc::run(m, name))
        .or_else(|| super::prims_integer::run(m, name));
    match result {
        Some(r) => r,
        None if std::mem::take(&mut m.prim_failed) => 0,
        None => match crate::interp::prims_ghc::sized_alias(name) {
            // Int64#, Word64# and the sized Int/Word operations: the base operation, narrowed.
            Some((base, narrow)) => {
                let r = run(m, base, nargs);
                match narrow {
                    None => r,
                    Some(n) => {
                        let v = m.heap_get(r, 1) as i64;
                        let v = match n {
                            "narrow8Int#" => v as i8 as i64,
                            "narrow16Int#" => v as i16 as i64,
                            "narrow32Int#" => v as i32 as i64,
                            "narrow8Word#" => v as u8 as i64,
                            "narrow16Word#" => v as u16 as i64,
                            _ => v as u32 as i64,
                        };
                        int_value(m, v)
                    }
                }
            }
            None => {
                m.exc = m.error_value(&format!("unknown primitive {name}"));
                0
            }
        },
    }
}

/// A literal of the program: see `hsc_mk_literal`.
pub fn literal(m: &mut Machine, text: &str, kind: u64) -> Word {
    match kind {
        0 => string_list(m, text),
        1 => integer_value(m, BigInt::parse(text).unwrap_or_else(|| BigInt::from_i64(0))),
        2 => {
            // A fractional literal is a `Ratio Integer`: `numerator :% denominator`. The parts are kept
            // as roots while the next one is built.
            let (num, den) = super::super::interp::prims::ratio_parts(text);
            let nv = integer_value(m, num);
            m.stable.push(nv);
            let dv = integer_value(m, den);
            m.stable.push(dv);
            m.reserve(3);
            let dv = m.stable.pop().unwrap_or(0);
            let nv = m.stable.pop().unwrap_or(0);
            con_value(m, "GHC.Internal.Real.:%", 2, &[nv, dv])
        }
        _ => {
            let bytes = text.as_bytes();
            let bp = bytes_object(m, bytes);
            let p = m.alloc(3);
            m.heap_set(p, heap::header(kind::ADDR, 2, 0));
            m.heap_set(p + 8, bp);
            m.heap_set(p + 16, 0);
            p
        }
    }
}

/// A byte array object holding `bytes`, followed by a NUL.
pub fn bytes_object(m: &mut Machine, bytes: &[u8]) -> Word {
    let len = bytes.len() + 1;
    let words = len.div_ceil(8);
    let p = m.alloc(1 + words);
    m.heap_set(p, heap::header(kind::BYTES, words as u32, len as u32));
    for i in 0..words {
        m.heap_set(p + 8 + 8 * i as Word, 0);
    }
    for (i, b) in bytes.iter().enumerate() {
        // SAFETY: the object has `words * 8` bytes after its header, and `i < len <= that`.
        unsafe { *((p + 8) as *mut u8).add(i) = *b };
    }
    p
}

/// A Haskell string: a list of characters.
pub fn string_list(m: &mut Machine, text: &str) -> Word {
    let chars: Vec<char> = text.chars().collect();
    // Each character is a 2-word Char and a 3-word cons; the nil is 2 words. Reserved up front, so
    // the list being built is not moved by a collection in the middle.
    m.reserve(2 + chars.len() * 5);
    let nil = con_value(m, "Builtin.[]", 0, &[]);
    let mut list = nil;
    for c in chars.iter().rev() {
        let ch = char_value(m, *c);
        list = con_value(m, "Builtin.:", 2, &[ch, list]);
    }
    list
}

/// A `Char#` (or `Char`) object.
pub fn char_value(m: &mut Machine, c: char) -> Word {
    let p = m.alloc(2);
    m.heap_set(p, heap::header(CHAR, 1, 0));
    m.heap_set(p + 8, c as Word);
    p
}

/// A constructor object of the named constructor with the fields given (pointers).
pub fn con_value(m: &mut Machine, name: &str, arity: usize, fields: &[Word]) -> Word {
    let id = match m.con_ids.get(name) {
        Some(&i) => i,
        None => {
            // A constructor the program does not mention: it matches no alternative.
            u32::MAX
        }
    };
    let p = m.alloc(1 + arity);
    m.heap_set(p, heap::header(kind::CON, arity as u32, id));
    for (i, f) in fields.iter().enumerate() {
        m.heap_set(p + 8 * (1 + i as Word), *f);
    }
    p
}

/// An `Integer` object: `IS` for a machine word, else `IP`/`IN` with a byte array of its magnitude.
pub fn integer_value(m: &mut Machine, b: BigInt) -> Word {
    match b.to_i64() {
        Some(i) => {
            m.reserve(4);
            let iv = int_value(m, i);
            con_value(m, "GHC.Num.Integer.IS", 1, &[iv])
        }
        None => {
            let neg = b.is_neg();
            let mag = if neg { b.abs() } else { b.clone() };
            let limbs: Vec<u64> = (0..limb64_count(&mag)).map(|i| limb64(&mag, i)).collect();
            let words = limbs.len();
            m.reserve(words + 4);
            let bp = m.alloc(1 + words);
            m.heap_set(bp, heap::header(kind::BYTES, words as u32, 0));
            for (i, l) in limbs.iter().enumerate() {
                m.heap_set(bp + 8 + 8 * i as Word, *l);
            }
            let name = if neg { "GHC.Num.Integer.IN" } else { "GHC.Num.Integer.IP" };
            con_value(m, name, 1, &[bp])
        }
    }
}

/// The value of an `Integer` object (IS, IP or IN), forced: the machine word of an `IS` box may be
/// a thunk. None when forcing raised.
pub fn integer_of(m: &mut Machine, p: Word) -> Option<BigInt> {
    let name = m.con_name(p).to_string();
    if name == "GHC.Num.Integer.IS" {
        let field = m.heap_get(p, 1);
        let iv = m.force(field)?;
        return Some(BigInt::from_i64(int_of(m, iv)));
    }
    // The limbs object is a field, which may be a thunk: it is forced before its words are read.
    let field = m.heap_get(p, 1);
    let bp = m.force(field)?;
    let words = heap::size_of(m.heap_get(bp, 0)) as usize;
    let limbs: Vec<u64> = (0..words).map(|i| m.heap_get(bp, 1 + i)).collect();
    Some(BigInt::from_u64_limbs(name == "GHC.Num.Integer.IN", &limbs))
}
