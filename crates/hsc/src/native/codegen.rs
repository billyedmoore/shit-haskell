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

use super::stg::{Atom, SAlt, SClosure, SExpr, SLit, SProgram, RAW, RAW_CHAR};
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
    /// Total slots: raw slots, locals, the closure and the arguments.
    s: u32,
    /// The arguments.
    a: u32,
    /// The raw slots (unboxed numbers, at the frame's low end).
    r: u32,
}

impl Frame {
    /// The offset, from the frame's base (its lowest address), of a slot. Raw slots come first,
    /// then the locals, the closure (slot 0) and the arguments (slots 1..=a).
    fn off(&self, k: u32) -> i64 {
        if k & RAW != 0 {
            return 8 * (k & !(RAW | RAW_CHAR)) as i64;
        }
        let (s, a, r, k) = (self.s as i64, self.a as i64, self.r as i64, k as i64);
        if k == 0 {
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
    /// The offset from the stack pointer of a slot.
    fn slot(&self, k: u32) -> String {
        format!("{}(%rbp)", self.f.off(k) + 8 * self.pushed)
    }
}

pub struct Gen {
    out: String,
    labels: u64,
    con_ids: HashMap<String, u32>,
    con_names: Vec<String>,
    prim_ids: HashMap<String, u32>,
    prim_names: Vec<String>,
    /// Entry labels and fixed arities in program-global index order.
    known_globals: Vec<(u32, String)>,
    /// Closure bodies still to generate: (label, closure, name for debugging).
    pending: Vec<(String, SClosure, String)>,
    /// The bodies in the order they are emitted (their addresses increase): (label, name).
    body_names: Vec<(String, String)>,
    /// The name of the body being generated.
    current_name: String,
    /// Static objects (label, header, payload: a number or a label).
    statics: Vec<(String, u64, String)>,
    /// Shared static cells for constructors without fields, indexed by constructor ID.
    nullary_cons: HashMap<u32, String>,
    /// Byte strings (label, bytes).
    strings: Vec<(String, Vec<u8>)>,
    /// The frame slots still used after the scrutinee being generated returns (innermost last).
    live: Vec<std::collections::HashSet<u32>>,
    /// The slots each join point's code uses (a jump to it uses them).
    join_uses: HashMap<u32, std::collections::HashSet<u32>>,
    /// The worker entries (label, raw parameters) in program-global index order.
    workers: Vec<Option<(String, Vec<bool>)>>,
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
}

/// The header word of an object: `kind | size << 8 | aux << 32`.
fn header(kind: u8, size: u64, aux: u64) -> u64 {
    kind as u64 | (size << 8) | (aux << 32)
}

/// The words a closure occupies: its header, code word and captures (at least two).
fn closure_words(c: &SClosure) -> usize {
    (c.captures.len() + 2).max(2)
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

/// Argument registers for the calls into the runtime (rdi carries the machine record).
const RT_ARGS: [&str; 5] = ["%rsi", "%rdx", "%rcx", "%r8", "%r9"];

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
            statics: Vec::new(),
            nullary_cons: HashMap::new(),
            strings: Vec::new(),
            joins: HashMap::new(),
            bool_cells: None,
            fast_done: None,
            slow_only: false,
            raw_dest: None,
            workers: Vec::new(),
            live: Vec::new(),
            join_uses: HashMap::new(),
        }
    }

