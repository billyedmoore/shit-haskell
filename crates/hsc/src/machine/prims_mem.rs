//! Memory primitives: `MutVar#`, boxed arrays, byte arrays and `Addr#`. A byte array is a heap object
//! with its bytes after the header; an `Addr#` is an ADDR object: the byte array it points into and an
//! offset (so the collector can move the array, and the pointer follows).

use super::heap::{self, kind, Word};
use super::prims::{con_value, force_arg, int_value};
use super::Machine;
use crate::native::codegen::CHAR;

/// The state-passing answer `(# s, v #)`.
pub fn state_pair(m: &mut Machine, v: Word) -> Word {
    con_value(m, "Builtin.(,)", 2, &[0, v])
}

/// The unit object (the answer of a state-only primitive).
pub fn unit(m: &mut Machine) -> Word {
    con_value(m, "Builtin.()", 0, &[])
}

fn int_arg(m: &mut Machine, k: usize) -> Option<i64> {
    let v = force_arg(m, k)?;
    Some(m.heap_get(v, 1) as i64)
}

/// The address of the bytes of a byte array object, plus an offset: where element reads go.
fn data_ptr(m: &Machine, bytes: Word) -> Word {
    bytes + 8
}

/// Runs a memory primitive, if `name` is one.
pub fn run(m: &mut Machine, name: &str) -> Option<Word> {
    let r = match name {
        "newMutVar#" => {
            let v = m.args[0];
            let r = m.alloc(2);
            m.heap_set(r, heap::header(kind::REF, 1, 0));
            m.heap_set(r + 8, v);
            state_pair(m, r)
        }
        "readMutVar#" => {
            let Some(r) = force_arg(m, 0) else { return Some(0) };
            let v = m.heap_get(r, 1);
            state_pair(m, v)
        }
        "writeMutVar#" => {
            let Some(r) = force_arg(m, 0) else { return Some(0) };
            let v = m.args[1];
            m.heap_set(r + 8, v);
            unit(m)
        }
        "atomicModifyMutVar2#" => {
            // f applied to the old contents; the contents become the pair's first component.
            let Some(_) = force_arg(m, 0) else { return Some(0) };
            let r = m.args[0];
            let old = m.heap_get(r, 1);
            m.args[2] = old;
            let f = m.args[1];
            let Some(res) = m.apply_nested(f, &[old]) else { return Some(0) };
            // The collector may have moved everything: read the roots again.
            let (r, old) = (m.args[0], m.args[2]);
            let new_contents = m.heap_get(res, 1);
            m.heap_set(r + 8, new_contents);
            con_value(m, "Builtin.(,,)", 3, &[0, old, res])
        }
        "newArray#" => {
            let Some(n) = int_arg(m, 0) else { return Some(0) };
            let n = n.max(0) as usize;
            let a = m.alloc(1 + n);
            let init = m.args[1];
            m.heap_set(a, heap::header(kind::ARRAY, n as u32, 0));
            for i in 0..n {
                m.heap_set(a + 8 + 8 * i as Word, init);
            }
            state_pair(m, a)
        }
        "readArray#" => {
            let (Some(_), Some(i)) = (force_arg(m, 0), int_arg(m, 1)) else { return Some(0) };
            let a = m.args[0];
            let v = m.heap_get(a, 1 + i as usize);
            state_pair(m, v)
        }
        "writeArray#" => {
            let (Some(_), Some(i)) = (force_arg(m, 0), int_arg(m, 1)) else { return Some(0) };
            let a = m.args[0];
            let v = m.args[2];
            m.heap_set(a + 8 + 8 * i as Word, v);
            unit(m)
        }
        "indexArray#" => {
            let (Some(_), Some(i)) = (force_arg(m, 0), int_arg(m, 1)) else { return Some(0) };
            let a = m.args[0];
            m.heap_get(a, 1 + i as usize)
        }
        "sizeofArray#" | "sizeofMutableArray#" => {
            let Some(a) = force_arg(m, 0) else { return Some(0) };
            let n = heap::size_of(m.heap_get(a, 0)) as i64;
            int_value(m, n)
        }
        "unsafeFreezeArray#" | "unsafeThawArray#" | "unsafeFreezeByteArray#" | "unsafeThawByteArray#" => {
            let Some(a) = force_arg(m, 0) else { return Some(0) };
            state_pair(m, a)
        }
        "newByteArray#" | "newPinnedByteArray#" | "newAlignedPinnedByteArray#" => {
            let Some(n) = int_arg(m, 0) else { return Some(0) };
            let n = n.max(0) as usize;
            let words = n.div_ceil(8);
            let b = m.alloc(1 + words);
            m.heap_set(b, heap::header(kind::BYTES, words as u32, n as u32));
            for i in 0..words {
                m.heap_set(b + 8 + 8 * i as Word, 0);
            }
            state_pair(m, b)
        }
        "sizeofByteArray#" | "sizeofMutableByteArray#" | "getSizeofMutableByteArray#" => {
            let Some(b) = force_arg(m, 0) else { return Some(0) };
            let n = m.heap_get(b, 0) >> 32;
            let v = int_value(m, n as i64);
            if name == "getSizeofMutableByteArray#" { state_pair(m, v) } else { v }
        }
        "byteArrayContents#" | "mutableByteArrayContents#" => {
            let Some(_) = force_arg(m, 0) else { return Some(0) };
            let p = m.alloc(3);
            let b = m.args[0];
            m.heap_set(p, heap::header(kind::ADDR, 2, 0));
            m.heap_set(p + 8, b);
            m.heap_set(p + 16, 0);
            p
        }
        "nullAddr#" => {
            let p = m.alloc(3);
            m.heap_set(p, heap::header(kind::ADDR, 2, 0));
            m.heap_set(p + 8, 0);
            m.heap_set(p + 16, 0);
            p
        }
        "plusAddr#" => {
            let (Some(_), Some(n)) = (force_arg(m, 0), int_arg(m, 1)) else { return Some(0) };
            let a = m.args[0];
            let base = m.heap_get(a, 1);
            let off = m.heap_get(a, 2) as i64 + n;
            let p = m.alloc(3);
            m.heap_set(p, heap::header(kind::ADDR, 2, 0));
            m.heap_set(p + 8, base);
            m.heap_set(p + 16, off as Word);
            p
        }
        "eqAddr#" | "minusAddr#" => {
            let (Some(a), Some(b)) = (force_arg(m, 0), force_arg(m, 1)) else { return Some(0) };
            let (ba, oa) = (m.heap_get(a, 1), m.heap_get(a, 2) as i64);
            let (bb, ob) = (m.heap_get(b, 1), m.heap_get(b, 2) as i64);
            if name == "eqAddr#" {
                int_value(m, (ba == bb && oa == ob) as i64)
            } else {
                int_value(m, oa - ob)
            }
        }
        _ => return element_access(m, name),
    };
    Some(r)
}

