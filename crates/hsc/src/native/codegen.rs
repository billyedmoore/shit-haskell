//! Code generation: STG to x86-64 assembly (AT&T syntax).
//!
//! Registers (see NATIVE.md): `rbp` is the Haskell stack pointer, `r12`/`r13` the heap pointer and
//! its limit, `r14` the value or closure being entered, `rbx` the machine record. A body's frame is
//! `s` words: the closure (slot 0), the `a` arguments (slots 1..=a), then the locals. The arguments
//! of a call are pushed below the caller's continuation, so the callee's frame ends where its
//! continuation (a return address) begins. A return address's info (two words before it) gives the
//! kind and size of the frame directly above it, which is what the collector walks.
//!
//! `let` and `case` occur only in tail position with nothing pushed; a case pushes its return
//! address below the current frame, and its scrutinee is a simple expression (the lowering makes
//! nested cases join points).

use std::collections::HashMap;
use std::fmt::Write;

use super::stg::{Atom, SAlt, SClosure, SExpr, SLit, SProgram};
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
    /// A body's frame: all words are pointers.
    pub const BODY: u64 = 1;
    /// An update frame: the thunk being evaluated (one pointer).
    pub const UPDATE: u64 = 2;
    /// A catch frame: the handler (one pointer, the other word is unused).
    pub const CATCH: u64 = 3;
    /// An apply frame: a count word, then that many pointers.
    pub const APPLY: u64 = 4;
    /// The bottom of a stack segment: returns to the runtime.
    pub const STOP: u64 = 5;
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
    /// Total slots: closure, arguments and locals.
    s: u32,
    /// The arguments.
    a: u32,
}

