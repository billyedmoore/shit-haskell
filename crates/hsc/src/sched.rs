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

/// A thread's number: 1 for the main thread, increasing with every fork.
pub type Tid = u64;

/// Set at the end of every time slice once a second thread exists: the running thread gives way
/// at its next safe point.
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
}

pub struct Sched<V> {
    cur: Tid,
    next_id: Tid,
    threads: HashMap<Tid, Thread<V>>,
    runq: VecDeque<Tid>,
    /// Sleeping threads with the instants they wake at.
    sleepers: Vec<(Instant, Tid)>,
    /// The queues of the `MVar`s that have waiters, by key.
    waits: HashMap<u64, VecDeque<Waiter<V>>>,
    /// The threads whose transactions retried.
    stm_waiters: VecDeque<Tid>,
    next_key: u64,
    baton: Arc<Baton>,
    slice: Duration,
}

impl<V> Sched<V> {
    /// A scheduler with the main thread running; `slice_ms` is the time slice.
    pub fn new(slice_ms: u64) -> Sched<V> {
        let mut threads = HashMap::new();
        threads.insert(1, Self::thread(0));
        Sched {
            cur: 1,
            next_id: 2,
            threads,
            runq: VecDeque::new(),
            sleepers: Vec::new(),
            waits: HashMap::new(),
            stm_waiters: VecDeque::new(),
            next_key: 1,
            baton: Arc::new(Baton { turn: Mutex::new(1) }),
            slice: Duration::from_millis(slice_ms),
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
        }
    }

    /// The running thread.
    pub fn current(&self) -> Tid {
        self.cur
    }

