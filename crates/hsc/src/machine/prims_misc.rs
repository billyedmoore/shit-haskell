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
        // Weak pointers are never collected here, so a finalizer runs only when the program
        // finalizes the pointer (`finalizeWeak#`, as `finalizeForeignPtr` does). A weak object is
        // REF-shaped: its value and its Haskell finalizer (0 for none), the index of its state
        // (`Machine::weaks`) in the header.
        "mkWeakNoFinalizer#" | "mkWeak#" => {
            let id = m.weaks.len() as u32;
            m.weaks.push(Default::default());
            let w = m.alloc(3);
            let (v, fin) = (m.args[1], if name == "mkWeak#" { m.args[2] } else { 0 });
            m.heap_set(w, heap::header(kind::REF, 2, id));
            m.heap_set(w + 8, v);
            m.heap_set(w + 16, fin);
            state_pair(m, w)
        }
        "deRefWeak#" => {
            let Some(_) = force_arg(m, 0) else { return Some(0) };
            let w = m.args[0];
            if weak_state(m, w).is_some_and(|s| s.dead) {
                let u = unit(m);
                let t = int_value(m, 0);
                return Some(con_value(m, "Builtin.(,,)", 3, &[0, t, u]));
            }
            // The value itself: `deRefWeak` makes the `Just`.
            let t = int_value(m, 1);
            let v = m.heap_get(m.args[0], 1);
            con_value(m, "Builtin.(,,)", 3, &[0, t, v])
        }
        "addCFinalizerToWeak#" => {
            let (fb, fo) = super::prims_ffi::addr_arg(m, 0)?;
            let (pb, po) = super::prims_ffi::addr_arg(m, 1)?;
            let has_env = super::prims_ffi::int_arg(m, 2)? != 0;
            let (eb, eo) = super::prims_ffi::addr_arg(m, 3)?;
            let _ = force_arg(m, 4)?;
            let w = m.args[4];
            let entry = (
                super::prims_ffi::raw(fb, fo) as u64,
                super::prims_ffi::raw(pb, po) as u64,
                has_env,
                super::prims_ffi::raw(eb, eo) as u64,
            );
            let added = match weak_index(m, w) {
                Some(i) if !m.weaks[i].dead => {
                    m.weaks[i].c_finalizers.push(entry);
                    1
                }
                _ => 0,
            };
            let n = int_value(m, added);
            state_pair(m, n)
        }
        "finalizeWeak#" => {
            let _ = force_arg(m, 0)?;
            let w = m.args[0];
            let alive = match weak_index(m, w) {
                Some(i) => m.weaks[i].finalize(),
                None => false,
            };
            let fin = m.heap_get(w, 2);
            let u = unit(m);
            if !alive || fin == 0 {
                let t = int_value(m, 0);
                con_value(m, "Builtin.(,,)", 3, &[0, t, u])
            } else {
                let fin = m.heap_get(m.args[0], 2);
                let t = int_value(m, 1);
                con_value(m, "Builtin.(,,)", 3, &[0, t, fin])
            }
        }
        _ => return None,
    };
    Some(r)
}

/// The index of a weak object's state (`Machine::weaks`), if it is one.
fn weak_index(m: &Machine, w: Word) -> Option<usize> {
    let h = m.heap_get(w, 0);
    let i = heap::aux_of(h) as usize;
    (heap::kind_of(h) == kind::REF && heap::size_of(h) == 2 && i < m.weaks.len()).then_some(i)
}

fn weak_state(m: &Machine, w: Word) -> Option<&crate::ccall::WeakState> {
    weak_index(m, w).map(|i| &m.weaks[i])
}
