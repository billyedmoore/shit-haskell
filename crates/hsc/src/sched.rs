//! The scheduler of Haskell threads, shared by the interpreter and the native runtime.
//!
//! Every Haskell thread runs on an OS thread of its own, and exactly one of them runs at a time:
//! the holder of the baton. A thread gives the baton away when it blocks, yields, is preempted at
//! the end of its time slice, or finishes; the scheduler's state is touched only by the holder, so
//! it needs no lock of its own. The order of events follows GHC's single-capability runtime, which
//! is what programs observe: a forked thread goes to the back of the run queue and its parent
//! keeps running; the waiters of an `MVar` are served first come first served, and a value is
//! handed to the waiter directly; a thread woken up goes to the back of the run queue; when
//! nothing can run and nothing sleeps, every thread blocked on an `MVar` or in a transaction is
//! told that it is blocked indefinitely, in the order of creation with the main thread last.
//!
//! The scheduler is generic in the values it holds for threads (exceptions on their way to a
//! thread, values handed over by an `MVar`): heap words in the native runtime, which the collector
//! updates through `roots`, and interpreter values in the interpreter.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, Once};
use std::time::{Duration, Instant};

#[repr(C)]
struct PollFd {
    fd: i32,
    events: i16,
    revents: i16,
}

extern "C" {
    fn poll(fds: *mut PollFd, n: u64, timeout: i32) -> i32;
}

/// Whether a descriptor is ready for reading (or writing), without waiting.
pub fn fd_ready(fd: i32, write: bool) -> bool {
    const POLLIN: i16 = 1;
    const POLLOUT: i16 = 4;
    let mut p = PollFd { fd, events: if write { POLLOUT } else { POLLIN }, revents: 0 };
    // SAFETY: one pollfd, no waiting.
    let r = unsafe { poll(&mut p, 1, 0) };
    // An error (a closed descriptor) also ends the wait: the operation that follows reports it.
    r != 0
}

/// A thread's number: 1 for the main thread, increasing with every fork.
pub type Tid = u64;

/// A hasher for small integer keys (thread numbers, `MVar` keys): one multiply instead of
/// SipHash, which dominated each thread switch. The keys are not chosen by an adversary.
#[derive(Default, Clone, Copy)]
pub struct IntHash(u64);

impl std::hash::Hasher for IntHash {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0.rotate_left(5) ^ b as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
        }
    }
    fn write_u64(&mut self, n: u64) {
        self.0 = (self.0 ^ n).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    }
}

/// A map keyed by small integers.
pub type IntMap<K, V> = HashMap<K, V, std::hash::BuildHasherDefault<IntHash>>;

/// Set at the end of every time slice once a second thread exists: the running thread gives way
/// at its next safe point. Generated code reads it (as `hsc_preempt`) on its inline fast paths.
#[export_name = "hsc_preempt"]
pub static PREEMPT: AtomicBool = AtomicBool::new(false);

static TICKER: Once = Once::new();

/// The baton: the number of the thread whose turn it is.
pub struct Baton {
    turn: Mutex<Tid>,
}

impl Baton {
    /// Gives the turn to a thread, which waits on `cv`.
    fn pass(&self, to: Tid, cv: &Condvar) {
        *self.turn.lock().unwrap_or_else(|e| e.into_inner()) = to;
        cv.notify_all();
    }

    /// Waits for the turn of thread `me`.
    pub fn wait(&self, me: Tid, cv: &Condvar) {
        let mut turn = self.turn.lock().unwrap_or_else(|e| e.into_inner());
        while *turn != me {
            turn = cv.wait(turn).unwrap_or_else(|e| e.into_inner());
        }
    }
}

/// What a blocked thread waits for.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Why {
    /// A take or a put on the `MVar` with this key.
    MVar(u64),
    /// A read of the `MVar` with this key.
    MVarRead(u64),
    /// A transaction that retried: a commit of another one.
    Stm,
    /// The end of a delay.
    Delay,
    /// The delivery of an exception thrown to this (masked) thread.
    Throw(Tid),
    /// The end of a safe foreign call, made without the baton.
    Foreign,
}

#[derive(Clone, Copy, PartialEq)]
enum State {
    /// Running, or in the run queue.
    Ready,
    Blocked(Why),
}

