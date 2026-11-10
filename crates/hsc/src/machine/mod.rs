//! The native machine's runtime: the machine record that generated code runs against, the stack
//! walk the collector uses, and the runtime entry points that generated code calls.
//!
//! The record's first words are read and written by generated code; their offsets are fixed by
//! `native::codegen::mach` (see NATIVE.md).

pub mod entry;
pub mod gc;
pub mod heap;
pub mod prims;
pub mod prims_ffi;
pub mod prims_integer;
pub mod prims_misc;
pub mod prims_mem;
pub mod prims_num;

use std::cell::Cell as StdCell;

use heap::{kind, Heap, Word};

/// Words of the Haskell stack per thread: a large reserve, committed by the kernel on use.
const STACK_WORDS: usize = 1 << 26;
/// Words of the heap semispace at start.
const HEAP_WORDS: usize = 1 << 22;
/// Argument slots of the record (primitive operations take their arguments here).
pub const ARG_SLOTS: usize = 16;

/// The machine record. The first fields are the layout generated code uses.
#[repr(C)]
pub struct Machine {
    /// Next free heap address.
    pub hp: Word,
    /// The end of the heap.
    pub hplim: Word,
    /// The Haskell stack pointer, saved on entering the runtime.
    pub sp: Word,
    /// The number of pointer words at the top of the stack (the innermost frame), for the collector.
    pub top: Word,
    /// The global closures (roots).
    pub globals: *mut Word,
    /// The value being returned or entered.
    pub r1: Word,
    /// The number of arguments of an application.
    pub nargs: Word,
    /// The code to enter after an application slow path.
    pub code: Word,
    /// The arguments of a primitive operation (roots).
    pub args: [Word; ARG_SLOTS],
    // Runtime-only state from here on.
    pub heap: Heap,
    /// The Haskell stack: `stack_base..stack_top` words, growing down from the top.
    pub stack: Vec<Word>,
    /// The exception being raised (a pointer), if any.
    pub exc: Word,
    /// The tables of the program.
    pub prog: Program,
    /// Constructor name -> id, for the constructors the runtime builds.
    pub con_ids: std::collections::HashMap<String, u32>,
    /// Extra roots the runtime keeps (stable pointers and similar), by index.
    pub stable: Vec<Word>,
    /// Forked threads not yet run: their actions (state-token functions). A thread runs when another
    /// blocks on an MVar, or when the program delays or yields; the actions are roots.
    pub runq: std::collections::VecDeque<Word>,
    /// The per-name pointer slots of the RTS (`getOrSet…`), by name.
    pub named: std::collections::HashMap<String, Word>,
    /// The global closures (the GC roots `globals` points to).
    pub global_vec: Vec<Word>,
    /// The bindings the runtime uses, by name: their index among the globals.
    pub rts_globals: std::collections::HashMap<String, usize>,
    /// The program's name, as the runtime reports errors.
    pub prog_name: String,
    /// The bodies of the program in address order, with their names (for diagnostics).
    pub bodies: Vec<(Word, String)>,
    /// The argument slots of the primitive operations that are waiting on a nested evaluation.
    pub saved_args: Vec<[Word; ARG_SLOTS]>,
    /// Set when a nested evaluation a primitive made raised: the primitive answers the exception.
    pub prim_failed: bool,
}

/// The tables the generated program provides (see `native::codegen`).
pub struct Program {
    pub globals: Vec<(i64, usize)>,
    pub con_names: Vec<String>,
    pub prim_names: Vec<String>,
}

thread_local! {
    /// The machine of the running thread, for runtime entry points that need it without a pointer.
    static CURRENT: StdCell<usize> = const { StdCell::new(0) };
}

impl Machine {
    /// A machine with the program's tables and the global closures allocated.
    pub fn new(prog: Program) -> Box<Machine> {
        let mut heap = Heap::new(HEAP_WORDS);
        let base = heap.addr(0);
        let limit = base + (HEAP_WORDS as Word) * 8;
        let stack = vec![0 as Word; STACK_WORDS];
        let mut m = Box::new(Machine {
            hp: base,
            hplim: limit,
            sp: 0,
            top: 0,
            globals: std::ptr::null_mut(),
            r1: 0,
            nargs: 0,
            code: 0,
            args: [0; ARG_SLOTS],
            heap,
            stack: Vec::new(),
            exc: 0,
            prog,
            con_ids: std::collections::HashMap::new(),
            stable: vec![0],
            runq: std::collections::VecDeque::new(),
            named: std::collections::HashMap::new(),
            global_vec: Vec::new(),
            rts_globals: std::collections::HashMap::new(),
            prog_name: std::env::args().next().and_then(|a| std::path::Path::new(&a).file_name().map(|s| s.to_string_lossy().into_owned())).unwrap_or_else(|| "main".into()),
            saved_args: Vec::new(),
            prim_failed: false,
            bodies: Vec::new(),
        });
        m.stack = stack;
        m.sp = m.stack.as_mut_ptr() as Word + (STACK_WORDS as Word) * 8;
        for (i, n) in m.prog.con_names.iter().enumerate() {
            m.con_ids.insert(n.clone(), i as u32);
        }
        CURRENT.with(|c| c.set(&*m as *const Machine as usize));
        m
    }