    fn label(&mut self, prefix: &str) -> String {
        self.labels += 1;
        format!(".L{prefix}{}", self.labels)
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
        for c in super::RTS_CONS {
            g.con_id(c);
        }
        let mut global_labels = Vec::new();
        for gl in &prog.globals {
            let l = g.label("g");
            global_labels.push(l.clone());
            // Register every entry before emitting bodies, including forward/mutual calls.
            let arity = if gl.closure.captures.is_empty() { gl.closure.arity } else { 0 };
            g.known_globals.push((arity, l.clone()));
            g.pending.push((l, gl.closure.clone(), gl.name.clone()));
            let worker = gl.worker.as_ref().map(|(c, raw)| {
                let wl = g.label("w");
                g.pending.push((wl.clone(), c.clone(), format!("{}/worker", gl.name)));
                (wl, raw.clone())
            });
            g.workers.push(worker);
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
        g.out.push_str("    .data\n    .p2align 3\n");
        g.out.push_str("hsc_globals:\n");
        for (gl, l) in prog.globals.iter().zip(&global_labels) {
            let kind = if gl.closure.arity > 0 { gl.closure.arity as i64 } else { -1 };
            let _ = writeln!(g.out, "    .quad {kind}, {l}");
        }

        for (l, h, w) in std::mem::take(&mut g.statics) {
            let _ = writeln!(g.out, "{l}:\n    .quad {h}, {w}");
        }
        for (l, bytes) in std::mem::take(&mut g.strings) {
            let list: Vec<String> = bytes.iter().map(|b| b.to_string()).collect();
            let _ = writeln!(g.out, "{l}:\n    .byte {}", list.join(","));
        }
        let cons = std::mem::take(&mut g.con_names);
        g.out.push_str("hsc_con_names:\n");
        for (i, n) in cons.iter().enumerate() {
            let _ = writeln!(g.out, "    .quad .Lcn{i}, {}", n.len());
        }
        for (i, n) in cons.iter().enumerate() {
            let _ = writeln!(g.out, ".Lcn{i}:\n    .ascii \"{}\"", escape(n));
        }
        let prims = std::mem::take(&mut g.prim_names);
        g.out.push_str("hsc_prim_names:\n");
        for (i, n) in prims.iter().enumerate() {
            let _ = writeln!(g.out, "    .quad .Lpn{i}, {}", n.len());
        }
        for (i, n) in prims.iter().enumerate() {
            let _ = writeln!(g.out, ".Lpn{i}:\n    .ascii \"{}\"", escape(n));
        }
        // The body table: entry addresses in increasing order, with their names (for diagnostics).
        g.out.push_str("    .data\n    .p2align 3\n    .globl hsc_body_table\nhsc_body_table:\n");
        let bodies = std::mem::take(&mut g.body_names);
        let bodies_count = bodies.len();
        for (i, (l, _)) in bodies.iter().enumerate() {
            let _ = writeln!(g.out, "    .quad {l}, .Lbn{i}, {}", bodies[i].1.len());
        }
        let _ = writeln!(g.out, "    .globl hsc_nbodies\nhsc_nbodies:\n    .quad {}", bodies.len());
        for (i, (_, n)) in bodies.iter().enumerate() {
            let _ = writeln!(g.out, ".Lbn{i}:\n    .ascii \"{}\"", escape(n));
        }
        // The bindings the runtime uses: name and index, for those the program has.
        let rts: Vec<(&str, u32)> =
            super::RTS_GLOBALS.iter().filter_map(|n| prog.index.get(*n).map(|i| (*n, *i))).collect();
        g.out.push_str("    .p2align 3\nhsc_rts_globals:\n");
        for (i, (n, idx)) in rts.iter().enumerate() {
            let _ = writeln!(g.out, "    .quad .Lrg{i}, {}, {idx}", n.len());
        }
        for (i, (n, _)) in rts.iter().enumerate() {
            let _ = writeln!(g.out, ".Lrg{i}:\n    .ascii \"{}\"", escape(n));
        }
        g.out.push_str("    .p2align 3\n");
        let main_index = prog.index.get("hsc$main").or_else(|| prog.index.get("Main.main")).copied().unwrap_or(0);
        // The table the runtime reads (`machine::entry::NativeTable`).
        let _ = writeln!(
            g.out,
            "    .globl hsc_table\nhsc_table:\n    .quad hsc_globals, {}, hsc_con_names, {}, hsc_prim_names, {}, {}, hsc_body_table, {}, hsc_rts_globals, {}",
            prog.globals.len(),
            cons.len(),
            prims.len(),
            main_index,
            bodies_count,
            rts.len()
        );
        g.out.push_str("    .section .note.GNU-stack,\"\",@progbits\n");
        g.out
    }

    /// A closure's code: the prologue, then its expression in tail position.
    fn closure_code(&mut self, label: &str, c: &SClosure, name: &str) {
        self.current_name = name.to_string();
        let f = Frame { s: c.nslots + c.nraw, a: c.arity, r: c.nraw };
        self.put_label(label);
        // R1 is the closure; the arguments are above the new frame (they were pushed by the caller).
        self.emit(&format!("subq ${}, %rbp", 8 * (f.s - f.a) as i64));
        // The pointer locals start empty for the collector; raw slots are not read by it.
        for k in (f.a + 1)..(f.s - f.r) {
            self.emit(&format!("movq $0, {}(%rbp)", f.off(k)));
        }
        self.emit(&format!("movq %r14, {}(%rbp)", f.off(0)));
        // A worker's raw parameters arrive as numbers in their argument slots: they move to raw
        // slots, and the argument slots are emptied, before anything can collect.
        for (arg, raw) in &c.raw_args {
            self.emit(&format!("movq {}(%rbp), %rcx", f.off(*arg)));
            self.emit(&format!("movq %rcx, {}(%rbp)", f.off(*raw)));
            self.emit(&format!("movq $0, {}(%rbp)", f.off(*arg)));
        }
        self.expr(&c.body, Pos { f, mode: Mode::Tail, pushed: 0 });
    }

    /// Leaves a WHNF value in R1 as the result of the expression.
    fn ret(&mut self, p: Pos) {
        match p.mode {
            Mode::Tail => {
                self.emit(&format!("addq ${}, %rbp", 8 * p.f.s as i64));
                self.emit("jmp *(%rbp)");
            }
            Mode::Scrut => self.emit("jmp *(%rbp)"),
        }
    }

    /// Evaluates the value in R1 (possibly a thunk) as the result of the expression.
    fn eval_result(&mut self, p: Pos) {
        if p.mode == Mode::Tail {
            self.emit(&format!("addq ${}, %rbp", 8 * p.f.s as i64));
        }
        self.emit("jmp hsc_eval");
    }

    fn known_global_entry(&self, func: &Atom, nargs: usize) -> Option<&str> {
        let Atom::Global(index) = func else { return None };
        let (arity, entry) = self.known_globals.get(*index as usize)?;
        (*arity > 0 && *arity as usize == nargs).then_some(entry.as_str())
    }

    fn expr(&mut self, e: &SExpr, p: Pos) {
        match e {
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
            SExpr::Atom(a) => {
                self.load(a, p, "%r14");
                self.clear_dead(std::slice::from_ref(a), p);
                self.eval_result(p);
            }
            SExpr::App(func, args) => {
                // An exact call of a function with a worker passes its raw parameters as numbers.
                let worker = match func {
                    Atom::Global(g) => self.workers.get(*g as usize).cloned().flatten().filter(|(_, r)| r.len() == args.len()),
                    _ => None,
                };
                let direct = match &worker {
                    Some((l, _)) => Some(l.clone()),
                    None => self.known_global_entry(func, args.len()).map(str::to_owned),
                };
                let raw_pos = worker.map(|(_, r)| r).unwrap_or_default();
                // The function is loaded first (into R1, which nothing below clobbers). The arguments
                // are then copied one at a time, through a scratch register, into a new argument area
                // below the frame; a tail call moves that area up over the frame it replaces.
                let n = args.len() as i64;
                self.load(func, p, "%r14");
                self.emit(&format!("subq ${}, %rbp", 8 * n));
                let inner = Pos { pushed: p.pushed + n, ..p };
                for (i, a) in args.iter().enumerate() {
                    if raw_pos.get(i).copied().unwrap_or(false) {
                        match a {
                            Atom::Slot(k) if k & RAW != 0 => self.emit(&format!("movq {}, %rcx", inner.slot(*k))),
                            Atom::Lit(SLit::Int(v)) => self.emit(&format!("movabsq ${v}, %rcx")),
                            Atom::Lit(SLit::Char(c)) => {
                                self.emit(&format!("movq ${}, %rcx", crate::syntax::lexer::char_code(*c)))
                            }
                            other => panic!("internal: worker argument {other:?} is not raw"),
                        }
                    } else {
                        self.load(a, inner, "%rcx");
                    }
                    self.emit(&format!("movq %rcx, {}(%rbp)", 8 * i as i64));
                }
                if p.mode == Mode::Tail {
                    let s = p.f.s as i64;
                    // Highest first: the destination of an argument may be the source of a later one.
                    for i in (0..n).rev() {
                        self.emit(&format!("movq {}(%rbp), %rcx", 8 * i));
                        self.emit(&format!("movq %rcx, {}(%rbp)", 8 * (i + s)));
                    }
                    self.emit(&format!("addq ${}, %rbp", 8 * s));
                } else {
                    let mut used = vec![func.clone()];
                    used.extend(args.iter().cloned());
                    self.clear_dead(&used, inner);
                }
                self.emit(&format!("movl ${}, %ecx", n));
                if let Some(entry) = direct {
                    self.emit(&format!("jmp {entry}"));
                } else {
                    self.emit("jmp hsc_apply");
                }
            }
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
                let total: usize = binds.iter().map(|(_, c)| closure_words(c)).sum();
                self.alloc(total, p);
                // Every closure's address is in its slot before any closure is filled (letrec).
                let mut off = 0usize;
                let mut offs = Vec::new();
                for (slot, c) in binds {
                    self.emit(&format!("leaq {}(%rax), %rcx", 8 * off));
                    self.emit(&format!("movq %rcx, {}", p.slot(*slot)));
                    offs.push(off);
                    off += closure_words(c);
                }
                for ((_, c), base) in binds.iter().zip(offs) {
                    self.fill_closure(c, base, p);
                }
                self.expr(body, p);
            }
            SExpr::Case(scrut, _, alts) if matches!(scrut.as_ref(), SExpr::Atom(a) if is_raw(a)) => {
                // A case of a raw number: its literal alternatives compare the number itself (the
                // binder is unused, see `stg::raw_uses`).
                let SExpr::Atom(Atom::Slot(k)) = scrut.as_ref() else { unreachable!() };
                self.emit(&format!("movq {}, %rax", p.slot(*k)));
                let labels: Vec<String> = alts.iter().map(|_| self.label("alt")).collect();
                let mut default = false;
                for (a, l) in alts.iter().zip(&labels) {
                    match a {
                        SAlt::Lit(lit, _) => {
                            let v = match lit {
                                Lit::Int(i) => *i as u64,
                                Lit::Char(c) => crate::syntax::lexer::char_code(*c) as u64,
                                other => panic!("internal: raw case on literal {other:?}"),
                            };
                            self.emit(&format!("movabsq ${v}, %rcx"));
                            self.emit("cmpq %rcx, %rax");
                            self.emit(&format!("je {l}"));
                        }
                        SAlt::Default(_) => {
                            self.emit(&format!("jmp {l}"));
                            default = true;
                            break;
                        }
                        SAlt::Con(..) => panic!("internal: raw case on a constructor"),
                    }
                }
                if !default {
                    self.fail("Non-exhaustive patterns in case", p);
                }
                for (a, l) in alts.iter().zip(&labels) {
                    self.put_label(l);
                    match a {
                        SAlt::Lit(_, body) | SAlt::Default(body) => self.expr(body, p),
                        SAlt::Con(..) => unreachable!(),
                    }
                }
            }
            SExpr::Case(scrut, bslot, alts) => {
                let cont = self.label("k");
                let raw_binder = *bslot & RAW != 0;
                // A variable that is already a value (not a thunk or an indirection) goes straight
                // to the alternatives, without the return address and `hsc_eval`.
                let ready = match scrut.as_ref() {
                    SExpr::Atom(a @ (Atom::Slot(_) | Atom::Free(_) | Atom::Global(_))) => {
                        let ready = self.label("ready");
                        let eval = self.label("eval");
                        let value = self.label("value");
                        self.load(a, p, "%r14");
                        self.emit("testq %r14, %r14");
                        self.emit(&format!("je {value}"));
                        self.emit(&format!("cmpb ${THUNK}, (%r14)"));
                        self.emit(&format!("je {eval}"));
                        self.emit(&format!("cmpb ${IND}, (%r14)"));
                        self.emit(&format!("je {eval}"));
                        // A value: as on the slow path, its slot is cleared when nothing uses it
                        // after (only here: the slow path loads the slot again).
                        self.put_label(&value);
                        let live = self.case_live(alts, p);
                        self.live.push(live);
                        self.clear_dead(std::slice::from_ref(a), Pos { mode: Mode::Scrut, ..p });
                        self.live.pop();
                        self.emit(&format!("jmp {ready}"));
                        self.put_label(&eval);
                        Some(ready)
                    }
                    // A primitive with an inline fast path is computed in place (before the return
                    // address, which only its slow path needs).
                    SExpr::Prim(name, args) => {
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
                    _ => None,
                };
                // Push the return address (its frame is this body's, above it).
                self.emit("subq $8, %rbp");
                self.emit(&format!("leaq {cont}(%rip), %rax"));
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
                    self.emit("movq 8(%r14), %rcx");
                    self.emit(&format!("movq %rcx, {}", p.slot(*bslot)));
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
                self.emit(&format!("movq %r14, {}", p.slot(*bslot)));
                self.alts(alts, *bslot, p);
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
                    self.emit(&format!("movq %rcx, {}", p.slot(*s)));
                }
                self.emit(&format!("jmp {l}"));
            }
        }
    }

