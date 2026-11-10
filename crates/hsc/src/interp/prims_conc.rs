//! The primitives of concurrency in the interpreter, by their GHC names: threads, `MVar#`, the
//! masking of asynchronous exceptions, and transactions over `TVar#`. The order of events is the
//! shared scheduler's (`crate::sched`); this file moves the interpreter's values through it.
//!
//! A transaction runs to its end without giving way to other threads, writing its variables in
//! place and keeping the old contents in a log: `retry#` and an exception undo the writes.

use std::cell::RefCell;
use std::rc::Rc;

use super::conc::{self, Locked};
use super::prims::int_of;
use super::{new_cell, unit_value, Cell, ConVal, RR, Runtime, RtError, Thunk, Value};
use crate::sched::{Interrupt, Thrown, WaitKind};

fn unit_cell() -> Cell {
    new_cell(Thunk::Done(unit_value()))
}

fn cell_of(v: Value) -> Cell {
    new_cell(Thunk::Done(v))
}

fn tuple(args: Vec<Cell>) -> Value {
    let name = match args.len() {
        2 => "Builtin.(,)",
        3 => "Builtin.(,,)",
        _ => "Builtin.(,,,)",
    };
    Value::Con(Rc::new(ConVal { name, args }))
}

/// `(# s, v #)` with the second component as it is (not evaluated).
fn with_state(c: Cell) -> RR {
    Ok(tuple(vec![unit_cell(), c]))
}

fn int_result(n: i64) -> RR {
    with_state(cell_of(Value::Int(n)))
}

fn run_io(rt: &Runtime, io: &Cell, state: &Cell) -> RR {
    let f = rt.force(io)?;
    rt.apply(f, state.clone())
}

/// The error that raises what the scheduler tells this thread to raise.
pub fn interrupt_error(rt: &Runtime, i: Interrupt<Cell>) -> RtError {
    let library = |name: &str, text: &str| match rt.global(name).map(|c| rt.force(&c)) {
        Some(Ok(v)) => RtError::Async(v),
        _ => RtError::Error(text.into()),
    };
    match i {
        Interrupt::Exception(cell) => match rt.force(&cell) {
            Ok(v) => RtError::Async(v),
            Err(e) => e,
        },
        Interrupt::BlockedOnMVar => library(
            "GHC.Internal.IO.Exception.blockedIndefinitelyOnMVar",
            "thread blocked indefinitely in an MVar operation",
        ),
        Interrupt::BlockedOnSTM => library(
            "GHC.Internal.IO.Exception.blockedIndefinitelyOnSTM",
            "thread blocked indefinitely in an STM transaction",
        ),
    }
}

/// Raises what is waiting for this thread, if anything (after every switch of threads).
fn check(rt: &Runtime) -> Result<(), RtError> {
    match conc::interrupt() {
        Some(i) => Err(interrupt_error(rt, i)),
        None => Ok(()),
    }
}

type MVar = Rc<RefCell<Option<Cell>>>;

fn mvar(rt: &Runtime, c: &Cell) -> Result<(MVar, u64), RtError> {
    match rt.force(c)? {
        Value::MVar(m) => {
            let key = Rc::as_ptr(&m) as usize as u64;
            Ok((m, key))
        }
        _ => Err(RtError::Fail("internal: MVar operation on a non-MVar".into())),
    }
}

fn tvar(rt: &Runtime, c: &Cell) -> Result<Rc<RefCell<Cell>>, RtError> {
    match rt.force(c)? {
        Value::Ref(r) => Ok(r),
        _ => Err(RtError::Fail("internal: TVar operation on a non-TVar".into())),
    }
}

/// Takes the contents of a full `MVar`; the first thread waiting to put, if any, puts.
fn take_full(mv: &MVar, key: u64) -> Option<Cell> {
    let cell = mv.borrow_mut().take()?;
    let putter = conc::with(|s| if s.front_kind(key) == Some(WaitKind::Putter) { s.pop_waiter(key) } else { None });
    if let Some(w) = putter {
        *mv.borrow_mut() = w.value;
        conc::with(|s| s.wake(w.thread, None));
    }
    Some(cell)
}

/// Puts into an empty `MVar`: the readers waiting see the value, and the first taker takes it.
fn put_empty(mv: &MVar, key: u64, value: Cell) {
    loop {
        match conc::with(|s| s.pop_waiter(key)) {
            Some(w) if w.kind == WaitKind::Reader => conc::with(|s| s.wake(w.thread, Some(value.clone()))),
            Some(w) => {
                conc::with(|s| s.wake(w.thread, Some(value)));
                return;
            }
            None => {
                *mv.borrow_mut() = Some(value);
                return;
            }
        }
    }
}

/// Waits in the queue of an `MVar`; answers the value handed over by the thread that woke this one.
fn wait(rt: &Runtime, key: u64, kind: WaitKind, value: Option<Cell>) -> Result<Option<Cell>, RtError> {
    if conc::with(|s| s.enqueue(key, kind, value, kind == WaitKind::Reader)) {
        conc::switch();
    }
    check(rt)?;
    Ok(conc::with(|s| s.take_handoff()))
}

