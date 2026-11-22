//! The native machine's runtime: the machine record that generated code runs against, the stack
//! walk the collector uses, and the runtime entry points that generated code calls.
//!
//! The record's first words are read and written by generated code; their offsets are fixed by
//! `native::codegen::mach` (see NATIVE.md).

pub mod conc;
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
/// Default nursery size (1 MiB).
const NURSERY_WORDS: usize = 1 << 17;
/// Initial old-generation semispace (32 MiB, faulted in as it fills). This leaves room for
/// promotions while building large live structures before a major collection becomes necessary.
const OLD_WORDS: usize = 1 << 22;
/// Initial capacity (words) of the remembered-set buffer for old-gen `THUNK -> IND` updates.
const MUT_BUF_WORDS: usize = 4096;
/// Argument slots of the record (primitive operations take their arguments here).
pub const ARG_SLOTS: usize = 16;

/// The machine record. The first fields are the layout generated code uses.
#[repr(C)]
pub struct Machine {
    /// Next free heap address (in the nursery).
    pub hp: Word,
    /// The end of the nursery.
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
    /// The number of the running Haskell thread (generated code marks the thunks it evaluates).
    pub tid: Word,
    /// Start address of the nursery (for the `hsc_upd_ret` generational write barrier).
    pub nursery_base: Word,
    /// Byte length of the nursery (`nursery.len() * 8`).
    pub nursery_span: Word,
    /// Next free slot in `mut_buf` (remembered set for old-gen `THUNK -> IND` updates).
    pub mut_ptr: *mut Word,
    /// End of `mut_buf`.
    pub mut_lim: *mut Word,
    // Runtime-only state from here on.
    pub heap: Heap,
    /// Backing storage for `mut_ptr..mut_lim`.
    pub mut_buf: Vec<Word>,
    /// Reusable buffer of root-slot addresses (avoids allocating a `Vec` on every minor GC).
    root_locs: Vec<*mut Word>,
    /// The Haskell stack: `stack_base..stack_top` words, growing down from the top.
    pub stack: Vec<Word>,
    /// The exception being raised (a pointer), if any.
    pub exc: Word,
    /// The tables of the program.
    pub prog: Program,
    /// Constructor name -> id, lazily populated on first non-RTS lookup.
    pub con_ids: std::collections::HashMap<&'static str, u32>,
    /// Extra roots the runtime keeps (stable pointers and similar), by index.
    pub stable: Vec<Word>,
    /// The weak pointers' states, by the index in a weak object's header (see `prims_misc`).
    pub weaks: Vec<crate::ccall::WeakState>,
    /// The scheduler of the Haskell threads; the values it holds are roots.
    pub sched: crate::sched::Sched<Word>,
    /// The coroutines of the threads that are not running.
    pub coros: conc::Coros,
    /// Number of active primitive arguments in `args` (saved across coroutine switches).
    pub prim_nargs: usize,
    /// The writes of the running transaction: each variable with its contents before.
    pub stm_log: Vec<(Word, Word)>,
    /// The transactions the running thread is in.
    pub stm_depth: u32,
    /// Set when a nested run failed because of `retry#` rather than an exception.
    pub stm_retry: bool,
    /// The per-name pointer slots of the RTS (`getOrSet…`), by name.
    pub named: std::collections::HashMap<String, Word>,
    /// The global closures (the GC roots `globals` points to).
    pub global_vec: Vec<Word>,
    /// The bindings the runtime uses, by name: their index among the globals.
    pub rts_globals: std::collections::HashMap<&'static str, usize>,
    /// The program's name, as the runtime reports errors.
    pub prog_name: String,
    /// The bodies of the program in address order, with their names (for diagnostics).
    pub bodies: entry::DiagnosticBodies,
    /// The argument slots of the primitive operations that are waiting on a nested evaluation.
    pub saved_args: Vec<[Word; ARG_SLOTS]>,
    /// Set when a nested evaluation a primitive made raised: the primitive answers the exception.
    pub prim_failed: bool,
    /// Lazily classified dispatch entry per program-local primitive ID.
    pub(super) prim_dispatch: Vec<PrimDispatch>,
    /// The module that ran each ordinary primitive (0 not yet; see `prims::run_ordinary`).
    pub ordinary_module: Vec<u8>,
    /// How each C call primitive ran the first time (see `prims::run_ccall`).
    pub(super) ccall_plans: Vec<prims::CPlan>,
    /// Selector thunks (`code_addr -> (con_id, field_word)`) built lazily on first GC.
    pub selectors: gc::SelectorTable,
    pub(super) selectors_raw: (*const [u32; 3], usize),
    pub(super) selectors_loaded: bool,
}