/// What a thread must raise in itself.
pub enum Interrupt<V> {
    /// An exception thrown to it by another thread.
    Exception(V),
    /// `BlockedIndefinitelyOnMVar`.
    BlockedOnMVar,
    /// `BlockedIndefinitelyOnSTM`.
    BlockedOnSTM,
}

/// How a thread waits on an `MVar`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum WaitKind {
    Taker,
    Reader,
    Putter,
}

/// A thread in the queue of an `MVar`; a putter waits with its value.
pub struct Waiter<V> {
    pub thread: Tid,
    pub kind: WaitKind,
    pub value: Option<V>,
}

/// An `MVar` waiter queue optimized for the common single-waiter case (zero heap allocation).
struct WaitQueue<V> {
    first: Option<Waiter<V>>,
    rest: VecDeque<Waiter<V>>,
}

impl<V> Default for WaitQueue<V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<V> WaitQueue<V> {
    const fn new() -> Self {
        WaitQueue { first: None, rest: VecDeque::new() }
    }

    #[inline(always)]
    fn is_empty(&self) -> bool {
        self.first.is_none()
    }

    #[inline(always)]
    fn front(&self) -> Option<&Waiter<V>> {
        self.first.as_ref()
    }

    #[inline]
    fn push_back(&mut self, w: Waiter<V>) {
        if self.first.is_none() {
            self.first = Some(w);
        } else {
            self.rest.push_back(w);
        }
    }

    #[inline]
    fn push_front(&mut self, w: Waiter<V>) {
        if let Some(old) = self.first.replace(w) {
            self.rest.push_front(old);
        }
    }

    #[inline]
    fn pop_front(&mut self) -> Option<Waiter<V>> {
        let w = self.first.take()?;
        if !self.rest.is_empty() {
            self.first = self.rest.pop_front();
        }
        Some(w)
    }

    fn retain(&mut self, mut f: impl FnMut(&Waiter<V>) -> bool) {
        if !self.rest.is_empty() {
            self.rest.retain(|w| f(w));
        }
        if let Some(w) = &self.first {
            if !f(w) {
                self.first = self.rest.pop_front();
            }
        }
    }

    fn for_each_mut(&mut self, mut f: impl FnMut(&mut Waiter<V>)) {
        if let Some(w) = self.first.as_mut() {
            f(w);
        }
        for w in self.rest.iter_mut() {
            f(w);
        }
    }
}

/// Number of low `MVar` keys kept in a direct-indexed table (zero hashing).
const INLINE_WAITS: usize = 1024;

/// The answer of `throw_to`.
#[derive(PartialEq)]
pub enum Thrown {
    /// The exception is on its way (or the target is gone): the thrower continues.
    Done,
    /// The target is masked: the thrower is blocked until the exception is received, and switches.
    Blocked,
}

struct Thread<V> {
    cv: Arc<Condvar>,
    state: State,
    /// 0 unmasked, 1 masked uninterruptibly, 2 masked interruptibly (as `getMaskingState#`).
    mask: u8,
    /// Exceptions thrown to this thread and not yet raised, each with the thrower blocked on it.
    pending: VecDeque<(V, Option<Tid>)>,
    /// The value an `MVar` operation handed to this thread when it woke it up.
    handoff: Option<V>,
    /// Taken out of a blocking operation by an exception: it is raised whatever the masking state.
    interrupted: bool,
    /// Found blocked for ever: the exception to raise is this one, whatever the masking state.
    marker: Option<Why>,
    /// In a foreign call: the OS thread making it, and whether the call is interruptible (an
    /// exception thrown to the thread then interrupts the system call with a signal).
    os_thread: u64,
    interruptible: bool,
}

pub struct Sched<V> {
    cur: Tid,
    next_id: Tid,
    /// Threads indexed directly by `Tid` (`threads[0]` is always `None`).
    threads: Vec<Option<Box<Thread<V>>>>,
    live_threads: usize,
    runq: VecDeque<Tid>,
    /// Sleeping threads with the instants they wake at.
    sleepers: Vec<(Instant, Tid)>,
    /// Direct-indexed waiter queues for `MVar` keys `< INLINE_WAITS`.
    inline_waits: Vec<WaitQueue<V>>,
    /// Waiter queues for `MVar` keys `>= INLINE_WAITS`.
    waits: IntMap<u64, WaitQueue<V>>,
    /// The threads whose transactions retried.
    stm_waiters: VecDeque<Tid>,
    next_key: u64,
    baton: Arc<Baton>,
    slice: Duration,
    /// The threads whose safe foreign calls have returned, waiting to be queued (they do not hold
    /// the baton, so they leave their number here), and the condition the idle scheduler waits on.
    returned: Arc<(Mutex<Vec<Tid>>, Condvar)>,
    /// The threads in safe foreign calls.
    in_foreign: usize,
    /// Set when the scheduler had nothing to run but signals had arrived: the switching thread
    /// starts their handlers (see `take_signal_wake`).
    signal_wake: bool,
}

