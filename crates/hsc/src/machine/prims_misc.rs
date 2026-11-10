//! Control and state primitives: exceptions raised by primitives, masking (which runs its action),
//! stable pointers, weak pointers and stable names, `MVar`s (single-threaded for now), and the
//! no-op primitives of the GHC runtime.

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
        "yield#" | "delay#" => {
            // Other threads run to their ends (cooperative scheduling).
            while m.run_one_thread() {}
            unit(m)
        }
        "fork#" | "forkOn#" => {
            // The action runs later, when another thread blocks or the program delays or yields.
            let f = if name == "forkOn#" { m.args[1] } else { m.args[0] };
            m.runq.push_back(f);
            let tid = m.runq.len() as i64 + 1;
            let t = int_value(m, tid);
            state_pair(m, t)
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
        "maskAsyncExceptions#" | "maskUninterruptible#" | "unmaskAsyncExceptions#" => {
            // The action runs with the state token; masking has no effect on this single thread.
            let f = m.args[0];
            let s = m.args[1];
            let _ = s;
            m.apply_nested(f, &[0])?
        }
        "getMaskingState#" => {
            let v = int_value(m, 0);
            state_pair(m, v)
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
        "myThreadId#" => {
            let v = int_value(m, 1);
            state_pair(m, v)
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
        "newMVar#" => {
            let r = m.alloc(2);
            m.heap_set(r, heap::header(kind::MVAR, 1, 0));
            m.heap_set(r + 8, 0);
            state_pair(m, r)
        }
        "takeMVar#" | "readMVar#" => {
            let Some(_) = force_arg(m, 0) else { return Some(0) };
            // While it is empty, queued threads run: one of them may fill it.
            while m.heap_get(m.args[0], 1) == 0 {
                if !m.run_one_thread() {
                    // No other thread can fill it: the program is blocked forever.
                    m.exc = m.error_value("thread blocked indefinitely in an MVar operation");
                    return Some(0);
                }
            }
            let mv = m.args[0];
            let v = m.heap_get(mv, 1);
            if name == "takeMVar#" {
                m.heap_set(mv + 8, 0);
            }
            state_pair(m, v)
        }
        "putMVar#" => {
            let Some(_) = force_arg(m, 0) else { return Some(0) };
            // While it is full, queued threads run: one of them may empty it.
            while m.heap_get(m.args[0], 1) != 0 {
                if !m.run_one_thread() {
                    m.exc = m.error_value("thread blocked indefinitely in an MVar operation");
                    return Some(0);
                }
            }
            let mv = m.args[0];
            let v = m.args[1];
            m.heap_set(mv + 8, v);
            unit(m)
        }
        "tryTakeMVar#" | "tryReadMVar#" => {
            let Some(_) = force_arg(m, 0) else { return Some(0) };
            let mv = m.args[0];
            let v = m.heap_get(mv, 1);
            if v == 0 {
                let zero = int_value(m, 0);
                let none = con_value(m, "Builtin.(,,)", 3, &[0, zero, 0]);
                return Some(none);
            }
            if name == "tryTakeMVar#" {
                m.heap_set(mv + 8, 0);
            }
            let one = int_value(m, 1);
            con_value(m, "Builtin.(,,)", 3, &[0, one, v])
        }
        "tryPutMVar#" => {
            let Some(_) = force_arg(m, 0) else { return Some(0) };
            let mv = m.args[0];
            if m.heap_get(mv, 1) != 0 {
                let zero = int_value(m, 0);
                return Some(state_pair(m, zero));
            }
            let v = m.args[1];
            m.heap_set(mv + 8, v);
            let one = int_value(m, 1);
            state_pair(m, one)
        }
        "isEmptyMVar#" => {
            let Some(_) = force_arg(m, 0) else { return Some(0) };
            let mv = m.args[0];
            let empty = (m.heap_get(mv, 1) == 0) as i64;
            let v = int_value(m, empty);
            state_pair(m, v)
        }
        _ => return None,
    };
    Some(r)
}
