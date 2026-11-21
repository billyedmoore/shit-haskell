//! Code generation: STG to x86-64 assembly (AT&T syntax).
//!
//! Registers (see NATIVE.md): `rbp` is the Haskell stack pointer, `r12`/`r13` the heap pointer and
//! its limit, `r14` the value or closure being entered, `rbx` the machine record. A body's frame is
//! `s` words: the closure (slot 0), the `a` arguments (slots 1..=a), then the locals. The arguments
//! of a call are pushed below the caller's continuation, so the callee's frame ends where its
//! continuation (a return address) begins. A return address's info (two words before it) gives the
//! kind and size of the frame directly above it, which is what the collector walks.
//!
//! A case pushes its return address below the current frame, whose info gives that frame's size.
//! Its scrutinee is usually simple (the lowering makes nested cases join points), but may be a
//! case itself (`seq`): that case's return address then sits below the outer one's, and its info
//! gives an empty frame (the collector reads the outer return address next).

use std::collections::HashMap;
use std::fmt::Write;

use super::stg::{Atom, SAlt, SBind, SClosure, SExpr, SLit, SProgram, MAX_REG_RAW_ARGS, RAW, RAW_CHAR};
use crate::core::Lit;

/// Header kinds (mirrors `machine::heap::kind`).
pub const CON: u8 = 1;
pub const INT: u8 = 2;
pub const DOUBLE: u8 = 3;
pub const CHAR: u8 = 4;
pub const FUN: u8 = 6;
pub const PAP: u8 = 7;
pub const THUNK: u8 = 8;
pub const IND: u8 = 9;
pub const REF: u8 = 11;

/// Frame kinds, as the collector reads them from the info of a return address.
pub mod frame {
    /// A body's frame. Its size word is `words | raw << 32`: the lowest `raw` of its `words` hold
    /// raw (unboxed) numbers, the rest pointers. The same encoding gives the words at the top of
    /// the stack that the runtime is told about (`mach::TOP`).
    pub const BODY: u64 = 1;

    /// The words of a body frame (or of the top of the stack), from its size word.
    pub fn words(size: u64) -> u64 {
        size & 0xffff_ffff
    }

    /// The raw (non-pointer) words at its low end.
    pub fn raw(size: u64) -> u64 {
        size >> 32
    }
    /// An update frame: the thunk being evaluated (one pointer).
    pub const UPDATE: u64 = 2;
    /// A catch frame: the handler (one pointer, the other word is unused).
    pub const CATCH: u64 = 3;
    /// An apply frame: a count word, then that many pointers.
    pub const APPLY: u64 = 4;
    /// The bottom of a stack segment: returns to the runtime.
    pub const STOP: u64 = 5;
    /// A mask frame: the masking state to put back (one word, not a pointer).
    pub const MASK: u64 = 6;
}

/// Machine record offsets (`machine::Machine` is `#[repr(C)]` with these fields).
pub mod mach {
    pub const HP: i64 = 0;
    pub const HPLIM: i64 = 8;
    pub const SP: i64 = 16;
    pub const TOP: i64 = 24;
    pub const GLOBALS: i64 = 32;
    pub const R1: i64 = 40;
    pub const NARGS: i64 = 48;
    pub const CODE: i64 = 56;
    pub const ARGS: i64 = 64;
    pub const TID: i64 = 192;
}

/// Where an expression's result goes.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Mode {
    /// Tail position: the body's frame is popped and the result goes to the continuation above it.
    Tail,
    /// A case scrutinee: the return address of the case is on top of the stack.
    Scrut,
}

/// The frame of the body being generated.
#[derive(Clone, Copy)]
struct Frame {
    /// Total slots: raw slots, locals, optional closure slot, and stack arguments.
    s: u32,
    /// The stack arguments.
    a: u32,
    /// The raw slots (unboxed numbers, at the frame's low end).
    r: u32,
    /// Whether slot 0 (the closure pointer) is present in the frame.
    has_closure: bool,
}

impl Frame {
    /// The offset, from the frame's base (its lowest address), of a slot. Raw slots come first,
    /// then the locals, the optional closure (slot 0) and the stack arguments (slots 1..=a).
    fn off(&self, k: u32) -> i64 {
        if k & RAW != 0 {
            return 8 * (k & !(RAW | RAW_CHAR)) as i64;
        }
        let (s, a, r, k) = (self.s as i64, self.a as i64, self.r as i64, k as i64);
        if k == 0 {
            assert!(self.has_closure, "internal: slot 0 accessed in closure-less frame");
            8 * (s - a - 1)
        } else if k <= a {
            8 * (s - a + k - 1)
        } else {
            8 * (r + k - a - 1)
        }
    }

    /// The size word that describes the frame to the runtime (`frame::BODY`).
    fn size_word(&self) -> u64 {
        self.s as u64 | (self.r as u64) << 32
    }
}

/// Where the code of an expression stands relative to its frame.
#[derive(Clone, Copy)]
struct Pos {
    f: Frame,
    mode: Mode,
    /// Words pushed below the frame's base: one for a scrutinee's return address, more for arguments.
    pushed: i64,
}

impl Pos {
    /// The byte offset from `%rbp` of a slot.
    #[inline]
    fn slot_off(&self, k: u32) -> i64 {
        self.f.off(k) + 8 * self.pushed
    }

    /// The offset from the stack pointer of a slot.
    fn slot(&self, k: u32) -> String {
        format!("{}(%rbp)", self.slot_off(k))
    }
}

pub struct Gen {
    out: String,
    labels: u64,
    con_ids: HashMap<String, u32>,
    con_names: Vec<String>,
    prim_ids: HashMap<String, u32>,
    prim_names: Vec<String>,
    /// Entry labels, fixed arities, and `uses_closure` in program-global index order.
    known_globals: Vec<(u32, String, bool)>,
    /// Closure bodies still to generate: (label, closure, name for debugging).
    pending: Vec<(String, SClosure, String)>,
    /// The bodies in the order they are emitted (their addresses increase): (label, name).
    body_names: Vec<(String, String)>,
    /// The name of the body being generated.
    current_name: String,
    /// Self-tail-loop metadata for the body being generated: `(entry_label, loop_label, stack_arity)`.
    current_self: Option<(String, String, u32)>,
    /// Static objects (label, header, payload: a number or a label).
    statics: Vec<(String, u64, String)>,
    /// Deduplicated static numeric/char/IS literal cells, keyed by `(header, payload_bits)`.
    static_cells: HashMap<(u64, u64), String>,
    /// Shared static cells for constructors without fields, indexed by constructor ID.
    nullary_cons: HashMap<u32, String>,
    /// Global indices whose body is a nullary constructor (loaded directly via `leaq`).
    nullary_globals: HashMap<u32, String>,
    /// Byte strings (label, bytes).
    strings: Vec<(String, Vec<u8>)>,
    /// Deduplicated byte strings.
    string_pool: HashMap<Vec<u8>, String>,
    /// The frame slots still used after the scrutinee being generated returns (innermost last).
    live: Vec<std::collections::HashSet<u32>>,
    /// The slots each join point's code uses (a jump to it uses them).
    join_uses: HashMap<u32, std::collections::HashSet<u32>>,
    /// The worker entries (label, raw parameters, returns raw, uses_closure) in program-global index order.
    workers: Vec<Option<(String, Vec<bool>, bool, bool)>>,
    /// The body being generated returns its result raw (see `stg::raw_returns`).
    ret_raw: bool,
    /// The raw slot an inline primitive's number goes to (a case binder kept unboxed).
    raw_dest: Option<u32>,
    /// Where an inline primitive's value goes when it is a case scrutinee computed in place.
    fast_done: Option<String>,
    /// The next primitive is a scrutinee whose fast path is already emitted: only the slow path.
    slow_only: bool,
    /// The static INT cells 0 and 1 (made when first used).
    bool_cells: Option<(String, String)>,
    /// The code labels and parameter slots of the join points generated so far.
    joins: HashMap<u32, (String, Vec<u32>)>,
    /// Raw slots whose numbers are in registers (see `CACHE_REGS`), oldest first.
    cache: Vec<Cached>,
    /// The raw slots each expression of the body being generated uses (sorted; by address).
    raw_uses: HashMap<*const SExpr, Vec<u32>>,
    /// Single-capture selector thunks `(label, con_id, field_word)` for GC short-circuiting.
    selectors: Vec<(String, u32, u32)>,
}

/// A raw slot's number held in a register; `dirty` when the slot itself may not have it yet.
#[derive(Clone, Copy)]
struct Cached {
    slot: u32,
    reg: &'static str,
    dirty: bool,
}

/// A raw operand: a register or an immediate.
#[derive(Clone, Copy)]
enum Src {
    Reg(&'static str),
    Imm(i64),
}

/// The registers that hold raw slots' numbers through straight-line arithmetic. They are
/// caller-saved and nothing else in a body keeps values in them across code it generates, so
/// they are stored back (`Gen::flush`) before any code that calls out, allocates or jumps to a
/// label reached from elsewhere.
const CACHE_REGS: [&str; 6] = ["%rsi", "%rdi", "%r8", "%r9", "%r10", "%r11"];

/// The low byte of a cache register (for `setcc`).
fn byte_reg(r: &str) -> &'static str {
    match r {
        "%rsi" => "%sil",
        "%rdi" => "%dil",
        "%r8" => "%r8b",
        "%r9" => "%r9b",
        "%r10" => "%r10b",
        "%r11" => "%r11b",
        _ => panic!("internal: no byte register for {r}"),
    }
}

/// The 32-bit part of a cache register.
fn low32(r: &str) -> &'static str {
    match r {
        "%rsi" => "%esi",
        "%rdi" => "%edi",
        "%r8" => "%r8d",
        "%r9" => "%r9d",
        "%r10" => "%r10d",
        "%r11" => "%r11d",
        _ => panic!("internal: no 32-bit register for {r}"),
    }
}

/// The multiplier and shift that divide a signed 64-bit number by `d` (2 <= |d| < 2^62) with a
/// high multiplication (Hacker's Delight, 10-1): `q = (mulhi(m, n) + (n if d > 0 and m < 0) -
/// (n if d < 0 and m > 0)) >> s`, plus one when that is negative.
fn signed_magic(d: i64) -> (i64, u32) {
    // Unsigned 64-bit arithmetic, wrapping as in the reference.
    let two63: u64 = 1 << 63;
    let ad = d.unsigned_abs();
    let t = two63 + ((d as u64) >> 63);
    let anc = t - 1 - t % ad;
    let mut p = 63u32;
    let (mut q1, mut r1) = (two63 / anc, two63 - (two63 / anc) * anc);
    let (mut q2, mut r2) = (two63 / ad, two63 - (two63 / ad) * ad);
    loop {
        p += 1;
        q1 = q1.wrapping_mul(2);
        r1 = r1.wrapping_mul(2);
        if r1 >= anc {
            q1 = q1.wrapping_add(1);
            r1 = r1.wrapping_sub(anc);
        }
        q2 = q2.wrapping_mul(2);
        r2 = r2.wrapping_mul(2);
        if r2 >= ad {
            q2 = q2.wrapping_add(1);
            r2 = r2.wrapping_sub(ad);
        }
        let delta = ad - r2;
        if !(q1 < delta || (q1 == delta && r1 == 0)) {
            break;
        }
    }
    let m = q2.wrapping_add(1) as i64;
    let m = if d < 0 { m.wrapping_neg() } else { m };
    (m, p - 64)
}

/// A literal divisor that `signed_magic` handles.
fn magic_divisor(a: &Atom) -> Option<i64> {
    match a {
        Atom::Lit(SLit::Int(d)) if d.unsigned_abs() >= 2 && d.unsigned_abs() < 1 << 62 => Some(*d),
        _ => None,
    }
}

/// Whether `name args` with a raw binder is computed inline from raw numbers and literals alone,
/// with no slow path (no cell checks, no faulting division): see `Gen::raw_op`.
fn total_raw_op(name: &str, args: &[Atom]) -> bool {
    let Some(k) = int_op(name) else { return false };
    let arity = if matches!(k, K::Un(_) | K::Ord | K::Chr | K::TagToEnum) { 1 } else { 2 };
    let in_char = matches!(k, K::CharCmp(_) | K::Ord);
    let num = |a: &Atom| match a {
        Atom::Slot(k) => k & RAW != 0,
        Atom::Lit(SLit::Int(_)) => !in_char,
        Atom::Lit(SLit::Char(_)) => in_char,
        _ => false,
    };
    args.len() == arity
        && k != K::TagToEnum
        && args.iter().all(num)
        && (!matches!(k, K::Div(_)) || magic_divisor(&args[1]).is_some())
}