extern "C" {
    fn pthread_self() -> u64;
    fn pthread_kill(thread: u64, sig: i32) -> i32;
}

/// The turn of nobody: the baton is free (a thread in C gave it up with nothing to run). A waiting
/// thread claims it and makes the scheduling decision (see `Sched::wait_turn`).
const FREE: Tid = 0;

/// What a thread in a safe foreign call needs to come back: see `Sched::enter_foreign`.
pub struct ForeignReturn<V> {
    sched: *mut Sched<V>,
    returned: Arc<(Mutex<Vec<Tid>>, Condvar)>,
    me: Tid,
    cv: Arc<Condvar>,
}

impl<V> ForeignReturn<V> {
    /// Back from C: the thread asks to be queued and waits for its turn.
    pub fn wait(self) {
        {
            let (list, cv) = &*self.returned;
            list.lock().unwrap_or_else(|e| e.into_inner()).push(self.me);
            cv.notify_all();
        }
        // SAFETY: the scheduler outlives every thread; it is touched only with the turn.
        unsafe { Sched::wait_turn(self.sched, self.me, &self.cv) };
    }
}

impl<V> Sched<V> {
    /// A scheduler with the main thread running; `slice_ms` is the time slice.
    pub fn new(slice_ms: u64) -> Sched<V> {
        crate::signals::init();
        let threads = vec![None, Some(Box::new(Self::thread(0)))];
        Sched {
            cur: 1,
            next_id: 2,
            threads,
            live_threads: 1,
            runq: VecDeque::new(),
            sleepers: Vec::new(),
            inline_waits: Vec::new(),
            waits: IntMap::default(),
            stm_waiters: VecDeque::new(),
            next_key: 1,
            baton: Arc::new(Baton { turn: Mutex::new(1) }),
            slice: Duration::from_millis(slice_ms),
            returned: Arc::new((Mutex::new(Vec::new()), Condvar::new())),
            in_foreign: 0,
            signal_wake: false,
        }
    }

    fn thread(mask: u8) -> Thread<V> {
        Thread {
            cv: Arc::new(Condvar::new()),
            state: State::Ready,
            mask,
            pending: VecDeque::new(),
            handoff: None,
            interrupted: false,
            marker: None,
            os_thread: 0,
            interruptible: false,
        }
    }

    #[inline(always)]
    fn thread_ref(&self, id: Tid) -> Option<&Thread<V>> {
        self.threads.get(id as usize).and_then(|o| o.as_deref())
    }

    #[inline(always)]
    fn thread_mut(&mut self, id: Tid) -> Option<&mut Thread<V>> {
        self.threads.get_mut(id as usize).and_then(|o| o.as_deref_mut())
    }

    fn insert_thread(&mut self, id: Tid, t: Thread<V>) {
        let idx = id as usize;
        if self.threads.len() <= idx {
            self.threads.resize_with(idx + 1, || None);
        }
        self.threads[idx] = Some(Box::new(t));
        self.live_threads += 1;
    }

    fn remove_thread(&mut self, id: Tid) -> Option<Box<Thread<V>>> {
        let r = self.threads.get_mut(id as usize).and_then(Option::take);
        if r.is_some() {
            self.live_threads -= 1;
        }
        r
    }

    #[inline(always)]
    fn wait_get(&self, key: u64) -> Option<&WaitQueue<V>> {
        let idx = key as usize;
        if idx < INLINE_WAITS {
            self.inline_waits.get(idx)
        } else {
            self.waits.get(&key)
        }
    }

    #[inline(always)]
    fn wait_get_mut(&mut self, key: u64) -> Option<&mut WaitQueue<V>> {
        let idx = key as usize;
        if idx < INLINE_WAITS {
            self.inline_waits.get_mut(idx)
        } else {
            self.waits.get_mut(&key)
        }
    }

