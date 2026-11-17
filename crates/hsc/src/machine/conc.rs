//! Concurrency in the native runtime: Haskell threads, `MVar#`, masking, asynchronous exceptions
//! and transactions over `TVar#`, on the shared scheduler (`crate::sched`).
//!
//! Every Haskell thread is a coroutine on the one OS thread that runs Haskell code, with a C stack
//! and a Haskell stack of its own; the heap, the globals and the machine record are shared. A
//! thread switches only inside the runtime (a primitive, the wait for a thunk another thread
//! evaluates): its registers in the record are saved in its context, the C stacks are swapped
//! (`hsc_coro_swap`, a few instructions instead of an OS thread's sleep and wake-up), and the
//! registers are put back when it runs again; while it waits the collector finds its roots through
//! that context. A safe foreign call runs on a worker OS thread while the caller is blocked, so
//! that the other threads go on (as GHC's runtime does).
//!
//! A transaction runs without giving way to other threads, writing its variables in place and
//! keeping the old contents in a log: `retry#` and exceptions undo the writes.

use super::heap::{self, kind, Word};
use super::prims::{force_arg, int_of, int_value};
use super::prims_mem::{state_pair, unit};
use super::{Machine, ARG_SLOTS};
use crate::sched::{Interrupt, Thrown, Tid, WaitKind, PREEMPT};
use std::sync::atomic::Ordering;

extern "C" {
    fn hsc_enter(m: *mut Machine, closure: Word) -> i32;
    fn hsc_stop_ret();
    fn hsc_mask_ret();
}

// The switch between coroutines: `hsc_coro_swap(save, to)` keeps the callee-saved registers on
// the current stack, stores its pointer in `*save` and continues on the stack `to` (saved the same
// way). A new coroutine's stack starts with registers whose return goes to `hsc_coro_start`, which
// calls the function in r13 with the argument in r12.
std::arch::global_asm!(
    ".text",
    ".p2align 4",
    ".globl hsc_coro_swap",
    "hsc_coro_swap:",
    "push rbp",
    "push rbx",
    "push r12",
    "push r13",
    "push r14",
    "push r15",
    "mov [rdi], rsp",
    "mov rsp, rsi",
    "pop r15",
    "pop r14",
    "pop r13",
    "pop r12",
    "pop rbx",
    "pop rbp",
    "ret",
    ".p2align 4",
    ".globl hsc_coro_start",
    "hsc_coro_start:",
    "mov rdi, r12",
    "call r13",
    "ud2",
);

extern "C" {
    fn hsc_coro_swap(save: *mut usize, to: usize);
    fn hsc_coro_start();
    fn mmap(addr: *mut u8, len: usize, prot: i32, flags: i32, fd: i32, off: i64) -> *mut u8;
    fn munmap(addr: *mut u8, len: usize) -> i32;
    fn mprotect(addr: *mut u8, len: usize, prot: i32) -> i32;
    fn __errno_location() -> *mut i32;
    fn pthread_self() -> u64;
}

/// The C stack of a coroutine (reserved; the kernel commits it on use), with a guard page.
pub struct CStack {
    base: *mut u8,
}

impl CStack {
    fn new() -> Option<CStack> {
        const PROT_RW: i32 = 3;
        const FLAGS: i32 = 0x02 | 0x20 | 0x4000 | 0x20000; // private, anonymous, no reserve, stack
        // SAFETY: a fresh anonymous mapping.
        let base = unsafe { mmap(std::ptr::null_mut(), FORK_C_STACK, PROT_RW, FLAGS, -1, 0) };
        if base.is_null() || base as isize == -1 {
            return None;
        }
        // SAFETY: the lowest page of the mapping becomes the guard.
        unsafe { mprotect(base, 4096, 0) };
        Some(CStack { base })
    }

    fn top(&self) -> usize {
        self.base as usize + FORK_C_STACK
    }
}

impl Drop for CStack {
    fn drop(&mut self) {
        // SAFETY: the mapping made in `new`, no longer in use.
        unsafe { munmap(self.base, FORK_C_STACK) };
    }
}

/// A coroutine's saved stack pointer (boxed: `hsc_coro_swap` writes it while the table changes).
pub struct Coro {
    sp: usize,
    stack: Option<CStack>,
}