/// The inline `Int#`/`Word#`/`Char#` operations (see `Gen::int_fast_path`).
#[derive(PartialEq, Clone, Copy)]
enum K { Bin(&'static str), Cmp(&'static str), Shift(&'static str), Div(bool), Un(&'static str), CharCmp(&'static str), Ord, Chr, TagToEnum }

fn int_op(name: &str) -> Option<K> {
    Some(match name {
        "+#" => K::Bin("addq"), "-#" => K::Bin("subq"), "*#" => K::Bin("imulq"),
        "andI#" => K::Bin("andq"), "orI#" => K::Bin("orq"), "xorI#" => K::Bin("xorq"),
        "==#" => K::Cmp("sete"), "/=#" => K::Cmp("setne"), "<#" => K::Cmp("setl"),
        "<=#" => K::Cmp("setle"), ">#" => K::Cmp("setg"), ">=#" => K::Cmp("setge"),
        "uncheckedIShiftL#" => K::Shift("shlq"), "uncheckedIShiftRA#" => K::Shift("sarq"),
        "uncheckedIShiftRL#" => K::Shift("shrq"),
        "quotInt#" => K::Div(true), "remInt#" => K::Div(false),
        "negateInt#" => K::Un("negq"), "notI#" => K::Un("notq"),
        // Char# comparisons are on the (unsigned) code points.
        "eqChar#" => K::CharCmp("sete"), "neChar#" => K::CharCmp("setne"), "ltChar#" => K::CharCmp("setb"),
        "leChar#" => K::CharCmp("setbe"), "gtChar#" => K::CharCmp("seta"), "geChar#" => K::CharCmp("setae"),
        "ord#" => K::Ord, "chr#" => K::Chr,
        // Bool, as the runtime's primitive takes it.
        "tagToEnum#" => K::TagToEnum,
        // Word# shares Int#'s cells; its comparisons are unsigned.
        "plusWord#" => K::Bin("addq"), "minusWord#" => K::Bin("subq"), "timesWord#" => K::Bin("imulq"),
        "and#" => K::Bin("andq"), "or#" => K::Bin("orq"), "xor#" => K::Bin("xorq"),
        "eqWord#" => K::Cmp("sete"), "neWord#" => K::Cmp("setne"), "ltWord#" => K::Cmp("setb"),
        "leWord#" => K::Cmp("setbe"), "gtWord#" => K::Cmp("seta"), "geWord#" => K::Cmp("setae"),
        "uncheckedShiftL#" => K::Shift("shlq"), "uncheckedShiftRL#" => K::Shift("shrq"),
        _ => return None,
    })
}

/// The header word of an object: `kind | size << 8 | aux << 32`.
fn header(kind: u8, size: u64, aux: u64) -> u64 {
    kind as u64 | (size << 8) | (aux << 32)
}

/// The words a closure occupies: its header, code word and captures (at least two).
fn closure_words(c: &SClosure) -> usize {
    (c.captures.len() + 2).max(2)
}

/// The heap words an `SBind` allocates (0 for static nullary constructors).
fn bind_words(b: &SBind) -> usize {
    match b {
        SBind::Closure(c) => closure_words(c),
        SBind::Con(_, args) if args.is_empty() => 0,
        SBind::Con(_, args) => (args.len() + 1).max(2),
    }
}

/// Whether an atom is a raw slot (an unboxed number).
fn is_raw(a: &Atom) -> bool {
    matches!(a, Atom::Slot(k) if k & RAW != 0)
}

/// The cell kind of a primitive's raw operands, when they are boxed for the runtime.
fn operand_kind(prim: &str) -> u8 {
    if prim.ends_with("Char#") || prim == "ord#" {
        CHAR
    } else {
        INT
    }
}

/// A slot use that stands for every slot (nothing may be cleared).
const ALL_LIVE: u32 = u32::MAX;

impl Gen {
    fn new() -> Gen {
        Gen {
            out: String::new(),
            labels: 0,
            con_ids: HashMap::new(),
            con_names: Vec::new(),
            prim_ids: HashMap::new(),
            prim_names: Vec::new(),
            known_globals: Vec::new(),
            pending: Vec::new(),
            body_names: Vec::new(),
            current_name: String::new(),
            current_self: None,
            statics: Vec::new(),
            static_cells: HashMap::new(),
            nullary_cons: HashMap::new(),
            nullary_globals: HashMap::new(),
            strings: Vec::new(),
            string_pool: HashMap::new(),
            joins: HashMap::new(),
            bool_cells: None,
            fast_done: None,
            slow_only: false,
            raw_dest: None,
            workers: Vec::new(),
            ret_raw: false,
            live: Vec::new(),
            join_uses: HashMap::new(),
            cache: Vec::new(),
            raw_uses: HashMap::new(),
            selectors: Vec::new(),
        }
    }

    #[inline]
    fn next_id(&mut self) -> u64 {
        self.labels += 1;
        self.labels
    }

    fn label(&mut self, prefix: &str) -> String {
        let id = self.next_id();
        format!(".L{prefix}{id}")
    }

    fn con_id(&mut self, name: &str) -> u32 {
        if let Some(&i) = self.con_ids.get(name) {
            return i;
        }
        let i = self.con_names.len() as u32;
        self.con_names.push(name.to_string());
        self.con_ids.insert(name.to_string(), i);
        i
    }

    fn prim_id(&mut self, name: &str) -> u32 {
        if let Some(&i) = self.prim_ids.get(name) {
            return i;
        }
        let i = self.prim_names.len() as u32;
        self.prim_names.push(name.to_string());
        self.prim_ids.insert(name.to_string(), i);
        i
    }

    fn emit(&mut self, line: &str) {
        self.out.push_str("    ");
        self.out.push_str(line);
        self.out.push('\n');
    }

    fn put_label(&mut self, l: &str) {
        self.out.push_str(l);
        self.out.push_str(":\n");
    }

    /// Generates the assembly of a whole program.
    pub fn program(prog: &SProgram) -> String {
        let mut g = Gen::new();
        g.out.reserve((prog.globals.len() * 1024).max(65536));
        for c in super::RTS_CONS {
            g.con_id(c);
        }
        let used_workers = collect_called_workers(prog);
        let mut global_labels = Vec::with_capacity(prog.globals.len());
        for (gi, gl) in prog.globals.iter().enumerate() {
            let l = g.label("g");
            global_labels.push(l.clone());
            // Register every entry before emitting bodies, including forward/mutual calls.
            let arity = if gl.closure.captures.is_empty() { gl.closure.arity } else { 0 };
            g.known_globals.push((arity, l.clone(), gl.closure.uses_closure));
            if gl.closure.arity == 0 && gl.closure.captures.is_empty() {
                match &gl.closure.body {
                    SExpr::Con(name, args) if args.is_empty() => {
                        let id = g.con_id(name);
                        let lbl = g.nullary_con(id);
                        g.nullary_globals.insert(gi as u32, lbl);
                    }
                    SExpr::Atom(Atom::Lit(l)) => {
                        if let Some(lbl) = g.static_literal_label(l) {
                            g.nullary_globals.insert(gi as u32, lbl);
                        }
                    }
                    _ => {}
                }
            }
            g.pending.push((l, gl.closure.clone(), gl.name.clone()));
            let worker = if used_workers.contains(&(gi as u32)) {
                gl.worker.as_ref().map(|(c, raw)| {
                    let wl = g.label("w");
                    g.pending.push((wl.clone(), c.clone(), format!("{}/worker", gl.name)));
                    (wl, raw.clone(), c.ret_raw, c.uses_closure)
                })
            } else {
                None
            };
            g.workers.push(worker);
        }
        // Propagate static WHNF cells through zero-arity global aliases (e.g. `$fMonoidAll = True`).
        loop {
            let mut changed = false;
            for (gi, gl) in prog.globals.iter().enumerate() {
                let key = gi as u32;
                if gl.closure.arity == 0
                    && gl.closure.captures.is_empty()
                    && !g.nullary_globals.contains_key(&key)
                {
                    if let SExpr::Atom(Atom::Global(target)) = &gl.closure.body {
                        if let Some(lbl) = g.nullary_globals.get(target).cloned() {
                            g.nullary_globals.insert(key, lbl);
                            changed = true;
                        }
                    }
                }
            }
            if !changed {
                break;
            }
        }
        g.out.push_str("    .text\n");
        if std::env::var_os("HSC_DEBUG").is_some() {
            // Each runtime routine (a `hsc_` label) gets a type and size, for profilers.
            let mut current: Option<&str> = None;
            for line in RUNTIME_ASM.lines() {
                if let Some(name) = line.strip_suffix(':').filter(|n| n.starts_with("hsc_") && !n.contains(' ')) {
                    if let Some(prev) = current {
                        let _ = writeln!(g.out, "    .size {prev}, .-{prev}");
                    }
                    let _ = writeln!(g.out, "    .type {name}, @function");
                    current = Some(name);
                }
                g.out.push_str(line);
                g.out.push('\n');
            }
            if let Some(prev) = current {
                let _ = writeln!(g.out, "    .size {prev}, .-{prev}");
            }
        } else {
            g.out.push_str(RUNTIME_ASM);
        }
        // HSC_DEBUG: every body also gets a symbol named after its binding, for profilers.
        let symbols = std::env::var_os("HSC_DEBUG").is_some();
        let mut n = 0usize;
        while let Some((label, c, name)) = g.pending.pop() {
            let symbol = symbols.then(|| {
                n += 1;
                let clean: String = name.chars().map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' }).collect();
                let sym = format!("hs_{clean}_{n}");
                let _ = writeln!(g.out, "    .type {sym}, @function\n{sym}:");
                sym
            });
            g.body_names.push((label.clone(), name.clone()));
            g.closure_code(&label, &c, &name);
            if let Some(sym) = symbol {
                let _ = writeln!(g.out, "    .size {sym}, .-{sym}");
            }
        }
        // Immutable tables in .rodata using 32-bit relative offsets (zero dynamic relocations).
        g.out.push_str("    .section .rodata\n    .p2align 3\n");
        g.out.push_str("hsc_globals:\n");
        for (gl, l) in prog.globals.iter().zip(&global_labels) {
            let kind = if gl.closure.arity > 0 { gl.closure.arity as i32 } else { -1 };
            let _ = writeln!(g.out, "    .long {kind}, {l} - hsc_globals");
        }

        // Partition static cells: pure integer-payload cells go in .rodata (zero relocations);
        // pointer-payload cells (static IS boxes) go in .data.rel.ro.
        let statics = std::mem::take(&mut g.statics);
        let mut rel_statics = Vec::new();
        g.out.push_str("    .p2align 3\n");
        for (l, h, w) in statics {
            if w.starts_with('.') {
                rel_statics.push((l, h, w));
            } else {
                let _ = writeln!(g.out, "{l}:\n    .quad {h}, {w}");
            }
        }
        for (l, bytes) in std::mem::take(&mut g.strings) {
            let _ = writeln!(g.out, "{l}:");
            if !bytes.is_empty() {
                g.out.push_str("    .ascii \"");
                escape_bytes(&bytes, &mut g.out);
                g.out.push_str("\"\n");
            }
        }
        let cons = std::mem::take(&mut g.con_names);
        g.out.push_str("    .p2align 2\nhsc_con_names:\n");
        let mut off = cons.len() * 8;
        for n in &cons {
            let _ = writeln!(g.out, "    .long {off}, {}", n.len());
            off += n.len();
        }
        for n in &cons {
            if !n.is_empty() {
                let _ = writeln!(g.out, "    .ascii \"{}\"", escape(n));
            }
        }
        let prims = std::mem::take(&mut g.prim_names);
        g.out.push_str("    .p2align 2\nhsc_prim_names:\n");
        let mut off = prims.len() * 8;
        for n in &prims {
            let _ = writeln!(g.out, "    .long {off}, {}", n.len());
            off += n.len();
        }
        for n in &prims {
            if !n.is_empty() {
                let _ = writeln!(g.out, "    .ascii \"{}\"", escape(n));
            }
        }
        // The body table: entry addresses in increasing order, with their names (for diagnostics).
        // Each row is [code_off: i32, name_off: u32, name_len: u32] relative to `hsc_body_table`.
        g.out.push_str("    .p2align 2\n    .globl hsc_body_table\nhsc_body_table:\n");
        let bodies = std::mem::take(&mut g.body_names);
        let bodies_count = bodies.len();
        let mut off = bodies_count * 12;
        for (l, n) in &bodies {
            let _ = writeln!(g.out, "    .long {l} - hsc_body_table, {off}, {}", n.len());
            off += n.len();
        }
        for (_, n) in &bodies {
            if !n.is_empty() {
                let _ = writeln!(g.out, "    .ascii \"{}\"", escape(n));
            }
        }
        let _ = writeln!(g.out, "    .p2align 3\n    .globl hsc_nbodies\nhsc_nbodies:\n    .quad {bodies_count}");
        // The bindings the runtime uses: [name_off: u32, name_len: u32, global_idx: u32].
        let rts: Vec<(&str, u32)> =
            super::RTS_GLOBALS.iter().filter_map(|n| prog.index.get(*n).map(|i| (*n, *i))).collect();
        g.out.push_str("    .p2align 2\nhsc_rts_globals:\n");
        let mut off = rts.len() * 12;
        for (n, idx) in &rts {
            let _ = writeln!(g.out, "    .long {off}, {}, {idx}", n.len());
            off += n.len();
        }
        for (n, _) in &rts {
            if !n.is_empty() {
                let _ = writeln!(g.out, "    .ascii \"{}\"", escape(n));
            }
        }
        // Selector thunks: [code_off: i32, con_id: u32, field_word: u32] relative to `hsc_selectors`.
        let selectors = std::mem::take(&mut g.selectors);
        g.out.push_str("    .p2align 2\nhsc_selectors:\n");
        for (l, con_id, field_word) in &selectors {
            let _ = writeln!(g.out, "    .long {l} - hsc_selectors, {con_id}, {field_word}");
        }
        g.out.push_str("    .section .data.rel.ro,\"aw\",@progbits\n    .p2align 3\n");
        for (l, h, w) in rel_statics {
            let _ = writeln!(g.out, "{l}:\n    .quad {h}, {w}");
        }
        let main_index = prog.index.get("hsc$main").or_else(|| prog.index.get("Main.main")).copied().unwrap_or(0);
        // The table the runtime reads (`machine::entry::NativeTable`).
        let _ = writeln!(
            g.out,
            "    .globl hsc_table\nhsc_table:\n    .quad hsc_globals, {}, hsc_con_names, {}, hsc_prim_names, {}, {}, hsc_body_table, {}, hsc_rts_globals, {}, hsc_selectors, {}",
            prog.globals.len(),
            cons.len(),
            prims.len(),
            main_index,
            bodies_count,
            rts.len(),
            selectors.len()
        );
        g.out.push_str("    .section .note.GNU-stack,\"\",@progbits\n");
        g.out
    }

    /// Recognises a single-capture selector thunk (`case Free(0) of Con [.., f_i, ..] -> f_i`)
    /// so the garbage collector can short-circuit it when `Free(0)` is already evaluated.
    fn selector_info(&mut self, c: &SClosure) -> Option<(u32, u32)> {
        if c.arity != 0 || c.captures.len() != 1 {
            return None;
        }
        let SExpr::Case(scrut, bslot, alts) = &c.body else {
            return None;
        };
        if *bslot & RAW != 0 || !matches!(scrut.as_ref(), SExpr::Atom(Atom::Free(0))) {
            return None;
        }
        let mut found: Option<(&str, u32)> = None;
        for alt in alts {
            match alt {
                SAlt::Con(con, fields, SExpr::Atom(Atom::Slot(s))) if *s & RAW == 0 => {
                    if let Some(pos) = fields.iter().position(|f| *f == *s) {
                        if found.is_some() {
                            return None;
                        }
                        found = Some((con.as_str(), (pos + 1) as u32));
                    }
                }
                SAlt::Default(_) => break,
                _ => {}
            }
        }
        let (con, field_word) = found?;
        let con_id = self.con_id(con);
        Some((con_id, field_word))
    }

    /// A closure's code: the prologue, then its expression in tail position.
    fn closure_code(&mut self, label: &str, c: &SClosure, name: &str) {
        self.current_name = name.to_string();
        if let Some((con_id, field_word)) = self.selector_info(c) {
            self.selectors.push((label.to_string(), con_id, field_word));
        }
        let has_closure = c.uses_closure;
        let ptr_slots = if has_closure { c.nslots } else { c.nslots - 1 };
        let f = Frame { s: ptr_slots + c.nraw, a: c.arity, r: c.nraw, has_closure };
        self.ret_raw = c.ret_raw;
        let has_params = c.arity > 0 || !c.raw_args.is_empty();
        let loop_lbl = if has_params && c.raw_args.iter().all(|(a, _)| *a == 0) {
            let ll = self.label("loop");
            self.current_self = Some((label.to_string(), ll.clone(), c.arity));
            Some(ll)
        } else {
            self.current_self = None;
            None
        };
        self.put_label(label);
        // R1 is the closure (if used); stack arguments sit above the new frame.
        if f.s > f.a {
            let _ = writeln!(self.out, "    subq ${}, %rbp", 8 * (f.s - f.a) as i64);
        }
        if has_closure {
            let _ = writeln!(self.out, "    movq %r14, {}(%rbp)", f.off(0));
        }
        if let Some(ll) = &loop_lbl {
            self.put_label(ll);
        }
        // The pointer locals start empty for the collector; raw slots are not read by it.
        for k in (f.a + 1)..c.nslots {
            let _ = writeln!(self.out, "    movq $0, {}(%rbp)", f.off(k));
        }
        // A worker's first `MAX_REG_RAW_ARGS` raw parameters arrive directly in `CACHE_REGS[0..]`
        // (`arg == 0`); any further ones arrive in stack argument slots and move to raw slots.
        let p = Pos { f, mode: Mode::Tail, pushed: 0 };
        self.cache.clear();
        self.raw_uses.clear();
        let mut joins = HashMap::new();
        self.find_raw_uses(&c.body, &mut joins);
        let mut reg_idx = 0usize;
        for &(arg, raw) in &c.raw_args {
            if arg == 0 {
                self.cache_put(raw, CACHE_REGS[reg_idx], true);
                reg_idx += 1;
            } else {
                let r = if self.cache.len() < CACHE_REGS.len() { Some(self.fresh_reg(p, &[])) } else { None };
                match r {
                    Some(r) => {
                        let _ = writeln!(self.out, "    movq {}(%rbp), {r}", f.off(arg));
                        self.cache_put(raw, r, true);
                    }
                    None => {
                        let _ = writeln!(self.out, "    movq {}(%rbp), %rcx", f.off(arg));
                        let _ = writeln!(self.out, "    movq %rcx, {}(%rbp)", f.off(raw));
                    }
                }
                let _ = writeln!(self.out, "    movq $0, {}(%rbp)", f.off(arg));
            }
        }
        // A function's entry is a preemption point (inline arithmetic has none, and every loop
        // goes through one): at the end of a time slice another thread runs, and an exception
        // thrown to this one is raised.
        if has_params {
            let go = self.next_id();
            self.emit("cmpb $0, hsc_preempt(%rip)");
            let _ = writeln!(self.out, "    je .L{go}");
            // The cached numbers are stored around the call (the registers do not survive it).
            let cached = self.cache.clone();
            for c in &cached {
                let _ = writeln!(self.out, "    movq {}, {}(%rbp)", c.reg, p.slot_off(c.slot));
            }
            self.rt_call("hsc_yield", p, 0);
            for c in &cached {
                let _ = writeln!(self.out, "    movq {}(%rbp), {}", p.slot_off(c.slot), c.reg);
            }
            let _ = writeln!(self.out, ".L{go}:");
        }
        self.expr(&c.body, p);
        self.current_self = None;
    }

    /// Leaves a WHNF value in R1 as the result of the expression.
    fn ret(&mut self, p: Pos) {
        match p.mode {
            Mode::Tail => {
                if p.f.s > 0 {
                    let _ = writeln!(self.out, "    addq ${}, %rbp", 8 * p.f.s as i64);
                }
                self.emit("jmp *(%rbp)");
            }
            Mode::Scrut => self.emit("jmp *(%rbp)"),
        }
    }

    /// Evaluates the value in R1 (possibly a thunk) as the result of the expression.
    fn eval_result(&mut self, p: Pos) {
        if p.mode == Mode::Tail && p.f.s > 0 {
            let _ = writeln!(self.out, "    addq ${}, %rbp", 8 * p.f.s as i64);
        }
        self.emit("jmp hsc_eval");
    }

    fn known_global_entry(&self, func: &Atom, nargs: usize) -> Option<(&str, bool)> {
        let Atom::Global(index) = func else { return None };
        let (arity, entry, uses_closure) = self.known_globals.get(*index as usize)?;
        (*arity > 0 && *arity as usize == nargs).then_some((entry.as_str(), *uses_closure))
    }

    /// Generates an expression. Only straight-line raw arithmetic, a case of a raw number, a
    /// tail call, and a worker call use the numbers cached in registers; anything else finds them
    /// in their slots. Every expression ends in a transfer of control, so nothing is cached after it.
    fn expr(&mut self, e: &SExpr, p: Pos) {
        self.drop_dead(e, p);
        if !self.keeps_cache(e, p) {
            self.flush(p);
        }
        self.expr_node(e, p);
        self.cache.clear();
    }

    /// Records the raw slots each expression in `e` uses (its own, and those of the join points
    /// it jumps to), answering `e`'s.
    fn find_raw_uses(&mut self, e: &SExpr, joins: &mut HashMap<u32, Vec<u32>>) -> Vec<u32> {
        let mut out: Vec<u32> = Vec::new();
        let atom = |a: &Atom, out: &mut Vec<u32>| {
            if let Atom::Slot(k) = a {
                if k & RAW != 0 {
                    out.push(*k);
                }
            }
        };
        match e {
            SExpr::Atom(a) => atom(a, &mut out),
            SExpr::App(f, args) => {
                atom(f, &mut out);
                args.iter().for_each(|a| atom(a, &mut out));
            }
            SExpr::CallWorker(_, args) | SExpr::Con(_, args) | SExpr::Prim(_, args) => args.iter().for_each(|a| atom(a, &mut out)),
            SExpr::Let(binds, body) => {
                for (_, b) in binds {
                    b.atoms().iter().for_each(|a| atom(a, &mut out));
                }
                out.extend(self.find_raw_uses(body, joins));
            }
            SExpr::Case(scrut, _, alts) => {
                out.extend(self.find_raw_uses(scrut, joins));
                for a in alts {
                    match a {
                        SAlt::Con(_, _, b) | SAlt::Lit(_, b) | SAlt::Default(b) => out.extend(self.find_raw_uses(b, joins)),
                    }
                }
            }
            SExpr::Closure(c) => c.captures.iter().for_each(|a| atom(a, &mut out)),
            SExpr::Fail(_) => {}
            SExpr::Join(id, _, code, body) => {
                let uses = self.find_raw_uses(code, joins);
                joins.insert(*id, uses.clone());
                out.extend(uses);
                out.extend(self.find_raw_uses(body, joins));
            }
            SExpr::Jump(id, args) => {
                args.iter().for_each(|a| atom(a, &mut out));
                // A join point defined outside the body (none should be): everything is live.
                match joins.get(id) {
                    Some(u) => out.extend(u.iter().copied()),
                    None => out.push(ALL_LIVE),
                }
            }
        }
        out.sort_unstable();
        out.dedup();
        self.raw_uses.insert(e as *const SExpr, out.clone());
        out
    }

    /// Whether raw slot `k` may be used by `e` or by what follows it (for a scrutinee, the
    /// enclosing cases' alternatives).
    fn raw_live(&self, k: u32, e: &SExpr, p: Pos) -> bool {
        let Some(uses) = self.raw_uses.get(&(e as *const SExpr)) else { return true };
        if uses.binary_search(&k).is_ok() || uses.binary_search(&ALL_LIVE).is_ok() {
            return true;
        }
        p.mode == Mode::Scrut && self.live.last().is_none_or(|l| l.contains(&k) || l.contains(&ALL_LIVE))
    }

    /// Forgets the cached numbers that neither `e` nor what follows it uses.
    fn drop_dead(&mut self, e: &SExpr, p: Pos) {
        let cache = std::mem::take(&mut self.cache);
        self.cache = cache.into_iter().filter(|c| self.raw_live(c.slot, e, p)).collect();
    }

    /// Whether an expression's code reads cached numbers from their registers (see `expr`).
    fn keeps_cache(&self, e: &SExpr, p: Pos) -> bool {
        let tail = p.mode == Mode::Tail;
        // Atoms that are loaded without calling the runtime.
        let plain = |a: &Atom| !matches!(a, Atom::Lit(l) if !matches!(l, SLit::Int(_) | SLit::Char(_) | SLit::Double(_)) && !matches!(l, SLit::Integer(s) if s.parse::<i64>().is_ok()));
        match e {
            SExpr::Case(scrut, b, alts) => match scrut.as_ref() {
                SExpr::Prim(name, args) => b & RAW != 0 && matches!(&alts[..], [SAlt::Default(_)]) && total_raw_op(name, args),
                SExpr::Atom(a) => is_raw(a),
                SExpr::CallWorker(_, args) => args.iter().all(plain),
                _ => false,
            },
            SExpr::Atom(Atom::Slot(k)) => k & RAW != 0 && tail && self.ret_raw,
            SExpr::Atom(Atom::Lit(SLit::Int(_) | SLit::Char(_))) => tail && self.ret_raw,
            SExpr::App(f, args) => tail && plain(f) && args.iter().all(plain),
            SExpr::CallWorker(_, args) => args.iter().all(plain),
            _ => false,
        }
    }

    /// The register holding raw slot `k`'s number, if one does.
    fn cached(&self, k: u32) -> Option<&'static str> {
        self.cache.iter().find(|c| c.slot == k).map(|c| c.reg)
    }

    /// Stores the cached numbers their slots may not have yet, and forgets the registers.
    fn flush(&mut self, p: Pos) {
        for c in std::mem::take(&mut self.cache) {
            if c.dirty {
                self.emit(&format!("movq {}, {}", c.reg, p.slot(c.slot)));
            }
        }
    }

    /// In `Mode::Scrut`, stores only the dirty cached raw slots that are still live after the
    /// scrutinee returns, keeping the cache entries intact for argument moves.
    fn flush_live_raws(&mut self, p: Pos) {
        let live = self.live.last();
        for c in &mut self.cache {
            if c.dirty && live.is_none_or(|l| l.contains(&c.slot) || l.contains(&ALL_LIVE)) {
                let _ = writeln!(self.out, "    movq {}, {}(%rbp)", c.reg, p.slot_off(c.slot));
                c.dirty = false;
            }
        }
    }

    /// A cache register for a new number, none of `avoid`: a free one, or the oldest one's (its
    /// number stored first).
    fn fresh_reg(&mut self, p: Pos, avoid: &[&str]) -> &'static str {
        if let Some(r) = CACHE_REGS.iter().find(|r| !avoid.contains(r) && self.cache.iter().all(|c| c.reg != **r)) {
            return r;
        }
        let i = self.cache.iter().position(|c| !avoid.contains(&c.reg)).expect("internal: no cache register");
        let c = self.cache.remove(i);
        if c.dirty {
            self.emit(&format!("movq {}, {}", c.reg, p.slot(c.slot)));
        }
        c.reg
    }