/// The element types of byte arrays and addresses: the name, the width in bytes, the kind.
fn element(name: &str) -> Option<(usize, &'static str)> {
    Some(match name {
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
        _ => return None,
    })
}

/// `index/read/write <Elem>Array#` and `<Elem>OffAddr#`. Arguments are forced first; then the bytes
/// are read or written at a raw address, which holds until the next allocation (the result object is
/// built after the bytes are copied).
fn element_access(m: &mut Machine, name: &str) -> Option<Word> {
    let (access, rest) = ["index", "read", "write"].iter().find_map(|a| name.strip_prefix(a).map(|r| (*a, r)))?;
    let (elem, on_addr) = if let Some(e) = rest.strip_suffix("OffAddr#") {
        (e, true)
    } else if let Some(e) = rest.strip_suffix("Array#") {
        (e, false)
    } else {
        return None;
    };
    let (width, ekind) = element(elem)?;
    // Arguments: the array (or address), the index, and for writes the value.
    force_arg(m, 0)?;
    int_arg(m, 1)?;
    if access == "write" {
        force_arg(m, 2)?;
    }
    let target = m.args[0];
    let index = m.heap_get(m.args[1], 1) as i64;
    let (base, base_off) = if on_addr { (m.heap_get(target, 1), m.heap_get(target, 2) as i64) } else { (target, 0) };
    let addr = (base + 8) as i64 + base_off + index * width as i64;
    if access == "write" {
        let v = m.args[2];
        let bits = m.heap_get(v, 1);
        let bytes = bits.to_le_bytes();
        // SAFETY: the element lies within the byte array, which does not move during this copy.
        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), addr as *mut u8, width) };
        return Some(unit(m));
    }
    // Read the bytes first: building the result may collect.
    let mut buf = [0u8; 8];
    // SAFETY: the element lies within the array, which is live (the arguments are roots).
    unsafe { std::ptr::copy_nonoverlapping(addr as *const u8, buf.as_mut_ptr(), width) };
    let bits = u64::from_le_bytes(buf);
    let v = match ekind {
        "char8" => char_object(m, bits as u8 as char),
        "char32" => char_object(m, char::from_u32(bits as u32).unwrap_or('\u{fffd}')),
        "int" => {
            let v = match width {
                1 => bits as u8 as i8 as i64,
                2 => bits as u16 as i16 as i64,
                4 => bits as u32 as i32 as i64,
                _ => bits as i64,
            };
            int_value(m, v)
        }
        "word" => {
            let v = match width {
                1 => bits as u8 as i64,
                2 => bits as u16 as i64,
                4 => bits as u32 as i64,
                _ => bits as i64,
            };
            int_value(m, v)
        }
        "double" => super::prims_num::double_value(m, f64::from_bits(bits)),
        _ => super::prims_num::double_value(m, f32::from_bits(bits as u32) as f64),
    };
    Some(if access == "read" { state_pair(m, v) } else { v })
}

/// A `Char#` object.
fn char_object(m: &mut Machine, c: char) -> Word {
    let p = m.alloc(2);
    m.heap_set(p, heap::header(CHAR, 1, 0));
    m.heap_set(p + 8, c as Word);
    p
}