    /// Allocates `words` words without collecting (the heap has room at start-up).
    pub fn alloc_no_gc(&mut self, words: usize) -> Word {
        let p = self.hp;
        self.hp += (words as Word) * 8;
        assert!(self.hp <= self.hplim, "heap exhausted at start-up");
        p
    }

    /// Writes a heap word at an address.
    pub fn heap_set(&mut self, p: Word, v: Word) {
        // SAFETY: callers pass addresses of objects in the current heap space.
        unsafe { *(p as *mut Word) = v }
    }

    /// Reads word `i` of the object at `p` (word 0 is its header).
    pub fn heap_get(&self, p: Word, i: usize) -> Word {
        // SAFETY: callers pass addresses of objects in the current heap space.
        unsafe { *((p as *const Word).add(i)) }
    }

    /// Reads a stack word at an address.
    pub fn stack_load(&self, a: Word) -> Word {
        // SAFETY: the addresses come from the stack walk and the frame layout.
        unsafe { *(a as *const Word) }
    }

    /// Writes a stack word at an address.
    pub fn stack_store(&mut self, a: Word, v: Word) {
        // SAFETY: as for `stack_load`.
        unsafe { *(a as *mut Word) = v }
    }

    /// An exception value for a failure the runtime reports (a constructor with the message).
    /// It is the library's own value: `NonTermination` for a computation that depends on itself, and
    /// otherwise an `ErrorCall` with the message, as a `SomeException`.
    pub fn error_value(&mut self, msg: &str) -> Word {
        if msg == "<<loop>>" {
            if let Some(g) = self.rts_global("GHC.Internal.Control.Exception.Base.nonTerminationException") {
                if let Some(v) = self.force(g) {
                    return v;
                }
            }
        }
        if self.rts_globals.contains_key("GHC.Internal.Exception.errorCallException") {
            // The text is built first: the function is read after the allocation (which may collect).
            let text = prims::string_list(self, msg);
            let f = self.rts_global("GHC.Internal.Exception.errorCallException").unwrap_or(0);
            if let Some(v) = self.apply_nested(f, &[text]) {
                return v;
            }
        }
        // A program without the library: a bare constructor.
        self.exc = 0;
        self.prim_failed = false;
        let p = self.alloc(2);
        let id = self.con_ids.get("GHC.Internal.Exception.ErrorCall").copied().unwrap_or(0);
        self.heap_set(p, heap::header(kind::CON, 0, id));
        p
    }

    /// A binding the runtime uses, if the program has it.
    pub fn rts_global(&self, name: &str) -> Option<Word> {
        self.rts_globals.get(name).map(|i| self.global_vec[*i])
    }

    /// The name of the constructor of an evaluated object.
    pub fn con_name(&self, p: Word) -> &str {
        let id = heap::aux_of(self.heap_get(p, 0)) as usize;
        self.prog.con_names.get(id).map(|s| s.as_str()).unwrap_or("")
    }

    /// Whether a scrutinee (evaluated) matches a literal pattern given as text, by kind.
    pub fn literal_matches(&self, scrut: Word, text: &str, lit_kind: u64) -> bool {
        let h = self.heap_get(scrut, 0);
        match (lit_kind, heap::kind_of(h)) {
            (0, kind::INT) => text.parse::<i64>().ok() == Some(self.heap_get(scrut, 1) as i64),
            (1, kind::CHAR) => text.parse::<u32>().ok() == Some(self.heap_get(scrut, 1) as u32),
            (3, kind::DOUBLE) => text.parse::<u64>().ok() == Some(self.heap_get(scrut, 1)),
            _ => false,
        }
    }

    /// Allocates `words` words (header included), collecting when the heap is full. The caller
    /// keeps the stack state (`sp`, `top`) current, and the record's arguments are roots.
    /// Makes room for `words` more words in one collection at most, so that a run of allocations
    /// that follows cannot collect and move the objects already built in it.
    pub fn reserve(&mut self, words: usize) {
        let bytes = (words as Word) * 8;
        if self.hp + bytes > self.hplim {
            self.collect(words);
        }
    }

    pub fn alloc(&mut self, words: usize) -> Word {
        let bytes = (words.max(2) as Word) * 8;
        if self.hp + bytes > self.hplim {
            self.collect(words);
        }
        let p = self.hp;
        self.hp += bytes;
        p
    }

