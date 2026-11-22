//! The primitive operations of the native machine, by name. Each takes its arguments (object
//! addresses, possibly unevaluated) and answers an object address, or 0 with the exception raised.
//! `Int#`, `Word#` and `Char#` values are INT and CHAR objects holding the number in their payload.

use super::heap::{self, kind, Word};
use super::Machine;
use crate::interp::bigint::{limb64, limb64_count, BigInt};
use crate::native::codegen::{CHAR, INT};

// Immutable machine integers, matching GHC's small boxed-Int range. Like native literal
// cells, these have no pointer fields and live outside the collector's semispaces.
const SMALL_INT_MIN: i64 = -16;
const SMALL_INT_MAX: i64 = 255;
const SMALL_INT_COUNT: usize = (SMALL_INT_MAX - SMALL_INT_MIN + 1) as usize;
static SMALL_INTS: [[Word; 2]; SMALL_INT_COUNT] = {
    let mut cells = [[0; 2]; SMALL_INT_COUNT];
    let mut i = 0;
    while i < SMALL_INT_COUNT {
        cells[i] = [heap::header(INT_KIND, 1, 0), (SMALL_INT_MIN + i as i64) as Word];
        i += 1;
    }
    cells
};

#[inline]
fn cached_int(n: i64) -> Option<Word> {
    if (SMALL_INT_MIN..=SMALL_INT_MAX).contains(&n) {
        Some(SMALL_INTS[(n - SMALL_INT_MIN) as usize].as_ptr() as Word)
    } else {
        None
    }
}

/// An immutable `Int#` object, reusing a static cell for a small value.
#[inline]
pub fn int_value(m: &mut Machine, n: i64) -> Word {
    if let Some(p) = cached_int(n) {
        return p;
    }
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

/// Common ordinary primitives, classified once when the program starts. Names outside this
/// bounded set retain the general dispatcher (including all encoded foreign calls).
const ORDINARY_NAMES: &[&str] = &[
    "ratioLit", "+#", "-#", "*#", "quotInt#", "remInt#", "==#", "/=#", "<#", "<=#", ">#", ">=#",
    "andI#", "orI#", "xorI#", "notI#", "negateInt#", "uncheckedIShiftL#", "uncheckedIShiftRA#",
    "plusWord#", "minusWord#", "timesWord#", "quotWord#", "remWord#", "and#", "or#", "xor#",
    "eqWord#", "neWord#", "ltWord#", "leWord#", "gtWord#", "geWord#", "uncheckedShiftL#",
    "uncheckedShiftRL#", "int2Word#", "word2Int#", "ord#", "chr#", "eqChar#", "neChar#",
    "ltChar#", "leChar#", "gtChar#", "geChar#", "int2Double#", "double2Int#", "word2Double#",
    "+##", "-##", "*##", "/##", "==##", "/=##", "<##", "<=##", ">##", ">=##", "tagToEnum#",
    "newMutVar#", "readMutVar#", "writeMutVar#", "atomicModifyMutVar2#", "casMutVar#",
    "newMVar#", "takeMVar#", "readMVar#", "putMVar#", "tryTakeMVar#", "tryReadMVar#",
    "tryPutMVar#", "isEmptyMVar#",
    // Integer arithmetic (an `Integer` accumulator calls these at every step).
    "integerAdd", "integerSub", "integerMul", "integerNegate", "integerAbs", "integerSignum",
    "integerEq", "integerLe", "integerLt", "integerQuot", "integerRem", "integerDiv", "integerMod",
    "integerQuotRem", "integerDivMod",
    "integerToInt", "integerAnd", "integerOr", "integerXor", "integerShiftL", "integerShiftR",
    "integerToDouble", "integerGcd", "integerTestBit", "integerPopCount",
];

pub(super) fn ordinary_name(name: &str) -> Option<&'static str> {
    ORDINARY_NAMES.iter().copied().find(|&candidate| candidate == name)
}