    /// Records that `reg` holds raw slot `k`'s number; `dirty` when the slot does not have it.
    fn cache_put(&mut self, k: u32, reg: &'static str, dirty: bool) {
        self.cache.retain(|c| c.slot != k && c.reg != reg);
        self.cache.push(Cached { slot: k, reg, dirty });
    }

    /// A raw operand (a raw slot, kept in a cache register from now on, or a literal).
    fn raw_src(&mut self, a: &Atom, p: Pos, avoid: &[&str]) -> Src {
        match a {
            Atom::Slot(k) if k & RAW != 0 => {
                if let Some(i) = self.cache.iter().position(|c| c.slot == *k) {
                    // Used now: the last to be evicted.
                    let c = self.cache.remove(i);
                    self.cache.push(c);
                    return Src::Reg(c.reg);
                }
                let r = self.fresh_reg(p, avoid);
                self.emit(&format!("movq {}, {r}", p.slot(*k)));
                self.cache_put(*k, r, false);
                Src::Reg(r)
            }
            Atom::Lit(SLit::Int(v)) => Src::Imm(*v),
            Atom::Lit(SLit::Char(c)) => Src::Imm(crate::syntax::lexer::char_code(*c) as i64),
            other => panic!("internal: raw operand {other:?}"),
        }
    }

    /// `movq src, dst` for a raw operand.
    fn mov_src(&mut self, s: Src, dst: &str) {
        match s {
            Src::Reg(r) => self.emit(&format!("movq {r}, {dst}")),
            Src::Imm(v) if i32::try_from(v).is_ok() => self.emit(&format!("movq ${v}, {dst}")),
            Src::Imm(v) => self.emit(&format!("movabsq ${v}, {dst}")),
        }
    }

    /// A raw operand as the source of a two-operand instruction (a large immediate goes through
    /// `scratch` first).
    fn src_text(&mut self, s: Src, scratch: &str) -> String {
        match s {
            Src::Reg(r) => r.to_string(),
            Src::Imm(v) if i32::try_from(v).is_ok() => format!("${v}"),
            Src::Imm(v) => {
                self.emit(&format!("movabsq ${v}, {scratch}"));
                scratch.to_string()
            }
        }
    }

    /// A total inline operation on raw numbers (`total_raw_op`) whose result goes to raw slot
    /// `dest`, kept in a cache register.
    fn raw_op(&mut self, name: &str, args: &[Atom], dest: u32, p: Pos) {
        let k = int_op(name).expect("internal: not an inline operation");
        let s0 = self.raw_src(&args[0], p, &[]);
        let r0 = match s0 { Src::Reg(r) => Some(r), Src::Imm(_) => None };
        let s1 = match args.get(1) {
            Some(a) => Some(self.raw_src(a, p, &r0.into_iter().collect::<Vec<_>>())),
            None => None,
        };
        let mut avoid: Vec<&str> = r0.into_iter().collect();
        if let Some(Src::Reg(r)) = s1 {
            avoid.push(r);
        }
        let d = self.fresh_reg(p, &avoid);
        match k {
            K::Bin(op) => {
                self.mov_src(s0, d);
                let t = self.src_text(s1.unwrap(), "%rdx");
                self.emit(&format!("{op} {t}, {d}"));
            }
            K::Cmp(set) | K::CharCmp(set) => {
                self.mov_src(s0, d);
                if matches!(k, K::CharCmp(_)) {
                    let t = match s1.unwrap() {
                        Src::Reg(r) => low32(r).to_string(),
                        Src::Imm(v) => format!("${}", v as u32 as i32),
                    };
                    self.emit(&format!("cmpl {t}, {}", low32(d)));
                } else {
                    let t = self.src_text(s1.unwrap(), "%rdx");
                    self.emit(&format!("cmpq {t}, {d}"));
                }
                self.emit(&format!("{set} {}", byte_reg(d)));
                self.emit(&format!("movzbl {}, {}", byte_reg(d), low32(d)));
            }
            K::Shift(op) => {
                self.mov_src(s0, d);
                match s1.unwrap() {
                    Src::Imm(v) => self.emit(&format!("{op} ${}, {d}", v & 63)),
                    Src::Reg(r) => {
                        self.emit(&format!("movq {r}, %rcx"));
                        self.emit(&format!("{op} %cl, {d}"));
                    }
                }
            }
            K::Div(quot) => {
                let Some(Src::Imm(dv)) = s1 else { panic!("internal: division by a variable inline") };
                let (m, sh) = signed_magic(dv);
                let n = match s0 {
                    Src::Reg(r) => r.to_string(),
                    Src::Imm(_) => {
                        self.mov_src(s0, "%rcx");
                        "%rcx".to_string()
                    }
                };
                self.emit(&format!("movabsq ${m}, %rax"));
                self.emit(&format!("imulq {n}"));
                if dv > 0 && m < 0 {
                    self.emit(&format!("addq {n}, %rdx"));
                } else if dv < 0 && m > 0 {
                    self.emit(&format!("subq {n}, %rdx"));
                }
                if sh > 0 {
                    self.emit(&format!("sarq ${sh}, %rdx"));
                }
                self.emit("movq %rdx, %rax");
                self.emit("shrq $63, %rax");
                self.emit("addq %rax, %rdx");
                if quot {
                    self.emit(&format!("movq %rdx, {d}"));
                } else {
                    if i32::try_from(dv).is_ok() {
                        self.emit(&format!("imulq ${dv}, %rdx"));
                    } else {
                        self.emit(&format!("movabsq ${dv}, %rax"));
                        self.emit("imulq %rax, %rdx");
                    }
                    self.emit(&format!("movq {n}, {d}"));
                    self.emit(&format!("subq %rdx, {d}"));
                }
            }
            K::Un(op) => {
                self.mov_src(s0, d);
                self.emit(&format!("{op} {d}"));
            }
            K::Ord => self.mov_src(s0, d),
            K::Chr => {
                self.mov_src(s0, d);
                self.emit(&format!("movl {}, {}", low32(d), low32(d)));
            }
            K::TagToEnum => unreachable!("not total"),
        }
        self.cache_put(dest, d, true);
    }

    /// A comparison whose result only a case on it then uses (`case a <# b of 1# -> x; _ -> y`):
    /// a compare and conditional jumps, without the 0 or 1. Answers whether it applied.
    fn compare_and_branch(&mut self, name: &str, args: &[Atom], b: u32, body: &SExpr, p: Pos) -> bool {
        let Some(k @ (K::Cmp(set) | K::CharCmp(set))) = int_op(name) else { return false };
        let SExpr::Case(scrut, _, alts) = body else { return false };
        if !matches!(scrut.as_ref(), SExpr::Atom(Atom::Slot(s)) if *s == b) {
            return false;
        }
        let end = alts.iter().position(|a| matches!(a, SAlt::Default(_))).map_or(alts.len(), |i| i + 1);
        let alts = &alts[..end];
        let body_of = |a: &SAlt| match a {
            SAlt::Lit(_, e) | SAlt::Default(e) | SAlt::Con(_, _, e) => e as *const SExpr,
        };
        let uses_b = alts.iter().any(|a| self.raw_uses.get(&body_of(a)).is_none_or(|u| u.binary_search(&b).is_ok() || u.binary_search(&ALL_LIVE).is_ok()));
        if uses_b || p.mode == Mode::Scrut && self.live.last().is_none_or(|l| l.contains(&b) || l.contains(&ALL_LIVE)) {
            return false;
        }
        if alts.iter().any(|a| matches!(a, SAlt::Lit(l, _) if !matches!(l, Lit::Int(_)))) || alts.iter().any(|a| matches!(a, SAlt::Con(..))) {
            return false;
        }
        let s0 = self.raw_src(&args[0], p, &[]);
        let avoid: Vec<&str> = match s0 { Src::Reg(r) => vec![r], Src::Imm(_) => vec![] };
        let s1 = self.raw_src(&args[1], p, &avoid);
        let lhs = match s0 {
            Src::Reg(r) => r.to_string(),
            Src::Imm(_) => {
                self.mov_src(s0, "%rcx");
                "%rcx".to_string()
            }
        };
        if matches!(k, K::CharCmp(_)) {
            let t = match s1 {
                Src::Reg(r) => low32(r).to_string(),
                Src::Imm(v) => format!("${}", v as u32 as i32),
            };
            let l = if lhs == "%rcx" { "%ecx" } else { low32(&lhs) };
            self.emit(&format!("cmpl {t}, {l}"));
        } else {
            let t = self.src_text(s1, "%rdx");
            self.emit(&format!("cmpq {t}, {lhs}"));
        }
        let cc = &set[3..];
        let negated = match cc {
            "e" => "ne", "ne" => "e", "l" => "ge", "le" => "g", "g" => "le", "ge" => "l",
            "b" => "ae", "be" => "a", "a" => "be", "ae" => "b",
            other => panic!("internal: condition {other}"),
        };
        let labels: Vec<String> = alts.iter().map(|_| self.label("alt")).collect();
        for (a, l) in alts.iter().zip(&labels) {
            match a {
                SAlt::Lit(Lit::Int(1), _) => self.emit(&format!("j{cc} {l}")),
                SAlt::Lit(Lit::Int(0), _) => self.emit(&format!("j{negated} {l}")),
                _ => {}
            }
        }
        let snapshot = self.cache.clone();
        match alts.last() {
            Some(SAlt::Default(body)) => self.expr(body, p),
            _ => self.fail("Non-exhaustive patterns in case", p),
        }
        for (a, l) in alts.iter().zip(&labels) {
            if let SAlt::Lit(Lit::Int(0 | 1), body) = a {
                self.put_label(l);
                self.cache = snapshot.clone();
                self.expr(body, p);
            }
        }
        true
    }

    /// A comparison with at least one boxed operand whose raw result is immediately branched on
    /// (`case a <# b as r of _ -> case r of 0# -> f; _ -> t`): checks the boxed operand tags
    /// inline, compares payloads and branches directly without materializing 0/1 or spilling to
    /// the frame, with an out-of-line slow path for thunks/indirections.
    fn boxed_compare_and_branch(
        &mut self,
        name: &str,
        args: &[Atom],
        bslot: u32,
        alts: &[SAlt],
        p: Pos,
    ) -> bool {
        let Some(k @ (K::Cmp(set) | K::CharCmp(set))) = int_op(name) else { return false };
        let in_kind = if matches!(k, K::CharCmp(_)) { CHAR } else { INT };
        let lit_ok = |l: &SLit| matches!((l, in_kind), (SLit::Int(_), INT) | (SLit::Char(_), CHAR));
        if args.len() != 2 || args.iter().any(|a| matches!(a, Atom::Lit(l) if !lit_ok(l))) {
            return false;
        }
        let [SAlt::Default(SExpr::Case(inner_scrut, _, inner_alts))] = alts else { return false };
        if !matches!(inner_scrut.as_ref(), SExpr::Atom(Atom::Slot(s)) if *s == bslot) {
            return false;
        }
        let end = inner_alts.iter().position(|a| matches!(a, SAlt::Default(_))).map_or(inner_alts.len(), |i| i + 1);
        let inner_alts = &inner_alts[..end];
        let body_of = |a: &SAlt| match a {
            SAlt::Lit(_, e) | SAlt::Default(e) | SAlt::Con(_, _, e) => e as *const SExpr,
        };
        let uses_b = inner_alts.iter().any(|a| {
            self.raw_uses
                .get(&body_of(a))
                .is_none_or(|u| u.binary_search(&bslot).is_ok() || u.binary_search(&ALL_LIVE).is_ok())
        });
        if uses_b || (p.mode == Mode::Scrut && self.live.last().is_none_or(|l| l.contains(&bslot) || l.contains(&ALL_LIVE))) {
            return false;
        }
        if inner_alts.iter().any(|a| match a {
            SAlt::Lit(Lit::Int(0 | 1), _) | SAlt::Default(_) => false,
            _ => true,
        }) {
            return false;
        }
        let slow = self.label("slow");
        for a in args {
            if is_raw(a) || matches!(a, Atom::Lit(_)) {
                continue;
            }
            self.load(a, p, "%rax");
            let _ = writeln!(self.out, "    cmpb ${in_kind}, (%rax)");
            let _ = writeln!(self.out, "    jne {slow}");
        }
        self.operand(&args[0], p, "%rcx");
        self.operand(&args[1], p, "%rdx");
        match k {
            K::Cmp(_) => self.emit("cmpq %rdx, %rcx"),
            K::CharCmp(_) => self.emit("cmpl %edx, %ecx"),
            _ => unreachable!(),
        }
        let cc = &set[3..];
        let negated = match cc {
            "e" => "ne", "ne" => "e", "l" => "ge", "le" => "g", "g" => "le", "ge" => "l",
            "b" => "ae", "be" => "a", "a" => "be", "ae" => "b",
            other => panic!("internal: condition {other}"),
        };
        let labels: Vec<String> = inner_alts.iter().map(|_| self.label("alt")).collect();
        for (a, l) in inner_alts.iter().zip(&labels) {
            match a {
                SAlt::Lit(Lit::Int(1), _) => {
                    let _ = writeln!(self.out, "    j{cc} {l}");
                }
                SAlt::Lit(Lit::Int(0), _) => {
                    let _ = writeln!(self.out, "    j{negated} {l}");
                }
                _ => {}
            }
        }
        let def_lbl = self.label("def");
        self.put_label(&def_lbl);
        match inner_alts.last() {
            Some(SAlt::Default(body)) => self.expr(body, p),
            _ => self.fail("Non-exhaustive patterns in case", p),
        }
        for (a, l) in inner_alts.iter().zip(&labels) {
            if let SAlt::Lit(Lit::Int(0 | 1), body) = a {
                self.put_label(l);
                self.expr(body, p);
            }
        }
        // Cold slow path: evaluate via runtime primitive, then branch on its 0/1 result.
        let cont = self.label("k");
        self.put_label(&slow);
        self.emit("subq $8, %rbp");
        let _ = writeln!(self.out, "    leaq {cont}(%rip), %rax");
        self.emit("movq %rax, (%rbp)");
        let sp = Pos { f: p.f, mode: Mode::Scrut, pushed: p.pushed + 1 };
        let live = self.case_live(inner_alts, p);
        self.live.push(live);
        self.slow_only = true;
        self.prim(name, args, sp);
        self.live.pop();
        let size = if p.pushed == 0 { p.f.size_word() } else { 0 };
        let _ = writeln!(self.out, "    .quad {}, {}", frame::BODY, size);
        self.put_label(&cont);
        self.emit("addq $8, %rbp");
        self.emit("movq 8(%r14), %rcx");
        for (a, l) in inner_alts.iter().zip(&labels) {
            if let SAlt::Lit(Lit::Int(v @ (0 | 1)), _) = a {
                let _ = writeln!(self.out, "    cmpq ${v}, %rcx");
                let _ = writeln!(self.out, "    je {l}");
            }
        }
        let _ = writeln!(self.out, "    jmp {def_lbl}");
        true
    }