/// Cached dispatch target for one program-local primitive ID (classified on first call).
#[derive(Clone, Copy)]
pub(super) enum PrimDispatch {
    Unclassified,
    Conc(conc::ConcOp),
    Int(prims_num::IntOp),
    Integer(prims_integer::IntegerOp),
    Ordinary(&'static str),
    CCall,
}

/// The tables the generated program provides (see `native::codegen`).
/// Name tables are read directly from `.rodata` on demand so startup does zero decoding.
pub struct Program {
    pub nglobals: usize,
    pub con_names: *const [u32; 2],
    pub ncons: usize,
    pub prim_names: *const [u32; 2],
    pub nprims: usize,
}

impl Program {
    #[inline]
    pub fn con_name(&self, i: usize) -> &'static str {
        if i >= self.ncons || self.con_names.is_null() {
            return "";
        }
        // SAFETY: `con_names` points to immutable `.rodata` entries emitted by codegen.
        unsafe { entry::name_at(self.con_names, i) }
    }

    #[inline]
    pub fn prim_name(&self, i: usize) -> &'static str {
        if i >= self.nprims || self.prim_names.is_null() {
            return "";
        }
        // SAFETY: `prim_names` points to immutable `.rodata` entries emitted by codegen.
        unsafe { entry::name_at(self.prim_names, i) }
    }
}

thread_local! {
    /// The machine of the running thread, for runtime entry points that need it without a pointer.
    static CURRENT: StdCell<usize> = const { StdCell::new(0) };
}

impl Machine {
    /// A machine with the program's tables and the global closures allocated.
    pub fn new(prog: Program) -> Box<Machine> {
        // HSC_HEAP_WORDS: the initial old-gen heap (a small one makes collections frequent, for testing).
        let env_heap = std::env::var("HSC_HEAP_WORDS").ok().and_then(|v| v.parse::<usize>().ok());
        let env_nursery = std::env::var("HSC_NURSERY_WORDS").ok().and_then(|v| v.parse::<usize>().ok());
        let nursery_words = match (env_nursery, env_heap) {
            (Some(n), _) => n.max(1024),
            (None, Some(h)) => (h / 4).clamp(1024, NURSERY_WORDS),
            (None, None) => NURSERY_WORDS,
        };
        let old_words = env_heap
            .unwrap_or(OLD_WORDS)
            .max(4 * prog.nglobals + nursery_words.saturating_mul(2));
        let heap = Heap::new_generational(nursery_words, old_words);
        let nursery_base = heap.nursery_addr(0);
        let nursery_span = (heap.nursery.len() as Word) * 8;
        let stack = vec![0 as Word; STACK_WORDS];
        let mut_buf = vec![0 as Word; MUT_BUF_WORDS];
        let nprims = prog.nprims;
        let ccall_plans = vec![prims::CPlan::Unknown; nprims];
        // `prims::con_value` takes the runtime's constructors' ids from their position.
        for (i, c) in crate::native::RTS_CONS.iter().enumerate() {
            assert_eq!(
                (i < prog.ncons).then(|| prog.con_name(i)),
                Some(*c),
                "internal: runtime constructor {c} out of place"
            );
        }
        let mut m = Box::new(Machine {
            hp: nursery_base,
            hplim: nursery_base + nursery_span,
            sp: 0,
            top: 0,
            globals: std::ptr::null_mut(),
            r1: 0,
            nargs: 0,
            code: 0,
            args: [0; ARG_SLOTS],
            tid: 1,
            nursery_base,
            nursery_span,
            mut_ptr: std::ptr::null_mut(),
            mut_lim: std::ptr::null_mut(),
            heap,
            mut_buf,
            root_locs: Vec::with_capacity(256),
            stack: Vec::new(),
            exc: 0,
            prog,
            con_ids: std::collections::HashMap::new(),
            stable: vec![0],
            weaks: Vec::new(),
            sched: crate::sched::Sched::new(conc::SLICE_MS),
            coros: conc::Coros::default(),
            prim_nargs: 0,
            stm_log: Vec::new(),
            stm_depth: 0,
            stm_retry: false,
            named: std::collections::HashMap::new(),
            global_vec: Vec::new(),
            rts_globals: std::collections::HashMap::new(),
            prog_name: std::env::args().next().and_then(|a| std::path::Path::new(&a).file_name().map(|s| s.to_string_lossy().into_owned())).unwrap_or_else(|| "main".into()),
            saved_args: Vec::new(),
            prim_failed: false,
            prim_dispatch: vec![PrimDispatch::Unclassified; nprims],
            ordinary_module: vec![0; nprims],
            ccall_plans,
            selectors: gc::SelectorTable::default(),
            selectors_raw: (std::ptr::null(), 0),
            selectors_loaded: false,
            bodies: entry::DiagnosticBodies::default(),
        });
        m.mut_ptr = m.mut_buf.as_mut_ptr();
        // SAFETY: `mut_buf.len()` is the exact allocated slice length.
        m.mut_lim = unsafe { m.mut_ptr.add(m.mut_buf.len()) };
        m.stack = stack;
        m.sp = m.stack.as_mut_ptr() as Word + (STACK_WORDS as Word) * 8;
        set_current(&mut *m);
        m
    }

