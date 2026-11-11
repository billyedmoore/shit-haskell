//! Control and state primitives: exceptions raised by primitives, stable pointers, weak pointers
//! and stable names, and the no-op primitives of the GHC runtime (threads are in `conc`).

use super::heap::{self, kind, Word};
use super::prims::{con_value, force_arg, int_value};
use super::prims_mem::{state_pair, unit};
use super::Machine;

/// Runs a primitive of this family, if `name` is one.
pub fn run(m: &mut Machine, name: &str, nargs: usize) -> Option<Word> {
    let _ = nargs;
    let r = match name {
        "hsc_exit" => {
            // The program's own exit (the tests of the native machine use it to report a value).
            let v = force_arg(m, 0)?;
            std::process::exit(m.heap_get(v, 1) as i32);
        }
        "raise#" | "raiseIO#" => {
            m.exc = m.args[0];
            0
        }
        "reallyUnsafePtrEquality#" | "reallyUnsafePtrEquality" | "unsafePtrEquality#" => {
            let same = m.args[0] == m.args[1];
            int_value(m, same as i64)
        }
        "noDuplicate#" | "touch#" | "realWorld#" | "proxy#" | "void#" | "labelThread#" => {
            unit(m)
        }
        "runRW#" => {
            // runRW# f: f applied to the state token, answering its result.
            let f = force_arg(m, 0)?;
            m.apply_nested(f, &[0])?
        }
        "unsafeCoerce" | "unsafeCoerce#" | "coerce" | "lazy" | "oneShot" | "inline" | "noinline" => {
            // Identity on the value: forced, as the caller demands the value.
            let v = force_arg(m, 0)?;
            v
        }
        "keepAlive#" => {
            // keepAlive# x s f: f applied to the state token.
            let f = m.args[2];
            m.apply_nested(f, &[0])?
        }
        "tagToEnum#" => {
            // Bool, as the library uses it: the answer is the constructor for the number.
            let Some(n) = force_arg(m, 0).map(|v| m.heap_get(v, 1) as i64) else { return Some(0) };
            let name = if n != 0 { "GHC.Types.True" } else { "GHC.Types.False" };
            con_value(m, name, 0, &[])
        }
        "dataToTag#" => {
            let Some(v) = force_arg(m, 0) else { return Some(0) };
            let name = m.con_name(v).to_string();
            let tag = crate::interp::prims::con_tag(&name);
            int_value(m, tag)
        }
        "makeStablePtr#" => {
            let v = m.args[0];
            m.stable.push(v);
            let idx = (m.stable.len() - 1) as i64;
            let ip = int_value(m, idx);
            state_pair(m, ip)
        }
        "deRefStablePtr#" => {
            let i = force_arg(m, 0)?;
            let idx = m.heap_get(i, 1) as usize;
            let v = m.stable.get(idx).copied().unwrap_or(0);
            if v == 0 {
                m.exc = m.error_value("deRefStablePtr: freed");
                return Some(0);
            }
            state_pair(m, v)
        }
        "makeStableName#" => {
            let v = m.args[0];
            let _ = v;
            let n = int_value(m, 1);
            state_pair(m, n)
        }
        "mkWeakNoFinalizer#" | "mkWeak#" => {
            // The weak pointer is the key's value, kept alive: a REF-shaped object holding the value.
            let v = m.args[1];
            let w = m.alloc(2);
            m.heap_set(w, heap::header(kind::REF, 1, 0));
            m.heap_set(w + 8, v);
            state_pair(m, w)
        }
        "deRefWeak#" => {
            let Some(_) = force_arg(m, 0) else { return Some(0) };
            let w = m.args[0];
            let v = m.heap_get(w, 1);
            let just = con_value(m, "GHC.Internal.Maybe.Just", 1, &[v]);
            let t = int_value(m, 1);
            con_value(m, "Builtin.(,,)", 3, &[0, t, just])
        }
        _ => return None,
    };
    Some(r)
}