/// Entry for a name already classified as ordinary. Keep the literal's early return and the
/// failure flag's lifetime identical to `run`.
pub(super) fn run_ordinary(m: &mut Machine, id: usize, name: &'static str, nargs: usize) -> Word {
    if name == "ratioLit" {
        return force_arg(m, 0).unwrap_or(0);
    }
    m.prim_failed = false;
    // The module that ran this primitive before runs it again, without the others' name matches.
    let known = m.ordinary_module.get(id).copied().unwrap_or(0);
    if known != 0 {
        if let Some(r) = run_module(m, known, name, nargs) {
            return r;
        }
        if std::mem::take(&mut m.prim_failed) {
            return 0;
        }
    }
    for module in 1..=MODULES {
        if let Some(r) = run_module(m, module, name, nargs) {
            if let Some(slot) = m.ordinary_module.get_mut(id) {
                *slot = module;
            }
            return r;
        }
        if m.prim_failed {
            break;
        }
    }
    finish_ordinary(m, name, nargs)
}

/// The primitive modules, in the order `dispatch_ordinary` tries them.
const MODULES: u8 = 6;

fn run_module(m: &mut Machine, module: u8, name: &str, nargs: usize) -> Option<Word> {
    match module {
        1 => super::prims_num::run(m, name),
        2 => super::prims_mem::run(m, name),
        3 => super::prims_ffi::run(m, name, nargs),
        4 => super::prims_misc::run(m, name, nargs),
        5 => super::conc::run(m, name),
        _ => super::prims_integer::run(m, name),
    }
}

/// How a C call primitive runs, found by its first call: the name is parsed and the C function
/// looked up (dlsym) once, not at each call.
#[derive(Clone)]
pub enum CPlan {
    Unknown,
    /// A safe call of this C function (other threads run meanwhile).
    Safe(std::rc::Rc<CCall>, usize),
    /// The runtime's own version of the function.
    Own(std::rc::Rc<CCall>),
    /// An address import's address.
    Addr(u64),
    /// An unsafe call of this C function.
    Generic(std::rc::Rc<CCall>, usize),
    /// A `foreign import ccall "dynamic"` call through a `FunPtr` in argument 0.
    Dynamic(std::rc::Rc<CCall>),
}

pub struct CCall {
    sig: String,
    ret: char,
    sym: String,
    safe: bool,
    interruptible: bool,
}