    /// Collects the heap, with every root the machine holds.
    pub fn collect(&mut self, need: usize) {
        // Roots: the stack (walked from sp), the globals, the record's arguments and registers,
        // the exception, and the stable pointers.
        let mut locs: Vec<*mut Word> = Vec::new();
        self.stack_roots(&mut locs);
        for i in 0..self.prog.globals.len() {
            // SAFETY: the globals table has one slot per global, allocated with the machine.
            locs.push(unsafe { self.globals.add(i) });
        }
        for a in self.args.iter_mut() {
            locs.push(a as *mut Word);
        }
        for saved in self.saved_args.iter_mut() {
            for a in saved.iter_mut() {
                locs.push(a as *mut Word);
            }
        }
        locs.push(&mut self.r1 as *mut Word);
        locs.push(&mut self.exc as *mut Word);
        for s in self.stable.iter_mut() {
            locs.push(s as *mut Word);
        }
        for t in self.runq.iter_mut() {
            locs.push(t as *mut Word);
        }
        for v in self.named.values_mut() {
            locs.push(v as *mut Word);
        }
        // Copy the heap pointers into the collector's root vector and back.
        let mut roots: Vec<Word> = locs.iter().map(|&p| unsafe { *p }).collect();
        self.heap.hp = ((self.hp - self.heap.addr(0)) / 8) as usize;
        gc::collect(&mut self.heap, &mut roots, need);
        for (p, v) in locs.iter().zip(roots) {
            // SAFETY: each location is a live slot the collector was given.
            unsafe { **p = v };
        }
        self.hp = self.heap.addr(self.heap.hp);
        self.hplim = self.heap.addr(self.heap.from.len());
        assert!(self.hp + (need as Word) * 8 <= self.hplim, "heap exhausted");
    }

    /// The bodies of the frames on the stack, innermost first (for diagnostics).
    pub fn stack_bodies(&self) -> Vec<String> {
        let mut out = Vec::new();
        let mut p = self.sp + 8 * self.top;
        for _ in 0..64 {
            let ret = unsafe { *(p as *const Word) };
            if ret == 0 {
                break;
            }
            out.push(self.body_name(ret));
            let fkind = unsafe { *((ret - 16) as *const Word) };
            let size = unsafe { *((ret - 8) as *const Word) };
            p += 8;
            match fkind {
                crate::native::codegen::frame::BODY => p += 8 * size,
                crate::native::codegen::frame::UPDATE => p += 8,
                crate::native::codegen::frame::CATCH => p += 8,
                crate::native::codegen::frame::APPLY => {
                    let n = unsafe { *(p as *const Word) };
                    p += 8 * (1 + n);
                }
                _ => break,
            }
        }
        out
    }

    /// The pointer slots of the Haskell stack: the innermost `top` words, then every frame above
    /// the return addresses (see `native::codegen::frame`). A stop frame links to the outer stack.
    fn stack_roots(&mut self, out: &mut Vec<*mut Word>) {
        use crate::native::codegen::frame;
        let mut p = self.sp;
        let mut top = self.top;
        loop {
            for _ in 0..top {
                out.push(p as *mut Word);
                p += 8;
            }
            let ret = unsafe { *(p as *const Word) };
            if ret == 0 {
                break;
            }
            let fkind = unsafe { *((ret - 16) as *const Word) };
            let size = unsafe { *((ret - 8) as *const Word) };
            p += 8;
            top = 0;
            match fkind {
                frame::STOP => {
                    let outer_sp = unsafe { *(p as *const Word) };
                    let outer_top = unsafe { *((p + 8) as *const Word) };
                    if outer_sp == 0 {
                        break;
                    }
                    p = outer_sp;
                    top = outer_top;
                }
                frame::BODY => {
                    for i in 0..size {
                        out.push((p + 8 * i) as *mut Word);
                    }
                    p += 8 * size;
                }
                frame::UPDATE => {
                    out.push(p as *mut Word);
                    p += 8;
                }
                frame::CATCH => {
                    // The handler; the frame above the return address is that one word.
                    out.push(p as *mut Word);
                    p += 8;
                }
                frame::APPLY => {
                    let n = unsafe { *(p as *const Word) };
                    out.push(p as *mut Word);
                    for i in 0..n {
                        out.push((p + 8 + 8 * i) as *mut Word);
                    }
                    p += 8 * (1 + n);
                }
                other => panic!("internal: unknown frame kind {other} on the stack"),
            }
        }
    }
}

/// The machine of the running thread.
pub fn current() -> *mut Machine {
    CURRENT.with(|c| c.get()) as *mut Machine
}

fn kind_is_whnf(k: u8) -> bool {
    !matches!(k, kind::THUNK | kind::IND | kind::FORWARD)
}

pub fn is_whnf(h: Word) -> bool {
    kind_is_whnf(heap::kind_of(h))
}