/// The coroutines of the threads that are not running, and the stacks of finished ones (freed
/// by the next thread to run: a stack cannot be freed while it is in use).
#[derive(Default)]
pub struct Coros {
    map: crate::sched::IntMap<Tid, Box<Coro>>,
    dead: Vec<CStack>,
}

/// What a new coroutine starts with.
struct Start {
    mp: *mut Machine,
    id: Tid,
    slot: usize,
}

/// Continues on thread `next`'s coroutine; returns when this one (`me`) runs again.
///
/// # Safety
/// The caller holds no reference into the machine; `next` was suspended by a switch or is new.
unsafe fn swap_to(mp: *mut Machine, me: Tid, next: Tid) {
    let coros = &mut (*mp).coros;
    let to = match coros.map.get(&next) {
        Some(c) => c.sp,
        None => panic!("internal: thread {next} has no coroutine"),
    };
    let mine: *mut usize = &mut coros.map.entry(me).or_insert_with(|| Box::new(Coro { sp: 0, stack: None })).sp;
    hsc_coro_swap(mine, to);
    (*mp).coros.dead.clear();
}

/// The body of a forked thread's coroutine: it runs the action and ends the thread.
extern "C" fn coro_main(arg: usize) {
    // SAFETY: `fork` made the argument; this coroutine runs with the machine to itself.
    unsafe {
        let Start { mp, id, slot } = *Box::from_raw(arg as *mut Start);
        (*mp).coros.dead.clear();
        let mut stack: Vec<Word> = vec![0; FORK_STACK_WORDS];
        {
            let m = &mut *mp;
            let action = std::mem::replace(&mut m.stable[slot], 0);
            m.tid = id;
            m.saved_args.clear();
            m.stm_log.clear();
            m.stm_depth = 0;
            m.stm_retry = false;
            m.exc = 0;
            m.prim_failed = false;
            m.top = 0;
            // [state token][stop frame with no outer stack]
            let end = stack.as_mut_ptr() as Word + (FORK_STACK_WORDS as Word) * 8;
            let a = end - 24;
            m.stack_store(a, hsc_stop_ret as usize as Word);
            m.stack_store(a + 8, 0);
            m.stack_store(a + 16, 0);
            m.stack_store(a - 8, 0);
            m.sp = a - 8;
            m.nargs = 1;
            // A thread killed before it first runs does nothing.
            if m.sched.interrupt().is_none() {
                // An exception the thread does not handle ends it (forkIO's handler reports it).
                let _ = hsc_enter(mp, action);
            }
        }
        drop(stack);
        let next = (*mp).sched.finish_pick();
        // This coroutine is done: its stack goes once the next one runs.
        let coros = &mut (*mp).coros;
        if let Some(mut c) = coros.map.remove(&id) {
            if let Some(st) = c.stack.take() {
                coros.dead.push(st);
            }
        }
        let to = match coros.map.get(&next) {
            Some(c) => c.sp,
            None => panic!("internal: thread {next} has no coroutine"),
        };
        let mut gone = 0usize;
        hsc_coro_swap(&mut gone, to);
        unreachable!("a finished thread ran again");
    }
}

/// A job for a worker: a function of two words, and where to say it is done.
struct Job {
    run: unsafe fn(usize, usize),
    a: usize,
    b: usize,
    tid: Tid,
    errno: *mut i32,
    returned: std::sync::Arc<(std::sync::Mutex<Vec<Tid>>, std::sync::Condvar)>,
}

// SAFETY: the words point into the stack of a coroutine blocked until the job is done.
unsafe impl Send for Job {}

/// An idle worker OS thread for safe foreign calls.
#[derive(Clone)]
struct Worker {
    jobs: std::sync::mpsc::Sender<Job>,
    os_thread: u64,
}

static WORKERS: std::sync::Mutex<Vec<Worker>> = std::sync::Mutex::new(Vec::new());

