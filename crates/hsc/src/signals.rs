//! POSIX signals for both runtimes, as GHC's RTS handles them: `stg_sig_install` installs a C
//! handler that records each signal with its `siginfo_t`; at the next safe point (or when the
//! scheduler is idle) the runtime starts a thread running `GHC.Conc.Signal.hscRunPendingSignals`,
//! which takes the recorded signals (`hsc_next_signal`, `hsc_signal_info`) and runs the Haskell
//! handlers `setHandler` registered.

use std::sync::atomic::{AtomicBool, AtomicI32, AtomicUsize, Ordering};

const SLOTS: usize = 1024;
const INFO_BYTES: usize = 128;

/// The signals recorded and not yet taken: a ring written by the handler (async-signal-safe: plain
/// stores and atomics) and read by the runtime.
struct Ring {
    sig: [AtomicI32; SLOTS],
    info: [[std::cell::UnsafeCell<u8>; INFO_BYTES]; SLOTS],
    head: AtomicUsize,
    tail: AtomicUsize,
}

// SAFETY: a slot is written by the handler before `head` moves past it, and read by the runtime
// only after; the runtime reads one at a time (with the baton).
unsafe impl Sync for Ring {}

#[allow(clippy::declare_interior_mutable_const)]
const ZERO: AtomicI32 = AtomicI32::new(0);
#[allow(clippy::declare_interior_mutable_const)]
const BYTE: std::cell::UnsafeCell<u8> = std::cell::UnsafeCell::new(0);
#[allow(clippy::declare_interior_mutable_const)]
const INFO: [std::cell::UnsafeCell<u8>; INFO_BYTES] = [BYTE; INFO_BYTES];

static RING: Ring = Ring { sig: [ZERO; SLOTS], info: [INFO; SLOTS], head: AtomicUsize::new(0), tail: AtomicUsize::new(0) };

/// Set by the handler: there are signals to take.
static PENDING: AtomicBool = AtomicBool::new(false);
/// Whether some signal has a Haskell handler (the idle scheduler then waits for signals).
static HANDLERS: AtomicBool = AtomicBool::new(false);
/// The state `stg_sig_install` last set for each signal (STG_SIG_DFL when none was).
static STATE: [AtomicI32; 65] = [const { AtomicI32::new(STG_SIG_DFL) }; 65];
/// The siginfo copy the last `hsc_next_signal` left (malloc'd; the Haskell side frees it).
static LAST_INFO: AtomicUsize = AtomicUsize::new(0);

const STG_SIG_DFL: i32 = -1;
const STG_SIG_IGN: i32 = -2;
const STG_SIG_ERR: i32 = -3;
const STG_SIG_HAN: i32 = -4;
const STG_SIG_RST: i32 = -5;

#[repr(C)]
struct SigAction {
    handler: usize,
    mask: [u64; 16],
    flags: i32,
    restorer: usize,
}

const SA_SIGINFO: i32 = 4;
const SA_RESTART: i32 = 0x1000_0000;
const SA_RESETHAND: i32 = -0x8000_0000;

extern "C" {
    fn sigaction(sig: i32, act: *const SigAction, old: *mut SigAction) -> i32;
    fn sigemptyset(set: *mut [u64; 16]) -> i32;
    fn malloc(n: usize) -> *mut u8;
}

extern "C" fn handler(sig: i32, info: *const u8, _ctx: *const u8) {
    let h = RING.head.load(Ordering::Relaxed);
    let t = RING.tail.load(Ordering::Acquire);
    if h.wrapping_sub(t) >= SLOTS {
        return;
    }
    let slot = h % SLOTS;
    RING.sig[slot].store(sig, Ordering::Relaxed);
    for (k, b) in RING.info[slot].iter().enumerate() {
        // SAFETY: the kernel passes a siginfo_t of 128 bytes; the slot is the handler's to write.
        unsafe { *b.get() = if info.is_null() { 0 } else { *info.add(k) } };
    }
    RING.head.store(h.wrapping_add(1), Ordering::Release);
    PENDING.store(true, Ordering::Release);
    crate::sched::PREEMPT.store(true, Ordering::Relaxed);
}

extern "C" {
    fn pthread_sigmask(how: i32, set: *const [u64; 16], old: *mut [u64; 16]) -> i32;
    fn sigfillset(set: *mut [u64; 16]) -> i32;
}