    fn expr_node(&mut self, e: &SExpr, p: Pos) {
        match e {
            // A worker returning raw: its result's number goes back in R1.
            SExpr::Atom(Atom::Slot(k)) if *k & RAW != 0 && p.mode == Mode::Tail && self.ret_raw => {
                let src = self.cached(*k).map(str::to_owned).unwrap_or_else(|| p.slot(*k));
                self.emit(&format!("movq {src}, %r14"));
                self.ret(p);
            }
            SExpr::Atom(Atom::Lit(l @ (SLit::Int(_) | SLit::Char(_)))) if p.mode == Mode::Tail && self.ret_raw => {
                let v = match l {
                    SLit::Int(i) => *i,
                    SLit::Char(c) => crate::syntax::lexer::char_code(*c) as i64,
                    _ => unreachable!(),
                };
                if v == 0 {
                    self.emit("xorl %r14d, %r14d");
                } else if i32::try_from(v).is_ok() {
                    let _ = writeln!(self.out, "    movq ${v}, %r14");
                } else {
                    let _ = writeln!(self.out, "    movabsq ${}, %r14", v as u64);
                }
                self.ret(p);
            }
            SExpr::Atom(a) if is_raw(a) => {
                // A raw number as a result: boxed once.
                let Atom::Slot(k) = a else { unreachable!() };
                let kind = if k & RAW_CHAR != 0 { CHAR } else { INT };
                self.alloc(2, p);
                self.emit(&format!("movq ${}, (%rax)", header(kind, 1, 0)));
                self.emit(&format!("movq {}, %rcx", p.slot(*k)));
                self.emit("movq %rcx, 8(%rax)");
                self.emit("movq %rax, %r14");
                self.ret(p);
            }
            SExpr::Atom(Atom::Global(g)) if self.nullary_globals.contains_key(g) => {
                let lbl = self.nullary_globals[g].clone();
                let _ = writeln!(self.out, "    leaq {lbl}(%rip), %r14");
                self.ret(p);
            }
            SExpr::Atom(Atom::Lit(l))
                if matches!(l, SLit::Int(_) | SLit::Char(_) | SLit::Double(_))
                    || matches!(l, SLit::Integer(s) if s.parse::<i64>().is_ok()) =>
            {
                self.literal(l, p, "%r14");
                self.ret(p);
            }
            SExpr::Atom(a) => {
                self.load(a, p, "%r14");
                self.clear_dead(std::slice::from_ref(a), p);
                self.eval_result(p);
            }
            SExpr::CallWorker(g, args) => {
                let func = Atom::Global(*g);
                let worker = self.workers.get(*g as usize).cloned().flatten();
                assert!(worker.as_ref().is_some_and(|(_, r, _, _)| r.len() == args.len()), "internal: no worker for call");
                self.call(&func, args, worker, p);
            }
            SExpr::App(func, args) => self.call(func, args, None, p),
            SExpr::Con(name, args) => {
                let id = self.con_id(name);
                if args.is_empty() {
                    let label = self.nullary_con(id);
                    self.emit(&format!("leaq {label}(%rip), %r14"));
                } else {
                    self.alloc(1 + args.len(), p);
                    self.emit(&format!("movabsq ${}, %rcx", header(CON, args.len() as u64, id as u64)));
                    self.emit("movq %rcx, (%rax)");
                    for (i, a) in args.iter().enumerate() {
                        self.load(a, p, "%rcx");
                        self.emit(&format!("movq %rcx, {}(%rax)", 8 * (i + 1)));
                    }
                    self.emit("movq %rax, %r14");
                }
                self.ret(p);
            }
            SExpr::Prim(name, args) => self.prim(name, args, p),
            SExpr::Let(binds, body) => {
                let non_alloc = |a: &Atom| match a {
                    Atom::Lit(SLit::Str(_) | SLit::Frac(_) | SLit::Addr(_)) => false,
                    Atom::Lit(SLit::Integer(s)) => s.parse::<i64>().is_ok(),
                    _ => true,
                };
                let fuse_con = match body.as_ref() {
                    SExpr::Con(name, args)
                        if !args.is_empty()
                            && binds.iter().all(|(_, b)| b.atoms().iter().all(non_alloc))
                            && args.iter().all(non_alloc) =>
                    {
                        Some((name, args))
                    }
                    _ => None,
                };
                let total: usize = binds.iter().map(|(_, b)| bind_words(b)).sum();
                let con_words = fuse_con.map_or(0, |(_, args)| (1 + args.len()).max(2));
                if total + con_words > 0 {
                    self.alloc(total + con_words, p);
                }
                // Every binding's address is in its slot before any object is filled (letrec).
                let mut off = 0usize;
                let mut offs = Vec::with_capacity(binds.len());
                for (slot, b) in binds {
                    match b {
                        SBind::Con(name, args) if args.is_empty() => {
                            let id = self.con_id(name);
                            let lbl = self.nullary_con(id);
                            let _ = writeln!(self.out, "    leaq {lbl}(%rip), %rcx");
                            let _ = writeln!(self.out, "    movq %rcx, {}", p.slot(*slot));
                            offs.push(0);
                        }
                        _ => {
                            self.emit(&format!("leaq {}(%rax), %rcx", 8 * off));
                            self.emit(&format!("movq %rcx, {}", p.slot(*slot)));
                            offs.push(off);
                            off += bind_words(b);
                        }
                    }
                }
                for ((_, b), base) in binds.iter().zip(offs) {
                    match b {
                        SBind::Closure(c) => self.fill_closure(c, base, p),
                        SBind::Con(_, args) if args.is_empty() => {}
                        SBind::Con(name, args) => {
                            let id = self.con_id(name);
                            let hdr = header(CON, args.len() as u64, id as u64);
                            let _ = writeln!(self.out, "    movabsq ${hdr}, %rcx");
                            let _ = writeln!(self.out, "    movq %rcx, {}(%rax)", 8 * base);
                            for (i, a) in args.iter().enumerate() {
                                self.load(a, p, "%rdx");
                                let _ = writeln!(self.out, "    movq %rdx, {}(%rax)", 8 * (base + 1 + i));
                            }
                        }
                    }
                }
                if let Some((name, args)) = fuse_con {
                    let id = self.con_id(name);
                    let hdr = header(CON, args.len() as u64, id as u64);
                    let _ = writeln!(self.out, "    movabsq ${hdr}, %rcx");
                    let _ = writeln!(self.out, "    movq %rcx, {}(%rax)", 8 * off);
                    for (i, a) in args.iter().enumerate() {
                        self.load(a, p, "%rdx");
                        let _ = writeln!(self.out, "    movq %rdx, {}(%rax)", 8 * (off + 1 + i));
                    }
                    if off == 0 {
                        self.emit("movq %rax, %r14");
                    } else {
                        let _ = writeln!(self.out, "    leaq {}(%rax), %r14", 8 * off);
                    }
                    self.ret(p);
                } else {
                    self.expr(body, p);
                }
            }
            // An operation on raw numbers that cannot fail, for a raw binder: computed into a
            // register, with no return address or slow path.
            SExpr::Case(scrut, b, alts) if self.keeps_cache(e, p) && matches!(scrut.as_ref(), SExpr::Prim(..)) => {
                let (SExpr::Prim(name, args), [SAlt::Default(body)]) = (scrut.as_ref(), &alts[..]) else { unreachable!() };
                if self.compare_and_branch(name, args, *b, body, p) {
                    return;
                }
                self.raw_op(name, args, *b, p);
                self.expr(body, p);
            }
            SExpr::Case(scrut, _, alts) if matches!(scrut.as_ref(), SExpr::Atom(a) if is_raw(a)) => {
                // A case of a raw number: its literal alternatives compare the number itself (the
                // binder is unused, see `stg::raw_uses`). Each alternative starts with the
                // registers as they are here; the default one follows the comparisons.
                let SExpr::Atom(a) = scrut.as_ref() else { unreachable!() };
                if let Some(SAlt::Default(body)) = alts.first() {
                    // Only a default (a bang pattern): nothing to compare.
                    self.expr(body, p);
                    return;
                }
                let Src::Reg(r) = self.raw_src(a, p, &[]) else { unreachable!() };
                let end = alts.iter().position(|a| matches!(a, SAlt::Default(_))).map_or(alts.len(), |i| i + 1);
                let alts = &alts[..end];
                let labels: Vec<String> = alts.iter().map(|_| self.label("alt")).collect();
                let lit_count = alts.iter().filter(|a| matches!(a, SAlt::Lit(..))).count();
                if lit_count >= 8 {
                    let mut seen = std::collections::HashSet::with_capacity(lit_count);
                    let mut entries: Vec<(i64, &str)> = Vec::with_capacity(lit_count);
                    for (a, l) in alts.iter().zip(&labels) {
                        match a {
                            SAlt::Lit(lit, _) => {
                                let v = match lit {
                                    Lit::Int(i) => *i,
                                    Lit::Char(c) => crate::syntax::lexer::char_code(*c) as i64,
                                    other => panic!("internal: raw case on literal {other:?}"),
                                };
                                if seen.insert(v) {
                                    entries.push((v, l.as_str()));
                                }
                            }
                            SAlt::Default(_) => {}
                            SAlt::Con(..) => panic!("internal: raw case on a constructor"),
                        }
                    }
                    entries.sort_unstable_by_key(|&(v, _)| v);
                    let def_lbl = self.label("def");
                    self.emit_lit_bsearch(r, &entries, &def_lbl);
                    self.put_label(&def_lbl);
                } else {
                    for (a, l) in alts.iter().zip(&labels) {
                        match a {
                            SAlt::Lit(lit, _) => {
                                let v = match lit {
                                    Lit::Int(i) => *i,
                                    Lit::Char(c) => crate::syntax::lexer::char_code(*c) as i64,
                                    other => panic!("internal: raw case on literal {other:?}"),
                                };
                                let t = self.src_text(Src::Imm(v), "%rcx");
                                self.emit(&format!("cmpq {t}, {r}"));
                                self.emit(&format!("je {l}"));
                            }
                            SAlt::Default(_) => {}
                            SAlt::Con(..) => panic!("internal: raw case on a constructor"),
                        }
                    }
                }
                let snapshot = self.cache.clone();
                match alts.last() {
                    Some(SAlt::Default(body)) => self.expr(body, p),
                    _ => self.fail("Non-exhaustive patterns in case", p),
                }
                for (a, l) in alts.iter().zip(&labels) {
                    if let SAlt::Lit(_, body) = a {
                        self.put_label(l);
                        self.cache = snapshot.clone();
                        self.expr(body, p);
                    }
                }
            }
            SExpr::Case(scrut, bslot, alts) => {
                let raw_binder = *bslot & RAW != 0;
                if raw_binder {
                    if let SExpr::Prim(name, args) = scrut.as_ref() {
                        if self.boxed_compare_and_branch(name, args, *bslot, alts, p) {
                            return;
                        }
                    }
                }
                let cont = self.label("k");
                let mut fast_body: Option<String> = None;
                // A variable that is already a value (not a thunk or an indirection) goes straight
                // to the alternatives, without the return address and `hsc_eval`.
                let ready = match scrut.as_ref() {
                    SExpr::Atom(a @ (Atom::Slot(_) | Atom::Free(_) | Atom::Global(_))) => {
                        let ready = self.label("ready");
                        let eval = self.label("eval");
                        let value = self.label("value");
                        // An indirection (an updated thunk, as a captured variable often is) is
                        // followed here: only a thunk needs the evaluation.
                        let check = self.label("chk");
                        self.load(a, p, "%r14");
                        self.put_label(&check);
                        self.emit("testq %r14, %r14");
                        let _ = writeln!(self.out, "    je {value}");
                        // Kinds 1..7 (CON, INT, DOUBLE, CHAR, INTEGER, FUN, PAP) are all values.
                        let _ = writeln!(self.out, "    cmpb ${THUNK}, (%r14)");
                        let _ = writeln!(self.out, "    jb {value}");
                        let _ = writeln!(self.out, "    je {eval}");
                        let _ = writeln!(self.out, "    cmpb ${IND}, (%r14)");
                        let _ = writeln!(self.out, "    jne {value}");
                        self.emit("movq 8(%r14), %r14");
                        let _ = writeln!(self.out, "    jmp {check}");
                        // A value: as on the slow path, its slot is cleared when nothing uses it
                        // after (only here: the slow path loads the slot again).
                        self.put_label(&value);
                        let live = self.case_live(alts, p);
                        self.live.push(live);
                        self.clear_dead(std::slice::from_ref(a), Pos { mode: Mode::Scrut, ..p });
                        self.live.pop();
                        let _ = writeln!(self.out, "    jmp {ready}");
                        self.put_label(&eval);
                        Some(ready)
                    }
                    // A primitive with an inline fast path is computed in place (before the return
                    // address, which only its slow path needs).
                    SExpr::Prim(name, args) => {
                        let mut matched_mv = false;
                        if !raw_binder && !args.is_empty() && !is_raw(&args[0]) && !matches!(args[0], Atom::Lit(_)) {
                            let non_alloc = |a: &Atom| {
                                !is_raw(a)
                                    && match a {
                                        Atom::Lit(SLit::Str(_) | SLit::Frac(_) | SLit::Addr(_)) => false,
                                        Atom::Lit(SLit::Integer(s)) => s.parse::<i64>().is_ok(),
                                        _ => true,
                                    }
                            };
                            if name == "readMutVar#" && args.len() == 2 {
                                if let Some(SAlt::Con(c, slots, body)) = alts.first() {
                                    if c == "Builtin.(,)" && slots.len() == 2 {
                                        let mut uses = std::collections::HashSet::new();
                                        self.slot_uses(body, &mut uses);
                                        if !uses.contains(bslot) {
                                            let body_lbl = self.label("mv_body");
                                            let slow = self.label("slow");
                                            self.load(&args[0], p, "%rax");
                                            let _ = writeln!(self.out, "    cmpb ${REF}, (%rax)");
                                            let _ = writeln!(self.out, "    jne {slow}");
                                            let _ = writeln!(self.out, "    movq $0, {}(%rbp)", p.slot_off(slots[0]));
                                            self.emit("movq 8(%rax), %rcx");
                                            let _ = writeln!(self.out, "    movq %rcx, {}(%rbp)", p.slot_off(slots[1]));
                                            let _ = writeln!(self.out, "    jmp {body_lbl}");
                                            self.put_label(&slow);
                                            self.slow_only = true;
                                            fast_body = Some(body_lbl);
                                            matched_mv = true;
                                        }
                                    }
                                }
                            } else if name == "writeMutVar#" && args.len() == 3 && non_alloc(&args[1]) {
                                if let [SAlt::Default(body)] = &alts[..] {
                                    if !matches!(body, SExpr::Fail(_)) {
                                        let mut uses = std::collections::HashSet::new();
                                        self.slot_uses(body, &mut uses);
                                        let body_lbl = self.label("mv_body");
                                        let slow = self.label("slow");
                                        self.load(&args[0], p, "%rax");
                                        let _ = writeln!(self.out, "    cmpb ${REF}, (%rax)");
                                        let _ = writeln!(self.out, "    jne {slow}");
                                        self.load(&args[1], p, "%rcx");
                                        self.emit("movq %rcx, 8(%rax)");
                                        if uses.contains(bslot) {
                                            let unit_id = self.con_id("Builtin.()");
                                            let unit_lbl = self.nullary_con(unit_id);
                                            let _ = writeln!(self.out, "    leaq {unit_lbl}(%rip), %rcx");
                                            let _ = writeln!(self.out, "    movq %rcx, {}(%rbp)", p.slot_off(*bslot));
                                        }
                                        let _ = writeln!(self.out, "    jmp {body_lbl}");
                                        self.put_label(&slow);
                                        self.slow_only = true;
                                        fast_body = Some(body_lbl);
                                        matched_mv = true;
                                    }
                                }
                            }
                        }
                        if matched_mv {
                            None
                        } else {
                            let ready = self.label("ready");
                            let slow = self.label("slow");
                            self.fast_done = Some(ready.clone());
                            // A raw binder takes the number straight from the inline path.
                            self.raw_dest = raw_binder.then_some(*bslot);
                            let fast = self.int_fast_path(name, args, p, &slow);
                            self.raw_dest = None;
                            self.fast_done = None;
                            if fast {
                                self.put_label(&slow);
                                self.slow_only = true;
                                Some(ready)
                            } else {
                                None
                            }
                        }
                    }
                    _ => None,
                };
                // Push the return address (its frame is this body's, above it).
                self.emit("subq $8, %rbp");
                let _ = writeln!(self.out, "    leaq {cont}(%rip), %rax");
                self.emit("movq %rax, (%rbp)");
                let sp = Pos { f: p.f, mode: Mode::Scrut, pushed: p.pushed + 1 };
                // What the alternatives (and, for a nested case, what follows it) still use.
                let live = self.case_live(alts, p);
                self.live.push(live);
                self.expr(scrut, sp);
                self.live.pop();
                // The info of the return address: its kind at -16 and the size of the frame above it
                // at -8. Control reaches the label only by a return, never by falling through. A case
                // in a scrutinee (nested) has the outer case's return address above it, not the
                // frame: its own frame is empty (the collector reads the outer one's next).
                let size = if p.pushed == 0 { p.f.size_word() } else { 0 };
                let _ = writeln!(self.out, "    .quad {}, {}", frame::BODY, size);
                self.put_label(&cont);
                self.emit("addq $8, %rbp");
                if raw_binder {
                    // A cell (the runtime's answer, or the evaluated variable): its number goes to
                    // the raw binder. An inline primitive stored it already and continues after.
                    let stored = matches!(scrut.as_ref(), SExpr::Prim(..));
                    if !stored {
                        if let Some(ready) = &ready {
                            self.put_label(ready);
                        }
                    }
                    if self.returns_raw(scrut) {
                        // A worker returning raw answered the number itself in %r14: keep it in
                        // a cache register so straight-line arithmetic reads it without spilling.
                        let reg = CACHE_REGS[0];
                        let _ = writeln!(self.out, "    movq %r14, {reg}");
                        self.cache_put(*bslot, reg, true);
                    } else {
                        self.emit("movq 8(%r14), %rcx");
                        let _ = writeln!(self.out, "    movq %rcx, {}(%rbp)", p.slot_off(*bslot));
                    }
                    if stored {
                        if let Some(ready) = ready {
                            self.put_label(&ready);
                        }
                    }
                    let [SAlt::Default(body)] = &alts[..] else { panic!("internal: raw binder with alternatives") };
                    self.expr(body, p);
                    return;
                }
                if let Some(ready) = ready {
                    self.put_label(&ready);
                }
                let _ = writeln!(self.out, "    movq %r14, {}(%rbp)", p.slot_off(*bslot));
                self.alts(alts, *bslot, p, fast_body.as_deref());
            }
            SExpr::Closure(c) => {
                self.alloc(closure_words(c), p);
                self.fill_closure(c, 0, p);
                self.emit("movq %rax, %r14");
                self.ret(p);
            }
            SExpr::Fail(msg) => {
                self.fail(msg, p);
            }
            SExpr::Join(id, params, code, body) => {
                // Every expression ends in a transfer of control, so the join point's code follows
                // the body; jumps reach it in the same frame and mode.
                let l = self.label("join");
                self.joins.insert(*id, (l.clone(), params.clone()));
                let mut uses = std::collections::HashSet::new();
                self.slot_uses(code, &mut uses);
                self.join_uses.insert(*id, uses);
                self.expr(body, p);
                self.put_label(&l);
                self.expr(code, p);
            }
            SExpr::Jump(id, args) => {
                let (l, params) = self.joins[id].clone();
                // The parameters are fresh slots that no argument refers to.
                for (a, s) in args.iter().zip(&params) {
                    self.load(a, p, "%rcx");
                    let _ = writeln!(self.out, "    movq %rcx, {}(%rbp)", p.slot_off(*s));
                }
                let _ = writeln!(self.out, "    jmp {l}");
            }
        }
    }

    /// Emits a binary search over sorted `(val, target_label)` pairs, comparing `reg` (never
    /// `%rcx`, which is scratch for 64-bit immediates) against each pivot and jumping to
    /// `default_lbl` on miss.
    fn emit_lit_bsearch(&mut self, reg: &str, entries: &[(i64, &str)], default_lbl: &str) {
        if entries.len() <= 4 {
            for &(v, lbl) in entries {
                let t = self.src_text(Src::Imm(v), "%rcx");
                self.emit(&format!("cmpq {t}, {reg}"));
                self.emit(&format!("je {lbl}"));
            }
            let _ = writeln!(self.out, "    jmp {default_lbl}");
            return;
        }
        let mid = entries.len() / 2;
        let (v, lbl) = entries[mid];
        let left_lbl = self.label("bs");
        let t = self.src_text(Src::Imm(v), "%rcx");
        self.emit(&format!("cmpq {t}, {reg}"));
        self.emit(&format!("je {lbl}"));
        let _ = writeln!(self.out, "    jl {left_lbl}");
        self.emit_lit_bsearch(reg, &entries[mid + 1..], default_lbl);
        self.put_label(&left_lbl);
        self.emit_lit_bsearch(reg, &entries[..mid], default_lbl);
    }

    /// The alternatives of a case: the value is in R1 and in the binder slot `bslot`.
    fn alts(&mut self, alts: &[SAlt], bslot: u32, p: Pos, fast_body: Option<&str>) {
        let labels: Vec<String> = alts.iter().map(|_| self.label("alt")).collect();
        let mut has_default = false;
        let end = alts.iter().position(|a| matches!(a, SAlt::Default(_))).unwrap_or(alts.len());
        let all_int = end >= 8 && alts[..end].iter().all(|a| matches!(a, SAlt::Lit(Lit::Int(_), _)));
        let all_char = !all_int && end >= 8 && alts[..end].iter().all(|a| matches!(a, SAlt::Lit(Lit::Char(_), _)));
        if all_int || all_char {
            let has_def = end < alts.len();
            let def_lbl = if has_def { labels[end].clone() } else { self.label("def") };
            let kind = if all_char { CHAR } else { INT };
            let _ = writeln!(self.out, "    cmpb ${kind}, (%r14)");
            let _ = writeln!(self.out, "    jne {def_lbl}");
            if all_char {
                self.emit("movl 8(%r14), %eax");
            } else {
                self.emit("movq 8(%r14), %rax");
            }
            let mut seen = std::collections::HashSet::with_capacity(end);
            let mut entries: Vec<(i64, &str)> = Vec::with_capacity(end);
            for (a, l) in alts[..end].iter().zip(&labels[..end]) {
                let v = match a {
                    SAlt::Lit(Lit::Int(i), _) => *i,
                    SAlt::Lit(Lit::Char(c), _) => crate::syntax::lexer::char_code(*c) as i64,
                    _ => unreachable!(),
                };
                if seen.insert(v) {
                    entries.push((v, l.as_str()));
                }
            }
            entries.sort_unstable_by_key(|&(v, _)| v);
            self.emit_lit_bsearch("%rax", &entries, &def_lbl);
            if !has_def {
                self.put_label(&def_lbl);
            }
            has_default = has_def;
        } else {
            let con_count = alts.iter().filter(|a| matches!(a, SAlt::Con(..))).count();
            let load_con_tag = con_count >= 2 && alts.iter().all(|a| matches!(a, SAlt::Con(..) | SAlt::Default(_)));
            if load_con_tag {
                self.emit("movl 4(%r14), %ecx");
            }
            for (a, l) in alts.iter().zip(&labels) {
                match a {
                    SAlt::Con(name, _, _) => {
                        let id = self.con_id(name);
                        if load_con_tag {
                            let _ = writeln!(self.out, "    cmpl ${id}, %ecx");
                        } else {
                            let _ = writeln!(self.out, "    cmpl ${id}, 4(%r14)");
                        }
                        let _ = writeln!(self.out, "    je {l}");
                    }
                    SAlt::Lit(lit @ (Lit::Int(_) | Lit::Char(_) | Lit::Double(_)), _) => {
                        // A number or character: its kind and payload are compared inline.
                        let next = self.label("lit");
                        let (kind, payload, cmp) = match lit {
                            Lit::Int(i) => (INT, *i as u64, "cmpq %rcx, 8(%r14)"),
                            Lit::Char(c) => (CHAR, crate::syntax::lexer::char_code(*c) as u64, "cmpl %ecx, 8(%r14)"),
                            Lit::Double(d) => (DOUBLE, d.to_bits(), "cmpq %rcx, 8(%r14)"),
                            _ => unreachable!(),
                        };
                        let _ = writeln!(self.out, "    cmpb ${kind}, (%r14)");
                        let _ = writeln!(self.out, "    jne {next}");
                        let _ = writeln!(self.out, "    movabsq ${payload}, %rcx");
                        self.emit(cmp);
                        let _ = writeln!(self.out, "    je {l}");
                        self.put_label(&next);
                    }
                    SAlt::Lit(lit, _) => {
                        // The runtime compares the scrutinee with the literal's text (no allocation).
                        let (kind, text) = lit_text(lit);
                        let t = self.string_label(text.as_bytes());
                        let _ = writeln!(self.out, "    movq {}(%rbp), %rsi", p.slot_off(bslot));
                        let _ = writeln!(self.out, "    leaq {t}(%rip), %rdx");
                        let _ = writeln!(self.out, "    movl ${}, %ecx", text.len());
                        let _ = writeln!(self.out, "    movl ${kind}, %r8d");
                        self.rt_call("hsc_lit_matches", p, 4);
                        self.emit("testl %eax, %eax");
                        let _ = writeln!(self.out, "    jnz {l}");
                    }
                    SAlt::Default(_) => {
                        let _ = writeln!(self.out, "    jmp {l}");
                        has_default = true;
                        break;
                    }
                }
            }
        }
        if !has_default {
            // Diagnostic: the scrutinee is left in the record for the failure's message.
            let _ = writeln!(self.out, "    movq {}(%rbp), %rax", p.slot_off(bslot));
            let _ = writeln!(self.out, "    movq %rax, {}(%rbx)", mach::R1);
            let names: Vec<String> = alts
                .iter()
                .map(|a| match a {
                    SAlt::Con(n, _, _) => n.clone(),
                    SAlt::Lit(l, _) => format!("{:?}", l),
                    SAlt::Default(_) => "_".into(),
                })
                .collect();
            let fail_msg = format!("Non-exhaustive patterns in case [{}]", names.join(" | "));
            self.fail(&fail_msg, p);
        }
        for (idx, (a, l)) in alts.iter().zip(&labels).enumerate() {
            self.put_label(l);
            match a {
                SAlt::Con(_, slots, body) => {
                    for (i, s) in slots.iter().enumerate() {
                        let _ = writeln!(self.out, "    movq {}(%r14), %rcx", 8 * (i + 1));
                        let _ = writeln!(self.out, "    movq %rcx, {}(%rbp)", p.slot_off(*s));
                    }
                    if idx == 0 {
                        if let Some(fb) = fast_body {
                            self.put_label(fb);
                        }
                    }
                    self.expr(body, p);
                }
                SAlt::Default(SExpr::Fail(msg)) => {
                    // Diagnostic: a pattern failure leaves the scrutinee in the record.
                    let _ = writeln!(self.out, "    movq {}(%rbp), %rax", p.slot_off(bslot));
                    let _ = writeln!(self.out, "    movq %rax, {}(%rbx)", mach::R1);
                    self.fail(msg, p);
                }
                SAlt::Lit(_, body) | SAlt::Default(body) => {
                    if idx == 0 {
                        if let Some(fb) = fast_body {
                            self.put_label(fb);
                        }
                    }
                    self.expr(body, p);
                }
            }
        }
    }