fn handed(c: Option<Cell>) -> RR {
    match c {
        Some(c) => with_state(c),
        None => Err(RtError::Fail("internal: woken from an MVar without a value".into())),
    }
}

/// Runs an action with a masking state, and puts the state back afterwards. An exception that
/// waited for the thread to unmask is raised at once.
fn with_mask(rt: &Runtime, state: u8, args: &[Cell]) -> RR {
    let old = conc::with(|s| s.mask());
    if old == state {
        return run_io(rt, &args[0], &args[1]);
    }
    conc::with(|s| s.set_mask(state));
    if state == 0 {
        if let Err(e) = check(rt) {
            conc::with(|s| s.set_mask(old));
            return Err(e);
        }
    }
    let r = run_io(rt, &args[0], &args[1]);
    conc::with(|s| s.set_mask(old));
    if old == 0 && r.is_ok() {
        check(rt)?;
    }
    r
}

/// The masking state of this thread (for `catch#`, which puts it back for the handler).
pub fn mask() -> u8 {
    conc::with(|s| s.mask())
}

/// Runs the handler of a `catch#` that was entered in masking state `saved`: masked (as GHC runs
/// handlers), and with the state of the `catch#` back afterwards.
pub fn handle(rt: &Runtime, saved: u8, run: impl FnOnce() -> RR) -> RR {
    conc::with(|s| s.set_mask(if saved == 0 { 2 } else { saved }));
    let r = run();
    conc::with(|s| s.set_mask(saved));
    if saved == 0 && r.is_ok() {
        check(rt)?;
    }
    r
}

/// Starts a Haskell thread on an OS thread of its own; it runs when the scheduler gives it the turn.
fn fork(rt: &Runtime, action: &Cell) -> RR {
    let (id, baton, cv) = conc::with(|s| s.spawn());
    let rt_ptr = Locked(rt as *const Runtime);
    let payload = Locked(action.clone());
    let spawned = std::thread::Builder::new().stack_size(4 << 30).spawn(move || {
        let (rt_ptr, payload) = (rt_ptr, payload);
        baton.wait(id, &cv);
        conc::set_stack_base();
        conc::set_current(id as i64);
        {
            // SAFETY: the runtime is leaked and lives for the whole process.
            let rt: &Runtime = unsafe { &*rt_ptr.0 };
            let cell = payload.0;
            // A thread killed before it first runs does nothing.
            let outcome = match conc::interrupt() {
                Some(_) => Ok(unit_value()),
                None => run_io(rt, &cell, &unit_cell()),
            };
            match outcome {
                Err(RtError::Exit(code)) => {
                    super::prims::flush_stdout();
                    std::process::exit(code);
                }
                Err(RtError::Error(msg)) | Err(RtError::Fail(msg)) | Err(RtError::ErrorLoc(msg, _)) => {
                    super::prims::flush_stdout();
                    eprintln!("{}: {}", rt.prog_name, msg);
                }
                _ => {}
            }
        }
        // Everything the thread held is dropped: the turn passes on.
        conc::finish();
    });
    if spawned.is_err() {
        return Err(RtError::Error("cannot create a thread".into()));
    }
    int_result(id as i64)
}

/// `throwTo`: raises an exception in another thread, waiting for it when the target is masked.
fn throw_to(rt: &Runtime, args: &[Cell]) -> RR {
    let target = int_of(rt, &args[0])?;
    let exc = rt.force(&args[1])?;
    if target == conc::current_thread_id() {
        return Err(RtError::Async(exc));
    }
    if conc::with(|s| s.throw_to(target as u64, cell_of(exc))) == Thrown::Blocked {
        conc::switch();
    }
    check(rt)?;
    Ok(unit_value())
}

/// `atomically#`: runs a transaction until it commits.
fn atomically(rt: &Runtime, args: &[Cell]) -> RR {
    loop {
        let mark = conc::stm_mark();
        conc::stm_enter(true);
        let r = run_io(rt, &args[0], &args[1]);
        conc::stm_enter(false);
        match r {
            Ok(v) => {
                if conc::stm_commit(mark) {
                    conc::with(|s| s.stm_wake_all());
                }
                return Ok(v);
            }
            Err(RtError::Retry) => {
                conc::stm_rollback(mark);
                if conc::in_stm() {
                    // A transaction inside a transaction (through unsafe IO): the outer one retries.
                    return Err(RtError::Retry);
                }
                if conc::with(|s| s.stm_block()) {
                    conc::switch();
                }
                check(rt)?;
            }
            Err(e) => {
                conc::stm_rollback(mark);
                return Err(e);
            }
        }
    }
}