impl Frame {
    /// The offset, from the frame's base (its lowest address), of a slot.
    fn off(&self, k: u32) -> i64 {
        let (s, a, k) = (self.s as i64, self.a as i64, k as i64);
        if k == 0 {
            8 * (s - a - 1)
        } else if k <= a {
            8 * (s - a + k - 1)
        } else {
            8 * (k - a - 1)
        }
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
    /// Closure bodies still to generate: (label, closure, name for debugging).
    pending: Vec<(String, SClosure, String)>,
    /// The bodies in the order they are emitted (their addresses increase): (label, name).
    body_names: Vec<(String, String)>,
    /// The name of the body being generated.
    current_name: String,
    /// Static objects (label, header, payload).
    statics: Vec<(String, u64, u64)>,
    /// Byte strings (label, bytes).
    strings: Vec<(String, Vec<u8>)>,
}

/// The header word of an object: `kind | size << 8 | aux << 32`.
fn header(kind: u8, size: u64, aux: u64) -> u64 {
    kind as u64 | (size << 8) | (aux << 32)
}

/// The words a closure occupies: its header, code word and captures (at least two).
fn closure_words(c: &SClosure) -> usize {
    (c.captures.len() + 2).max(2)
}

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
            pending: Vec::new(),
            body_names: Vec::new(),
            current_name: String::new(),
            statics: Vec::new(),
            strings: Vec::new(),
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
            g.pending.push((l, gl.closure.clone(), gl.name.clone()));
        }
        g.out.push_str("    .text\n");
        g.out.push_str(RUNTIME_ASM);
        while let Some((label, c, name)) = g.pending.pop() {
            g.body_names.push((label.clone(), name.clone()));
            g.closure_code(&label, &c, &name);
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
        let f = Frame { s: c.nslots, a: c.arity };
        self.put_label(label);
        // R1 is the closure; the arguments are above the new frame (they were pushed by the caller).
        self.emit(&format!("subq ${}, %rbp", 8 * (f.s - f.a) as i64));
        for k in (f.a + 1)..f.s {
            self.emit(&format!("movq $0, {}(%rbp)", f.off(k)));
        }
        self.emit(&format!("movq %r14, {}(%rbp)", f.off(0)));
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

    fn expr(&mut self, e: &SExpr, p: Pos) {
        match e {
            SExpr::Atom(a) => {
                self.load(a, p, "%r14");
                self.eval_result(p);
            }
            SExpr::App(func, args) => {
                // The function is loaded first (into R1, which nothing below clobbers). The arguments
                // are then copied one at a time, through a scratch register, into a new argument area
                // below the frame; a tail call moves that area up over the frame it replaces.
                let n = args.len() as i64;
                self.load(func, p, "%r14");
                self.emit(&format!("subq ${}, %rbp", 8 * n));
                let inner = Pos { pushed: p.pushed + n, ..p };
                for (i, a) in args.iter().enumerate() {
                    self.load(a, inner, "%rcx");
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
                }
                self.emit(&format!("movl ${}, %ecx", n));
                self.emit("jmp hsc_apply");
            }
            SExpr::Con(name, args) => {
                let id = self.con_id(name);
                self.alloc(1 + args.len(), p);
                self.emit(&format!("movabsq ${}, %rcx", header(CON, args.len() as u64, id as u64)));
                self.emit("movq %rcx, (%rax)");
                for (i, a) in args.iter().enumerate() {
                    self.load(a, p, "%rcx");
                    self.emit(&format!("movq %rcx, {}(%rax)", 8 * (i + 1)));
                }
                self.emit("movq %rax, %r14");
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
            SExpr::Case(scrut, bslot, alts) => {
                let cont = self.label("k");
                // Push the return address (its frame is this body's, above it).
                self.emit("subq $8, %rbp");
                self.emit(&format!("leaq {cont}(%rip), %rax"));
                self.emit("movq %rax, (%rbp)");
                let sp = Pos { f: p.f, mode: Mode::Scrut, pushed: p.pushed + 1 };
                self.expr(scrut, sp);
                // The info of the return address: its kind at -16 and the size of the frame above it
                // at -8. Control reaches the label only by a return, never by falling through.
                let _ = writeln!(self.out, "    .quad {}, {}", frame::BODY, p.f.s);
                self.put_label(&cont);
                self.emit("addq $8, %rbp");
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
            g.statics.push((lbl.clone(), header(kind, 1, 0), payload));
            lbl
        };
        let lbl = match l {
            SLit::Int(i) => stat(self, INT, *i as u64),
            SLit::Char(c) => stat(self, CHAR, *c as u64),
            SLit::Double(d) => stat(self, DOUBLE, d.to_bits()),
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
                let t = self.string_label(text.as_bytes());
                self.emit(&format!("leaq {t}(%rip), %rsi"));
                self.emit(&format!("movq ${}, %rdx", text.len()));
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
        let id = self.prim_id(name);
        for (i, a) in args.iter().enumerate() {
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

    /// Calls the runtime with the machine record in rdi, after recording the heap and stack state
    /// it needs: the stack pointer, and the number of pointer words at its top (the frame, or none
    /// when a return address is on top).
    fn rt_call(&mut self, name: &str, p: Pos, nargs: usize) {
        let _ = nargs;
        let top = if p.pushed == 0 { p.f.s as i64 } else { 0 };
        self.emit(&format!("movq %r12, {}(%rbx)", mach::HP));
        self.emit(&format!("movq ${top}, {}(%rbx)", mach::TOP));
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
        Lit::Char(c) => (1, (*c as u32).to_string()),
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
    movq (%r14), %rax
    movzbl %al, %ecx
    cmpb $9, %cl
    je .Leval_ind
    cmpb $8, %cl
    je .Leval_thunk
    jmp *(%rbp)
.Leval_ind:
    movq 8(%r14), %r14
    jmp hsc_eval
.Leval_thunk:
    movq %rax, %rdx
    shrq $32, %rdx
    testq %rdx, %rdx
    jnz .Leval_loop
    movq $1, %rdx
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
    call hsc_nontermination
    movq %r15, %rsp
    movq 0(%rbx), %r12
    movq 8(%rbx), %r13
    movq 16(%rbx), %rbp
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
    cmpb $6, %dl
    jne .Lap_slow
    movq %rax, %rdx
    shrq $32, %rdx
    cmpq %rcx, %rdx
    jne .Lap_slow
    movq 8(%r14), %rdx
    jmp *%rdx
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