    /// A pattern-match failure: the runtime builds the exception (with the stack in its state at
    /// this point), and it is raised.
    fn fail(&mut self, msg: &str, p: Pos) {
        let l = self.string_label(msg.as_bytes());
        let _ = writeln!(self.out, "    leaq {l}(%rip), %rsi");
        let _ = writeln!(self.out, "    movl ${}, %edx", msg.len());
        self.emit_top(p);
        self.emit("jmp hsc_fail_rt");
    }

    /// Loads an atom into a register.
    fn load(&mut self, a: &Atom, p: Pos, reg: &str) {
        match a {
            Atom::Slot(k) if k & RAW != 0 => panic!("internal: raw slot {k:#x} used as a pointer in {}", self.current_name),
            Atom::Slot(k) => {
                let _ = writeln!(self.out, "    movq {}(%rbp), {reg}", p.slot_off(*k));
            }
            Atom::Free(i) => {
                let _ = writeln!(self.out, "    movq {}(%rbp), {reg}", p.slot_off(0));
                let _ = writeln!(self.out, "    movq {}({reg}), {reg}", 8 * (2 + *i as i64));
            }
            Atom::Global(g) => {
                if let Some(lbl) = self.nullary_globals.get(g) {
                    let _ = writeln!(self.out, "    leaq {lbl}(%rip), {reg}");
                } else {
                    let _ = writeln!(self.out, "    movq {}(%rbx), {reg}", mach::GLOBALS);
                    let _ = writeln!(self.out, "    movq {}({reg}), {reg}", 8 * *g as i64);
                }
            }
            Atom::Lit(l) => self.literal(l, p, reg),
        }
    }

    fn static_cell(&mut self, kind: u8, payload: u64) -> String {
        let hdr = header(kind, 1, 0);
        if let Some(lbl) = self.static_cells.get(&(hdr, payload)) {
            return lbl.clone();
        }
        let lbl = self.label("lit");
        self.statics.push((lbl.clone(), hdr, payload.to_string()));
        self.static_cells.insert((hdr, payload), lbl.clone());
        lbl
    }

    fn static_literal_label(&mut self, l: &SLit) -> Option<String> {
        Some(match l {
            SLit::Int(i) => self.static_cell(INT, *i as u64),
            SLit::Char(c) => self.static_cell(CHAR, crate::syntax::lexer::char_code(*c) as u64),
            SLit::Double(d) => self.static_cell(DOUBLE, d.to_bits()),
            // An `Integer` that fits in an `Int` is `IS` of it: a static cell for both.
            SLit::Integer(s) => {
                let val = s.parse::<i64>().ok()? as u64;
                let id = self.con_id("GHC.Num.Integer.IS");
                let hdr = header(CON, 1, id as u64);
                if let Some(lbl) = self.static_cells.get(&(hdr, val)) {
                    lbl.clone()
                } else {
                    let small = self.static_cell(INT, val);
                    let lbl = self.label("lit");
                    self.statics.push((lbl.clone(), hdr, small));
                    self.static_cells.insert((hdr, val), lbl.clone());
                    lbl
                }
            }
            _ => return None,
        })
    }

    fn literal(&mut self, l: &SLit, p: Pos, reg: &str) {
        if let Some(lbl) = self.static_literal_label(l) {
            let _ = writeln!(self.out, "    leaq {lbl}(%rip), {reg}");
            return;
        }
        // Strings, big integers, fractions and addresses: the runtime builds the object (the
        // call allocates, so the atom is loaded last).
        let (kind, text) = match l {
            SLit::Str(s) => (0u64, s.clone()),
            SLit::Integer(s) => (1, s.clone()),
            SLit::Frac(s) => (2, s.clone()),
            SLit::Addr(s) => (3, s.clone()),
            _ => unreachable!("numbers and characters are static"),
        };
        // An `Addr#` literal's characters are its bytes (as the interpreter takes them).
        let latin1 = kind == 3 && text.chars().all(|c| (c as u32) < 256);
        let bytes: Vec<u8> = if latin1 { text.chars().map(|c| c as u32 as u8).collect() } else { text.into_bytes() };
        let t = self.string_label(&bytes);
        let _ = writeln!(self.out, "    leaq {t}(%rip), %rsi");
        let _ = writeln!(self.out, "    movl ${}, %edx", bytes.len());
        let _ = writeln!(self.out, "    movl ${kind}, %ecx");
        self.rt_call("hsc_mk_literal", p, 3);
        if reg != "%rax" {
            let _ = writeln!(self.out, "    movq %rax, {reg}");
        }
    }

    fn nullary_con(&mut self, id: u32) -> String {
        if let Some(label) = self.nullary_cons.get(&id) {
            return label.clone();
        }
        let label = self.label("con");
        // A zero-field object still occupies two words, like every other native object.
        self.statics.push((label.clone(), header(CON, 0, id as u64), "0".into()));
        self.nullary_cons.insert(id, label.clone());
        label
    }

    fn string_label(&mut self, bytes: &[u8]) -> String {
        if let Some(l) = self.string_pool.get(bytes) {
            return l.clone();
        }
        let l = self.label("str");
        self.strings.push((l.clone(), bytes.to_vec()));
        self.string_pool.insert(bytes.to_vec(), l.clone());
        l
    }

    /// Allocates `words` words into %rax. A full heap calls the collector with the frame as it is.
    fn alloc(&mut self, words: usize, p: Pos) {
        // Every object has at least two words (the collector relies on it).
        let words = words.max(2);
        let ok = self.next_id();
        let _ = writeln!(self.out, "    leaq {}(%r12), %rax", 8 * words);
        self.emit("cmpq %r13, %rax");
        let _ = writeln!(self.out, "    jbe .L{ok}");
        let _ = writeln!(self.out, "    movl ${words}, %esi");
        self.rt_call("hsc_gc", p, 1);
        let _ = writeln!(self.out, ".L{ok}:");
        self.emit("movq %r12, %rax");
        let _ = writeln!(self.out, "    leaq {}(%r12), %r12", 8 * words);
    }

    /// Fills the closure at word `base` of the block in %rax; its body is generated separately.
    fn fill_closure(&mut self, c: &SClosure, base: usize, p: Pos) {
        let code = self.label("c");
        self.pending.push((code.clone(), c.clone(), format!("{}/closure", self.current_name)));
        let hdr = if c.arity > 0 {
            header(FUN, c.captures.len() as u64 + 1, c.arity as u64)
        } else {
            header(THUNK, (c.captures.len() as u64 + 1).max(1), 0)
        };
        let _ = writeln!(self.out, "    movabsq ${hdr}, %rcx");
        let _ = writeln!(self.out, "    movq %rcx, {}(%rax)", 8 * base);
        let _ = writeln!(self.out, "    leaq {code}(%rip), %rcx");
        let _ = writeln!(self.out, "    movq %rcx, {}(%rax)", 8 * (base + 1));
        for (i, cap) in c.captures.iter().enumerate() {
            // The captured atom is loaded through %rdx so that %rax (the block) survives.
            self.load(cap, p, "%rdx");
            let _ = writeln!(self.out, "    movq %rdx, {}(%rax)", 8 * (base + 2 + i));
        }
    }

    /// `catch# io handler s`: a catch frame (handler) is pushed, then `io` is applied to the state
    /// token. Its normal return pops the frame; an exception unwinds to it.
    fn catch(&mut self, args: &[Atom], p: Pos) {
        self.load(&args[1], p, "%rdx");
        self.load(&args[2], p, "%r8");
        self.load(&args[0], p, "%r14");
        if p.mode == Mode::Tail && p.f.s > 0 {
            let _ = writeln!(self.out, "    addq ${}, %rbp", 8 * p.f.s as i64);
        }
        self.emit("subq $16, %rbp");
        self.emit("leaq hsc_catch_ret(%rip), %rax");
        self.emit("movq %rax, (%rbp)");
        self.emit("movq %rdx, 8(%rbp)");
        self.emit("subq $8, %rbp");
        self.emit("movq %r8, (%rbp)");
        self.emit("movl $1, %ecx");
        self.emit("jmp hsc_apply");
    }

    /// A primitive: its arguments are copied into the machine's argument buffer (which the collector
    /// scans), then the runtime runs it; a zero answer raises the pending exception.
    fn prim(&mut self, name: &str, args: &[Atom], p: Pos) {
        if name == "catch#" && args.len() == 3 {
            self.catch(args, p);
            return;
        }
        let slow = self.label("slow");
        // A case scrutinee whose fast path was already emitted in place has only its slow path.
        let fast = !std::mem::take(&mut self.slow_only) && self.int_fast_path(name, args, p, &slow);
        if fast {
            self.put_label(&slow);
        }
        let id = self.prim_id(name);
        for (i, a) in args.iter().enumerate() {
            if let Atom::Slot(k) = a {
                if k & RAW != 0 {
                    // A raw operand is boxed for the runtime (the arguments in the record are roots,
                    // so a collection here keeps those already stored).
                    self.alloc(2, p);
                    let _ = writeln!(self.out, "    movq ${}, (%rax)", header(operand_kind(name), 1, 0));
                    let _ = writeln!(self.out, "    movq {}(%rbp), %rcx", p.slot_off(*k));
                    self.emit("movq %rcx, 8(%rax)");
                    let _ = writeln!(self.out, "    movq %rax, {}(%rbx)", mach::ARGS + 8 * i as i64);
                    continue;
                }
            }
            self.load(a, p, "%rdx");
            let _ = writeln!(self.out, "    movq %rdx, {}(%rbx)", mach::ARGS + 8 * i as i64);
        }
        let _ = writeln!(self.out, "    movl ${id}, %esi");
        let _ = writeln!(self.out, "    movl ${}, %edx", args.len());
        self.rt_call("hsc_prim", p, 2);
        self.ret(p);
    }

    /// An `Int#` operation on arguments that are already INT cells, computed inline; otherwise (or
    /// when a division could fault) control goes to `slow`, the runtime's primitive. (Thread
    /// switches wait for a function's entry, see `closure_code`.) Answers whether the fast path
    /// was emitted.
    fn int_fast_path(&mut self, name: &str, args: &[Atom], p: Pos, slow: &str) -> bool {
        if self.raw_dest.is_none() && self.mutvar_fast_path(name, args, p, slow) {
            return true;
        }
        if self.integer_fast_path(name, args, p, slow) {
            return true;
        }
        let Some(k) = int_op(name) else { return false };
        let arity = if matches!(k, K::Un(_) | K::Ord | K::Chr | K::TagToEnum) { 1 } else { 2 };
        let (in_kind, out_kind) = match k {
            K::CharCmp(_) => (CHAR, INT),
            K::Ord => (CHAR, INT),
            K::Chr => (INT, CHAR),
            _ => (INT, INT),
        };
        let lit_ok = |l: &SLit| matches!((l, in_kind), (SLit::Int(_), INT) | (SLit::Char(_), CHAR));
        if args.len() != arity || args.iter().any(|a| matches!(a, Atom::Lit(l) if !lit_ok(l))) {
            return false;
        }
        // A raw destination (a case binder kept unboxed) takes the number itself.
        let dest = self.raw_dest;
        if dest.is_some() && k == K::TagToEnum {
            return false;
        }
        // Boxed operands must already be cells of the operand kind; raw ones are numbers.
        for a in args {
            // Raw operands and literals are numbers already.
            if is_raw(a) || matches!(a, Atom::Lit(_)) {
                continue;
            }
            self.load(a, p, "%rax");
            self.emit(&format!("cmpb ${in_kind}, (%rax)"));
            self.emit(&format!("jne {slow}"));
        }
        if matches!(k, K::Div(_)) {
            // A divisor of 0 or -1 takes the runtime's path (x86 faults on minBound / -1).
            self.operand(&args[1], p, "%rax");
            self.emit("leaq 1(%rax), %rcx");
            self.emit("cmpq $1, %rcx");
            self.emit(&format!("jbe {slow}"));
        }
        if k == K::TagToEnum {
            let t = self.con_id("GHC.Types.True");
            let f = self.con_id("GHC.Types.False");
            let (t, f) = (self.nullary_con(t), self.nullary_con(f));
            self.operand(&args[0], p, "%rcx");
            self.emit("testq %rcx, %rcx");
            self.emit(&format!("leaq {f}(%rip), %r14"));
            self.emit(&format!("leaq {t}(%rip), %rax"));
            self.emit("cmovneq %rax, %r14");
            self.fast_ret(p);
            return true;
        }
        if matches!(k, K::Cmp(_) | K::CharCmp(_)) {
            self.operand(&args[0], p, "%rcx");
            self.operand(&args[1], p, "%rdx");
            let (cmp, set) = match k {
                K::Cmp(set) => ("cmpq %rdx, %rcx", set),
                K::CharCmp(set) => ("cmpl %edx, %ecx", set),
                _ => unreachable!(),
            };
            self.emit(cmp);
            if let Some(d) = dest {
                self.emit(&format!("{set} %cl"));
                self.emit("movzbl %cl, %ecx");
                self.emit(&format!("movq %rcx, {}", p.slot(d)));
            } else {
                // A comparison answers one of two shared static cells: nothing is allocated.
                let (zero, one) = self.bool_cells();
                self.emit(&format!("leaq {zero}(%rip), %r14"));
                self.emit(&format!("leaq {one}(%rip), %rax"));
                let cmov = set.replacen("set", "cmov", 1);
                self.emit(&format!("{cmov}q %rax, %r14"));
            }
            self.fast_ret(p);
            return true;
        }
        // A boxed result's cell is allocated first: a collection moves the boxed operands, so
        // they are loaded after.
        if dest.is_none() {
            self.alloc(2, p);
            self.emit("movq %rax, %r8");
        }
        self.operand(&args[0], p, "%rcx");
        if arity == 2 {
            self.operand(&args[1], p, "%rdx");
        }
        match k {
            K::Bin(op) => self.emit(&format!("{op} %rdx, %rcx")),
            K::Shift(op) => {
                self.emit("xchgq %rdx, %rcx");
                self.emit(&format!("{op} %cl, %rdx"));
                self.emit("movq %rdx, %rcx");
            }
            K::Div(quot) => {
                self.emit("movq %rcx, %rax");
                self.emit("movq %rdx, %rcx");
                self.emit("cqto");
                self.emit("idivq %rcx");
                self.emit(if quot { "movq %rax, %rcx" } else { "movq %rdx, %rcx" });
            }
            K::Un(op) => self.emit(&format!("{op} %rcx")),
            K::Ord => {}
            // The code point is the low 32 bits (as the runtime takes it).
            K::Chr => self.emit("movl %ecx, %ecx"),
            K::Cmp(_) | K::CharCmp(_) | K::TagToEnum => unreachable!("answered without allocating"),
        }
        match dest {
            Some(d) => self.emit(&format!("movq %rcx, {}", p.slot(d))),
            None => {
                self.emit(&format!("movq ${}, (%r8)", header(out_kind, 1, 0)));
                self.emit("movq %rcx, 8(%r8)");
                self.emit("movq %r8, %r14");
            }
        }
        self.fast_ret(p);
        true
    }