    #[inline(always)]
    fn wait_entry(&mut self, key: u64) -> &mut WaitQueue<V> {
        let idx = key as usize;
        if idx < INLINE_WAITS {
            if self.inline_waits.len() <= idx {
                self.inline_waits.resize_with(idx + 1, WaitQueue::new);
            }
            &mut self.inline_waits[idx]
        } else {
            self.waits.entry(key).or_default()
        }
    }

    /// Whether the last switch came back only so that signal handlers are started (the thread is
    /// still blocked: it starts them and switches again).
    pub fn take_signal_wake(&mut self) -> bool {
        std::mem::take(&mut self.signal_wake)
    }

    /// In the child of a fork: only the running thread exists (the other OS threads are gone).
    pub fn only_current(&mut self) {
        let me = self.cur as usize;
        for (i, slot) in self.threads.iter_mut().enumerate() {
            if i != me {
                *slot = None;
            }
        }
        self.live_threads = usize::from(self.thread_ref(self.cur).is_some());
        self.runq.clear();
        self.sleepers.clear();
        self.inline_waits.clear();
        self.waits.clear();
        self.stm_waiters.clear();
        self.in_foreign = 0;
        self.returned.0.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }

    /// The running thread.
    #[inline(always)]
    pub fn current(&self) -> Tid {
        self.cur
    }

    #[inline(always)]
    fn me(&mut self) -> &mut Thread<V> {
        self.threads[self.cur as usize].as_deref_mut().expect("the running thread is in the table")
    }

    /// A key for a new `MVar`.
    pub fn key(&mut self) -> u64 {
        self.next_key += 1;
        self.next_key - 1
    }

    /// A new thread, queued behind the others; it inherits the masking state of its parent. The OS
    /// thread that runs it first waits for its turn with `Baton::wait` on the condition answered.
    pub fn spawn(&mut self) -> (Tid, Arc<Baton>, Arc<Condvar>) {
        let slice = self.slice;
        TICKER.call_once(|| {
            let _ = std::thread::Builder::new().name("hsc-ticker".into()).spawn(move || loop {
                crate::signals::block_all();
                std::thread::sleep(slice);
                PREEMPT.store(true, Ordering::Relaxed);
            });
        });
        let id = self.next_id;
        self.next_id += 1;
        let t = Self::thread(self.me().mask);
        let cv = t.cv.clone();
        self.insert_thread(id, t);
        self.runq.push_back(id);
        (id, self.baton.clone(), cv)
    }

    /// The masking state of the running thread.
    #[inline(always)]
    pub fn mask(&self) -> u8 {
        self.thread_ref(self.cur).map_or(0, |t| t.mask)
    }

    #[inline(always)]
    pub fn set_mask(&mut self, mask: u8) {
        self.me().mask = mask;
    }

    /// Whether more than one thread exists.
    #[inline(always)]
    pub fn multi(&self) -> bool {
        self.live_threads > 1
    }

    /// Queues the running thread behind the others (the caller then switches).
    pub fn yield_cur(&mut self) {
        self.wake_due();
        let me = self.cur;
        self.runq.push_back(me);
    }

    /// The end of a time slice: queues the running thread if another can run, and says so.
    pub fn preempt(&mut self) -> bool {
        self.wake_due();
        if self.runq.is_empty() {
            return false;
        }
        let me = self.cur;
        self.runq.push_back(me);
        true
    }

    /// Whether the running thread may block now. It may not when an exception waits for it and it is
    /// not masked uninterruptibly: the exception is raised in place of the blocking operation.
    #[inline(always)]
    fn may_block(&mut self) -> bool {
        let t = self.me();
        if !t.pending.is_empty() && t.mask != 1 {
            t.interrupted = true;
            return false;
        }
        true
    }

    /// Blocks the running thread in the queue of an `MVar` (at the front for a reader). Answers
    /// false when it must raise an exception first (see `interrupt`); else the caller switches.
    #[inline]
    pub fn enqueue(&mut self, key: u64, kind: WaitKind, value: Option<V>, front: bool) -> bool {
        if !self.may_block() {
            return false;
        }
        let me = self.cur;
        let w = Waiter { thread: me, kind, value };
        let q = self.wait_entry(key);
        if front {
            q.push_front(w);
        } else {
            q.push_back(w);
        }
        self.me().state = State::Blocked(if kind == WaitKind::Reader { Why::MVarRead(key) } else { Why::MVar(key) });
        true
    }