    /// Classifies primitive `id` on first use and caches its dispatch target.
    #[inline]
    pub(super) fn classify_prim(&mut self, id: usize) -> PrimDispatch {
        let name = self.prog.prim_name(id);
        let d = if let Some(op) = conc::ConcOp::classify(name) {
            PrimDispatch::Conc(op)
        } else if let Some(op) = prims_num::IntOp::classify(name) {
            PrimDispatch::Int(op)
        } else if let Some(op) = prims_integer::IntegerOp::classify(name) {
            PrimDispatch::Integer(op)
        } else if let Some(ord) = prims::ordinary_name(name) {
            PrimDispatch::Ordinary(ord)
        } else if crate::ccall::parse(name).is_none() {
            PrimDispatch::Ordinary(name)
        } else {
            PrimDispatch::CCall
        };
        self.prim_dispatch[id] = d;
        d
    }

    /// Lazily resolves a non-RTS constructor name to its program ID.
    pub fn lookup_con_id(&mut self, name: &str) -> Option<u32> {
        if self.con_ids.is_empty() && self.prog.ncons > 0 {
            self.con_ids.reserve(self.prog.ncons);
            for i in 0..self.prog.ncons {
                self.con_ids.insert(self.prog.con_name(i), i as u32);
            }
        }
        self.con_ids.get(name).copied()
    }

    /// Allocates `words` words directly in the old generation at start-up (for global closures,
    /// so minor collections can skip scanning `global_vec`).
    pub fn alloc_old_no_gc(&mut self, words: usize) -> Word {
        let at = self.heap.old_hp;
        self.heap.old_hp += words;
        assert!(self.heap.old_hp <= self.heap.from.len(), "old heap exhausted at start-up");
        self.heap.old_addr(at)
    }

