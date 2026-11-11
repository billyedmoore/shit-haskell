//! Concurrency in the interpreter. Each Haskell thread is an OS thread, but only the holder of the
//! scheduler's baton runs Haskell code (see `crate::sched`), so the reference-counted runtime data
//! is never touched concurrently. Threads switch when they block, yield, finish, or reach the end
//! of their time slice at a safe point of the evaluator.

use std::cell::{Cell as StdCell, RefCell, UnsafeCell};
use std::rc::Rc;
use std::sync::atomic::Ordering;

use super::Cell;
use crate::sched::{Interrupt, Sched, PREEMPT};

/// The interpreter's scheduler: it holds cells (exceptions on their way, values of `MVar`s).
pub type S = Sched<Cell>;

struct Global(UnsafeCell<Option<S>>);
// SAFETY: the scheduler is only used by the thread holding the baton.
unsafe impl Sync for Global {}

static SCHED: Global = Global(UnsafeCell::new(None));

/// The time slice of the interpreter's threads, in milliseconds.
const SLICE_MS: u64 = 50;

/// The scheduler (made at first use, by the main thread).
pub fn sched() -> *mut S {
    // SAFETY: only the baton holder runs this, and the reference does not outlive the call.
    unsafe { (*SCHED.0.get()).get_or_insert_with(|| Sched::new(SLICE_MS)) as *mut S }
}

/// Runs `f` on the scheduler. `f` must not switch threads.
pub fn with<R>(f: impl FnOnce(&mut S) -> R) -> R {
    // SAFETY: only the baton holder runs this, and `f` returns before any other thread runs.
    f(unsafe { &mut *sched() })
}

/// Gives the baton to the next thread; returns when this thread has it again. The thread has been
/// queued or blocked in the scheduler.
pub fn switch() {
    loop {
        start_signal_handlers();
        // SAFETY: no reference into the scheduler is held here.
        unsafe { S::switch(sched()) }
        if !with(|s| s.take_signal_wake()) {
            break;
        }
    }
}

/// The program's runtime (for starting signal handlers from the scheduler).
static RUNTIME: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

pub fn set_runtime(rt: &super::Runtime) {
    RUNTIME.store(rt as *const super::Runtime as usize, Ordering::Relaxed);
}

/// Signals have arrived: a thread runs their Haskell handlers (`hscRunPendingSignals`).
pub fn start_signal_handlers() {
    if !crate::signals::pending() {
        return;
    }
    let p = RUNTIME.load(Ordering::Relaxed) as *const super::Runtime;
    if p.is_null() {
        return;
    }
    crate::signals::taken();
    // SAFETY: the runtime lives as long as the program.
    let rt = unsafe { &*p };
    if let Some(g) = rt.global("GHC.Internal.Conc.Signal.hscRunPendingSignals") {
        let _ = super::prims_conc::fork(rt, &g);
    }
}

/// Lets the other threads run, then continues.
pub fn yield_now() {
    with(|s| s.yield_cur());
    switch();
}

/// What this thread must raise now, if anything.
pub fn interrupt() -> Option<Interrupt<Cell>> {
    with(|s| s.interrupt())
}

/// Ends this Haskell thread: the baton goes to the next one. Everything reference-counted that the
/// OS thread held must have been dropped.
pub fn finish() {
    // SAFETY: no reference into the scheduler is held here.
    unsafe { S::finish(sched()) }
}

/// Safe point in the evaluator: at the end of a time slice the other threads run; an exception
/// thrown to this thread meanwhile is answered.
#[inline]
pub fn safe_point() -> Option<Interrupt<Cell>> {
    if !PREEMPT.load(Ordering::Relaxed) {
        return None;
    }
    slice_end()
}

#[cold]
fn slice_end() -> Option<Interrupt<Cell>> {
    // A transaction is not interrupted: its writes are in place until it commits.
    if STM_DEPTH.with(|d| d.get()) > 0 {
        return None;
    }
    PREEMPT.store(false, Ordering::Relaxed);
    start_signal_handlers();
    if with(|s| s.preempt()) {
        switch();
    }
    interrupt()
}

thread_local! {
    /// The transactions this thread is running (nested by `atomically#` only once, in practice).
    static STM_DEPTH: StdCell<u32> = const { StdCell::new(0) };
    /// The writes of the running transaction: each variable with the contents it had.
    static STM_LOG: RefCell<Vec<(Rc<RefCell<Cell>>, Cell)>> = const { RefCell::new(Vec::new()) };
}

/// Enters (true) or leaves a transaction.
pub fn stm_enter(enter: bool) {
    STM_DEPTH.with(|d| d.set(if enter { d.get() + 1 } else { d.get().saturating_sub(1) }));
}

/// Whether this thread is in a transaction.
pub fn in_stm() -> bool {
    STM_DEPTH.with(|d| d.get()) > 0
}

/// The number of writes the running transaction has made (a mark to roll back to).
pub fn stm_mark() -> usize {
    STM_LOG.with(|l| l.borrow().len())
}

/// Records a write of the running transaction: the variable and its contents before.
pub fn stm_log(var: Rc<RefCell<Cell>>, old: Cell) {
    STM_LOG.with(|l| l.borrow_mut().push((var, old)));
}

/// Undoes the writes made after a mark.
pub fn stm_rollback(mark: usize) {
    STM_LOG.with(|l| {
        let mut log = l.borrow_mut();
        while log.len() > mark {
            if let Some((var, old)) = log.pop() {
                *var.borrow_mut() = old;
            }
        }
    });
}

/// Forgets the writes made after a mark (they stay); answers whether there were any.
pub fn stm_commit(mark: usize) -> bool {
    STM_LOG.with(|l| {
        let mut log = l.borrow_mut();
        let wrote = log.len() > mark;
        log.truncate(mark);
        wrote
    })
}

thread_local! {
    static STACK_BASE: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Records the current stack position as the top of this thread's stack. Must be called
/// first thing on a thread that runs Haskell code.
pub fn set_stack_base() {
    let marker = 0u8;
    STACK_BASE.with(|c| c.set(&marker as *const u8 as usize));
}

/// Usable stack per Haskell thread, less a margin for Rust code that runs below the check.
pub const STACK_LIMIT: usize = (4usize << 30) - (256usize << 20);

/// Whether this thread has used up its stack, so evaluation must stop with an overflow.
pub fn stack_exhausted() -> bool {
    let marker = 0u8;
    let sp = &marker as *const u8 as usize;
    let base = STACK_BASE.with(|c| c.get());
    base != 0 && base.saturating_sub(sp) > STACK_LIMIT
}

/// Wraps a value that is only used while holding the global lock.
pub struct Locked<T>(pub T);
// SAFETY: values are only accessed by the thread holding the interpreter lock.
unsafe impl<T> Send for Locked<T> {}

thread_local! {
    static CURRENT: StdCell<i64> = const { StdCell::new(1) };
}

/// The number of the Haskell thread this OS thread runs.
pub fn current_thread_id() -> i64 {
    CURRENT.with(|c| c.get())
}

/// Records the number of the thread that is starting.
pub fn set_current(id: i64) {
    CURRENT.with(|c| c.set(id));
}