/// Blocks every signal in the calling OS thread: the runtime's own threads (the one waiting for
/// the program, the ticker) take none, so a signal goes to a thread running Haskell code, as in
/// GHC's single-threaded runtime, and `blockSignals` there keeps it pending.
pub fn block_all() {
    let mut set = [0u64; 16];
    // SAFETY: a full set, applied to this thread only.
    unsafe {
        sigfillset(&mut set);
        pthread_sigmask(0, &set, std::ptr::null_mut());
    }
}

extern "C" fn empty_handler(_sig: i32) {}

/// At start-up, as GHC's RTS: SIGPIPE gets a handler that does nothing, so that a write to a
/// closed pipe fails with EPIPE (rather than ending the program) and an interruptible foreign call
/// can be woken by it.
pub fn init() {
    static DONE: std::sync::Once = std::sync::Once::new();
    DONE.call_once(|| {
        let mut act = SigAction { handler: empty_handler as usize, mask: [0; 16], flags: 0, restorer: 0 };
        // SAFETY: a complete action for SIGPIPE.
        unsafe {
            sigemptyset(&mut act.mask);
            sigaction(13, &act, std::ptr::null_mut());
        }
    });
}

/// Whether signals wait to be handled.
pub fn pending() -> bool {
    PENDING.load(Ordering::Acquire)
}

/// Whether some signal has a Haskell handler.
pub fn handlers_installed() -> bool {
    HANDLERS.load(Ordering::Relaxed)
}

/// Marks the pending signals as being taken care of (a handler thread has been started).
pub fn taken() {
    PENDING.store(false, Ordering::Release);
    // A signal that arrived meanwhile is still in the ring: the handler thread takes it too, as it
    // empties the ring; set the flag again for one that might come after it.
    if RING.head.load(Ordering::Acquire) != RING.tail.load(Ordering::Acquire) {
        PENDING.store(true, Ordering::Release);
    }
}

/// `stg_sig_install(sig, spi, mask)`: the RTS's signal installer. Answers the previous state.
pub extern "C" fn stg_sig_install(sig: i32, spi: i32, mask: *const [u64; 16]) -> i32 {
    if !(1..65).contains(&sig) {
        return STG_SIG_ERR;
    }
    let mut act = SigAction { handler: 0, mask: [0; 16], flags: 0, restorer: 0 };
    // SAFETY: an empty set is written into the action.
    unsafe { sigemptyset(&mut act.mask) };
    if !mask.is_null() {
        // SAFETY: the caller passes a sigset_t.
        act.mask = unsafe { *mask };
    }
    match spi {
        STG_SIG_DFL => act.handler = 0,
        STG_SIG_IGN => act.handler = 1,
        STG_SIG_HAN | STG_SIG_RST => {
            act.handler = handler as usize;
            act.flags = SA_SIGINFO | SA_RESTART | if spi == STG_SIG_RST { SA_RESETHAND } else { 0 };
            HANDLERS.store(true, Ordering::Relaxed);
        }
        _ => return STG_SIG_ERR,
    }
    // SAFETY: a complete action for a valid signal.
    if unsafe { sigaction(sig, &act, std::ptr::null_mut()) } != 0 {
        return STG_SIG_ERR;
    }
    STATE[sig as usize].swap(spi, Ordering::Relaxed)
}

/// `hsc_next_signal()`: the next recorded signal (-1 when none); its siginfo copy is left for
/// `hsc_signal_info`.
pub extern "C" fn hsc_next_signal() -> i32 {
    let t = RING.tail.load(Ordering::Relaxed);

    if t == RING.head.load(Ordering::Acquire) {
        return -1;
    }
    let slot = t % SLOTS;
    let sig = RING.sig[slot].load(Ordering::Relaxed);
    // SAFETY: a fresh block of the size copied.
    let copy = unsafe { malloc(INFO_BYTES) };
    if !copy.is_null() {
        for (k, b) in RING.info[slot].iter().enumerate() {
            // SAFETY: the slot was written before `head` passed it; the copy has room.
            unsafe { *copy.add(k) = *b.get() };
        }
    }
    LAST_INFO.store(copy as usize, Ordering::Relaxed);
    RING.tail.store(t.wrapping_add(1), Ordering::Release);
    sig
}

/// `hsc_signal_info()`: the siginfo copy of the signal `hsc_next_signal` answered.
pub extern "C" fn hsc_signal_info() -> *mut u8 {
    LAST_INFO.swap(0, Ordering::Relaxed) as *mut u8
}