    /// The alternatives of a case: the value is in R1 and in the binder slot `bslot`.
    fn alts(&mut self, alts: &[SAlt], bslot: u32, p: Pos) {
        let labels: Vec<String> = alts.iter().map(|_| self.label("alt")).collect();
        let mut has_default = false;
        for (a, l) in alts.iter().zip(&labels) {
            match a {
                SAlt::Con(name, _, _) => {
                    let id = self.con_id(name);
                    self.emit("movq (%r14), %rcx");
                    self.emit("shrq $32, %rcx");
                    self.emit(&format!("cmpq ${id}, %rcx"));
                    self.emit(&format!("je {l}"));
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
                    self.emit(&format!("cmpb ${kind}, (%r14)"));
                    self.emit(&format!("jne {next}"));
                    self.emit(&format!("movabsq ${payload}, %rcx"));
                    self.emit(cmp);
                    self.emit(&format!("je {l}"));
                    self.put_label(&next);
                }
                SAlt::Lit(lit, _) => {
                    // The runtime compares the scrutinee with the literal's text (no allocation).
                    let (kind, text) = lit_text(lit);
                    let t = self.string_label(text.as_bytes());
                    self.emit(&format!("movq {}, %rsi", p.slot(bslot)));
                    self.emit(&format!("leaq {t}(%rip), %rdx"));
                    self.emit(&format!("movq ${}, %rcx", text.len()));
                    self.emit(&format!("movq ${kind}, %r8"));
                    self.rt_call("hsc_lit_matches", p, 4);
                    self.emit("testl %eax, %eax");
                    self.emit(&format!("jnz {l}"));
                }
                SAlt::Default(_) => {
                    self.emit(&format!("jmp {l}"));
                    has_default = true;
                    break;
                }
            }
        }
        if !has_default {
            // Diagnostic: the scrutinee is left in the record for the failure's message.
            self.emit(&format!("movq {}, %rax", p.slot(bslot)));
            self.emit(&format!("movq %rax, {}(%rbx)", mach::R1));
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
        for (a, l) in alts.iter().zip(&labels) {
            self.put_label(l);
            match a {
                SAlt::Con(_, slots, body) => {
                    for (i, s) in slots.iter().enumerate() {
                        self.emit(&format!("movq {}(%r14), %rcx", 8 * (i + 1)));
                        self.emit(&format!("movq %rcx, {}", p.slot(*s)));
                    }
                    self.expr(body, p);
                }
                SAlt::Default(SExpr::Fail(msg)) => {
                    // Diagnostic: a pattern failure leaves the scrutinee in the record.
                    self.emit(&format!("movq {}, %rax", p.slot(bslot)));
                    self.emit(&format!("movq %rax, {}(%rbx)", mach::R1));
                    self.fail(msg, p);
                }
                SAlt::Lit(_, body) | SAlt::Default(body) => self.expr(body, p),
            }
        }
    }

    /// A pattern-match failure: the runtime builds the exception (with the stack in its state at
    /// this point), and it is raised.
    fn fail(&mut self, msg: &str, p: Pos) {
        let l = self.string_label(msg.as_bytes());
        self.emit(&format!("leaq {l}(%rip), %rsi"));
        self.emit(&format!("movq ${}, %rdx", msg.len()));
        self.rt_call("hsc_fail", p, 2);
        self.emit("jmp hsc_raise_pending");
    }

    fn push_args(&mut self, n: usize) {
        if n > 0 {
            self.emit(&format!("subq ${}, %rbp", 8 * n));
        }
        for i in 0..n {
            self.emit(&format!("movq {}, {}(%rbp)", arg_reg(i), 8 * i));
        }
    }

    /// Loads an atom into a register.
    fn load(&mut self, a: &Atom, p: Pos, reg: &str) {
        match a {
            Atom::Slot(k) if k & RAW != 0 => panic!("internal: raw slot {k:#x} used as a pointer in {}", self.current_name),
            Atom::Slot(k) => self.emit(&format!("movq {}, {reg}", p.slot(*k))),
            Atom::Free(i) => {
                self.emit(&format!("movq {}, {reg}", p.slot(0)));
                self.emit(&format!("movq {}({reg}), {reg}", 8 * (2 + *i as i64)));
            }
            Atom::Global(g) => {
                self.emit(&format!("movq {}(%rbx), {reg}", mach::GLOBALS));
                self.emit(&format!("movq {}({reg}), {reg}", 8 * *g as i64));
            }
            Atom::Lit(l) => self.literal(l, p, reg),
        }
    }

    fn literal(&mut self, l: &SLit, p: Pos, reg: &str) {
        let stat = |g: &mut Gen, kind: u8, payload: u64| {
            let lbl = g.label("lit");
            g.statics.push((lbl.clone(), header(kind, 1, 0), payload.to_string()));
            lbl
        };
        let lbl = match l {
            SLit::Int(i) => stat(self, INT, *i as u64),
            SLit::Char(c) => stat(self, CHAR, crate::syntax::lexer::char_code(*c) as u64),
            SLit::Double(d) => stat(self, DOUBLE, d.to_bits()),
            // An `Integer` that fits in an `Int` is `IS` of it: a static cell for both.
            SLit::Integer(s) if s.parse::<i64>().is_ok() => {
                let small = stat(self, INT, s.parse::<i64>().unwrap_or(0) as u64);
                let id = self.con_id("GHC.Num.Integer.IS");
                let lbl = self.label("lit");
                self.statics.push((lbl.clone(), header(CON, 1, id as u64), small));
                lbl
            }
            other => {
                // Strings, big integers, fractions and addresses: the runtime builds the object (the
                // call allocates, so the atom is loaded last).
                let (kind, text) = match other {
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
                self.emit(&format!("leaq {t}(%rip), %rsi"));
                self.emit(&format!("movq ${}, %rdx", bytes.len()));
                self.emit(&format!("movq ${kind}, %rcx"));
                self.rt_call("hsc_mk_literal", p, 3);
                if reg != "%rax" {
                    self.emit(&format!("movq %rax, {reg}"));
                }
                return;
            }
        };
        self.emit(&format!("leaq {lbl}(%rip), {reg}"));
        let _ = p;
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
        let l = self.label("str");
        self.strings.push((l.clone(), bytes.to_vec()));
        l
    }

    /// Allocates `words` words into %rax. A full heap calls the collector with the frame as it is.
    fn alloc(&mut self, words: usize, p: Pos) {
        // Every object has at least two words (the collector relies on it).
        let words = words.max(2);
        let ok = self.label("ok");
        self.emit(&format!("leaq {}(%r12), %rax", 8 * words));
        self.emit("cmpq %r13, %rax");
        self.emit(&format!("jbe {ok}"));
        self.emit(&format!("movq ${words}, %rsi"));
        self.rt_call("hsc_gc", p, 1);
        self.put_label(&ok);
        self.emit("movq %r12, %rax");
        self.emit(&format!("leaq {}(%r12), %r12", 8 * words));
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
        self.emit(&format!("movabsq ${hdr}, %rcx"));
        self.emit(&format!("movq %rcx, {}(%rax)", 8 * base));
        self.emit(&format!("leaq {code}(%rip), %rcx"));
        self.emit(&format!("movq %rcx, {}(%rax)", 8 * (base + 1)));
        for (i, cap) in c.captures.iter().enumerate() {
            // The captured atom is loaded through %rdx so that %rax (the block) survives.
            self.load(cap, p, "%rdx");
            self.emit(&format!("movq %rdx, {}(%rax)", 8 * (base + 2 + i)));
        }
    }

    /// `catch# io handler s`: a catch frame (handler) is pushed, then `io` is applied to the state
    /// token. Its normal return pops the frame; an exception unwinds to it.
    fn catch(&mut self, args: &[Atom], p: Pos) {
        self.load(&args[1], p, "%rdx");
        self.load(&args[2], p, "%r8");
        self.load(&args[0], p, "%r14");
        if p.mode == Mode::Tail {
            self.emit(&format!("addq ${}, %rbp", 8 * p.f.s as i64));
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
                    self.emit(&format!("movq ${}, (%rax)", header(operand_kind(name), 1, 0)));
                    self.emit(&format!("movq {}, %rcx", p.slot(*k)));
                    self.emit("movq %rcx, 8(%rax)");
                    self.emit(&format!("movq %rax, {}(%rbx)", mach::ARGS + 8 * i as i64));
                    continue;
                }
            }
            self.load(a, p, "%rdx");
            self.emit(&format!("movq %rdx, {}(%rbx)", mach::ARGS + 8 * i as i64));
        }
        self.emit(&format!("movq ${id}, %rsi"));
        self.emit(&format!("movq ${}, %rdx", args.len()));
        self.rt_call("hsc_prim", p, 2);
        self.emit("testq %rax, %rax");
        self.emit("jz hsc_raise_pending");
        self.emit("movq %rax, %r14");
        self.ret(p);
    }