    /// The kind of the first waiter of an `MVar`.
    #[inline(always)]
    pub fn front_kind(&self, key: u64) -> Option<WaitKind> {
        self.wait_get(key).and_then(|q| q.front()).map(|w| w.kind)
    }

    /// Takes the first waiter of an `MVar` out of its queue; the caller wakes it up.
    #[inline]
    pub fn pop_waiter(&mut self, key: u64) -> Option<Waiter<V>> {
        let idx = key as usize;
        if idx < INLINE_WAITS {
            self.inline_waits.get_mut(idx)?.pop_front()
        } else {
            let q = self.waits.get_mut(&key)?;
            let w = q.pop_front();
            if q.is_empty() {
                self.waits.remove(&key);
            }
            w
        }
    }

    /// Makes a blocked thread (already out of its queue) runnable, with a value handed to it.
    #[inline]
    pub fn wake(&mut self, t: Tid, handoff: Option<V>) {
        if let Some(th) = self.thread_mut(t) {
            th.state = State::Ready;
            th.handoff = handoff;
            self.runq.push_back(t);
        }
    }

    /// The value handed to the running thread by the operation that woke it.
    #[inline(always)]
    pub fn take_handoff(&mut self) -> Option<V> {
        self.me().handoff.take()
    }

    /// Blocks the running thread for a time (microseconds). Answers as `enqueue`.
    pub fn sleep_cur(&mut self, micros: u64) -> bool {
        if !self.may_block() {
            return false;
        }
        let me = self.cur;
        self.sleepers.push((Instant::now() + Duration::from_micros(micros), me));
        self.me().state = State::Blocked(Why::Delay);
        true
    }

    /// Blocks the running thread until a transaction commits. Answers as `enqueue`.
    pub fn stm_block(&mut self) -> bool {
        if !self.may_block() {
            return false;
        }
        let me = self.cur;
        self.stm_waiters.push_back(me);
        self.me().state = State::Blocked(Why::Stm);
        true
    }

    /// A transaction that wrote has committed: the threads that retried run theirs again.
    pub fn stm_wake_all(&mut self) {
        while let Some(t) = self.stm_waiters.pop_front() {
            self.wake(t, None);
        }
    }

    /// Takes a blocked thread out of what it waits in and makes it runnable.
    fn unblock(&mut self, t: Tid, why: Why) {
        match why {
            // A thread in C comes back by itself; what was thrown to it waits for it.
            Why::Foreign => return,
            Why::MVar(key) | Why::MVarRead(key) => {
                if let Some(q) = self.wait_get_mut(key) {
                    q.retain(|w| w.thread != t);
                    if q.is_empty() && (key as usize) >= INLINE_WAITS {
                        self.waits.remove(&key);
                    }
                }
            }
            Why::Stm => self.stm_waiters.retain(|w| *w != t),
            Why::Delay => self.sleepers.retain(|(_, w)| *w != t),
            Why::Throw(target) => {
                // The exception it was throwing is withdrawn.
                if let Some(th) = self.thread_mut(target) {
                    th.pending.retain(|(_, from)| *from != Some(t));
                }
            }
        }
        if let Some(th) = self.thread_mut(t) {
            th.state = State::Ready;
            self.runq.push_back(t);
        }
    }

    /// Throws an exception to another thread. An unmasked target, or one blocked while masked
    /// interruptibly, will raise it when it next runs, and the thrower continues. Otherwise the
    /// thrower is blocked until the target receives the exception: the caller switches.
    pub fn throw_to(&mut self, target: Tid, exc: V) -> Thrown {
        // A foreign call that has returned is not interrupted (its worker may be in another one).
        self.take_returned();
        let me = self.cur;
        let Some(t) = self.thread_mut(target) else { return Thrown::Done };
        let blocked = match t.state {
            State::Blocked(why) => Some(why),
            State::Ready => None,
        };
        if t.mask == 0 || (t.mask == 2 && blocked.is_some()) {
            t.pending.push_back((exc, None));
            // A thread in an interruptible foreign call is woken from its system call (EINTR), as
            // GHC's RTS does, with SIGPIPE (whose handler does nothing).
            if blocked == Some(Why::Foreign) && t.interruptible && t.os_thread != 0 {
                // SAFETY: the OS thread exists while its Haskell thread is in the call.
                unsafe { pthread_kill(t.os_thread, 13) };
            }
            if let Some(why) = blocked {
                t.interrupted = true;
                self.unblock(target, why);
            }
            return Thrown::Done;
        }
        if !self.may_block() {
            // The thrower has an exception of its own to raise first; this one is not thrown.
            return Thrown::Done;
        }
        if let Some(t) = self.thread_mut(target) {
            t.pending.push_back((exc, Some(me)));
        }
        self.me().state = State::Blocked(Why::Throw(target));
        Thrown::Blocked
    }