/// An idle worker, or a new one.
fn worker() -> Option<Worker> {
    if let Some(w) = WORKERS.lock().unwrap_or_else(|e| e.into_inner()).pop() {
        return Some(w);
    }
    let (tx, rx) = std::sync::mpsc::channel::<Job>();
    let (idt, idr) = std::sync::mpsc::channel::<u64>();
    let me = tx.clone();
    std::thread::Builder::new()
        .name("hsc-foreign".into())
        .stack_size(FORK_C_STACK)
        .spawn(move || {
            // SAFETY: no arguments.
            let os_thread = unsafe { pthread_self() };
            let _ = idt.send(os_thread);
            let back = Worker { jobs: me, os_thread };
            while let Ok(job) = rx.recv() {
                // SAFETY: the caller is blocked until the job is done (see `foreign_call`).
                unsafe {
                    (job.run)(job.a, job.b);
                    *job.errno = *__errno_location();
                }
                WORKERS.lock().unwrap_or_else(|e| e.into_inner()).push(back.clone());
                let (list, cv) = &*job.returned;
                list.lock().unwrap_or_else(|e| e.into_inner()).push(job.tid);
                cv.notify_all();
            }
        })
        .ok()?;
    let os_thread = idr.recv().ok()?;
    Some(Worker { jobs: tx, os_thread })
}

/// Runs the call in `*a` (an `Option<F>`) and leaves its result in `*b` (an `Option<R>`).
unsafe fn run_call<F: FnOnce() -> R, R>(a: usize, b: usize) {
    let f = (*(a as *mut Option<F>)).take().expect("internal: a foreign call ran twice");
    *(b as *mut Option<R>) = Some(f());
}

/// Words of the Haskell stack of a forked thread (reserved; the kernel commits it on use).
const FORK_STACK_WORDS: usize = 1 << 26;
/// The C stack of a forked thread: nested evaluations of the runtime use it.
const FORK_C_STACK: usize = 512 << 20;
/// The time slice of the native threads, in milliseconds.
pub const SLICE_MS: u64 = 20;

/// The registers of a thread that waits for its turn.
pub struct Ctx {
    sp: Word,
    top: Word,
    r1: Word,
    nargs: Word,
    code: Word,
    args: [Word; ARG_SLOTS],
    exc: Word,
    saved_args: Vec<[Word; ARG_SLOTS]>,
    prim_failed: bool,
    stm_log: Vec<(Word, Word)>,
    stm_depth: u32,
    stm_retry: bool,
}

impl Ctx {
    fn save(m: &mut Machine) -> Ctx {
        Ctx {
            sp: m.sp,
            top: m.top,
            r1: m.r1,
            nargs: m.nargs,
            code: m.code,
            args: m.args,
            exc: m.exc,
            saved_args: std::mem::take(&mut m.saved_args),
            prim_failed: m.prim_failed,
            stm_log: std::mem::take(&mut m.stm_log),
            stm_depth: m.stm_depth,
            stm_retry: m.stm_retry,
        }
    }

    fn restore(self, m: &mut Machine, tid: Tid) {
        m.sp = self.sp;
        m.top = self.top;
        m.r1 = self.r1;
        m.nargs = self.nargs;
        m.code = self.code;
        m.args = self.args;
        m.exc = self.exc;
        m.saved_args = self.saved_args;
        m.prim_failed = self.prim_failed;
        m.stm_log = self.stm_log;
        m.stm_depth = self.stm_depth;
        m.stm_retry = self.stm_retry;
        m.tid = tid;
    }

    /// The heap pointers of the context, for the collector.
    pub fn roots(&mut self, out: &mut Vec<*mut Word>) {
        super::stack_roots(self.sp, self.top, out);
        for a in self.args.iter_mut() {
            out.push(a);
        }
        for saved in self.saved_args.iter_mut() {
            for a in saved.iter_mut() {
                out.push(a);
            }
        }
        out.push(&mut self.r1);
        out.push(&mut self.exc);
        for (t, v) in self.stm_log.iter_mut() {
            out.push(t);
            out.push(v);
        }
    }
}