    fn me(&mut self) -> &mut Thread<V> {
        self.threads.get_mut(&self.cur).expect("the running thread is in the table")
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
                std::thread::sleep(slice);
                PREEMPT.store(true, Ordering::Relaxed);
            });
        });
        let id = self.next_id;
        self.next_id += 1;
        let t = Self::thread(self.me().mask);
        let cv = t.cv.clone();
        self.threads.insert(id, t);
        self.runq.push_back(id);
        (id, self.baton.clone(), cv)
    }

    /// The masking state of the running thread.
    pub fn mask(&self) -> u8 {
        self.threads.get(&self.cur).map_or(0, |t| t.mask)
    }

    pub fn set_mask(&mut self, mask: u8) {
        self.me().mask = mask;
    }

    /// Whether more than one thread exists.
    pub fn multi(&self) -> bool {
        self.threads.len() > 1
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
    pub fn enqueue(&mut self, key: u64, kind: WaitKind, value: Option<V>, front: bool) -> bool {
        if !self.may_block() {
            return false;
        }
        let me = self.cur;
        let w = Waiter { thread: me, kind, value };
        let q = self.waits.entry(key).or_default();
        if front {
            q.push_front(w);
        } else {
            q.push_back(w);
        }
        self.me().state = State::Blocked(if kind == WaitKind::Reader { Why::MVarRead(key) } else { Why::MVar(key) });
        true
    }

    /// The kind of the first waiter of an `MVar`.
    pub fn front_kind(&self, key: u64) -> Option<WaitKind> {
        self.waits.get(&key).and_then(|q| q.front()).map(|w| w.kind)
    }

    /// Takes the first waiter of an `MVar` out of its queue; the caller wakes it up.
    pub fn pop_waiter(&mut self, key: u64) -> Option<Waiter<V>> {
        let q = self.waits.get_mut(&key)?;
        let w = q.pop_front();
        if q.is_empty() {
            self.waits.remove(&key);
        }
        w
    }

    /// Makes a blocked thread (already out of its queue) runnable, with a value handed to it.
    pub fn wake(&mut self, t: Tid, handoff: Option<V>) {
        if let Some(th) = self.threads.get_mut(&t) {
            th.state = State::Ready;
            th.handoff = handoff;
            self.runq.push_back(t);
        }
    }

    /// The value handed to the running thread by the operation that woke it.
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
            Why::MVar(key) | Why::MVarRead(key) => {
                if let Some(q) = self.waits.get_mut(&key) {
                    q.retain(|w| w.thread != t);
                    if q.is_empty() {
                        self.waits.remove(&key);
                    }
                }
            }
            Why::Stm => self.stm_waiters.retain(|w| *w != t),
            Why::Delay => self.sleepers.retain(|(_, w)| *w != t),
            Why::Throw(target) => {
                // The exception it was throwing is withdrawn.
                if let Some(th) = self.threads.get_mut(&target) {
                    th.pending.retain(|(_, from)| *from != Some(t));
                }
            }
        }
        if let Some(th) = self.threads.get_mut(&t) {
            th.state = State::Ready;
            self.runq.push_back(t);
        }
    }

    /// Throws an exception to another thread. An unmasked target, or one blocked while masked
    /// interruptibly, will raise it when it next runs, and the thrower continues. Otherwise the
    /// thrower is blocked until the target receives the exception: the caller switches.
    pub fn throw_to(&mut self, target: Tid, exc: V) -> Thrown {
        let me = self.cur;
        let Some(t) = self.threads.get_mut(&target) else { return Thrown::Done };
        let blocked = match t.state {
            State::Blocked(why) => Some(why),
            State::Ready => None,
        };
        if t.mask == 0 || (t.mask == 2 && blocked.is_some()) {
            t.pending.push_back((exc, None));
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
        if let Some(t) = self.threads.get_mut(&target) {
            t.pending.push_back((exc, Some(me)));
        }
        self.me().state = State::Blocked(Why::Throw(target));
        Thrown::Blocked
    }

    /// What the running thread must raise now, if anything: the notice that it is blocked for ever,
    /// or an exception thrown to it that its masking state lets in. Called at safe points, after
    /// every switch, and when the thread unmasks.
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
        self.threads.get(&self.cur).is_some_and(|t| t.marker.is_some() || (!t.pending.is_empty() && (t.mask == 0 || t.interrupted)))
    }

    fn wake_thrower(&mut self, s: Tid, target: Tid) {
        if self.threads.get(&s).is_some_and(|t| t.state == State::Blocked(Why::Throw(target))) {
            self.wake(s, None);
        }
    }

    /// The status of a thread as `threadStatus#` numbers it.
    pub fn status(&self, t: Tid) -> i64 {
        match self.threads.get(&t).map(|t| t.state) {
            None => 16,
            Some(State::Ready) => 0,
            Some(State::Blocked(Why::MVar(_))) => 1,
            Some(State::Blocked(Why::MVarRead(_))) => 14,
            Some(State::Blocked(Why::Stm)) => 6,
            Some(State::Blocked(Why::Delay)) => 5,
            Some(State::Blocked(Why::Throw(_))) => 12,
        }
    }

    /// Moves the sleepers whose time has come to the run queue, earliest first.
    fn wake_due(&mut self) {
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
            .filter_map(|(id, t)| match t.state {
                State::Blocked(why @ (Why::MVar(_) | Why::MVarRead(_) | Why::Stm)) => Some((*id, why)),
                _ => None,
            })
            .collect();
        if found.is_empty() {
            found = self
                .threads
                .iter()
                .filter_map(|(id, t)| match t.state {
                    State::Blocked(why) => Some((*id, why)),
                    State::Ready => None,
                })
                .collect();
        }
        found.sort_by_key(|(id, _)| (*id == 1, *id));
        for (id, why) in found {
            self.unblock(id, why);
            if let Some(t) = self.threads.get_mut(&id) {
                t.marker = Some(why);
            }
        }
    }

    /// The thread to run next; the running one has been queued, blocked or removed.
    fn next(&mut self) -> Tid {
        loop {
            self.wake_due();
            if let Some(t) = self.runq.pop_front() {
                return t;
            }
            if let Some(first) = self.sleepers.iter().map(|s| s.0).min() {
                let now = Instant::now();
                if first > now {
                    std::thread::sleep(first - now);
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
            let cv = |s: &Sched<V>, t: Tid| s.threads.get(&t).map(|t| t.cv.clone());
            match (cv(s, me), cv(s, next)) {
                (Some(a), Some(b)) => (s.baton.clone(), me, a, next, b),
                _ => return,
            }
        };
        baton.pass(next, &next_cv);
        baton.wait(me, &my_cv);
    }

    /// Ends the running thread: the throwers blocked on it continue, and the baton goes to the next
    /// thread. The OS thread must not run Haskell code afterwards.
    ///
    /// # Safety
    /// As for `switch`.
    pub unsafe fn finish(this: *mut Sched<V>) {
        let s = &mut *this;
        let me = s.cur;
        if let Some(t) = s.threads.remove(&me) {
            for (_, from) in t.pending {
                if let Some(f) = from {
                    s.wake_thrower(f, me);
                }
            }
        }
        let next = s.next();
        s.cur = next;
        if let Some(cv) = s.threads.get(&next).map(|t| t.cv.clone()) {
            let baton = s.baton.clone();
            baton.pass(next, &cv);
        }
    }

    /// Every value the scheduler holds (for the collector of the native runtime).
    pub fn roots(&mut self, f: &mut dyn FnMut(&mut V)) {
        for t in self.threads.values_mut() {
            for (v, _) in t.pending.iter_mut() {
                f(v);
            }
            if let Some(v) = t.handoff.as_mut() {
                f(v);
            }
        }
        for q in self.waits.values_mut() {
            for w in q.iter_mut() {
                if let Some(v) = w.value.as_mut() {
                    f(v);
                }
            }
        }
    }
}