    /// Write barrier for boxed `ARRAY` mutations: if `a` is in the old generation and not yet
    /// queued for the next minor GC, marks bit 0 of its header `aux` field and pushes it to
    /// `heap.dirty_arrays`.
    #[inline]
    pub fn record_old_array(&mut self, a: Word) {
        if !self.heap.in_nursery(a) {
            let h = self.heap_get(a, 0);
            if heap::aux_of(h) == 0 {
                self.heap_set(a, h | (1u64 << 32));
                self.heap.dirty_arrays.push(a);
            }
        }
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
        let id = self.lookup_con_id("GHC.Internal.Exception.ErrorCall").unwrap_or(0);
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
        self.prog.con_name(id)
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

    /// Allocates a pinned byte array of `n` bytes, its data aligned to `align` (zeroed). Pinned
    /// arrays are never moved; a major collection is due when many bytes were pinned since the last.
    pub fn alloc_pinned_bytes(&mut self, n: usize, align: usize) -> Word {
        let budget = (self.heap.from.len() * 8).max(64 << 20);
        if self.heap.pinned_since_gc > budget {
            self.collect_major(0);
        }
        let words = n.div_ceil(8);
        self.heap.alloc_pinned(1 + words, align, heap::header(kind::BYTES, words as u32, n as u32))
    }

    /// Collects the heap (minor GC when old-gen has room for the nursery, else major GC).
    pub fn collect(&mut self, need: usize) {
        self.collect_impl(need, false);
    }

    /// Forces a major collection (reclaiming old-gen garbage and unreachable pinned objects).
    pub fn collect_major(&mut self, need: usize) {
        self.collect_impl(need, true);
    }

    fn collect_impl(&mut self, need: usize, force_major: bool) {
        if !self.selectors_loaded {
            self.selectors_loaded = true;
            let (raw, count) = self.selectors_raw;
            if !raw.is_null() && count > 0 {
                let sel_base = raw as usize;
                let entries: Vec<(Word, u32, u32)> = (0..count)
                    .map(|i| {
                        // SAFETY: `selectors_raw` points to immutable `.rodata` emitted by codegen.
                        let [code_off, con_id, field_word] = unsafe { *raw.add(i) };
                        let code = sel_base.wrapping_add_signed(code_off as i32 as isize) as Word;
                        (code, con_id, field_word)
                    })
                    .collect();
                self.selectors = gc::SelectorTable::new(&entries);
            }
        }
        let nursery_used = ((self.hp - self.nursery_base) / 8) as usize;
        self.heap.hp = nursery_used;
        // SAFETY: `mut_ptr` always lies within `mut_buf[..=mut_buf.len()]`.
        let mut_used = unsafe { self.mut_ptr.offset_from(self.mut_buf.as_mut_ptr()) as usize };
        let pinned_budget = (self.heap.from.len() * 8).max(64 << 20);
        let need_major = force_major
            || self.heap.needs_major()
            || self.heap.pinned_since_gc > pinned_budget;

        // Reuse `root_locs` across collections to avoid allocating a new root buffer each time.
        let mut locs = std::mem::take(&mut self.root_locs);
        locs.clear();
        stack_roots(self.sp, self.top, &mut locs);
        if need_major {
            // Startup globals live in old-gen; minor GC skips them (old CAF updates are in `mut_buf`).
            for i in 0..self.prog.nglobals {
                // SAFETY: the globals table has one slot per global, allocated with the machine.
                locs.push(unsafe { self.globals.add(i) });
            }
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
        for (t, v) in self.stm_log.iter_mut() {
            locs.push(t as *mut Word);
            locs.push(v as *mut Word);
        }
        self.coros.roots(&mut locs);
        self.sched.roots(&mut |v: &mut Word| locs.push(v as *mut Word));
        for v in self.named.values_mut() {
            locs.push(v as *mut Word);
        }

        // SAFETY: the root slots above remain live and disjoint from the heap during collection.
        unsafe {
            if need_major {
                gc::collect_major(&mut self.heap, &locs, need, &self.selectors);
            } else {
                gc::collect_minor(&mut self.heap, &locs, &self.mut_buf[..mut_used], need, &self.selectors);
            }
        }
        self.root_locs = locs;
        self.mut_ptr = self.mut_buf.as_mut_ptr();
        self.nursery_base = self.heap.nursery_addr(0);
        self.nursery_span = (self.heap.nursery.len() as Word) * 8;
        self.hp = self.nursery_base;
        self.hplim = self.nursery_base + self.nursery_span;
        static CENSUS: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        if *CENSUS.get_or_init(|| std::env::var_os("HSC_GC_CENSUS").is_some()) {
            self.census();
        }
        assert!(self.hp + (need as Word) * 8 <= self.hplim, "heap exhausted");
    }

    /// HSC_GC_CENSUS: the live objects by kind, and the functions, thunks and constructors by
    /// code or name (printed after each collection, for finding space leaks).
    fn census(&self) {
        let mut kinds: std::collections::HashMap<u8, (usize, usize)> = std::collections::HashMap::new();
        let mut codes: std::collections::HashMap<String, (usize, usize)> = std::collections::HashMap::new();
        let mut i = 0;
        while i < self.heap.old_hp {
            let h = self.heap.from[i];
            let total = heap::total_words(h);
            let e = kinds.entry(heap::kind_of(h)).or_default();
            e.0 += 1;
            e.1 += total;
            let key = match heap::kind_of(h) {
                kind::FUN | kind::THUNK => format!("{} {}", if heap::kind_of(h) == kind::FUN { "fun" } else { "thunk" }, self.body_name(self.heap.from[i + 1])),
                kind::CON => format!("con {}", self.con_name(self.heap.addr(i))),
                kind::PAP => format!("pap of {} args", heap::size_of(h).saturating_sub(1)),
                _ => String::new(),
            };
            if !key.is_empty() {
                let c = codes.entry(key).or_default();
                c.0 += 1;
                c.1 += total;
            }
            i += total;
        }
        // HSC_GC_RETAINER=<name>: the chain of objects that retain one object of that name.
        if let Some(want) = std::env::var_os("HSC_GC_RETAINER").map(|v| v.to_string_lossy().into_owned()) {
            let name_of = |i: usize| -> String {
                let h = self.heap.from[i];
                match heap::kind_of(h) {
                    kind::FUN | kind::THUNK => format!("{} {}", if heap::kind_of(h) == kind::FUN { "fun" } else { "thunk" }, self.body_name(self.heap.from[i + 1])),
                    kind::CON => format!("con {}", self.con_name(self.heap.addr(i))),
                    k => format!("kind {k}"),
                }
            };
            let mut parent: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
            let mut target = None;
            let mut i = 0;
            while i < self.heap.old_hp {
                let h = self.heap.from[i];
                let total = heap::total_words(h);
                let (first, count) = heap::pointer_fields(h);
                for f in first..first + count {
                    let v = self.heap.from[i + f];
                    if self.heap.in_old(v) {
                        parent.entry(self.heap.index(v)).or_insert(i);
                    }
                }
                if target.is_none() && name_of(i).contains(&want) {
                    target = Some(i);
                }
                i += total;
            }
            let mut cur = target;
            let mut seen = std::collections::HashSet::new();
            let mut chain = Vec::new();
            while let Some(c) = cur {
                if !seen.insert(c) {
                    chain.push(format!("(cycle back to {})", name_of(c)));
                    break;
                }
                chain.push(name_of(c));
                cur = parent.get(&c).copied();
            }
            eprintln!("  retainer chain of {} objects; its first 12 and last 25:", chain.len());
            for c in chain.iter().take(12) {
                eprintln!("  retained by: {c}");
            }
            eprintln!("  ...");
            for c in chain.iter().skip(chain.len().saturating_sub(25)) {
                eprintln!("  retained by: {c}");
            }
        }
        let mut ks: Vec<_> = kinds.into_iter().collect();
        ks.sort_by_key(|(_, (_, w))| std::cmp::Reverse(*w));
        for (k, (n, w)) in ks {
            eprintln!("  kind {k}: {n} objects, {w} words");
        }
        let mut cs: Vec<_> = codes.into_iter().collect();
        cs.sort_by_key(|(_, (_, w))| std::cmp::Reverse(*w));
        for (c, (n, w)) in cs.into_iter().take(20) {
            eprintln!("  {c}: {n} objects, {w} words");
        }
        let stack_top = self.stack.as_ptr() as Word + (self.stack.len() as Word) * 8;
        eprintln!("  stack: {} words", stack_top.saturating_sub(self.sp) / 8);
        // The frames on the stack, by kind and body.
        let mut frames: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        let mut p = self.sp + 8 * crate::native::codegen::frame::words(self.top);
        loop {
            let ret = unsafe { *(p as *const Word) };
            if ret == 0 {
                break;
            }
            let fkind = unsafe { *((ret - 16) as *const Word) };
            let size = crate::native::codegen::frame::words(unsafe { *((ret - 8) as *const Word) });
            p += 8;
            use crate::native::codegen::frame;
            let what = match fkind {
                frame::BODY => {
                    p += 8 * size;
                    "body"
                }
                frame::UPDATE => {
                    let t = unsafe { *(p as *const Word) };
                    p += 8;
                    let h = unsafe { *(t as *const Word) };
                    let name = if heap::kind_of(h) == kind::THUNK {
                        self.body_name(unsafe { *((t + 8) as *const Word) })
                    } else {
                        format!("kind {}", heap::kind_of(h))
                    };
                    *frames.entry(format!("update of {name}")).or_default() += 1;
                    continue;
                }
                frame::CATCH | frame::MASK => {
                    p += 8;
                    "catch/mask"
                }
                frame::APPLY => {
                    let n = unsafe { *(p as *const Word) };
                    p += 8 * (1 + n);
                    "apply"
                }
                _ => break,
            };
            *frames.entry(format!("{what} {}", self.body_name(ret))).or_default() += 1;
        }
        let mut fs: Vec<_> = frames.into_iter().collect();
        fs.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
        for (f, n) in fs.into_iter().take(12) {
            eprintln!("  frame {f}: {n}");
        }
    }

    /// The bodies of the frames on the stack, innermost first (for diagnostics).
    pub fn stack_bodies(&self) -> Vec<String> {
        let mut out = Vec::new();
        let mut p = self.sp + 8 * crate::native::codegen::frame::words(self.top);
        for _ in 0..64 {
            let ret = unsafe { *(p as *const Word) };
            if ret == 0 {
                break;
            }
            out.push(self.body_name(ret));
            let fkind = unsafe { *((ret - 16) as *const Word) };
            let size = crate::native::codegen::frame::words(unsafe { *((ret - 8) as *const Word) });
            p += 8;
            match fkind {
                crate::native::codegen::frame::BODY => p += 8 * size,
                crate::native::codegen::frame::UPDATE => p += 8,
                crate::native::codegen::frame::CATCH | crate::native::codegen::frame::MASK => p += 8,
                crate::native::codegen::frame::APPLY => {
                    let n = unsafe { *(p as *const Word) };
                    p += 8 * (1 + n);
                }
                _ => break,
            }
        }
        out
    }
}

/// The pointer slots of the Haskell stack: the innermost `top` words, then every frame above
/// the return addresses (see `native::codegen::frame`). A stop frame links to the outer stack.
pub fn stack_roots(sp: Word, top: Word, out: &mut Vec<*mut Word>) {
    use crate::native::codegen::frame;
    let mut p = sp;
    let mut top = top;
    // The last frames walked (return address, kind, size, position), for a report of where a
    // frame went wrong: a ring of a few, since stacks can be millions of frames deep.
    const TRAIL: usize = 32;
    let mut trail = [(0 as Word, 0 as Word, 0 as Word, 0 as Word); TRAIL];
    let mut walked = 0usize;
    let first_top = top;
    loop {
        // The raw words at the low end of the top region are not pointers.
        for i in 0..frame::words(top) {
            if i >= frame::raw(top) {
                out.push(p as *mut Word);
            }
            p += 8;
        }
        let ret = unsafe { *(p as *const Word) };
        if ret == 0 {
            break;
        }
        let fkind = unsafe { *((ret - 16) as *const Word) };
        let size = unsafe { *((ret - 8) as *const Word) };
        if !(1..=6).contains(&fkind) {
            eprintln!("hsc: bad frame at {p:#x} (sp {sp:#x}, top {first_top}): word {ret:#x} (stack_roots is at {:#x}); the last frames before it, innermost first:", stack_roots as usize);
            for i in walked.saturating_sub(TRAIL)..walked {
                let (r, k, n, at) = trail[i % TRAIL];
                eprintln!("  ret {r:#x} kind {k} size {n} at {at:#x}");
            }
        }
        trail[walked % TRAIL] = (ret, fkind, size, p);
        walked += 1;
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
                for i in frame::raw(size)..frame::words(size) {
                    out.push((p + 8 * i) as *mut Word);
                }
                p += 8 * frame::words(size);
            }
            frame::UPDATE => {
                out.push(p as *mut Word);
                p += 8;
            }
            frame::MASK => {
                // The saved masking state: not a pointer.
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

/// Makes a machine the one of the running OS thread.
pub fn set_current(m: *mut Machine) {
    CURRENT.with(|c| c.set(m as usize));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native::codegen::mach;

    #[test]
    fn machine_field_offsets_match_codegen() {
        assert_eq!(std::mem::offset_of!(Machine, hp) as i64, mach::HP);
        assert_eq!(std::mem::offset_of!(Machine, hplim) as i64, mach::HPLIM);
        assert_eq!(std::mem::offset_of!(Machine, sp) as i64, mach::SP);
        assert_eq!(std::mem::offset_of!(Machine, top) as i64, mach::TOP);
        assert_eq!(std::mem::offset_of!(Machine, globals) as i64, mach::GLOBALS);
        assert_eq!(std::mem::offset_of!(Machine, r1) as i64, mach::R1);
        assert_eq!(std::mem::offset_of!(Machine, nargs) as i64, mach::NARGS);
        assert_eq!(std::mem::offset_of!(Machine, code) as i64, mach::CODE);
        assert_eq!(std::mem::offset_of!(Machine, args) as i64, mach::ARGS);
        assert_eq!(std::mem::offset_of!(Machine, tid) as i64, mach::TID);
        assert_eq!(std::mem::offset_of!(Machine, nursery_base) as i64, mach::NURSERY_BASE);
        assert_eq!(std::mem::offset_of!(Machine, nursery_span) as i64, mach::NURSERY_SPAN);
        assert_eq!(std::mem::offset_of!(Machine, mut_ptr) as i64, mach::MUT_PTR);
        assert_eq!(std::mem::offset_of!(Machine, mut_lim) as i64, mach::MUT_LIM);
    }
}