/// Gives the turn to the next thread (the running one has been queued or blocked in the
/// scheduler) and returns when this one has it again, with its registers back.
///
/// The machine is only reached through the pointer: other threads change it meanwhile.
#[inline(never)]
/// Runs a safe foreign call: when other threads exist, on a worker OS thread while this one is
/// blocked and the others run (errno comes back with the result).
pub fn foreign_call<R>(m: &mut Machine, interruptible: bool, call: impl FnOnce() -> R) -> R {
    if !m.sched.multi() {
        return call();
    }
    let Some(w) = worker() else { return call() };
    let mp: *mut Machine = m;
    let mut call = Some(call);
    let mut result: Option<R> = None;
    let mut errno: i32 = 0;
    // SAFETY: the job's words point at `call` and `result`, which live until this thread runs
    // again: it is blocked until the worker has finished with them.
    unsafe {
        let returned = (*mp).sched.foreign_begin(interruptible, w.os_thread);
        let job = Job {
            run: run_call_of(&call, &result),
            a: &mut call as *mut _ as usize,
            b: &mut result as *mut _ as usize,
            tid: (*mp).sched.current(),
            errno: &mut errno,
            returned,
        };
        if w.jobs.send(job).is_err() {
            panic!("internal: a foreign-call worker is gone");
        }
        switch(mp);
        *__errno_location() = errno;
    }
    result.expect("internal: a foreign call came back without its result")
}

/// `run_call` for the types of these two places.
fn run_call_of<F: FnOnce() -> R, R>(_: &Option<F>, _: &Option<R>) -> unsafe fn(usize, usize) {
    run_call::<F, R>
}

/// Gives the turn to the next thread (the running one has been queued or blocked in the
/// scheduler) and returns when this one has it again, with its registers back.
///
/// The machine is only reached through the pointer: other threads change it meanwhile.
#[inline(never)]
pub fn switch(mp: *mut Machine) {
    // SAFETY: the caller holds the baton; the machine outlives every thread.
    unsafe {
        loop {
            start_signal_handlers(&mut *mp);
            let me = (*mp).sched.current();
            let ctx = Ctx::save(&mut *mp);
            (*mp).suspended.insert(me, ctx);
            let next = (*mp).sched.pick_next();
            if next != me {
                swap_to(mp, me, next);
            }
            if let Some(ctx) = (*mp).suspended.remove(&me) {
                ctx.restore(&mut *mp, me);
            }
            if !(*mp).sched.take_signal_wake() {
                break;
            }
        }
    }
}

/// Signals have arrived: a thread runs their Haskell handlers (`hscRunPendingSignals`).
pub fn start_signal_handlers(m: &mut Machine) {
    if !crate::signals::pending() {
        return;
    }
    crate::signals::taken();
    let Some(g) = m.rts_global("GHC.Internal.Conc.Signal.hscRunPendingSignals") else { return };
    if let Some(action) = m.force(g) {
        fork(m, action);
    }
}

/// The exception for what the scheduler tells this thread to raise.
fn interrupt_value(m: &mut Machine, i: Interrupt<Word>) -> Word {
    let library = |m: &mut Machine, name: &str, text: &str| {
        if let Some(g) = m.rts_global(name) {
            if let Some(v) = m.force(g) {
                m.prim_failed = false;
                return v;
            }
        }
        m.error_value(text)
    };
    match i {
        Interrupt::Exception(e) => e,
        Interrupt::BlockedOnMVar => library(
            m,
            "GHC.Internal.IO.Exception.blockedIndefinitelyOnMVar",
            "thread blocked indefinitely in an MVar operation",
        ),
        Interrupt::BlockedOnSTM => library(
            m,
            "GHC.Internal.IO.Exception.blockedIndefinitelyOnSTM",
            "thread blocked indefinitely in an STM transaction",
        ),
    }
}

/// Raises in `exc` what waits for this thread, if anything; answers whether it did.
pub fn check(m: &mut Machine) -> bool {
    match m.sched.interrupt() {
        Some(i) => {
            m.exc = interrupt_value(m, i);
            true
        }
        None => false,
    }
}

/// The preemption point at the start of every primitive: at the end of a time slice another
/// thread runs. Answers true when an exception for this thread is raised (in `exc`) instead.
pub fn safe_point(mp: *mut Machine) -> bool {
    if !PREEMPT.load(Ordering::Relaxed) {
        return false;
    }
    // SAFETY: the caller passes the machine; no reference to it is held across `switch`.
    unsafe {
        if (*mp).stm_depth > 0 {
            return false;
        }
        PREEMPT.store(false, Ordering::Relaxed);
        start_signal_handlers(&mut *mp);
        if (*mp).sched.preempt() {
            switch(mp);
        }
        check(&mut *mp)
    }
}