pub fn call(rt: &Runtime, name: &str, args: &[Cell]) -> Option<RR> {
    let r: RR = match name {
        // ----- threads (a ThreadId# is the thread's number) -----
        "fork#" => fork(rt, &args[0]),
        "forkOn#" => fork(rt, &args[1]),
        "myThreadId#" => int_result(conc::current_thread_id()),
        "yield#" => {
            conc::yield_now();
            check(rt).map(|_| unit_value())
        }
        "delay#" => (|| {
            let micros = int_of(rt, &args[0])?.max(0) as u64;
            if conc::with(|s| s.sleep_cur(micros)) {
                conc::switch();
            }
            check(rt)?;
            Ok(unit_value())
        })(),
        "killThread#" => throw_to(rt, args),
        "threadStatus#" => int_of(rt, &args[0]).map(|t| {
            let status = conc::with(|s| s.status(t as u64));
            tuple(vec![unit_cell(), cell_of(Value::Int(status)), cell_of(Value::Int(0)), cell_of(Value::Int(0))])
        }),
        "isCurrentThreadBound#" => int_result(0),
        "labelThread#" => Ok(unit_value()),
        "noDuplicate#" => Ok(unit_value()),

        // ----- masking -----
        "maskAsyncExceptions#" => with_mask(rt, 2, args),
        "maskUninterruptible#" => with_mask(rt, 1, args),
        "unmaskAsyncExceptions#" => with_mask(rt, 0, args),
        "getMaskingState#" => int_result(mask() as i64),

        // ----- MVar# -----
        "newMVar#" => with_state(cell_of(Value::MVar(Rc::new(RefCell::new(None))))),
        "takeMVar#" => mvar(rt, &args[0]).and_then(|(mv, key)| match take_full(&mv, key) {
            Some(c) => with_state(c),
            None => handed(wait(rt, key, WaitKind::Taker, None)?),
        }),
        "readMVar#" => mvar(rt, &args[0]).and_then(|(mv, key)| {
            let current = mv.borrow().clone();
            match current {
                Some(c) => with_state(c),
                None => handed(wait(rt, key, WaitKind::Reader, None)?),
            }
        }),
        "putMVar#" => mvar(rt, &args[0]).and_then(|(mv, key)| {
            if mv.borrow().is_some() {
                wait(rt, key, WaitKind::Putter, Some(args[1].clone()))?;
            } else {
                put_empty(&mv, key, args[1].clone());
            }
            Ok(unit_value())
        }),
        "tryTakeMVar#" => mvar(rt, &args[0]).map(|(mv, key)| match take_full(&mv, key) {
            Some(c) => tuple(vec![unit_cell(), cell_of(Value::Int(1)), c]),
            None => tuple(vec![unit_cell(), cell_of(Value::Int(0)), unit_cell()]),
        }),
        "tryReadMVar#" => mvar(rt, &args[0]).map(|(mv, _)| {
            let current = mv.borrow().clone();
            match current {
                Some(c) => tuple(vec![unit_cell(), cell_of(Value::Int(1)), c]),
                None => tuple(vec![unit_cell(), cell_of(Value::Int(0)), unit_cell()]),
            }
        }),
        "tryPutMVar#" => mvar(rt, &args[0]).and_then(|(mv, key)| {
            if mv.borrow().is_some() {
                return int_result(0);
            }
            put_empty(&mv, key, args[1].clone());
            int_result(1)
        }),
        "isEmptyMVar#" => mvar(rt, &args[0]).and_then(|(mv, _)| int_result(mv.borrow().is_none() as i64)),

        // ----- transactions -----
        "newTVar#" => with_state(cell_of(Value::Ref(Rc::new(RefCell::new(args[0].clone()))))),
        "readTVar#" | "readTVarIO#" => tvar(rt, &args[0]).and_then(|r| {
            let contents = r.borrow().clone();
            with_state(contents)
        }),
        "writeTVar#" => tvar(rt, &args[0]).map(|r| {
            let old = std::mem::replace(&mut *r.borrow_mut(), args[1].clone());
            if conc::in_stm() {
                conc::stm_log(r, old);
            }
            unit_value()
        }),
        "atomically#" => atomically(rt, args),
        "retry#" => Err(RtError::Retry),
        // catchRetry# a b: when the first alternative retries, its writes are undone and the second runs.
        "catchRetry#" => {
            let mark = conc::stm_mark();
            match run_io(rt, &args[0], &args[2]) {
                Err(RtError::Retry) => {
                    conc::stm_rollback(mark);
                    run_io(rt, &args[1], &args[2])
                }
                other => other,
            }
        }
        // catchSTM# a h: an exception undoes the writes of the action, and the handler runs.
        "catchSTM#" => {
            let mark = conc::stm_mark();
            match run_io(rt, &args[0], &args[2]) {
                Ok(v) => Ok(v),
                Err(RtError::Retry) => Err(RtError::Retry),
                Err(RtError::Exit(code)) => Err(RtError::Exit(code)),
                Err(e) => {
                    conc::stm_rollback(mark);
                    match super::prims_ghc::exception_of(rt, e) {
                        Ok(exc) => (|| {
                            let handler = rt.force(&args[1])?;
                            let action = rt.apply(handler, cell_of(exc))?;
                            rt.apply(action, args[2].clone())
                        })(),
                        Err(e) => Err(e),
                    }
                }
            }
        }
        _ => return None,
    };
    Some(r)
}