    /// A call: the ordinary entry of `func`, or (an exact call) a global's worker, which takes its
    /// first `MAX_REG_RAW_ARGS` raw parameters in `CACHE_REGS` and any further parameters on the stack.
    fn call(&mut self, func: &Atom, args: &[Atom], worker: Option<(String, Vec<bool>, bool, bool)>, p: Pos) {
        let direct = match &worker {
            Some((l, _, _, uc)) => Some((l.clone(), *uc)),
            None => self.known_global_entry(func, args.len()).map(|(l, uc)| (l.to_owned(), uc)),
        };
        let mut reg_raw_args: Vec<&Atom> = Vec::new();
        let mut stack_args: Vec<(&Atom, bool)> = Vec::new();
        if let Some((_, ref raw_pos, _, _)) = worker {
            for (i, a) in args.iter().enumerate() {
                if raw_pos.get(i).copied().unwrap_or(false) {
                    if reg_raw_args.len() < MAX_REG_RAW_ARGS {
                        reg_raw_args.push(a);
                    } else {
                        stack_args.push((a, true));
                    }
                } else {
                    stack_args.push((a, false));
                }
            }
        } else {
            for a in args {
                stack_args.push((a, false));
            }
        }

        let stack_n = stack_args.len() as i64;
        let is_self_loop = p.mode == Mode::Tail
            && p.pushed == 0
            && match (&direct, &self.current_self) {
                (Some((entry, _)), Some((self_entry, _, self_arity))) => {
                    entry == self_entry && stack_args.len() == *self_arity as usize
                }
                _ => false,
            };

        if p.mode == Mode::Tail && p.pushed == 0 && self.in_place_args(&stack_args, &reg_raw_args, p) {
            if !is_self_loop && direct.as_ref().is_none_or(|(_, uc)| *uc) {
                self.load(func, p, "%r14");
            }
            // The stack arguments go straight to the top words of the frame, where the callee's
            // stack arguments live (none is read from a word another one replaces).
            let base = p.f.s as i64 - stack_n;
            for (i, (a, is_raw_arg)) in stack_args.iter().enumerate() {
                let dst = format!("{}(%rbp)", 8 * (base + i as i64));
                match a {
                    Atom::Slot(k) if k & RAW != 0 => match self.cached(*k) {
                        Some(r) => self.emit(&format!("movq {r}, {dst}")),
                        None if p.slot(*k) == dst => {}
                        None => {
                            self.emit(&format!("movq {}, %rcx", p.slot(*k)));
                            self.emit(&format!("movq %rcx, {dst}"));
                        }
                    },
                    Atom::Slot(k) if p.slot(*k) == dst => {}
                    Atom::Lit(l @ (SLit::Int(_) | SLit::Char(_))) if *is_raw_arg => {
                        let v = match l {
                            SLit::Int(v) => *v,
                            SLit::Char(c) => crate::syntax::lexer::char_code(*c) as i64,
                            _ => unreachable!(),
                        };
                        if i32::try_from(v).is_ok() {
                            self.emit(&format!("movq ${v}, {dst}"));
                        } else {
                            self.emit(&format!("movabsq ${v}, %rcx"));
                            self.emit(&format!("movq %rcx, {dst}"));
                        }
                    }
                    _ => {
                        self.load(a, p, "%rcx");
                        self.emit(&format!("movq %rcx, {dst}"));
                    }
                }
            }
            self.load_reg_raw_args(&reg_raw_args, p);
            if is_self_loop {
                let loop_lbl = self.current_self.as_ref().unwrap().1.clone();
                let _ = writeln!(self.out, "    jmp {loop_lbl}");
                return;
            }
            if base != 0 {
                let _ = writeln!(self.out, "    addq ${}, %rbp", 8 * base);
            }
            match direct {
                Some((entry, _)) => {
                    let _ = writeln!(self.out, "    jmp {entry}");
                }
                None => {
                    let _ = writeln!(self.out, "    movl ${stack_n}, %ecx");
                    self.emit("jmp hsc_apply");
                }
            }
            return;
        }

        if p.mode == Mode::Scrut && !self.cache.is_empty() {
            self.flush_live_raws(p);
        }
        if stack_n > 0 {
            let _ = writeln!(self.out, "    subq ${}, %rbp", 8 * stack_n);
        }
        let inner = Pos { pushed: p.pushed + stack_n, ..p };
        for (i, (a, is_raw_arg)) in stack_args.iter().enumerate() {
            if *is_raw_arg {
                match a {
                    Atom::Slot(k) if k & RAW != 0 => match self.cached(*k) {
                        Some(r) => self.emit(&format!("movq {r}, %rcx")),
                        None => self.emit(&format!("movq {}, %rcx", inner.slot(*k))),
                    },
                    Atom::Lit(SLit::Int(v)) => {
                        if i32::try_from(*v).is_ok() {
                            let _ = writeln!(self.out, "    movq ${v}, %rcx");
                        } else {
                            let _ = writeln!(self.out, "    movabsq ${v}, %rcx");
                        }
                    }
                    Atom::Lit(SLit::Char(c)) => {
                        let _ = writeln!(self.out, "    movq ${}, %rcx", crate::syntax::lexer::char_code(*c));
                    }
                    other => panic!("internal: worker argument {other:?} is not raw"),
                }
            } else {
                self.load(a, inner, "%rcx");
            }
            let _ = writeln!(self.out, "    movq %rcx, {}(%rbp)", 8 * i as i64);
        }
        if direct.as_ref().is_none_or(|(_, uc)| *uc) {
            self.load(func, inner, "%r14");
        }
        self.load_reg_raw_args(&reg_raw_args, inner);
        if p.mode == Mode::Tail {
            let s = p.f.s as i64;
            if s > 0 {
                // Highest first: the destination of an argument may be the source of a later one.
                for i in (0..stack_n).rev() {
                    let _ = writeln!(self.out, "    movq {}(%rbp), %rcx", 8 * i);
                    let _ = writeln!(self.out, "    movq %rcx, {}(%rbp)", 8 * (i + s));
                }
                let _ = writeln!(self.out, "    addq ${}, %rbp", 8 * s);
            }
        } else {
            let mut used = vec![func.clone()];
            used.extend(args.iter().cloned());
            self.clear_dead(&used, inner);
        }
        if let Some((entry, _)) = direct {
            let _ = writeln!(self.out, "    jmp {entry}");
        } else {
            let _ = writeln!(self.out, "    movl ${stack_n}, %ecx");
            self.emit("jmp hsc_apply");
        }
    }