/// A thunk under evaluation was entered (its header's aux is the evaluating thread). The same
/// thread: an infinite loop. Another thread: this one waits until the thunk is updated. Returns 0
/// to evaluate `r1` again, 1 with the exception in `exc`.
#[no_mangle]
pub extern "C" fn hsc_blackhole(mp: *mut Machine) -> i32 {
    loop {
        // SAFETY: generated code passes the machine; the reference ends before `switch`.
        let (owner, me) = unsafe {
            let m = &mut *mp;
            let h = m.heap_get(m.r1, 0);
            if heap::kind_of(h) != kind::THUNK || heap::aux_of(h) == 0 {
                return 0;
            }
            (heap::aux_of(h) as Word, m.tid)
        };
        if owner == me {
            // SAFETY: as above.
            let m = unsafe { &mut *mp };
            m.exc = m.error_value("<<loop>>");
            return 1;
        }
        // SAFETY: as above.
        unsafe { (*mp).sched.yield_cur() };
        switch(mp);
        // SAFETY: as above.
        if check(unsafe { &mut *mp }) {
            return 1;
        }
    }
}

/// Runs `f` on the state token with a masking state, and puts the state back afterwards (an
/// exception that waited for the thread to unmask is raised at once).
fn with_mask(m: &mut Machine, state: u8) -> Option<Word> {
    let old = m.sched.mask();
    let f = m.args[0];
    if old == state {
        return m.apply_nested(f, &[0]);
    }
    m.sched.set_mask(state);
    if state == 0 && check(m) {
        m.sched.set_mask(old);
        return Some(0);
    }
    let r = m.apply_nested(f, &[0]);
    m.sched.set_mask(old);
    if old == 0 && r.is_some() && check(m) {
        return Some(0);
    }
    r
}

/// The return of a mask frame `[hsc_mask_ret][saved state]` (pushed under a catch handler that
/// runs masked): the state of the `catch#` is back. Returns 1 when an exception that waited for
/// the thread to unmask is raised (in `exc`), else 0 to return `r1` to the frame above.
#[no_mangle]
pub extern "C" fn hsc_mask_restore(mp: *mut Machine) -> i32 {
    // SAFETY: generated code passes the machine, with sp at the frame.
    let m = unsafe { &mut *mp };
    let saved = m.stack_load(m.sp + 8) as u8;
    m.sp += 16;
    m.top = 0;
    m.sched.set_mask(saved);
    (saved == 0 && check(m)) as i32
}

/// Unwinding reached a catch frame: the handler runs masked. When the thread was unmasked, a mask
/// frame takes the catch frame's place and puts the state back when the handler returns; answers
/// whether it did (the handler's arguments then go below it).
pub fn mask_for_handler(m: &mut Machine, frame_at: Word) -> bool {
    if m.sched.mask() != 0 {
        return false;
    }
    m.sched.set_mask(2);
    m.stack_store(frame_at, hsc_mask_ret as usize as Word);
    m.stack_store(frame_at + 8, 0);
    true
}

/// The mask frame's saved state, when unwinding passes it.
pub fn unwind_mask(m: &mut Machine, saved: Word) {
    m.sched.set_mask(saved as u8);
}

/// Starts a Haskell thread: a coroutine that runs when the scheduler gives it the turn.
fn fork(m: &mut Machine, action: Word) -> Word {
    let Some(stack) = CStack::new() else {
        m.exc = m.error_value("cannot create a thread");
        return 0;
    };
    let (id, _, _) = m.sched.spawn();
    // The action waits in the stable table until the thread starts (it is a root there).
    m.stable.push(action);
    let slot = m.stable.len() - 1;
    let start = Box::into_raw(Box::new(Start { mp: m, id, slot })) as usize;
    // The registers `hsc_coro_swap` takes back first: r15, r14, r13 (the function), r12 (its
    // argument), rbx, rbp, then the return to `hsc_coro_start`, which finds the stack aligned.
    let sp = (stack.top() & !15) - 72;
    let words = [0, 0, coro_main as usize, start, 0, 0, hsc_coro_start as usize];
    for (i, w) in words.iter().enumerate() {
        // SAFETY: the top of the new stack, below its end.
        unsafe { *((sp + 8 * i) as *mut usize) = *w };
    }
    m.coros.map.insert(id, Box::new(Coro { sp, stack: Some(stack) }));
    let t = int_value(m, id as i64);
    state_pair(m, t)
}