/// A C call primitive by its program-local id (see `CPlan`).
pub fn run_ccall(m: &mut Machine, id: usize, nargs: usize) -> Word {
    m.prim_failed = false;
    match m.ccall_plans[id].clone() {
        CPlan::Dynamic(c) => {
            return super::prims_ffi::generic_dynamic(m, &c.sig, c.ret, c.safe, c.interruptible).unwrap_or(0);
        }
        CPlan::Safe(c, f) => {
            return super::prims_ffi::generic_call(m, f as *mut std::ffi::c_void, &c.sig, c.ret, true, c.interruptible).unwrap_or(0);
        }
        CPlan::Generic(c, f) => return super::prims_ffi::generic(m, f as *mut std::ffi::c_void, &c.sig, c.ret, c.safe).unwrap_or(0),
        CPlan::Addr(a) => return super::prims_ffi::address_value(m, a),
        CPlan::Own(c) => {
            if let Some(r) = super::prims_ffi::run(m, &c.sym, nargs) {
                return r;
            }
            if std::mem::take(&mut m.prim_failed) {
                return 0;
            }
        }
        CPlan::Unknown => {}
    }
    // The first call (or an own version that declined): as `run`, recording what ran.
    let name = m.prog.prim_name(id);
    let Some((sig, ret, sym)) = crate::ccall::parse(name) else { return run(m, name, nargs) };
    let c = std::rc::Rc::new(CCall {
        sig: sig.to_string(),
        ret,
        sym: sym.to_string(),
        safe: crate::ccall::is_safe(&name),
        interruptible: crate::ccall::is_interruptible(&name),
    });
    if c.sym == "dynamic" {
        m.ccall_plans[id] = CPlan::Dynamic(c.clone());
        return super::prims_ffi::generic_dynamic(m, &c.sig, c.ret, c.safe, c.interruptible).unwrap_or(0);
    }
    if c.safe {
        if let Some(f) = crate::ccall::resolve(&name) {
            m.ccall_plans[id] = CPlan::Safe(c.clone(), f as usize);
            return super::prims_ffi::generic_call(m, f, &c.sig, c.ret, true, c.interruptible).unwrap_or(0);
        }
    }
    if let Some(r) = super::prims_ffi::run(m, &c.sym, nargs) {
        m.ccall_plans[id] = CPlan::Own(c);
        return r;
    }
    if std::mem::take(&mut m.prim_failed) {
        return 0;
    }
    if let Some(a) = crate::ccall::address_import(&name) {
        m.ccall_plans[id] = CPlan::Addr(a);
        return super::prims_ffi::address_value(m, a);
    }
    if let Some(f) = crate::ccall::resolve(&name) {
        m.ccall_plans[id] = CPlan::Generic(c.clone(), f as usize);
        return super::prims_ffi::generic(m, f, &c.sig, c.ret, c.safe).unwrap_or(0);
    }
    run(m, &c.sym, nargs)
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
    // A foreign C call: the runtime's own version of the function, else the C function itself.
    if let Some((sig, ret, sym)) = crate::ccall::parse(name) {
        let (sig, sym) = (sig.to_string(), sym.to_string());
        if sym == "dynamic" {
            return super::prims_ffi::generic_dynamic(
                m,
                &sig,
                ret,
                crate::ccall::is_safe(name),
                crate::ccall::is_interruptible(name),
            )
            .unwrap_or(0);
        }
        // A safe call of a C function goes to C (so that other threads run meanwhile), not to the
        // runtime's own version of it.
        if crate::ccall::is_safe(name) {
            if let Some(f) = crate::ccall::resolve(name) {
                return super::prims_ffi::generic_call(m, f, &sig, ret, true, crate::ccall::is_interruptible(name)).unwrap_or(0);
            }
        }
        if let Some(r) = super::prims_ffi::run(m, &sym, nargs) {
            return r;
        }
        if std::mem::take(&mut m.prim_failed) {
            return 0;
        }
        if let Some(a) = crate::ccall::address_import(name) {
            return super::prims_ffi::address_value(m, a);
        }
        if let Some(f) = crate::ccall::resolve(name) {
            return super::prims_ffi::generic(m, f, &sig, ret, crate::ccall::is_safe(name)).unwrap_or(0);
        }
        return run(m, &sym, nargs);
    }
    dispatch_ordinary(m, name, nargs)
}

fn dispatch_ordinary(m: &mut Machine, name: &str, nargs: usize) -> Word {
    let result = super::prims_num::run(m, name)
        .or_else(|| super::prims_mem::run(m, name))
        .or_else(|| super::prims_ffi::run(m, name, nargs))
        .or_else(|| super::prims_misc::run(m, name, nargs))
        .or_else(|| super::conc::run(m, name))
        .or_else(|| super::prims_integer::run(m, name));
    match result {
        Some(r) => r,
        None => finish_ordinary(m, name, nargs),
    }
}

/// An ordinary primitive no module ran: failed, or a sized alias of a base operation.
fn finish_ordinary(m: &mut Machine, name: &str, nargs: usize) -> Word {
    match () {
        _ if std::mem::take(&mut m.prim_failed) => 0,
        _ if crate::interp::prims_ghc::small_alias(name).is_some() => {
            run(m, crate::interp::prims_ghc::small_alias(name).expect("an alias"), nargs)
        }
        _ => match crate::interp::prims_ghc::sized_alias(name) {
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
            let bytes: Vec<u8> = text.chars().map(|c| c as u32 as u8).collect();
            addr_literal(m, &bytes)
        }
    }
}