    /// Moves a worker's register-passed raw arguments into `CACHE_REGS[0..raws.len()]` using a
    /// cycle-safe parallel register transfer (breaking register cycles via `%rax`).
    fn load_reg_raw_args(&mut self, raws: &[&Atom], p: Pos) {
        if raws.is_empty() {
            return;
        }
        enum NonReg {
            Mem(i64),
            Imm(i64),
        }
        let mut reg_moves: Vec<(usize, &'static str)> = Vec::new();
        let mut non_reg: Vec<(usize, NonReg)> = Vec::new();
        for (i, a) in raws.iter().enumerate() {
            match a {
                Atom::Slot(k) if k & RAW != 0 => match self.cached(*k) {
                    Some(src_reg) => {
                        if src_reg != CACHE_REGS[i] {
                            reg_moves.push((i, src_reg));
                        }
                    }
                    None => non_reg.push((i, NonReg::Mem(p.slot_off(*k)))),
                },
                Atom::Lit(SLit::Int(v)) => non_reg.push((i, NonReg::Imm(*v))),
                Atom::Lit(SLit::Char(c)) => non_reg.push((i, NonReg::Imm(crate::syntax::lexer::char_code(*c) as i64))),
                other => panic!("internal: worker raw argument {other:?} is not raw"),
            }
        }
        while !reg_moves.is_empty() {
            if let Some(pos) = reg_moves
                .iter()
                .position(|&(dst_idx, _)| !reg_moves.iter().any(|&(_, src)| src == CACHE_REGS[dst_idx]))
            {
                let (dst_idx, src) = reg_moves.remove(pos);
                let _ = writeln!(self.out, "    movq {src}, {}", CACHE_REGS[dst_idx]);
            } else {
                // Pure cycle(s): save one source in %rax to break the cycle.
                let cycle_src = reg_moves[0].1;
                let _ = writeln!(self.out, "    movq {cycle_src}, %rax");
                for &mut (_, ref mut src) in &mut reg_moves {
                    if *src == cycle_src {
                        *src = "%rax";
                    }
                }
            }
        }
        for (dst_idx, src) in non_reg {
            let dst = CACHE_REGS[dst_idx];
            match src {
                NonReg::Mem(off) => {
                    let _ = writeln!(self.out, "    movq {off}(%rbp), {dst}");
                }
                NonReg::Imm(0) => {
                    let l32 = low32(dst);
                    let _ = writeln!(self.out, "    xorl {l32}, {l32}");
                }
                NonReg::Imm(v) if i32::try_from(v).is_ok() => {
                    let _ = writeln!(self.out, "    movq ${v}, {dst}");
                }
                NonReg::Imm(v) => {
                    let _ = writeln!(self.out, "    movabsq ${v}, {dst}");
                }
            }
        }
    }

    /// Whether a tail call's stack arguments can be written straight over the top `n` words of the
    /// frame: no argument (stack or uncached register-raw) is read from one of those words, other
    /// than its own destination, and none is a literal the runtime builds.
    fn in_place_args(&self, stack_args: &[(&Atom, bool)], reg_raw_args: &[&Atom], p: Pos) -> bool {
        let n = stack_args.len() as i64;
        let s = p.f.s as i64;
        if n > s {
            return false;
        }
        let range = 8 * (s - n)..8 * s;
        let stack_ok = stack_args.iter().enumerate().all(|(i, (a, _))| {
            let own = 8 * (s - n + i as i64);
            match a {
                Atom::Slot(k) if k & RAW != 0 && self.cached(*k).is_some() => true,
                Atom::Slot(k) => {
                    let off = p.f.off(*k);
                    off == own || !range.contains(&off)
                }
                Atom::Free(_) => !range.contains(&p.f.off(0)),
                Atom::Global(_) => true,
                Atom::Lit(l) => {
                    matches!(l, SLit::Int(_) | SLit::Char(_) | SLit::Double(_))
                        || matches!(l, SLit::Integer(s) if s.parse::<i64>().is_ok())
                }
            }
        });
        let raw_ok = reg_raw_args.iter().all(|a| match a {
            Atom::Slot(k) if k & RAW != 0 && self.cached(*k).is_none() => !range.contains(&p.f.off(*k)),
            _ => true,
        });
        stack_ok && raw_ok
    }

    /// Whether an expression's result is a raw number: an exact call of a worker returning raw,
    /// under the lets and argument evaluations that lowering wraps it in.
    fn returns_raw(&self, e: &SExpr) -> bool {
        match e {
            SExpr::CallWorker(g, _) => matches!(self.workers.get(*g as usize), Some(Some((_, _, true, _)))),
            // The thunks of the call's arguments, and the evaluation of its raw arguments.
            SExpr::Let(_, body) => self.returns_raw(body),
            SExpr::Case(_, k, alts) if k & RAW != 0 => matches!(&alts[..], [SAlt::Default(b)] if self.returns_raw(b)),
            _ => false,
        }
    }

    /// `readMutVar#` and `writeMutVar#` on an evaluated `MutVar#` (a REF cell), inline: the read
    /// answers the state pair `(,) [state, contents]`, the write stores the contents and answers
    /// `()`. Otherwise control goes to `slow`.
    fn mutvar_fast_path(&mut self, name: &str, args: &[Atom], p: Pos, slow: &str) -> bool {
        let read = match (name, args.len()) {
            ("readMutVar#", 2) => true,
            ("writeMutVar#", 3) => false,
            _ => return false,
        };
        if args.iter().any(is_raw) || matches!(args[0], Atom::Lit(_)) {
            return false;
        }
        self.load(&args[0], p, "%rax");
        self.emit(&format!("cmpb ${REF}, (%rax)"));
        self.emit(&format!("jne {slow}"));
        if read {
            let pair = self.con_id("Builtin.(,)");
            self.alloc(3, p);
            self.emit(&format!("movabsq ${}, %rcx", header(CON, 2, pair as u64)));
            self.emit("movq %rcx, (%rax)");
            self.emit("movq $0, 8(%rax)");
            // The variable is loaded after the allocation, which may have moved it.
            self.load(&args[0], p, "%rcx");
            self.emit("movq 8(%rcx), %rcx");
            self.emit("movq %rcx, 16(%rax)");
            self.emit("movq %rax, %r14");
        } else {
            self.load(&args[1], p, "%rcx");
            self.emit("movq %rcx, 8(%rax)");
            let unit = self.con_id("Builtin.()");
            let cell = self.nullary_con(unit);
            self.emit(&format!("leaq {cell}(%rip), %r14"));
        }
        self.fast_ret(p);
        true
    }

    /// Inline small-`Integer` (`IS`) fast path for hot `Integer` arithmetic, comparisons, and
    /// `Int#` conversions: validates `IS` + `INT` tags inline and falls back to `slow` on big
    /// integers, unforced thunks, or signed 64-bit overflow.
    fn integer_fast_path(&mut self, name: &str, args: &[Atom], p: Pos, slow: &str) -> bool {
        let dest = self.raw_dest;
        if name == "intToInteger" {
            if args.len() != 1 || dest.is_some() {
                return false;
            }
            match &args[0] {
                Atom::Lit(SLit::Int(v)) => {
                    self.literal(&SLit::Integer(v.to_string()), p, "%r14");
                    self.fast_ret(p);
                    return true;
                }
                Atom::Lit(_) => return false,
                Atom::Slot(k) if *k & RAW != 0 => {
                    let is_id = self.con_id("GHC.Num.Integer.IS");
                    let hdr_is = header(CON, 1, is_id as u64);
                    self.alloc(4, p);
                    let _ = writeln!(self.out, "    movq ${}, (%rax)", header(INT, 1, 0));
                    let _ = writeln!(self.out, "    movq {}, %rcx", p.slot(*k));
                    self.emit("movq %rcx, 8(%rax)");
                    let _ = writeln!(self.out, "    movabsq ${hdr_is}, %rcx");
                    self.emit("movq %rcx, 16(%rax)");
                    self.emit("movq %rax, 24(%rax)");
                    self.emit("leaq 16(%rax), %r14");
                    self.fast_ret(p);
                    return true;
                }
                _ => {
                    let is_id = self.con_id("GHC.Num.Integer.IS");
                    let hdr_is = header(CON, 1, is_id as u64);
                    self.load(&args[0], p, "%rax");
                    let _ = writeln!(self.out, "    cmpb ${INT}, (%rax)");
                    let _ = writeln!(self.out, "    jne {slow}");
                    self.alloc(2, p);
                    let _ = writeln!(self.out, "    movabsq ${hdr_is}, %rcx");
                    self.emit("movq %rcx, (%rax)");
                    self.load(&args[0], p, "%rdx");
                    self.emit("movq %rdx, 8(%rax)");
                    self.emit("movq %rax, %r14");
                    self.fast_ret(p);
                    return true;
                }
            }
        }

        enum IOp {
            Bin(&'static str),
            Neg,
            Cmp(&'static str),
            ToInt,
        }
        let op = match name {
            "integerAdd" => IOp::Bin("addq"),
            "integerSub" => IOp::Bin("subq"),
            "integerMul" => IOp::Bin("imulq"),
            "integerNegate" => IOp::Neg,
            "integerEq" => IOp::Cmp("cmoveq"),
            "integerLt" => IOp::Cmp("cmovlq"),
            "integerLe" => IOp::Cmp("cmovleq"),
            "integerToInt" => IOp::ToInt,
            _ => return false,
        };
        let arity = match op {
            IOp::Neg | IOp::ToInt => 1,
            IOp::Bin(_) | IOp::Cmp(_) => 2,
        };
        if args.len() != arity || (dest.is_some() && !matches!(op, IOp::ToInt)) {
            return false;
        }
        let small_lit = |a: &Atom| match a {
            Atom::Lit(SLit::Integer(s)) => s.parse::<i64>().ok(),
            _ => None,
        };
        if args.iter().any(|a| is_raw(a) || (matches!(a, Atom::Lit(_)) && small_lit(a).is_none())) {
            return false;
        }
        let is_id = self.con_id("GHC.Num.Integer.IS");
        let hdr_is = header(CON, 1, is_id as u64);

        // Validate every non-literal operand is an `IS` cell holding an evaluated `INT` cell.
        let mut need_hdr = true;
        for a in args {
            if small_lit(a).is_some() {
                continue;
            }
            if need_hdr {
                let _ = writeln!(self.out, "    movabsq ${hdr_is}, %rcx");
                need_hdr = false;
            }
            self.load(a, p, "%rax");
            self.emit("cmpq %rcx, (%rax)");
            let _ = writeln!(self.out, "    jne {slow}");
            self.emit("movq 8(%rax), %rax");
            let _ = writeln!(self.out, "    cmpb ${INT}, (%rax)");
            let _ = writeln!(self.out, "    jne {slow}");
        }

        if matches!(op, IOp::ToInt) {
            match small_lit(&args[0]) {
                Some(v) => match dest {
                    Some(d) => {
                        if i32::try_from(v).is_ok() {
                            let _ = writeln!(self.out, "    movq ${v}, {}", p.slot(d));
                        } else {
                            let _ = writeln!(self.out, "    movabsq ${}, %rcx", v as u64);
                            let _ = writeln!(self.out, "    movq %rcx, {}", p.slot(d));
                        }
                    }
                    None => {
                        let lbl = self.static_cell(INT, v as u64);
                        let _ = writeln!(self.out, "    leaq {lbl}(%rip), %r14");
                    }
                },
                None => match dest {
                    // `%rax` already holds the validated `INT` cell pointer from the check above.
                    Some(d) => {
                        self.emit("movq 8(%rax), %rcx");
                        let _ = writeln!(self.out, "    movq %rcx, {}", p.slot(d));
                    }
                    None => self.emit("movq %rax, %r14"),
                },
            }
            self.fast_ret(p);
            return true;
        }

        let load_val = |g: &mut Gen, a: &Atom, reg: &str| match small_lit(a) {
            Some(v) if i32::try_from(v).is_ok() => {
                let _ = writeln!(g.out, "    movq ${v}, {reg}");
            }
            Some(v) => {
                let _ = writeln!(g.out, "    movabsq ${}, {reg}", v as u64);
            }
            None => {
                g.load(a, p, reg);
                let _ = writeln!(g.out, "    movq 8({reg}), {reg}");
                let _ = writeln!(g.out, "    movq 8({reg}), {reg}");
            }
        };

        if let IOp::Cmp(cmov) = op {
            load_val(self, &args[0], "%rcx");
            match small_lit(&args[1]) {
                Some(v) if i32::try_from(v).is_ok() => {
                    let _ = writeln!(self.out, "    cmpq ${v}, %rcx");
                }
                _ => {
                    load_val(self, &args[1], "%rdx");
                    self.emit("cmpq %rdx, %rcx");
                }
            }
            let t = self.con_id("GHC.Types.True");
            let f = self.con_id("GHC.Types.False");
            let (t, f) = (self.nullary_con(t), self.nullary_con(f));
            let _ = writeln!(self.out, "    leaq {f}(%rip), %r14");
            let _ = writeln!(self.out, "    leaq {t}(%rip), %rax");
            let _ = writeln!(self.out, "    {cmov} %rax, %r14");
            self.fast_ret(p);
            return true;
        }

        // Arithmetic (`Add`, `Sub`, `Mul`, `Neg`): allocate 4 words (INT + IS) before reloading
        // operands so any GC moves them safely; on signed overflow roll back `%r12` and fall back.
        self.alloc(4, p);
        self.emit("movq %rax, %r8");
        load_val(self, &args[0], "%rcx");
        match op {
            IOp::Bin(ins @ ("addq" | "subq")) => match small_lit(&args[1]) {
                Some(v) if i32::try_from(v).is_ok() => {
                    let _ = writeln!(self.out, "    {ins} ${v}, %rcx");
                }
                _ => {
                    load_val(self, &args[1], "%rdx");
                    let _ = writeln!(self.out, "    {ins} %rdx, %rcx");
                }
            },
            IOp::Bin(ins) => {
                load_val(self, &args[1], "%rdx");
                let _ = writeln!(self.out, "    {ins} %rdx, %rcx");
            }
            IOp::Neg => self.emit("negq %rcx"),
            IOp::Cmp(_) | IOp::ToInt => unreachable!(),
        }
        let ok = self.label("iok");
        let _ = writeln!(self.out, "    jno {ok}");
        self.emit("subq $32, %r12");
        let _ = writeln!(self.out, "    jmp {slow}");
        self.put_label(&ok);
        let _ = writeln!(self.out, "    movq ${}, (%r8)", header(INT, 1, 0));
        self.emit("movq %rcx, 8(%r8)");
        let _ = writeln!(self.out, "    movabsq ${hdr_is}, %rcx");
        self.emit("movq %rcx, 16(%r8)");
        self.emit("movq %r8, 24(%r8)");
        self.emit("leaq 16(%r8), %r14");
        self.fast_ret(p);
        true
    }

    /// The number an operand of an inline primitive holds: a raw slot's, or a boxed cell's.
    fn operand(&mut self, a: &Atom, p: Pos, reg: &str) {
        match a {
            Atom::Slot(k) if k & RAW != 0 => {
                self.emit(&format!("movq {}, {reg}", p.slot(*k)));
                return;
            }
            Atom::Lit(SLit::Int(v)) => {
                self.emit(&format!("movabsq ${v}, {reg}"));
                return;
            }
            Atom::Lit(SLit::Char(c)) => {
                self.emit(&format!("movq ${}, {reg}", crate::syntax::lexer::char_code(*c)));
                return;
            }
            _ => {}
        }
        self.load(a, p, reg);
        self.emit(&format!("movq 8({reg}), {reg}"));
    }

    /// The slots used after a case's scrutinee: by its alternatives and, for a case in a
    /// scrutinee, by what follows the enclosing one.
    fn case_live(&self, alts: &[SAlt], p: Pos) -> std::collections::HashSet<u32> {
        let mut live = std::collections::HashSet::new();
        for a in alts {
            match a {
                SAlt::Con(_, _, b) | SAlt::Lit(_, b) | SAlt::Default(b) => self.slot_uses(b, &mut live),
            }
        }
        if p.mode == Mode::Scrut {
            if let Some(outer) = self.live.last() {
                live.extend(outer.iter().copied());
            }
        }
        live
    }

    /// In a scrutinee (the frame stays while the callee runs), clears the frame slots among
    /// `atoms` that nothing uses after it returns, so that the frame does not keep what the
    /// callee no longer needs (a `forM_` chain of actions, say) alive.
    fn clear_dead(&mut self, atoms: &[Atom], p: Pos) {
        if p.mode != Mode::Scrut {
            return;
        }
        let Some(live) = self.live.last() else { return };
        if live.contains(&ALL_LIVE) {
            return;
        }
        let mut dead: Vec<u32> = atoms
            .iter()
            .filter_map(|a| match a {
                Atom::Slot(k) if *k != 0 && k & RAW == 0 && !live.contains(k) => Some(*k),
                _ => None,
            })
            .collect();
        dead.sort_unstable();
        dead.dedup();
        for k in dead {
            self.emit(&format!("movq $0, {}", p.slot(k)));
        }
    }

    /// The frame slots an expression uses (including the code of the join points it jumps to).
    fn slot_uses(&self, e: &SExpr, out: &mut std::collections::HashSet<u32>) {
        self.slot_uses_in(e, out, &mut HashMap::new());
    }

    /// `slot_uses`, with the uses of the join points defined inside the expression in `local`.
    fn slot_uses_in(
        &self,
        e: &SExpr,
        out: &mut std::collections::HashSet<u32>,
        local: &mut HashMap<u32, std::collections::HashSet<u32>>,
    ) {
        let atom = |a: &Atom, out: &mut std::collections::HashSet<u32>| {
            match a {
                Atom::Slot(k) => {
                    out.insert(*k);
                }
                // A captured variable is read through the closure in slot 0.
                Atom::Free(_) => {
                    out.insert(0);
                }
                _ => {}
            }
        };
        match e {
            SExpr::Atom(a) => atom(a, out),
            SExpr::App(f, args) => {
                atom(f, out);
                args.iter().for_each(|a| atom(a, out));
            }
            SExpr::CallWorker(_, args) => args.iter().for_each(|a| atom(a, out)),
            SExpr::Con(_, args) | SExpr::Prim(_, args) => args.iter().for_each(|a| atom(a, out)),
            SExpr::Let(binds, body) => {
                for (_, b) in binds {
                    b.atoms().iter().for_each(|a| atom(a, out));
                }
                self.slot_uses_in(body, out, local);
            }
            SExpr::Case(s, _, alts) => {
                self.slot_uses_in(s, out, local);
                for a in alts {
                    match a {
                        SAlt::Con(_, _, b) | SAlt::Lit(_, b) | SAlt::Default(b) => self.slot_uses_in(b, out, local),
                    }
                }
            }
            SExpr::Closure(c) => c.captures.iter().for_each(|a| atom(a, out)),
            SExpr::Fail(_) => {}
            SExpr::Join(id, _, code, body) => {
                // The code first: jumps in the body (and code) to this join point use its slots.
                let mut uses = std::collections::HashSet::new();
                self.slot_uses_in(code, &mut uses, local);
                out.extend(uses.iter().copied());
                local.insert(*id, uses);
                self.slot_uses_in(body, out, local);
            }
            SExpr::Jump(id, args) => {
                args.iter().for_each(|a| atom(a, out));
                match local.get(id).or_else(|| self.join_uses.get(id)) {
                    Some(u) => out.extend(u.iter().copied()),
                    // Not known (join points are lowered before their uses, so this should not
                    // happen): everything counts as live.
                    None => {
                        out.insert(ALL_LIVE);
                    }
                }
            }
        }
    }

    /// The end of an inline primitive: its value (in R1) is returned, or, for a case scrutinee
    /// computed in place, goes straight to the alternatives.
    fn fast_ret(&mut self, p: Pos) {
        match self.fast_done.clone() {
            Some(l) => self.emit(&format!("jmp {l}")),
            None => self.ret(p),
        }
    }

    /// The static INT cells 0 and 1, shared by the inline comparisons.
    fn bool_cells(&mut self) -> (String, String) {
        if let Some(c) = &self.bool_cells {
            return c.clone();
        }
        let zero = self.static_cell(INT, 0);
        let one = self.static_cell(INT, 1);
        self.bool_cells = Some((zero.clone(), one.clone()));
        (zero, one)
    }

    /// Loads the frame's GC size word (`mach::TOP`) into `%rax` for an out-of-line runtime stub.
    fn emit_top(&mut self, p: Pos) {
        let top = if p.pushed == 0 { p.f.size_word() } else { 0 };
        if top == 0 {
            self.emit("xorl %eax, %eax");
        } else if top <= u32::MAX as u64 {
            let _ = writeln!(self.out, "    movl ${top}, %eax");
        } else {
            let _ = writeln!(self.out, "    movabsq ${top}, %rax");
        }
    }

    /// Calls an out-of-line runtime stub (`{name}_rt`) that saves HP/SP/TOP, aligns `%rsp`,
    /// calls `{name}` with `%rdi = %rbx`, and reloads HP/HPLIM.
    fn rt_call(&mut self, name: &str, p: Pos, nargs: usize) {
        let _ = nargs;
        self.emit_top(p);
        let _ = writeln!(self.out, "    call {name}_rt");
    }
}

/// A literal pattern's kind and text, as the runtime reads them.
/// Global indices whose worker is referenced by at least one `SExpr::CallWorker`.
fn collect_called_workers(prog: &SProgram) -> std::collections::HashSet<u32> {
    fn walk(e: &SExpr, out: &mut std::collections::HashSet<u32>) {
        match e {
            SExpr::CallWorker(g, _) => {
                out.insert(*g);
            }
            SExpr::Atom(_) | SExpr::App(..) | SExpr::Con(..) | SExpr::Prim(..) | SExpr::Fail(_) | SExpr::Jump(..) => {}
            SExpr::Let(binds, body) => {
                for (_, b) in binds {
                    if let SBind::Closure(c) = b {
                        walk(&c.body, out);
                    }
                }
                walk(body, out);
            }
            SExpr::Case(s, _, alts) => {
                walk(s, out);
                for a in alts {
                    match a {
                        SAlt::Con(_, _, b) | SAlt::Lit(_, b) | SAlt::Default(b) => walk(b, out),
                    }
                }
            }
            SExpr::Closure(c) => walk(&c.body, out),
            SExpr::Join(_, _, code, body) => {
                walk(code, out);
                walk(body, out);
            }
        }
    }
    let mut out = std::collections::HashSet::new();
    for gl in &prog.globals {
        walk(&gl.closure.body, &mut out);
        if let Some((c, _)) = &gl.worker {
            walk(&c.body, &mut out);
        }
    }
    out
}

fn lit_text(l: &Lit) -> (u64, String) {
    match l {
        Lit::Int(i) => (0, i.to_string()),
        Lit::Char(c) => (1, crate::syntax::lexer::char_code(*c).to_string()),
        Lit::Integer(s) => (2, s.clone()),
        Lit::Double(d) => (3, d.to_bits().to_string()),
        Lit::Frac(s) => (4, s.clone()),
        Lit::Str(s) => (5, s.clone()),
        Lit::Addr(s) => (6, s.clone()),
    }
}

fn escape_bytes(bytes: &[u8], out: &mut String) {
    for &b in bytes {
        match b {
            b'"' => out.push_str("\\\""),
            b'\\' => out.push_str("\\\\"),
            0x20..=0x7e => out.push(b as char),
            _ => {
                let _ = write!(out, "\\{:03o}", b);
            }
        }
    }
}

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    escape_bytes(s.as_bytes(), &mut out);
    out
}

/// The runtime routines of generated code (see NATIVE.md for the protocols):
/// `hsc_eval`: evaluate R1 to WHNF and return it to the continuation on top of the stack;
/// `hsc_apply`: apply R1 to %rcx arguments on top of the stack, the continuation above them;
/// the update, apply and catch return addresses; the raise path; and the entry and exit with the
/// runtime. The call to the runtime's `hsc_apply_slow` returns an action in %eax:
/// 1 enter (m.code, with the stack and R1 set), 2 return R1, 3 apply again (m.nargs), 4 raise.
const RUNTIME_ASM: &str = r#"
    .globl hsc_enter
    .globl hsc_enter_eval
    .globl hsc_stop_ret
    .globl hsc_apply_ret
    .globl hsc_eval
    .globl hsc_apply
    .globl hsc_raise_pending
    .globl hsc_mask_ret
hsc_enter_eval:
    pushq %rbx
    pushq %rbp
    pushq %r12
    pushq %r13
    pushq %r14
    pushq %r15
    movq %rdi, %rbx
    movq %rsi, %r14
    movq 16(%rbx), %rbp
    movq 0(%rbx), %r12
    movq 8(%rbx), %r13
    jmp hsc_eval
hsc_enter:
    pushq %rbx
    pushq %rbp
    pushq %r12
    pushq %r13
    pushq %r14
    pushq %r15
    movq %rdi, %rbx
    movq %rsi, %r14
    movq 16(%rbx), %rbp
    movq 0(%rbx), %r12
    movq 8(%rbx), %r13
    movq 48(%rbx), %rcx
    jmp hsc_apply
hsc_leave_ok:
    xorl %eax, %eax
    jmp hsc_leave
hsc_leave_exc:
    movl $1, %eax
hsc_leave:
    movq %r12, 0(%rbx)
    movq %r13, 8(%rbx)
    popq %r15
    popq %r14
    popq %r13
    popq %r12
    popq %rbp
    popq %rbx
    ret

hsc_eval:
    testq %r14, %r14
    je .Leval_void
    movq (%r14), %rax
    cmpb $8, %al
    jb .Leval_void
    je .Leval_thunk
    cmpb $9, %al
    je .Leval_ind
.Leval_void:
    jmp *(%rbp)
.Leval_ind:
    movq 8(%r14), %r14
    jmp hsc_eval
.Leval_thunk:
    movq %rax, %rdx
    shrq $32, %rdx
    testq %rdx, %rdx
    jnz .Leval_loop
    movq 192(%rbx), %rdx
    shlq $32, %rdx
    orq %rdx, %rax
    movq %rax, (%r14)
    subq $16, %rbp
    leaq hsc_upd_ret(%rip), %rcx
    movq %rcx, (%rbp)
    movq %r14, 8(%rbp)
    movq 8(%r14), %rcx
    jmp *%rcx
.Leval_loop:
    movq %r14, 40(%rbx)
    movq %r12, 0(%rbx)
    movq %rbp, 16(%rbx)
    movq $0, 24(%rbx)
    movq %rbx, %rdi
    movq %rsp, %r15
    andq $-16, %rsp
    call hsc_blackhole
    movq %r15, %rsp
    movq 0(%rbx), %r12
    movq 8(%rbx), %r13
    movq 16(%rbx), %rbp
    movq 40(%rbx), %r14
    testl %eax, %eax
    jz hsc_eval
    jmp hsc_raise_pending

    .p2align 4
    .quad 2, 1
hsc_upd_ret:
    movq 8(%rbp), %rcx
    movq (%rcx), %rdx
    movabsq $0x00000000ffffff00, %rax
    andq %rax, %rdx
    orq $9, %rdx
    movq %rdx, (%rcx)
    movq %r14, 8(%rcx)
    addq $16, %rbp
    jmp *(%rbp)

hsc_apply:
    movq (%r14), %rax
    movzbl %al, %edx
    cmpb $9, %dl
    je .Lap_ind
    cmpb $8, %dl
    je .Lap_eval
    cmpb $7, %dl
    je .Lap_pap
    cmpb $6, %dl
    jne .Lap_slow
    movq %rax, %rdx
    shrq $32, %rdx
    cmpq %rcx, %rdx
    jne .Lap_arity
    movq 8(%r14), %rdx
    jmp *%rdx
.Lap_arity:
    ja .Lap_slow
    # Over-application (arity rdx < rcx): the first rdx arguments move down two words, and an
    # apply frame [hsc_apply_ret][count] holds the rest, which stay where they are.
    subq $16, %rbp
    xorl %r8d, %r8d
.Lap_over_copy:
    cmpq %rdx, %r8
    jae .Lap_over_done
    movq 16(%rbp,%r8,8), %r9
    movq %r9, (%rbp,%r8,8)
    incq %r8
    jmp .Lap_over_copy
.Lap_over_done:
    leaq hsc_apply_ret(%rip), %r9
    movq %r9, (%rbp,%rdx,8)
    subq %rdx, %rcx
    movq %rcx, 8(%rbp,%rdx,8)
    movq 8(%r14), %rdx
    jmp *%rdx
.Lap_pap:
    # A partial application: its held arguments go below the new ones, and its function is
    # applied to all of them.
    movq %rax, %rdx
    shrq $32, %rdx
    leaq (,%rdx,8), %r8
    subq %r8, %rbp
    xorl %r8d, %r8d
.Lap_pap_copy:
    cmpq %rdx, %r8
    jae .Lap_pap_done
    movq 16(%r14,%r8,8), %r9
    movq %r9, (%rbp,%r8,8)
    incq %r8
    jmp .Lap_pap_copy
.Lap_pap_done:
    addq %rdx, %rcx
    movq 8(%r14), %r14
    jmp hsc_apply
.Lap_ind:
    movq 8(%r14), %r14
    jmp hsc_apply
.Lap_eval:
    subq $16, %rbp
    leaq hsc_apply_ret(%rip), %rdx
    movq %rdx, (%rbp)
    movq %rcx, 8(%rbp)
    jmp hsc_eval
.Lap_slow:
    movq %r12, 0(%rbx)
    movq %rbp, 16(%rbx)
    movq %r14, 40(%rbx)
    movq %rcx, 48(%rbx)
    movq %rcx, 24(%rbx)
    movq %rbx, %rdi
    movq %rsp, %r15
    andq $-16, %rsp
    call hsc_apply_slow
    movq %r15, %rsp
    movq 0(%rbx), %r12
    movq 8(%rbx), %r13
    movq 16(%rbx), %rbp
    movq 40(%rbx), %r14
    cmpl $1, %eax
    je .Lap_enter
    cmpl $2, %eax
    je .Lap_return
    cmpl $3, %eax
    je .Lap_again
    jmp hsc_raise_pending
.Lap_enter:
    movq 56(%rbx), %rax
    jmp *%rax
.Lap_return:
    jmp *(%rbp)
.Lap_again:
    movq 48(%rbx), %rcx
    jmp hsc_apply

    .p2align 4
    .quad 4, 0
hsc_apply_ret:
    movq 8(%rbp), %rcx
    addq $16, %rbp
    jmp hsc_apply

    .p2align 4
    .quad 3, 1
hsc_catch_ret:
    addq $16, %rbp
    jmp *(%rbp)

    .p2align 4
    .quad 6, 1
hsc_mask_ret:
    movq %r14, 40(%rbx)
    movq %r12, 0(%rbx)
    movq %rbp, 16(%rbx)
    movq $0, 24(%rbx)
    movq %rbx, %rdi
    movq %rsp, %r15
    andq $-16, %rsp
    call hsc_mask_restore
    movq %r15, %rsp
    movq 0(%rbx), %r12
    movq 8(%rbx), %r13
    movq 16(%rbx), %rbp
    movq 40(%rbx), %r14
    testl %eax, %eax
    jnz hsc_raise_pending
    jmp *(%rbp)

    .p2align 4
    .quad 5, 2
hsc_stop_ret:
    movq %r14, 40(%rbx)
    jmp hsc_leave_ok

hsc_raise_pending:
    movq %r12, 0(%rbx)
    movq %rbp, 16(%rbx)
    movq %rbx, %rdi
    movq %rsp, %r15
    andq $-16, %rsp
    call hsc_unwind
    movq %r15, %rsp
    movq 0(%rbx), %r12
    movq 8(%rbx), %r13
    movq 16(%rbx), %rbp
    movq 40(%rbx), %r14
    cmpl $1, %eax
    je .Lraise_caught
    jmp hsc_leave_exc
.Lraise_caught:
    movq 48(%rbx), %rcx
    jmp hsc_apply

.Lrt_raise:
    addq $8, %rsp
    jmp hsc_raise_pending

hsc_gc_rt:
    movq %r12, 0(%rbx)
    movq %rax, 24(%rbx)
    movq %rbp, 16(%rbx)
    movq %rbx, %rdi
    movq %rsp, %r15
    andq $-16, %rsp
    call hsc_gc
    movq %r15, %rsp
    movq 0(%rbx), %r12
    movq 8(%rbx), %r13
    ret

hsc_yield_rt:
    movq %r12, 0(%rbx)
    movq %rax, 24(%rbx)
    movq %rbp, 16(%rbx)
    movq %rbx, %rdi
    movq %rsp, %r15
    andq $-16, %rsp
    call hsc_yield
    movq %r15, %rsp
    movq 0(%rbx), %r12
    movq 8(%rbx), %r13
    testq %rax, %rax
    jnz .Lrt_raise
    ret

hsc_prim_rt:
    movq %r12, 0(%rbx)
    movq %rax, 24(%rbx)
    movq %rbp, 16(%rbx)
    movq %rbx, %rdi
    movq %rsp, %r15
    andq $-16, %rsp
    call hsc_prim
    movq %r15, %rsp
    movq 0(%rbx), %r12
    movq 8(%rbx), %r13
    testq %rax, %rax
    jz .Lrt_raise
    movq %rax, %r14
    ret

hsc_fail_rt:
    movq %r12, 0(%rbx)
    movq %rax, 24(%rbx)
    movq %rbp, 16(%rbx)
    movq %rbx, %rdi
    movq %rsp, %r15
    andq $-16, %rsp
    call hsc_fail
    movq %r15, %rsp
    movq 0(%rbx), %r12
    movq 8(%rbx), %r13
    jmp hsc_raise_pending

hsc_lit_matches_rt:
    movq %r12, 0(%rbx)
    movq %rax, 24(%rbx)
    movq %rbp, 16(%rbx)
    movq %rbx, %rdi
    movq %rsp, %r15
    andq $-16, %rsp
    call hsc_lit_matches
    movq %r15, %rsp
    movq 0(%rbx), %r12
    movq 8(%rbx), %r13
    ret

hsc_mk_literal_rt:
    movq %r12, 0(%rbx)
    movq %rax, 24(%rbx)
    movq %rbp, 16(%rbx)
    movq %rbx, %rdi
    movq %rsp, %r15
    andq $-16, %rsp
    call hsc_mk_literal
    movq %r15, %rsp
    movq 0(%rbx), %r12
    movq 8(%rbx), %r13
    ret

"#;

#[cfg(test)]
mod nullary_constructor_tests {
    use super::*;

    fn position(mode: Mode) -> Pos {
        Pos { f: Frame { s: 1, a: 0, r: 0, has_closure: true }, mode, pushed: 0 }
    }

    #[test]
    fn empty_constructors_share_static_cells_and_preserve_tags_in_both_modes() {
        let mut gen = Gen::new();
        gen.expr(&SExpr::Con("Test.A".into(), Vec::new()), position(Mode::Tail));
        gen.expr(&SExpr::Con("Test.B".into(), Vec::new()), position(Mode::Scrut));
        gen.expr(&SExpr::Con("Test.A".into(), Vec::new()), position(Mode::Scrut));
        assert_eq!(gen.statics.len(), 2);
        for name in ["Test.A", "Test.B"] {
            let id = gen.con_ids[name];
            let label = &gen.nullary_cons[&id];
            let cell = gen.statics.iter().find(|(l, _, _)| l == label).unwrap();
            assert_eq!((cell.1, cell.2.as_str()), (header(CON, 0, id as u64), "0"));
            assert!(gen.out.contains(&format!("leaq {label}(%rip), %r14")));
        }
        assert!(!gen.out.contains("hsc_gc"));
        assert!(!gen.out.contains("%r12"));
    }

    #[test]
    fn constructors_with_fields_keep_heap_allocation() {
        let mut gen = Gen::new();
        gen.expr(&SExpr::Con("Test.Box".into(), vec![Atom::Lit(SLit::Int(42))]), position(Mode::Tail));
        assert!(gen.nullary_cons.is_empty());
        assert!(gen.out.contains("hsc_gc"));
        assert!(gen.out.contains("movq %rcx, 8(%rax)"));
    }
}

#[cfg(test)]
mod known_global_call_tests {
    use super::*;
    use crate::native::stg::SGlobal;

    fn position(mode: Mode) -> Pos {
        Pos { f: Frame { s: 3, a: 2, r: 0, has_closure: true }, mode, pushed: if mode == Mode::Scrut { 1 } else { 0 } }
    }

    #[test]
    fn exact_global_calls_bypass_apply_in_both_return_modes() {
        for mode in [Mode::Tail, Mode::Scrut] {
            let mut gen = Gen::new();
            gen.known_globals.push((2, ".Lknown_entry".into(), false));
            gen.expr(&SExpr::App(Atom::Global(0), vec![Atom::Slot(1), Atom::Slot(2)]), position(mode));
            assert!(gen.out.ends_with("    jmp .Lknown_entry\n"));
            assert!(!gen.out.contains("jmp hsc_apply"));
            assert!(!gen.out.contains("%r14"));
        }
    }

    #[test]
    fn partial_overapplied_caf_and_local_calls_keep_generic_dispatch() {
        for (func, arity, nargs) in [
            (Atom::Global(0), 2, 1), (Atom::Global(0), 2, 3),
            (Atom::Global(0), 0, 1), (Atom::Slot(0), 2, 2), (Atom::Free(0), 2, 2),
        ] {
            let mut gen = Gen::new();
            gen.known_globals.push((arity, ".Lknown_entry".into(), true));
            let args = vec![Atom::Lit(SLit::Int(7)); nargs];
            gen.expr(&SExpr::App(func, args), position(Mode::Tail));
            assert!(gen.out.ends_with("    jmp hsc_apply\n"));
        }
    }

    #[test]
    fn forward_and_mutual_entries_are_registered_before_body_generation() {
        let globals = vec![
            SGlobal { name: "Main.main".into(), closure: SClosure { arity: 1, captures: Vec::new(), nslots: 2, nraw: 0, raw_args: Vec::new(), ret_raw: false, uses_closure: false, body: SExpr::App(Atom::Global(1), vec![Atom::Slot(1)]) }, worker: None },
            SGlobal { name: "Test.other".into(), closure: SClosure { arity: 1, captures: Vec::new(), nslots: 2, nraw: 0, raw_args: Vec::new(), ret_raw: false, uses_closure: false, body: SExpr::App(Atom::Global(0), vec![Atom::Slot(1)]) }, worker: None },
        ];
        let index = [("Main.main".to_string(), 0), ("Test.other".to_string(), 1)].into_iter().collect();
        let asm = Gen::program(&SProgram { globals, index });
        let table = asm.split_once("hsc_globals:\n").unwrap().1;
        for row in table.lines().take(2) {
            let label = row.rsplit_once(',').unwrap().1.split_whitespace().next().unwrap();
            assert!(asm.contains(&format!("jmp {label}\n")));
        }
    }

    #[test]
    fn worker_register_swap_breaks_cycle_and_loops_in_place() {
        // Worker takes 2 raw args in %rsi, %rdi and tail-calls itself swapping them.
        let r0 = RAW | 0;
        let r1 = RAW | 1;
        let worker_closure = SClosure {
            arity: 0,
            captures: Vec::new(),
            nslots: 1,
            nraw: 2,
            raw_args: vec![(0, r0), (0, r1)],
            ret_raw: true,
            uses_closure: false,
            body: SExpr::CallWorker(0, vec![Atom::Slot(r1), Atom::Slot(r0)]),
        };
        let globals = vec![SGlobal {
            name: "Test.swap".into(),
            closure: SClosure {
                arity: 2,
                captures: Vec::new(),
                nslots: 3,
                nraw: 0,
                raw_args: Vec::new(),
                ret_raw: false,
                uses_closure: false,
                body: SExpr::Atom(Atom::Slot(1)),
            },
            worker: Some((worker_closure, vec![true, true])),
        }];
        let index = [("Test.swap".to_string(), 0)].into_iter().collect();
        let asm = Gen::program(&SProgram { globals, index });
        // Swaps %rsi and %rdi via %rax and jumps directly to .Lloop without stack ops.
        assert!(asm.contains("movq %rdi, %rax\n    movq %rsi, %rdi\n    movq %rax, %rsi\n"), "{asm}");
        assert!(asm.contains("jmp .Lloop"), "{asm}");
    }

    #[test]
    fn unreferenced_worker_body_is_not_emitted() {
        let r0 = RAW | 0;
        let worker_closure = SClosure {
            arity: 0,
            captures: Vec::new(),
            nslots: 1,
            nraw: 1,
            raw_args: vec![(0, r0)],
            ret_raw: false,
            uses_closure: false,
            body: SExpr::Atom(Atom::Lit(SLit::Int(999))),
        };
        let globals = vec![SGlobal {
            name: "Test.unused_worker".into(),
            closure: SClosure {
                arity: 1,
                captures: Vec::new(),
                nslots: 2,
                nraw: 0,
                raw_args: Vec::new(),
                ret_raw: false,
                uses_closure: false,
                body: SExpr::Atom(Atom::Slot(1)),
            },
            worker: Some((worker_closure, vec![true])),
        }];
        let index = [("Test.unused_worker".to_string(), 0)].into_iter().collect();
        let asm = Gen::program(&SProgram { globals, index });
        assert!(!asm.contains("999"), "unreferenced worker body should not be emitted:\n{asm}");
    }

    #[test]
    fn multiway_literal_cases_use_binary_search_for_eight_or_more_alts() {
        let mut raw_alts: Vec<SAlt> = (0..10i64)
            .map(|i| SAlt::Lit(Lit::Int(i * 10), SExpr::Atom(Atom::Lit(SLit::Int(i + 100)))))
            .collect();
        raw_alts.push(SAlt::Default(SExpr::Atom(Atom::Lit(SLit::Int(0)))));
        let raw_case = SExpr::Case(Box::new(SExpr::Atom(Atom::Slot(RAW | 0))), 1, raw_alts);
        let mut g = Gen::new();
        g.expr(&raw_case, Pos { f: Frame { s: 2, a: 0, r: 1, has_closure: true }, mode: Mode::Tail, pushed: 0 });
        assert!(g.out.contains("jl .Lbs"), "raw literal case >= 8 alts should binary-search:\n{}", g.out);

        let mut boxed_alts: Vec<SAlt> = (0..10i64)
            .map(|i| SAlt::Lit(Lit::Char(char::from(b'a' + i as u8)), SExpr::Atom(Atom::Lit(SLit::Int(i)))))
            .collect();
        boxed_alts.push(SAlt::Default(SExpr::Atom(Atom::Lit(SLit::Int(-1)))));
        let boxed_case = SExpr::Case(Box::new(SExpr::Atom(Atom::Slot(1))), 2, boxed_alts);
        let mut g2 = Gen::new();
        g2.expr(&boxed_case, Pos { f: Frame { s: 3, a: 1, r: 0, has_closure: true }, mode: Mode::Tail, pushed: 0 });
        assert!(g2.out.contains("jl .Lbs"), "boxed char literal case >= 8 alts should binary-search:\n{}", g2.out);

        // Fewer than 8 alts keeps linear dispatch.
        let small_alts = vec![
            SAlt::Lit(Lit::Int(1), SExpr::Atom(Atom::Lit(SLit::Int(10)))),
            SAlt::Lit(Lit::Int(2), SExpr::Atom(Atom::Lit(SLit::Int(20)))),
            SAlt::Default(SExpr::Atom(Atom::Lit(SLit::Int(0)))),
        ];
        let small_case = SExpr::Case(Box::new(SExpr::Atom(Atom::Slot(RAW | 0))), 1, small_alts);
        let mut g3 = Gen::new();
        g3.expr(&small_case, Pos { f: Frame { s: 2, a: 0, r: 1, has_closure: true }, mode: Mode::Tail, pushed: 0 });
        assert!(!g3.out.contains("jl .Lbs"), "small literal case should stay linear:\n{}", g3.out);
    }
}

#[cfg(test)]
mod liveness_tests {
    use super::*;

    fn case_of_call(alt_uses: Vec<Atom>) -> SExpr {
        // case f@1 x@2 of _ -> Con [alt_uses]
        SExpr::Case(
            Box::new(SExpr::App(Atom::Slot(1), vec![Atom::Slot(2)])),
            3,
            vec![SAlt::Default(SExpr::Con("Test.K".into(), alt_uses))],
        )
    }

    fn gen(e: &SExpr, mode: Mode) -> String {
        let mut g = Gen::new();
        g.expr(e, Pos { f: Frame { s: 4, a: 2, r: 0, has_closure: true }, mode, pushed: 0 });
        g.out
    }

    #[test]
    fn a_scrutinee_call_clears_the_slots_nothing_uses_after_it() {
        // Neither the function nor its argument is used by the alternative: both are cleared.
        let out = gen(&case_of_call(vec![Atom::Lit(SLit::Int(1))]), Mode::Tail);
        assert_eq!(out.matches("movq $0,").count(), 2, "{out}");
        // The argument is used afterwards: only the function's slot is cleared.
        let out = gen(&case_of_call(vec![Atom::Slot(2)]), Mode::Tail);
        assert_eq!(out.matches("movq $0,").count(), 1, "{out}");
    }

    #[test]
    fn tail_calls_and_the_closure_slot_are_left_alone() {
        let out = gen(&SExpr::App(Atom::Slot(1), vec![Atom::Slot(2)]), Mode::Tail);
        assert!(!out.contains("movq $0,"), "{out}");
        let e = SExpr::Case(
            Box::new(SExpr::App(Atom::Slot(0), vec![Atom::Slot(2)])),
            3,
            vec![SAlt::Default(SExpr::Con("Test.K".into(), vec![Atom::Slot(2)]))],
        );
        assert!(!gen(&e, Mode::Tail).contains("movq $0,"));
    }

    #[test]
    fn slots_a_join_point_uses_stay_live_across_a_jump() {
        // join j = Con [x@2] in case f@1 of _ -> jump j
        let e = SExpr::Join(
            7,
            Vec::new(),
            Box::new(SExpr::Con("Test.K".into(), vec![Atom::Slot(2)])),
            Box::new(SExpr::Case(
                Box::new(SExpr::App(Atom::Slot(1), vec![Atom::Slot(2)])),
                3,
                vec![SAlt::Default(SExpr::Jump(7, Vec::new()))],
            )),
        );
        let out = gen(&e, Mode::Tail);
        assert_eq!(out.matches("movq $0,").count(), 1, "{out}");
    }
}

#[cfg(test)]
mod magic_division {
    use super::signed_magic;

    /// The quotient as the generated code computes it (see `Gen::raw_op`).
    fn quot(n: i64, d: i64) -> i64 {
        let (m, s) = signed_magic(d);
        let mut q = ((m as i128 * n as i128) >> 64) as i64;
        if d > 0 && m < 0 {
            q = q.wrapping_add(n);
        } else if d < 0 && m > 0 {
            q = q.wrapping_sub(n);
        }
        q >>= s;
        q.wrapping_add(((q as u64) >> 63) as i64)
    }

    #[test]
    fn quotients_by_constants_match_division() {
        let mut ds: Vec<i64> = (2..300).chain([1000, 1000003, 1000000007, 6700417, 1 << 20, (1 << 40) + 1, (1 << 62) - 1]).collect();
        ds.extend(ds.clone().iter().map(|d| -d));
        let mut ns: Vec<i64> = vec![0, 1, -1, i64::MAX, i64::MIN, i64::MAX - 1, i64::MIN + 1, 1 << 62, -(1 << 62)];
        let mut x: u64 = 0x9e3779b97f4a7c15;
        for _ in 0..2000 {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            ns.push(x as i64);
            ns.push((x >> (x % 60)) as i64);
            ns.push(-((x >> (x % 60)) as i64));
        }
        for &d in &ds {
            for &n in &ns {
                assert_eq!(quot(n, d), n.wrapping_div(d), "{n} quot {d}");
            }
        }
    }
}

#[cfg(test)]
mod sprint6_codegen_tests {
    use super::*;
    use crate::core::Lit;
    use crate::native::stg::{Atom, SAlt, SBind, SClosure, SExpr, SGlobal, SLit, SProgram, RAW};
    use std::collections::HashMap;

    fn simple_closure(arity: u32, nslots: u32, nraw: u32, body: SExpr) -> SClosure {
        SClosure {
            arity,
            captures: Vec::new(),
            nslots,
            nraw,
            raw_args: Vec::new(),
            body,
            ret_raw: false,
            uses_closure: false,
        }
    }

    fn make_prog(globals: Vec<SGlobal>) -> SProgram {
        let index: HashMap<String, u32> = globals
            .iter()
            .enumerate()
            .map(|(i, g)| (g.name.clone(), i as u32))
            .collect();
        SProgram { globals, index }
    }

    #[test]
    fn integer_fast_paths_emit_inline_is_ops_and_overflow_rollback() {
        let prog = make_prog(vec![
            SGlobal {
                name: "Test.add".into(),
                closure: simple_closure(
                    2,
                    3,
                    0,
                    SExpr::Prim("integerAdd".into(), vec![Atom::Slot(1), Atom::Slot(2)]),
                ),
                worker: None,
            },
            SGlobal {
                name: "Test.eq".into(),
                closure: simple_closure(
                    2,
                    3,
                    0,
                    SExpr::Prim("integerEq".into(), vec![Atom::Slot(1), Atom::Slot(2)]),
                ),
                worker: None,
            },
            SGlobal {
                name: "Test.toInt".into(),
                closure: simple_closure(
                    1,
                    2,
                    0,
                    SExpr::Prim("integerToInt".into(), vec![Atom::Slot(1)]),
                ),
                worker: None,
            },
        ]);
        let asm = Gen::program(&prog);
        assert!(asm.contains("addq %rdx, %rcx"), "expected inline addq for integerAdd:\n{asm}");
        assert!(asm.contains("jno "), "expected overflow branch for integerAdd:\n{asm}");
        assert!(asm.contains("subq $32, %r12"), "expected bump-pointer rollback on overflow:\n{asm}");
        assert!(asm.contains("cmoveq "), "expected inline comparison for integerEq:\n{asm}");
    }

    #[test]
    fn nullary_globals_propagate_through_aliases_and_static_literals() {
        // g0 = True; g1 = g0 (alias); g2 = 42# (static literal); g3 = g2 (alias); g4 = return g1; g5 = return g3
        let prog = make_prog(vec![
            SGlobal { name: "GHC.Types.True".into(), closure: simple_closure(0, 1, 0, SExpr::Con("GHC.Types.True".into(), vec![])), worker: None },
            SGlobal { name: "Test.aliasTrue".into(), closure: simple_closure(0, 1, 0, SExpr::Atom(Atom::Global(0))), worker: None },
            SGlobal { name: "Test.lit42".into(), closure: simple_closure(0, 1, 0, SExpr::Atom(Atom::Lit(SLit::Int(42)))), worker: None },
            SGlobal { name: "Test.aliasLit".into(), closure: simple_closure(0, 1, 0, SExpr::Atom(Atom::Global(2))), worker: None },
            SGlobal { name: "Test.useTrue".into(), closure: simple_closure(1, 2, 0, SExpr::Atom(Atom::Global(1))), worker: None },
            SGlobal { name: "Test.useLit".into(), closure: simple_closure(1, 2, 0, SExpr::Atom(Atom::Global(3))), worker: None },
        ]);
        let asm = Gen::program(&prog);
        let table = asm.split_once("hsc_globals:\n").unwrap().1;
        let labels: Vec<&str> = table
            .lines()
            .take(6)
            .map(|r| r.rsplit_once(',').unwrap().1.split_whitespace().next().unwrap())
            .collect();
        let g4_start = asm.find(&format!("{}:\n", labels[4])).expect("g4 label");
        let g4_end = asm[g4_start..].find("jmp *(%rbp)").expect("g4 ret");
        let g4 = &asm[g4_start..g4_start + g4_end];
        assert!(g4.contains("leaq .Lcon"), "expected direct static leaq for aliased True:\n{g4}");
        assert!(!g4.contains("hsc_eval"), "aliased True should not call hsc_eval:\n{g4}");
        let g5_start = asm.find(&format!("{}:\n", labels[5])).expect("g5 label");
        let g5_end = asm[g5_start..].find("jmp *(%rbp)").expect("g5 ret");
        let g5 = &asm[g5_start..g5_start + g5_end];
        assert!(g5.contains("leaq .Llit"), "expected direct static leaq for aliased literal:\n{g5}");
        assert!(!g5.contains("hsc_eval"), "aliased literal should not call hsc_eval:\n{g5}");
    }

    #[test]
    fn let_plus_con_fuses_into_single_heap_check() {
        // let a = Con2(x, y) in Con2(a, x) -> 3 + 3 = 6 words (48 bytes) in ONE hsc_gc_rt check.
        let body = SExpr::Let(
            vec![(3, SBind::Con("Builtin.:".into(), vec![Atom::Slot(1), Atom::Slot(2)]))],
            Box::new(SExpr::Con("Builtin.:".into(), vec![Atom::Slot(3), Atom::Slot(1)])),
        );
        let prog = make_prog(vec![SGlobal {
            name: "Test.pair".into(),
            closure: simple_closure(2, 4, 0, body),
            worker: None,
        }]);
        let asm = Gen::program(&prog);
        let table = asm.split_once("hsc_globals:\n").unwrap().1;
        let lbl = table.lines().next().unwrap().rsplit_once(',').unwrap().1.split_whitespace().next().unwrap();
        let g0_start = asm.find(&format!("{lbl}:\n")).expect("g0 label");
        let g0_end = asm[g0_start..].find("jmp *(%rbp)").expect("g0 ret");
        let g0 = &asm[g0_start..g0_start + g0_end];
        assert_eq!(g0.matches("call hsc_gc_rt").count(), 1, "expected single fused GC check:\n{g0}");
        assert!(g0.contains("leaq 48(%r12), %rax"), "expected 6-word (48-byte) combined bump:\n{g0}");
    }

    #[test]
    fn boxed_compare_and_branch_avoids_materializing_bool_int() {
        // case (x <# y) as raw0 of _ -> case raw0 of 0# -> 10#; _ -> 20#
        let body = SExpr::Case(
            Box::new(SExpr::Prim("<#".into(), vec![Atom::Slot(1), Atom::Slot(2)])),
            RAW | 0,
            vec![SAlt::Default(SExpr::Case(
                Box::new(SExpr::Atom(Atom::Slot(RAW | 0))),
                RAW | 1,
                vec![
                    SAlt::Lit(Lit::Int(0), SExpr::Atom(Atom::Lit(SLit::Int(10)))),
                    SAlt::Default(SExpr::Atom(Atom::Lit(SLit::Int(20)))),
                ],
            ))],
        );
        let prog = make_prog(vec![SGlobal {
            name: "Test.cmp".into(),
            closure: simple_closure(2, 3, 2, body),
            worker: None,
        }]);
        let asm = Gen::program(&prog);
        let table = asm.split_once("hsc_globals:\n").unwrap().1;
        let lbl = table.lines().next().unwrap().rsplit_once(',').unwrap().1.split_whitespace().next().unwrap();
        let g0_start = asm.find(&format!("{lbl}:\n")).expect("g0 label");
        let g0 = &asm[g0_start..asm[g0_start..].find("\n    .data").unwrap_or(asm.len() - g0_start) + g0_start];
        assert!(g0.contains("cmpq %rdx, %rcx"), "expected inline boxed cmpq:\n{g0}");
        assert!(g0.contains("jge "), "expected direct conditional jump without setl:\n{g0}");
        assert!(!g0.contains("setl "), "should not materialize 0/1 with setl:\n{g0}");
    }
}