    /// What the running thread must raise now, if anything: the notice that it is blocked for ever,
    /// or an exception thrown to it that its masking state lets in. Called at safe points, after
    /// every switch, and when the thread unmasks.
    #[inline]
    pub fn interrupt(&mut self) -> Option<Interrupt<V>> {
        let me = self.cur;
        let t = self.me();
        if let Some(why) = t.marker.take() {
            t.interrupted = false;
            t.handoff = None;
            return Some(if why == Why::Stm { Interrupt::BlockedOnSTM } else { Interrupt::BlockedOnMVar });
        }
        let forced = std::mem::take(&mut t.interrupted);
        if t.pending.is_empty() || !(t.mask == 0 || forced) {
            return None;
        }
        let (exc, from) = t.pending.pop_front()?;
        // What an `MVar` handed over while the exception was on its way is lost with the operation.
        t.handoff = None;
        if let Some(s) = from {
            self.wake_thrower(s, me);
        }
        Some(Interrupt::Exception(exc))
    }

    /// Whether `interrupt` would answer something for the running thread at a safe point.
    pub fn has_interrupt(&self) -> bool {
        self.thread_ref(self.cur).is_some_and(|t| t.marker.is_some() || (!t.pending.is_empty() && (t.mask == 0 || t.interrupted)))
    }

    fn wake_thrower(&mut self, s: Tid, target: Tid) {
        if self.thread_ref(s).is_some_and(|t| t.state == State::Blocked(Why::Throw(target))) {
            self.wake(s, None);
        }
    }

    /// The status of a thread as `threadStatus#` numbers it.
    pub fn status(&self, t: Tid) -> i64 {
        match self.thread_ref(t).map(|t| t.state) {
            None => 16,
            Some(State::Ready) => 0,
            Some(State::Blocked(Why::MVar(_))) => 1,
            Some(State::Blocked(Why::MVarRead(_))) => 14,
            Some(State::Blocked(Why::Stm)) => 6,
            Some(State::Blocked(Why::Delay)) => 5,
            Some(State::Blocked(Why::Throw(_))) => 12,
            Some(State::Blocked(Why::Foreign)) => 10,
        }
    }

    /// Queues the threads back from safe foreign calls.
    #[inline(always)]
    fn take_returned(&mut self) {
        if self.in_foreign == 0 {
            return;
        }
        let back: Vec<Tid> = std::mem::take(&mut *self.returned.0.lock().unwrap_or_else(|e| e.into_inner()));
        for t in back {
            self.in_foreign = self.in_foreign.saturating_sub(1);
            if let Some(th) = self.thread_mut(t) {
                th.state = State::Ready;
                self.runq.push_back(t);
            }
        }
    }

    /// Before a safe foreign call: when another thread can run, the running one gives it the baton
    /// for the time of the call, and the answer says how to come back. None when nothing else can
    /// run (the call is made with the baton).
    ///
    /// # Safety
    /// As for `switch`; after it the caller must not touch the scheduler (or the heap) until
    /// `ForeignReturn::wait` has given the turn back.
    pub unsafe fn enter_foreign(this: *mut Sched<V>, interruptible: bool) -> Option<ForeignReturn<V>> {
        let s = &mut *this;
        s.wake_due();
        let me = s.cur;
        let my_cv = s.thread_ref(me)?.cv.clone();
        // With nothing to run now, the baton is left free: a sleeper whose time comes, or this
        // thread when it is back, claims it.
        let next = s.runq.pop_front().unwrap_or(FREE);
        let next_cv = if next == FREE { my_cv.clone() } else { s.thread_ref(next)?.cv.clone() };
        if let Some(t) = s.thread_mut(me) {
            t.state = State::Blocked(Why::Foreign);
            t.interruptible = interruptible;
            t.os_thread = pthread_self();
        }
        s.in_foreign += 1;
        s.cur = next;
        let back = ForeignReturn { sched: this, returned: s.returned.clone(), me, cv: my_cv };
        s.baton.pass(next, &next_cv);
        Some(back)
    }