/// An `Addr#` literal: its bytes, NUL-terminated.
pub fn addr_literal(m: &mut Machine, bytes: &[u8]) -> Word {
    let bp = bytes_object(m, bytes);
    m.stable.push(bp);
    let p = m.alloc(3);
    let bp = m.stable.pop().unwrap_or(bp);
    m.heap_set(p, heap::header(kind::ADDR, 2, 0));
    m.heap_set(p + 8, bp);
    m.heap_set(p + 16, 0);
    p
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
        let ch = char_code_value(m, crate::syntax::lexer::char_code(*c));
        list = con_value(m, "Builtin.:", 2, &[ch, list]);
    }
    list
}

/// A `Char#` (or `Char`) object.
pub fn char_value(m: &mut Machine, c: char) -> Word {
    char_code_value(m, c as u32)
}

/// Static `CHAR` cells for ASCII/Latin-1 code points (`0..256`), matching codegen's `hsc_charlike`.
static CHARLIKE: [[Word; 2]; 256] = {
    let mut cells = [[0; 2]; 256];
    let mut i = 0;
    while i < 256 {
        cells[i] = [heap::header(CHAR, 1, 0), i as Word];
        i += 1;
    }
    cells
};

/// A boxed character by its code point: any value up to 0x10FFFF, surrogates included (GHC's
/// `Char` holds them, as the roundtrip encodings do).
#[inline(always)]
pub fn char_code_value(m: &mut Machine, c: u32) -> Word {
    if c < 256 {
        return CHARLIKE[c as usize].as_ptr() as Word;
    }
    let p = m.alloc(2);
    m.heap_set(p, heap::header(CHAR, 1, 0));
    m.heap_set(p + 8, c as Word);
    p
}

/// Static nullary constructor cells for every constructor in `RTS_CONS` (same order as `RTS_CONS`).
static STATIC_RTS_NULLARY: [[Word; 2]; crate::native::RTS_CONS.len()] = {
    let mut cells = [[0; 2]; crate::native::RTS_CONS.len()];
    let mut i = 0;
    while i < crate::native::RTS_CONS.len() {
        cells[i] = [heap::header(kind::CON, 0, i as u32), 0];
        i += 1;
    }
    cells
};

/// Indices of runtime constructors in `native::RTS_CONS`.
pub const CON_IN: u32 = 9;
pub const CON_IP: u32 = 10;
pub const CON_IS: u32 = 11;
pub const CON_FALSE: u32 = 12;
pub const CON_TRUE: u32 = 13;

/// Returns the static cell for a nullary constructor in `RTS_CONS` by index.
#[inline(always)]
pub fn static_rts_nullary(id: usize) -> Word {
    STATIC_RTS_NULLARY[id].as_ptr() as Word
}

/// Returns the static `Bool` constructor cell (`GHC.Types.True` / `GHC.Types.False`).
#[inline(always)]
pub fn bool_value(v: bool) -> Word {
    static_rts_nullary(if v { CON_TRUE as usize } else { CON_FALSE as usize })
}

/// Allocates a 1-field constructor with a known constructor id.
#[inline(always)]
pub fn con1_id(m: &mut Machine, id: u32, field: Word) -> Word {
    let p = m.alloc(2);
    m.heap_set(p, heap::header(kind::CON, 1, id));
    m.heap_set(p + 8, field);
    p
}

/// A constructor object of the named constructor with the fields given (pointers).
pub fn con_value(m: &mut Machine, name: &str, arity: usize, fields: &[Word]) -> Word {
    // The runtime's own constructors (tuples of IO results, lists, Bool) have the first ids, in
    // `RTS_CONS` order (codegen registers them first): found without hashing the name.
    let id = match crate::native::RTS_CONS.iter().position(|c| *c == name) {
        Some(i) => {
            if arity == 0 {
                return static_rts_nullary(i);
            }
            i as u32
        }
        None => m.lookup_con_id(name).unwrap_or(u32::MAX),
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
            con1_id(m, CON_IS, iv)
        }
        None => {
            let neg = b.is_neg();
            let words = limb64_count(&b);
            m.reserve(words + 4);
            let bp = m.alloc(1 + words);
            // The byte length makes it a ByteArray#, as GHC's BigNat# is.
            m.heap_set(bp, heap::header(kind::BYTES, words as u32, 8 * words as u32));
            for i in 0..words {
                m.heap_set(bp + 8 + 8 * i as Word, limb64(&b, i));
            }
            con1_id(m, if neg { CON_IN } else { CON_IP }, bp)
        }
    }
}

