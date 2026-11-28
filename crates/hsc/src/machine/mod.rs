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
/// Words of the heap semispace at start.
const HEAP_WORDS: usize = 1 << 22;
/// Words of the nursery (8 MB, near GHC's 4 MB allocation area: it mostly stays in the cache).
const NURSERY_WORDS: usize = 1 << 20;
/// The nursery grows up to this while many of its objects survive (a program building a large
/// structure), so that it is not copied in small steps; it shrinks back when few survive.
const NURSERY_MAX_WORDS: usize = 1 << 22;
/// Entries of the remembered set's buffer at start (it grows when full).
const REM_WORDS: usize = 1 << 12;
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
    /// The number of the running Haskell thread (generated code marks the thunks it evaluates).
    pub tid: Word,
    /// The nursery's address range `[nursery_lo, nursery_hi)`: an object outside it that is
    /// given a pointer into it is remembered (the write barrier). When the heap is not
    /// generational the range is everything, so the barrier never fires.
    pub nursery_lo: Word,
    pub nursery_hi: Word,
    /// The remembered set's buffer: the next entry, and its end (`rem_buf`).
    pub rem_ptr: Word,
    pub rem_end: Word,
    // Runtime-only state from here on.
    /// The remembered set: old objects that may point into the nursery (roots of a minor
    /// collection), filled from `rem_buf`'s start up to `rem_ptr`.
    pub rem_buf: Vec<Word>,
    pub heap: Heap,
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
    /// The root locations of the last collection (the buffer is reused).
    root_buf: Vec<*mut Word>,
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
        // HSC_HEAP_WORDS: the initial heap (a small one makes collections frequent, for testing).
        let words = std::env::var("HSC_HEAP_WORDS").ok().and_then(|v| v.parse().ok()).unwrap_or(HEAP_WORDS);
        // At least room for the program's global closures and some slack, whatever was asked.
        let words = words.max(4 * prog.nglobals + (1 << 14));
        let mut heap = Heap::new(words);
        // HSC_NURSERY_WORDS: the nursery (0: no nursery, one semispace heap). New objects are
        // allocated there; a minor collection promotes the survivors into the semispace.
        let nursery = std::env::var("HSC_NURSERY_WORDS").ok().and_then(|v| v.parse().ok()).unwrap_or(NURSERY_WORDS);
        let (base, limit) = if nursery > 0 {
            // Room for the global closures (allocated before the first collection).
            let n = nursery.max(4 * prog.nglobals + (1 << 14));
            heap.set_nursery(n, n.max(NURSERY_MAX_WORDS));
            let lo = heap.nursery.as_ptr() as Word;
            (lo, lo + (n as Word) * 8)
        } else {
            let base = heap.addr(0);
            (base, base + (words as Word) * 8)
        };
        let (nursery_lo, nursery_hi) =
            if nursery > 0 { (base, base + (heap.nursery.len() as Word) * 8) } else { (0, Word::MAX) };
        let mut rem_buf: Vec<Word> = vec![0; REM_WORDS];
        let rem_ptr = rem_buf.as_mut_ptr() as Word + 8;
        let rem_end = rem_buf.as_ptr() as Word + (REM_WORDS as Word) * 8;
        let stack = vec![0 as Word; STACK_WORDS];
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
            hp: base,
            hplim: limit,
            sp: 0,
            top: 0,
            globals: std::ptr::null_mut(),
            r1: 0,
            nargs: 0,
            code: 0,
            args: [0; ARG_SLOTS],
            nursery_lo,
            nursery_hi,
            rem_ptr,
            rem_end,
            rem_buf,
            heap,
            stack: Vec::new(),
            exc: 0,
            prog,
            con_ids: std::collections::HashMap::new(),
            stable: vec![0],
            weaks: Vec::new(),
            tid: 1,
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
            root_buf: Vec::new(),
            bodies: entry::DiagnosticBodies::default(),
        });
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

    /// Allocates `words` words without collecting (the heap has room at start-up).
    pub fn alloc_no_gc(&mut self, words: usize) -> Word {
        let p = self.hp;
        self.hp += (words as Word) * 8;
        assert!(self.hp <= self.hplim, "heap exhausted at start-up");
        p
    }

    /// The index of the remembered set's next entry in `rem_buf` (the entries are `1..`; entry 0
    /// is a sentinel, so the barrier can compare an object with the last one recorded).
    pub fn rem_count(&self) -> usize {
        ((self.rem_ptr - self.rem_buf.as_ptr() as Word) / 8) as usize
    }

    /// Records object `obj` in the remembered set (it may hold pointers into the nursery).
    pub fn remember(&mut self, obj: Word) {
        let n = self.rem_count();
        if self.rem_buf[n - 1] == obj {
            return;
        }
        if n == self.rem_buf.len() {
            self.rem_buf.resize(2 * n, 0);
            self.rem_end = self.rem_buf.as_ptr() as Word + (self.rem_buf.len() as Word) * 8;
        }
        self.rem_buf[n] = obj;
        self.rem_ptr = self.rem_buf.as_ptr() as Word + ((n + 1) as Word) * 8;
    }

    /// Writes pointer field `i` of object `obj`, which may be an old object (a mutation, not
    /// the initialisation of a new object): the write barrier remembers it when `v` is young.
    pub fn heap_write(&mut self, obj: Word, i: usize, v: Word) {
        // SAFETY: callers pass live objects and a field index within them.
        unsafe { *((obj as *mut Word).add(i)) = v };
        if (obj < self.nursery_lo || obj >= self.nursery_hi) && v >= self.nursery_lo && v < self.nursery_hi {
            self.remember(obj);
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
    /// arrays are never moved; a collection is due when many bytes were pinned since the last.
    pub fn alloc_pinned_bytes(&mut self, n: usize, align: usize) -> Word {
        let budget = (self.heap.from.len() * 8).max(64 << 20);
        if self.heap.pinned_since_gc > budget {
            self.collect(0);
        }
        let words = n.div_ceil(8);
        self.heap.alloc_pinned(1 + words, align, heap::header(kind::BYTES, words as u32, n as u32))
    }

    /// Collects the heap, with every root the machine holds.
    pub fn collect(&mut self, need: usize) {
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
        // Roots: the stack (walked from sp), the globals, the record's arguments and registers,
        // the exception, and the stable pointers.
        // The buffer is kept between collections: a deep stack gives millions of roots.
        let mut locs = std::mem::take(&mut self.root_buf);
        locs.clear();
        stack_roots(self.sp, self.top, &mut locs);
        for i in 0..self.prog.nglobals {
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
        for (t, v) in self.stm_log.iter_mut() {
            locs.push(t as *mut Word);
            locs.push(v as *mut Word);
        }
        self.coros.roots(&mut locs);
        self.sched.roots(&mut |v: &mut Word| locs.push(v as *mut Word));
        for v in self.named.values_mut() {
            locs.push(v as *mut Word);
        }
        // The collector updates the root locations in place.
        let roots = &locs[..];
        if self.heap.nursery.is_empty() {
            self.heap.hp = ((self.hp - self.heap.addr(0)) / 8) as usize;
            gc::collect_with_selectors(&mut self.heap, roots, need, &self.selectors, (0, 0), (0, 0));
        } else {
            let lo = self.heap.nursery.as_ptr() as Word;
            let used = (lo, self.hp - lo);
            let young = (lo, (self.heap.nursery.len() as Word) * 8);
            let cur = self.heap.surv_cur;
            let from_surv = (self.heap.surv_base(cur), (self.heap.surv_used as Word) * 8);
            let to_surv = (self.heap.surv_base(1 - cur) as *mut Word, self.heap.nursery_max);
            let n = self.rem_count();
            let remembered: Vec<Word> = self.rem_buf[1..n].to_vec();
            let mut still_young = Vec::new();
            let budget = (self.heap.from.len() * 8).max(64 << 20);
            // A request larger than the nursery empties the survivors too (a major collection),
            // so that the young space can be resized below.
            let minor = if self.heap.pinned_since_gc > budget || need > self.heap.nursery_max / 2 {
                None
            } else {
                gc::collect_minor(&mut self.heap, used, from_surv, to_surv, young, roots, &remembered, &mut still_young, &self.selectors)
            };
            match minor {
                Some(surv) => {
                    self.heap.surv_cur = 1 - cur;
                    self.heap.surv_used = surv;
                    let (words, usual) = (self.heap.nursery_words, NURSERY_WORDS.max(4 * self.prog.nglobals + (1 << 14)));
                    if 4 * surv > words && 2 * words <= self.heap.nursery_max {
                        self.heap.nursery_words = 2 * words;
                    } else if 32 * surv < words && words / 2 >= usual {
                        self.heap.nursery_words = words / 2;
                    }
                }
                None => {
                    // Room after it for a whole nursery of promotions (and the request).
                    let reserve = self.heap.nursery_words.max(need) + need;
                    gc::collect_with_selectors(&mut self.heap, roots, reserve, &self.selectors, used, from_surv);
                    self.heap.surv_used = 0;
                    still_young.clear();
                }
            }
            self.rem_ptr = self.rem_buf.as_mut_ptr() as Word + 8;
            for obj in still_young {
                self.remember(obj);
            }
            // The nursery is empty now: resized (survivors are promoted first) for a request
            // larger than it, and back to its usual size once such an object has been promoted.
            let len = self.heap.nursery_max;
            let usual = NURSERY_WORDS.max(4 * self.prog.nglobals + (1 << 14));
            let usual_max = usual.max(NURSERY_MAX_WORDS);
            if (need > len / 2 || (len > usual_max && need <= usual / 2)) && self.heap.surv_used == 0 {
                if need > len / 2 {
                    let want = (2 * need).next_power_of_two();
                    self.heap.set_nursery(want, want);
                } else {
                    self.heap.set_nursery(usual, usual_max);
                }
            }
        }
        // Given back once the stack is much shallower than at its deepest.
        if locs.capacity() > 4 * locs.len().max(1 << 16) {
            locs.shrink_to(2 * locs.len());
        }
        self.root_buf = locs;
        if self.heap.nursery.is_empty() {
            self.hp = self.heap.addr(self.heap.hp);
            self.hplim = self.heap.addr(self.heap.from.len());
        } else {
            if 2 * need > self.heap.nursery_words {
                self.heap.nursery_words = (2 * need).next_power_of_two().min(self.heap.nursery_max);
            }
            self.hp = self.heap.nursery.as_ptr() as Word;
            self.hplim = self.hp + (self.heap.nursery_words as Word) * 8;
            self.nursery_lo = self.hp;
            self.nursery_hi = self.hp + (self.heap.nursery.len() as Word) * 8;
        }
        if std::env::var_os("HSC_GC_CENSUS").is_some() {
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
        while i < self.heap.hp {
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
            while i < self.heap.hp {
                let h = self.heap.from[i];
                let total = heap::total_words(h);
                let (first, count) = heap::pointer_fields(h);
                for f in first..first + count {
                    let v = self.heap.from[i + f];
                    if self.heap.contains(v) {
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
                frame::BODY | frame::BODY_MAP => {
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
                crate::native::codegen::frame::BODY | crate::native::codegen::frame::BODY_MAP => p += 8 * size,
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
        if !(1..=7).contains(&fkind) {
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
            frame::BODY_MAP => {
                let mask = unsafe { *((ret - 24) as *const Word) };
                for i in frame::raw(size)..frame::words(size) {
                    if mask & (1 << i) != 0 {
                        out.push((p + 8 * i) as *mut Word);
                    }
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