    /// Waits for the turn of thread `me`. Meanwhile, when the baton is free, this thread claims it
    /// and makes the scheduling decision: the threads back from C are queued, the sleepers whose
    /// time has come woken, and the next runnable thread gets the turn (this one, or another);
    /// with nothing runnable, the baton is free again.
    ///
    /// # Safety
    /// As for `switch`.
    pub unsafe fn wait_turn(this: *mut Sched<V>, me: Tid, cv: &Condvar) {
        let baton = (*this).baton.clone();
        loop {
            {
                let mut turn = baton.turn.lock().unwrap_or_else(|e| e.into_inner());
                loop {
                    if *turn == me {
                        return;
                    }
                    if *turn == FREE {
                        *turn = me;
                        break;
                    }
                    turn = cv.wait_timeout(turn, Duration::from_millis(5)).unwrap_or_else(|e| e.into_inner()).0;
                }
            }
            // This thread has the turn (the scheduler is its own until it passes it on).
            let s = &mut *this;
            s.wake_due();
            match s.runq.pop_front() {
                Some(next) if next == me => {
                    s.cur = me;
                    return;
                }
                Some(next) => {
                    s.cur = next;
                    if let Some(ncv) = s.thread_ref(next).map(|t| t.cv.clone()) {
                        baton.pass(next, &ncv);
                    }
                }
                None => {
                    s.cur = FREE;
                    *baton.turn.lock().unwrap_or_else(|e| e.into_inner()) = FREE;
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
        }
    }

    /// Moves the sleepers whose time has come to the run queue, earliest first.
    #[inline]
    fn wake_due(&mut self) {
        self.take_returned();
        if self.sleepers.is_empty() {
            return;
        }
        let now = Instant::now();
        let mut due: Vec<(Instant, Tid)> = Vec::new();
        self.sleepers.retain(|s| {
            if s.0 <= now {
                due.push(*s);
                false
            } else {
                true
            }
        });
        due.sort_by_key(|s| s.0);
        for (_, t) in due {
            self.wake(t, None);
        }
    }

    /// Nothing can run and nothing sleeps: every thread blocked on an `MVar` or in a transaction is
    /// made runnable with the notice that it is blocked for ever, in the order of creation with the
    /// main thread last.
    fn resurrect(&mut self) {
        let mut found: Vec<(Tid, Why)> = self
            .threads
            .iter()
            .enumerate()
            .filter_map(|(id, slot)| match slot.as_deref()?.state {
                State::Blocked(why @ (Why::MVar(_) | Why::MVarRead(_) | Why::Stm)) => Some((id as Tid, why)),
                _ => None,
            })
            .collect();
        if found.is_empty() {
            found = self
                .threads
                .iter()
                .enumerate()
                .filter_map(|(id, slot)| match slot.as_deref()?.state {
                    State::Blocked(Why::Foreign) => None,
                    State::Blocked(why) => Some((id as Tid, why)),
                    State::Ready => None,
                })
                .collect();
        }
        found.sort_by_key(|(id, _)| (*id == 1, *id));
        for (id, why) in found {
            self.unblock(id, why);
            if let Some(t) = self.thread_mut(id) {
                t.marker = Some(why);
            }
        }
    }

    /// The thread to run next; the running one has been queued, blocked or removed.
    #[inline]
    fn next(&mut self) -> Tid {
        if self.in_foreign == 0 && self.sleepers.is_empty() {
            if let Some(t) = self.runq.pop_front() {
                return t;
            }
        }
        self.next_slow()
    }

    #[cold]
    fn next_slow(&mut self) -> Tid {
        loop {
            self.wake_due();
            if let Some(t) = self.runq.pop_front() {
                return t;
            }
            // Signals arrived and nothing can run: the switching thread starts their handlers.
            if crate::signals::pending() && self.thread_ref(self.cur).is_some() {
                self.signal_wake = true;
                return self.cur;
            }
            let first = self.sleepers.iter().map(|s| s.0).min();
            if self.in_foreign > 0 {
                // Threads are in C: wait for one to come back (or for the next sleeper).
                let (list, cv) = &*self.returned;
                let guard = list.lock().unwrap_or_else(|e| e.into_inner());
                if guard.is_empty() {
                    match first {
                        Some(t) => {
                            let wait = t.saturating_duration_since(Instant::now());
                            drop(cv.wait_timeout(guard, wait));
                        }
                        // Woken now and then to see signals (a handler cannot notify).
                        None => {
                            drop(cv.wait_timeout(guard, Duration::from_millis(10)));
                        }
                    }
                }
                continue;
            }
            if let Some(first) = first {
                let now = Instant::now();
                if first > now {
                    std::thread::sleep((first - now).min(Duration::from_millis(10)));
                }
                continue;
            }
            self.resurrect();
            if self.runq.is_empty() {
                // No thread is left to run at all (the main thread ends the program itself).
                std::process::exit(1);
            }
        }
    }

    /// Gives the baton to the next thread and waits for it to come back. The running thread has
    /// been queued or blocked by the caller.
    ///
    /// # Safety
    /// `this` points to the scheduler, which other threads use while this one waits: the caller holds
    /// no reference into it across the call.
    pub unsafe fn switch(this: *mut Sched<V>) {
        let (baton, me, my_cv, next, next_cv) = {
            let s = &mut *this;
            let me = s.cur;
            let next = s.next();
            if next == me {
                return;
            }
            s.cur = next;
            let cv = |s: &Sched<V>, t: Tid| s.thread_ref(t).map(|t| t.cv.clone());
            match (cv(s, me), cv(s, next)) {
                (Some(a), Some(b)) => (s.baton.clone(), me, a, next, b),
                _ => return,
            }
        };
        baton.pass(next, &next_cv);
        Self::wait_turn(this, me, &my_cv);
    }

    /// Ends the running thread: the throwers blocked on it continue, and the baton goes to the next
    /// thread. The OS thread must not run Haskell code afterwards.
    ///
    /// # Safety
    /// As for `switch`.
    pub unsafe fn finish(this: *mut Sched<V>) {
        let s = &mut *this;
        let me = s.cur;
        if let Some(t) = s.remove_thread(me) {
            for (_, from) in t.pending {
                if let Some(f) = from {
                    s.wake_thrower(f, me);
                }
            }
        }
        let next = s.next();
        s.cur = next;
        if let Some(cv) = s.thread_ref(next).map(|t| t.cv.clone()) {
            let baton = s.baton.clone();
            baton.pass(next, &cv);
        }
    }

    /// Green threads (the native runtime, where every Haskell thread is a coroutine on one OS
    /// thread): the thread to run after the running one has been queued or blocked, made the
    /// running one. It may be the same thread.
    #[inline]
    pub fn pick_next(&mut self) -> Tid {
        let next = self.next();
        self.cur = next;
        next
    }

    /// Green threads: ends the running thread (as `finish`) and answers the next, made the running
    /// one.
    pub fn finish_pick(&mut self) -> Tid {
        let me = self.cur;
        if let Some(t) = self.remove_thread(me) {
            for (_, from) in t.pending {
                if let Some(f) = from {
                    self.wake_thrower(f, me);
                }
            }
        }
        self.pick_next()
    }

    /// Green threads: the running thread makes a safe foreign call on the OS thread `os_thread`
    /// (a worker), and is blocked until the worker leaves its number in the list answered (the
    /// caller then switches).
    pub fn foreign_begin(&mut self, interruptible: bool, os_thread: u64) -> Arc<(Mutex<Vec<Tid>>, Condvar)> {
        let t = self.me();
        t.state = State::Blocked(Why::Foreign);
        t.interruptible = interruptible;
        t.os_thread = os_thread;
        self.in_foreign += 1;
        self.returned.clone()
    }

    /// Every value the scheduler holds (for the collector of the native runtime).
    pub fn roots(&mut self, f: &mut dyn FnMut(&mut V)) {
        for slot in self.threads.iter_mut() {
            if let Some(t) = slot.as_deref_mut() {
                for (v, _) in t.pending.iter_mut() {
                    f(v);
                }
                if let Some(v) = t.handoff.as_mut() {
                    f(v);
                }
            }
        }
        for q in self.inline_waits.iter_mut() {
            q.for_each_mut(|w| {
                if let Some(v) = w.value.as_mut() {
                    f(v);
                }
            });
        }
        for q in self.waits.values_mut() {
            q.for_each_mut(|w| {
                if let Some(v) = w.value.as_mut() {
                    f(v);
                }
            });
        }
    }
}
