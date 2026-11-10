//! The primitive operations of the native machine, by name. Each takes its arguments (object
//! addresses, possibly unevaluated) and answers an object address, or 0 with the exception raised.
//! `Int#`, `Word#` and `Char#` values are INT and CHAR objects holding the number in their payload.

use super::heap::{self, kind, Word};
use super::Machine;
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
    let _ = nargs;
    match name {
        "hsc_exit" => match int_arg(m, 0) {
            Some(code) => std::process::exit(code as i32),
            None => 0,
        },
        "+#" | "-#" | "*#" | "==#" | "/=#" | "<#" | "<=#" | ">#" | ">=#" | "quotInt#" | "remInt#" => {
            let (Some(a), Some(b)) = (int_arg(m, 0), int_arg(m, 1)) else { return 0 };
            let r = match name {
                "+#" => a.wrapping_add(b),
                "-#" => a.wrapping_sub(b),
                "*#" => a.wrapping_mul(b),
                "==#" => (a == b) as i64,
                "/=#" => (a != b) as i64,
                "<#" => (a < b) as i64,
                "<=#" => (a <= b) as i64,
                ">#" => (a > b) as i64,
                ">=#" => (a >= b) as i64,
                "quotInt#" => a.wrapping_div(b),
                _ => a.wrapping_rem(b),
            };
            int_value(m, r)
        }
        "negateInt#" => match int_arg(m, 0) {
            Some(a) => int_value(m, a.wrapping_neg()),
            None => 0,
        },
        _ => {
            let _ = (kind::INT, CHAR);
            m.exc = m.error_value(&format!("unknown primitive {name}"));
            0
        }
    }
}