/// The value of an `Integer` object (IS, IP or IN), forced: the machine word of an `IS` box may be
/// a thunk. None when forcing raised.
pub fn integer_of(m: &mut Machine, p: Word) -> Option<BigInt> {
    let id = heap::aux_of(m.heap_get(p, 0));
    let field = m.heap_get(p, 1);
    if id == CON_IS {
        let iv = m.force(field)?;
        return Some(BigInt::from_i64(int_of(m, iv)));
    }
    // The limbs object is a field, which may be a thunk: it is forced before its words are read.
    let bp = m.force(field)?;
    let words = heap::size_of(m.heap_get(bp, 0)) as usize;
    // SAFETY: `bp` is a forced `BYTES` object with `words` 64-bit payload words.
    let limbs = unsafe { std::slice::from_raw_parts((bp as *const u64).add(1), words) };
    Some(BigInt::from_u64_limbs(id == CON_IN, limbs))
}

#[cfg(test)]
mod dispatch_tests {
    use super::{ordinary_name, ORDINARY_NAMES};

    #[test]
    fn classified_names_cannot_be_foreign_calls() {
        for name in ORDINARY_NAMES {
            assert!(crate::ccall::parse(name).is_none(), "{name}");
        }
        assert_eq!(ordinary_name("+#"), Some("+#"));
        assert_eq!(ordinary_name("readMutVar#"), Some("readMutVar#"));
        for name in ["ccall:II>I:abs", "!ccall:II>I:abs", "capi:I>I:abs:stdlib.h:",
                     "ccall:+#", "unknown#", "plusWord8#"] {
            assert_eq!(ordinary_name(name), None, "{name}");
        }
    }
}

#[cfg(test)]
mod small_int_tests {
    use super::{cached_int, SMALL_INTS, SMALL_INT_MAX, SMALL_INT_MIN};
    use crate::machine::{gc, heap::{self, kind, Heap, Word}};

    #[test]
    fn cached_cells_have_exact_headers_payloads_and_stable_addresses() {
        for n in SMALL_INT_MIN..=SMALL_INT_MAX {
            let p = cached_int(n).unwrap();
            assert_eq!(Some(p), cached_int(n));
            assert_eq!(p % std::mem::align_of::<Word>() as Word, 0);
            // SAFETY: cached_int returns a process-lived static cell of two aligned words.
            let cell = unsafe { std::slice::from_raw_parts(p as *const Word, 2) };
            assert_eq!(cell[0], heap::header(kind::INT, 1, 0));
            assert_eq!(cell[1], n as Word);
        }
        for n in [i64::MIN, SMALL_INT_MIN - 1, SMALL_INT_MAX + 1, i64::MAX] {
            assert_eq!(cached_int(n), None);
        }
    }

    #[test]
    fn cached_cells_survive_direct_and_indirect_gc_roots() {
        let mut heap = Heap::new(64);
        let cached = cached_int(7).unwrap();
        let before = SMALL_INTS[(7 - SMALL_INT_MIN) as usize];
        let object = heap.try_alloc(2, heap::header(kind::CON, 1, 0)).unwrap();
        heap.set_word(object, 1, cached);
        let mut roots = [cached, object];
        gc::collect(&mut heap, &mut roots, 0);
        assert_eq!(roots[0], cached);
        assert_ne!(roots[1], object);
        assert_eq!(heap.word(roots[1], 1), cached);
        assert!(!heap.contains(cached));
        assert_eq!(SMALL_INTS[(7 - SMALL_INT_MIN) as usize], before);
    }
}