/// The key of an `MVar` object (its header's aux).
fn key_of(m: &Machine, mv: Word) -> u64 {
    heap::aux_of(m.heap_get(mv, 0)) as u64
}

/// Takes the contents of a full `MVar` (argument 0); the first thread waiting to put, if any, puts.
fn take_full(m: &mut Machine) -> Option<Word> {
    let mv = m.args[0];
    let v = m.heap_get(mv, 1);
    if v == 0 {
        return None;
    }
    let key = key_of(m, mv);
    m.heap_set(mv + 8, 0);
    if m.sched.front_kind(key) == Some(WaitKind::Putter) {
        if let Some(w) = m.sched.pop_waiter(key) {
            m.heap_set(mv + 8, w.value.unwrap_or(0));
            m.sched.wake(w.thread, None);
        }
    }
    Some(v)
}

/// Puts into an empty `MVar` (argument 0): the waiting readers see the value, the first taker takes it.
fn put_empty(m: &mut Machine, v: Word) {
    let mv = m.args[0];
    let key = key_of(m, mv);
    loop {
        match m.sched.pop_waiter(key) {
            Some(w) if w.kind == WaitKind::Reader => m.sched.wake(w.thread, Some(v)),
            Some(w) => {
                m.sched.wake(w.thread, Some(v));
                return;
            }
            None => {
                m.heap_set(mv + 8, v);
                return;
            }
        }
    }
}

/// Waits in the queue of the `MVar` in argument 0. Answers the value handed over, or None with the
/// exception raised in `exc`.
fn wait(mp: *mut Machine, kind_: WaitKind, value: Option<Word>) -> Option<Word> {
    // SAFETY: the reference ends before `switch`.
    let blocked = unsafe {
        let m = &mut *mp;
        let key = key_of(m, m.args[0]);
        m.sched.enqueue(key, kind_, value, kind_ == WaitKind::Reader)
    };
    if blocked {
        switch(mp);
    }
    // SAFETY: this thread has the turn again.
    let m = unsafe { &mut *mp };
    if check(m) {
        return None;
    }
    Some(m.sched.take_handoff().unwrap_or(0))
}

fn triple(m: &mut Machine, flag: i64, v: Word) -> Word {
    let f = int_value(m, flag);
    super::prims::con_value(m, "Builtin.(,,)", 3, &[0, f, v])
}

/// `throwTo`: raises an exception in another thread; waits for it when the target is masked.
fn throw_to(mp: *mut Machine) -> Word {
    // SAFETY: the reference ends before `switch`.
    let blocked = unsafe {
        let m = &mut *mp;
        let Some(t) = force_arg(m, 0) else { return 0 };
        let target = int_of(m, t) as Tid;
        let exc = m.args[1];
        if target == m.tid {
            m.exc = exc;
            return 0;
        }
        m.sched.throw_to(target, exc) == Thrown::Blocked
    };
    if blocked {
        switch(mp);
    }
    // SAFETY: this thread has the turn again.
    let m = unsafe { &mut *mp };
    if check(m) {
        return 0;
    }
    unit(m)
}

/// Undoes the writes of the running transaction made after a mark.
fn rollback(m: &mut Machine, mark: usize) {
    while m.stm_log.len() > mark {
        if let Some((tv, old)) = m.stm_log.pop() {
            m.heap_set(tv + 8, old);
        }
    }
}

/// Clears a failure that was a `retry#`; answers whether it was one.
fn take_retry(m: &mut Machine) -> bool {
    if std::mem::take(&mut m.stm_retry) {
        m.exc = 0;
        m.prim_failed = false;
        true
    } else {
        false
    }
}