    /// An `Int#` operation on arguments that are already INT cells, computed inline; otherwise (or
    /// when a thread switch is due, or a division could fault) control goes to `slow`, the
    /// runtime's primitive. Answers whether the fast path was emitted.
    fn int_fast_path(&mut self, name: &str, args: &[Atom], p: Pos, slow: &str) -> bool {
        #[derive(PartialEq)]
        enum K { Bin(&'static str), Cmp(&'static str), Shift(&'static str), Div(bool), Un(&'static str), CharCmp(&'static str), Ord, Chr, TagToEnum }
        let k = match name {
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
            _ => return false,
        };
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
        self.emit("cmpb $0, hsc_preempt(%rip)");
        self.emit(&format!("jne {slow}"));
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
            SExpr::Con(_, args) | SExpr::Prim(_, args) => args.iter().for_each(|a| atom(a, out)),
            SExpr::Let(binds, body) => {
                for (_, c) in binds {
                    c.captures.iter().for_each(|a| atom(a, out));
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
        let zero = self.label("int0");
        let one = self.label("int1");
        self.statics.push((zero.clone(), header(INT, 1, 0), "0".into()));
        self.statics.push((one.clone(), header(INT, 1, 0), "1".into()));
        self.bool_cells = Some((zero.clone(), one.clone()));
        (zero, one)
    }

    /// Calls the runtime with the machine record in rdi, after recording the heap and stack state
    /// it needs: the stack pointer, and the number of pointer words at its top (the frame, or none
    /// when a return address is on top).
    fn rt_call(&mut self, name: &str, p: Pos, nargs: usize) {
        let _ = nargs;
        let top = if p.pushed == 0 { p.f.size_word() } else { 0 };
        self.emit(&format!("movq %r12, {}(%rbx)", mach::HP));
        if top > i32::MAX as u64 {
            // A size word with raw slots does not fit an immediate store (%rax is no argument).
            self.emit(&format!("movabsq ${top}, %rax"));
            self.emit(&format!("movq %rax, {}(%rbx)", mach::TOP));
        } else {
            self.emit(&format!("movq ${top}, {}(%rbx)", mach::TOP));
        }
        self.emit(&format!("movq %rbp, {}(%rbx)", mach::SP));
        self.emit("movq %rbx, %rdi");
        // The C ABI wants a 16-byte aligned stack at each call; r15 keeps the old stack pointer (the
        // callee preserves it).
        self.emit("movq %rsp, %r15");
        self.emit("andq $-16, %rsp");
        self.emit(&format!("call {name}"));
        self.emit("movq %r15, %rsp");
        self.emit(&format!("movq {}(%rbx), %r12", mach::HP));
        self.emit(&format!("movq {}(%rbx), %r13", mach::HPLIM));
    }
}

/// A literal pattern's kind and text, as the runtime reads them.
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

/// The register holding the i-th argument while arguments are assembled.
fn arg_reg(i: usize) -> String {
    const REGS: [&str; 8] = ["%r8", "%r9", "%r10", "%r11", "%rsi", "%rdi", "%rdx", "%rax"];
    REGS[i.min(7)].to_string()
}

fn escape(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if c.is_ascii_graphic() || c == ' ' => out.push(c),
            c => {
                let mut buf = [0u8; 4];
                for b in c.encode_utf8(&mut buf).bytes() {
                    let _ = write!(out, "\\{:03o}", b);
                }
            }
        }
    }
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
    movzbl %al, %ecx
    cmpb $9, %cl
    je .Leval_ind
    cmpb $8, %cl
    je .Leval_thunk
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

"#;

#[cfg(test)]
mod nullary_constructor_tests {
    use super::*;

    fn position(mode: Mode) -> Pos {
        Pos { f: Frame { s: 1, a: 0, r: 0 }, mode, pushed: 0 }
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
        Pos { f: Frame { s: 3, a: 2, r: 0 }, mode, pushed: if mode == Mode::Scrut { 1 } else { 0 } }
    }

    #[test]
    fn exact_global_calls_bypass_apply_in_both_return_modes() {
        for mode in [Mode::Tail, Mode::Scrut] {
            let mut gen = Gen::new();
            gen.known_globals.push((2, ".Lknown_entry".into()));
            gen.expr(&SExpr::App(Atom::Global(0), vec![Atom::Slot(1), Atom::Slot(2)]), position(mode));
            assert!(gen.out.ends_with("    jmp .Lknown_entry\n"));
            assert!(!gen.out.contains("jmp hsc_apply"));
        }
    }

    #[test]
    fn partial_overapplied_caf_and_local_calls_keep_generic_dispatch() {
        for (func, arity, nargs) in [
            (Atom::Global(0), 2, 1), (Atom::Global(0), 2, 3),
            (Atom::Global(0), 0, 1), (Atom::Slot(0), 2, 2), (Atom::Free(0), 2, 2),
        ] {
            let mut gen = Gen::new();
            gen.known_globals.push((arity, ".Lknown_entry".into()));
            let args = vec![Atom::Lit(SLit::Int(7)); nargs];
            gen.expr(&SExpr::App(func, args), position(Mode::Tail));
            assert!(gen.out.ends_with("    jmp hsc_apply\n"));
        }
    }

    #[test]
    fn forward_and_mutual_entries_are_registered_before_body_generation() {
        let globals = vec![
            SGlobal { name: "Main.main".into(), closure: SClosure { arity: 1, captures: Vec::new(), nslots: 2, nraw: 0, raw_args: Vec::new(), body: SExpr::App(Atom::Global(1), vec![Atom::Slot(1)]) }, worker: None },
            SGlobal { name: "Test.other".into(), closure: SClosure { arity: 1, captures: Vec::new(), nslots: 2, nraw: 0, raw_args: Vec::new(), body: SExpr::App(Atom::Global(0), vec![Atom::Slot(1)]) }, worker: None },
        ];
        let index = [("Main.main".to_string(), 0), ("Test.other".to_string(), 1)].into_iter().collect();
        let asm = Gen::program(&SProgram { globals, index });
        let table = asm.split_once("hsc_globals:\n").unwrap().1;
        for row in table.lines().take(2) {
            let label = row.rsplit_once(',').unwrap().1.trim();
            assert!(asm.contains(&format!("jmp {label}\n")));
        }
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
        g.expr(e, Pos { f: Frame { s: 4, a: 2, r: 0 }, mode, pushed: 0 });
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
