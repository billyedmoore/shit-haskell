//! Concurrency with a global interpreter lock. Each Haskell thread is an OS thread, but
//! only the holder of the lock runs Haskell code, so the reference-counted runtime data
//! is never touched concurrently. Threads switch when they block (an `MVar` operation
//! that cannot proceed, or `threadDelay`), and when they finish.

use std::sync::{Condvar, Mutex};
use std::time::Duration;

static GIL_HELD: Mutex<bool> = Mutex::new(false);
static GIL_CV: Condvar = Condvar::new();

pub fn gil_acquire() {
    let mut held = GIL_HELD.lock().unwrap_or_else(|e| e.into_inner());
    while *held {
        held = GIL_CV.wait(held).unwrap_or_else(|e| e.into_inner());
    }
    *held = true;
}

pub fn gil_release() {
    let mut held = GIL_HELD.lock().unwrap_or_else(|e| e.into_inner());
    *held = false;
    GIL_CV.notify_all();
}

/// Gives other threads a chance to run, then continues holding the lock.
pub fn yield_now() {
    gil_release();
    std::thread::sleep(Duration::from_micros(50));
    gil_acquire();
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

use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};

static THREAD_IDS: AtomicI64 = AtomicI64::new(2);

thread_local! {
    static CURRENT: std::cell::Cell<i64> = const { std::cell::Cell::new(1) };
}

pub fn next_thread_id() -> i64 {
    THREAD_IDS.fetch_add(1, Ordering::SeqCst)
}

pub fn current_thread_id() -> i64 {
    CURRENT.with(|c| c.get())
}

/// Records the id of the thread that is starting.
pub fn set_current(id: i64) {
    CURRENT.with(|c| c.set(id));
}

/// Threads that have been started and not finished (the main thread counts as one).
pub static LIVE: AtomicUsize = AtomicUsize::new(1);

pub fn thread_started() {
    LIVE.fetch_add(1, Ordering::SeqCst);
}

pub fn thread_finished() {
    LIVE.fetch_sub(1, Ordering::SeqCst);
    let me = current_thread_id();
    let mut p = PENDING.lock().unwrap_or_else(|e| e.into_inner());
    p.retain(|(id, _)| *id != me);
    PENDING_N.store(p.len(), Ordering::SeqCst);
}

/// Asynchronous exceptions posted to threads (by `throwTo`), waiting to be raised there.
static PENDING: Mutex<Vec<(i64, Locked<super::Value>)>> = Mutex::new(Vec::new());
static PENDING_N: AtomicUsize = AtomicUsize::new(0);
static TICKS: AtomicUsize = AtomicUsize::new(0);

/// Posts an asynchronous exception to thread `id`; it is raised when that thread next
/// reaches a safe point or a blocking operation.
pub fn post_exception(id: i64, exc: super::Value) {
    let mut p = PENDING.lock().unwrap_or_else(|e| e.into_inner());
    p.push((id, Locked(exc)));
    PENDING_N.store(p.len(), Ordering::SeqCst);
}

/// Takes the asynchronous exception posted to the current thread, if there is one.
pub fn take_exception() -> Option<super::Value> {
    if PENDING_N.load(Ordering::Relaxed) == 0 {
        return None;
    }
    let me = current_thread_id();
    let mut p = PENDING.lock().unwrap_or_else(|e| e.into_inner());
    let pos = p.iter().position(|(id, _)| *id == me)?;
    let (_, Locked(exc)) = p.remove(pos);
    PENDING_N.store(p.len(), Ordering::SeqCst);
    Some(exc)
}

/// Safe point in the evaluator: lets other threads run every so often (preemption), and
/// returns an asynchronous exception posted to this thread, if any.
pub fn safe_point() -> Option<super::Value> {
    if LIVE.load(Ordering::Relaxed) > 1 && TICKS.fetch_add(1, Ordering::Relaxed) & 4095 == 4095 {
        yield_now();
    }
    take_exception()
}

/// Called when an MVar operation cannot proceed: lets other threads run. Returns false when
/// no other thread exists, so the blocked operation can never complete.
pub fn blocked_wait() -> bool {
    if LIVE.load(Ordering::SeqCst) <= 1 {
        return false;
    }
    yield_now();
    true
}