/// `atomically#`: runs a transaction until it commits.
fn atomically(mp: *mut Machine) -> Option<Word> {
    loop {
        // SAFETY: the reference ends before `switch`.
        let blocked = unsafe {
            let m = &mut *mp;
            let mark = m.stm_log.len();
            m.stm_depth += 1;
            let f = m.args[0];
            let r = m.apply_nested(f, &[0]);
            m.stm_depth -= 1;
            match r {
                Some(v) => {
                    let wrote = m.stm_log.len() > mark;
                    m.stm_log.truncate(mark);
                    if wrote {
                        m.sched.stm_wake_all();
                    }
                    return Some(v);
                }
                None if take_retry(m) => {
                    rollback(m, mark);
                    if m.stm_depth > 0 {
                        // A transaction inside a transaction: the outer one retries.
                        m.stm_retry = true;
                        m.prim_failed = true;
                        return None;
                    }
                    m.sched.stm_block()
                }
                None => {
                    rollback(m, mark);
                    return None;
                }
            }
        };
        if blocked {
            switch(mp);
        }
        // SAFETY: this thread has the turn again.
        let m = unsafe { &mut *mp };
        if check(m) {
            return Some(0);
        }
    }
}

/// Runs a primitive of this family, if `name` is one.
pub fn run(m: &mut Machine, name: &str) -> Option<Word> {
    let mp = m as *mut Machine;
    let r = match name {
        // ----- threads (a ThreadId# is the thread's number) -----
        "fork#" => {
            let f = m.args[0];
            fork(m, f)
        }
        "forkOn#" => {
            let f = m.args[1];
            fork(m, f)
        }
        "myThreadId#" => {
            let me = m.tid as i64;
            let t = int_value(m, me);
            state_pair(m, t)
        }
        "yield#" => {
            m.sched.yield_cur();
            switch(mp);
            // SAFETY: this thread has the turn again.
            let m = unsafe { &mut *mp };
            if check(m) {
                return Some(0);
            }
            unit(m)
        }
        "delay#" => {
            let t = force_arg(m, 0)?;
            let micros = int_of(m, t).max(0) as u64;
            if m.sched.sleep_cur(micros) {
                switch(mp);
            }
            // SAFETY: this thread has the turn again.
            let m = unsafe { &mut *mp };
            if check(m) {
                return Some(0);
            }
            unit(m)
        }
        "killThread#" => throw_to(mp),
        "waitRead#" | "waitWrite#" => {
            let t = force_arg(m, 0)?;
            let fd = int_of(m, t) as i32;
            let write = name == "waitWrite#";
            loop {
                // SAFETY: the machine is re-derived after every switch.
                let m = unsafe { &mut *mp };
                if crate::sched::fd_ready(fd, write) {
                    break;
                }
                if m.sched.sleep_cur(1000) {
                    switch(mp);
                }
                // SAFETY: as above.
                if check(unsafe { &mut *mp }) {
                    return Some(0);
                }
            }
            // SAFETY: as above.
            unit(unsafe { &mut *mp })
        }
        "threadStatus#" => {
            let t = force_arg(m, 0)?;
            let status = m.sched.status(int_of(m, t) as Tid);
            m.reserve(16);
            let s = int_value(m, status);
            let z = int_value(m, 0);
            super::prims::con_value(m, "Builtin.(,,,)", 4, &[0, s, z, z])
        }
        "isCurrentThreadBound#" => {
            let z = int_value(m, 0);
            state_pair(m, z)
        }

        // ----- masking -----
        "maskAsyncExceptions#" => with_mask(m, 2)?,
        "maskUninterruptible#" => with_mask(m, 1)?,
        "unmaskAsyncExceptions#" => with_mask(m, 0)?,
        "getMaskingState#" => {
            let state = m.sched.mask() as i64;
            let v = int_value(m, state);
            state_pair(m, v)
        }

        // ----- MVar# (the header's aux is the key of its queue in the scheduler) -----
        "newMVar#" => {
            let key = m.sched.key();
            let r = m.alloc(2);
            m.heap_set(r, heap::header(kind::MVAR, 1, key as u32));
            m.heap_set(r + 8, 0);
            state_pair(m, r)
        }
        "takeMVar#" => {
            force_arg(m, 0)?;
            let v = match take_full(m) {
                Some(v) => v,
                None => match wait(mp, WaitKind::Taker, None) {
                    Some(v) => v,
                    None => return Some(0),
                },
            };
            // SAFETY: this thread has the turn.
            let m = unsafe { &mut *mp };
            // Other threads may have run meanwhile and used up the reserved room.
            super::prims_mem::state_pair_rooted(m, v)
        }
        "readMVar#" => {
            let mv = force_arg(m, 0)?;
            let v = match m.heap_get(mv, 1) {
                0 => match wait(mp, WaitKind::Reader, None) {
                    Some(v) => v,
                    None => return Some(0),
                },
                v => v,
            };
            // SAFETY: this thread has the turn.
            let m = unsafe { &mut *mp };
            // Other threads may have run meanwhile and used up the reserved room.
            super::prims_mem::state_pair_rooted(m, v)
        }
        "putMVar#" => {
            let mv = force_arg(m, 0)?;
            let v = m.args[1];
            if m.heap_get(mv, 1) != 0 {
                if wait(mp, WaitKind::Putter, Some(v)).is_none() {
                    return Some(0);
                }
            } else {
                put_empty(m, v);
            }
            // SAFETY: this thread has the turn.
            let m = unsafe { &mut *mp };
            unit(m)
        }
        "tryTakeMVar#" => {
            force_arg(m, 0)?;
            match take_full(m) {
                Some(v) => triple(m, 1, v),
                None => triple(m, 0, 0),
            }
        }
        "tryReadMVar#" => {
            let mv = force_arg(m, 0)?;
            match m.heap_get(mv, 1) {
                0 => triple(m, 0, 0),
                v => triple(m, 1, v),
            }
        }
        "tryPutMVar#" => {
            let mv = force_arg(m, 0)?;
            let ok = m.heap_get(mv, 1) == 0;
            if ok {
                let v = m.args[1];
                put_empty(m, v);
            }
            let f = int_value(m, ok as i64);
            state_pair(m, f)
        }
        "isEmptyMVar#" => {
            let mv = force_arg(m, 0)?;
            let empty = (m.heap_get(mv, 1) == 0) as i64;
            let v = int_value(m, empty);
            state_pair(m, v)
        }

        // ----- transactions (a TVar# is a reference object) -----
        "newTVar#" => {
            let v = m.args[0];
            let r = m.alloc(2);
            m.heap_set(r, heap::header(kind::REF, 1, 0));
            m.heap_set(r + 8, v);
            state_pair(m, r)
        }
        "readTVar#" | "readTVarIO#" => {
            let tv = force_arg(m, 0)?;
            let v = m.heap_get(tv, 1);
            state_pair(m, v)
        }
        "writeTVar#" => {
            let tv = force_arg(m, 0)?;
            let old = m.heap_get(tv, 1);
            if m.stm_depth > 0 {
                m.stm_log.push((tv, old));
            }
            let v = m.args[1];
            m.heap_set(tv + 8, v);
            unit(m)
        }
        "atomically#" => atomically(mp)?,
        "retry#" => {
            m.stm_retry = true;
            let u = unit(m);
            m.exc = u;
            0
        }
        // catchRetry# a b s: when the first alternative retries, its writes are undone and the second runs.
        "catchRetry#" => {
            let mark = m.stm_log.len();
            let a = m.args[0];
            match m.apply_nested(a, &[0]) {
                Some(v) => v,
                None if take_retry(m) => {
                    rollback(m, mark);
                    let b = m.args[1];
                    m.apply_nested(b, &[0])?
                }
                None => return None,
            }
        }
        // catchSTM# a h s: an exception undoes the writes of the action, and the handler runs.
        "catchSTM#" => {
            let mark = m.stm_log.len();
            let a = m.args[0];
            match m.apply_nested(a, &[0]) {
                Some(v) => v,
                None if m.stm_retry => return None,
                None => {
                    rollback(m, mark);
                    let exc = std::mem::replace(&mut m.exc, 0);
                    m.prim_failed = false;
                    let h = m.args[1];
                    m.apply_nested(h, &[exc, 0])?
                }
            }
        }
        _ => return None,
    };
    Some(r)
}
