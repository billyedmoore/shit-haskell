//! A lazy interpreter for resolved code (see `code`). Thunks and closures share cells;
//! frames hold the slots of one function invocation, so evaluation never looks up
//! names.

pub mod bigint;
pub mod code;
pub mod conc;
pub mod prims;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::core::{Lit, TopBind};
use bigint::BigInt;
use code::{Acc, CAlt, Code, LamInfo};

pub type Cell = Rc<RefCell<Thunk>>;

/// The slots of one function invocation (or top-level binding), and its captures.
pub struct Frame {
    pub slots: RefCell<Vec<Cell>>,
    pub free: Rc<Vec<Cell>>,
}

pub type FrameRef = Rc<Frame>;

pub enum Thunk {
    Pending(&'static Code, FrameRef),
    Done(Value),
    /// Not yet initialised (a recursive binding); forcing it is an infinite loop.
    Blackhole,
    /// Evaluation raised a synchronous exception; forcing it again raises the same one.
    Raise(Rc<RtError>),
    /// Under evaluation by the given thread; re-entering it from that thread is an infinite
    /// loop, while another thread waits for it.
    Busy(i64),
}

#[derive(Clone)]
pub enum Value {
    Int(i64),
    Integer(Rc<BigInt>),
    Double(f64),
    Char(char),
    Con(Rc<ConVal>),
    Fun(Rc<FunVal>),
    /// A mutable reference (`IORef`): the cell it currently holds.
    Ref(Rc<RefCell<Cell>>),
    /// An immutable boxed vector (the cells are never reassigned after construction).
    Vec(Rc<RefCell<Vec<Cell>>>),
    /// An `Addr#`: a byte buffer and an offset into it.
    Addr(Rc<Vec<u8>>, usize),
    /// An `MVar`: empty, or holding a cell.
    MVar(Rc<RefCell<Option<Cell>>>),
}

pub struct ConVal {
    pub name: &'static str,
    pub args: Vec<Cell>,
}

pub struct FunVal {
    pub info: &'static LamInfo,
    pub free: Rc<Vec<Cell>>,
}

#[derive(Clone)]
pub enum RtError {
    /// A pattern-match failure or other compiler-generated failure.
    Fail(String),
    /// `error` called by the program.
    Error(String),
    /// `error` called by the program, with the call site (a backtrace frame).
    ErrorLoc(String, String),
    /// Exit with a status code.
    Exit(i32),
    /// A Haskell exception, as a `SomeException` value.
    Thrown(Value),
    /// An asynchronous exception (from `throwTo`), delivered at a safe point.
    Async(Value),
}

pub type RR = Result<Value, RtError>;

pub struct Runtime {
    /// Top-level cells, indexed as in `index`.
    pub cells: Vec<Cell>,
    pub index: HashMap<String, u32>,
    pub prog_name: String,
}

fn new_cell(t: Thunk) -> Cell {
    Rc::new(RefCell::new(t))
}

fn blank_frame(nslots: u32, free: Rc<Vec<Cell>>) -> FrameRef {
    let placeholder = new_cell(Thunk::Blackhole);
    Rc::new(Frame {
        slots: RefCell::new(vec![placeholder; nslots as usize]),
        free,
    })
}

impl Runtime {
    /// Compiles the program and creates a cell for each top-level binding. Top-level
    /// code is a constant: each binding is evaluated at most once, on first use.
    pub fn new(binds: Vec<TopBind>, prog_name: String) -> Runtime {
        let prog = code::compile_program(binds);
        let empty: Rc<Vec<Cell>> = Rc::new(Vec::new());
        let mut cells = Vec::with_capacity(prog.bodies.len());
        for (body, nslots) in prog.bodies {
            let body: &'static Code = Box::leak(Box::new(body));
            let frame = blank_frame(nslots, empty.clone());
            cells.push(new_cell(Thunk::Pending(body, frame)));
        }
        Runtime {
            cells,
            index: prog.index,
            prog_name,
        }
    }

    pub fn global(&self, name: &str) -> Option<Cell> {
        self.index.get(name).map(|i| self.cells[*i as usize].clone())
    }

    fn access(&self, acc: Acc, frame: &FrameRef) -> Cell {
        match acc {
            Acc::Local(i) => frame.slots.borrow()[i as usize].clone(),
            Acc::Free(i) => frame.free[i as usize].clone(),
            Acc::Global(i) => self.cells[i as usize].clone(),
        }
    }

    pub fn force(&self, cell: &Cell) -> RR {
        enum Step {
            Ret(Value),
            Run(&'static Code, FrameRef),
            Wait,
        }
        loop {
            let me = conc::current_thread_id();
            let step = {
                let mut t = cell.borrow_mut();
                match &*t {
                    Thunk::Done(v) => Step::Ret(v.clone()),
                    Thunk::Blackhole => return Err(RtError::Error("<<loop>>".into())),
                    Thunk::Raise(e) => return Err((**e).clone()),
                    Thunk::Busy(owner) if *owner == me => {
                        return Err(RtError::Error("<<loop>>".into()))
                    }
                    Thunk::Busy(_) => Step::Wait,
                    Thunk::Pending(code, frame) => {
                        let (code, frame) = (*code, frame.clone());
                        *t = Thunk::Busy(me);
                        Step::Run(code, frame)
                    }
                }
            };
            match step {
                Step::Ret(v) => return Ok(v),
                Step::Wait => conc::yield_now(),
                Step::Run(code, frame) => {
                    return match self.eval(code, &frame) {
                        Ok(v) => {
                            *cell.borrow_mut() = Thunk::Done(v.clone());
                            Ok(v)
                        }
                        Err(e @ (RtError::Async(_) | RtError::Exit(_))) => {
                            // Interrupted, not failed: the thunk can be evaluated again.
                            *cell.borrow_mut() = Thunk::Pending(code, frame);
                            Err(e)
                        }
                        Err(e) => {
                            *cell.borrow_mut() = Thunk::Raise(Rc::new(e.clone()));
                            Err(e)
                        }
                    };
                }
            }
        }
    }

    /// The cell for an expression: shared for variables, a value for literals, and a
    /// suspended computation otherwise.
    fn delay(&self, code: &'static Code, frame: &FrameRef) -> Cell {
        match code {
            Code::Var(acc) => self.access(*acc, frame),
            Code::Char(c) => new_cell(Thunk::Done(Value::Char(*c))),
            Code::Integer(b) => new_cell(Thunk::Done(Value::Integer(b.clone()))),
            Code::Int(i) => new_cell(Thunk::Done(Value::Int(*i))),
            _ => new_cell(Thunk::Pending(code, frame.clone())),
        }
    }

    /// Evaluates code to weak head normal form. Calls and `let`/`case` continuations in
    /// tail position loop rather than recurse, so tail-recursive programs run in
    /// constant Rust stack.
    pub fn eval(&self, code: &'static Code, frame: &FrameRef) -> RR {
        if conc::stack_exhausted() {
            return Err(RtError::Error("stack overflow".into()));
        }
        let mut code = code;
        let mut frame = frame.clone();
        'outer: loop {
            if let Some(exc) = conc::safe_point() {
                return Err(RtError::Async(exc));
            }
            match code {
                Code::Var(acc) => {
                    let cell = self.access(*acc, &frame);
                    return self.force(&cell);
                }
                Code::Integer(b) => return Ok(Value::Integer(b.clone())),
                Code::Int(i) => return Ok(Value::Int(*i)),
                Code::Double(d) => return Ok(Value::Double(*d)),
                Code::Frac(s) => return Ok(Value::Double(s.parse().unwrap_or(0.0))),
                Code::Char(c) => return Ok(Value::Char(*c)),
                Code::Str(s) => return Ok(string_value(s)),
                Code::Addr(bytes) => return Ok(Value::Addr(bytes.clone(), 0)),
                Code::App(f, a) => {
                    let fv = self.eval(f, &frame)?;
                    let arg = self.delay(a, &frame);
                    match fv {
                        Value::Fun(fun) => {
                            let info: &'static LamInfo = fun.info;
                            let new_frame = blank_frame(info.nslots, fun.free.clone());
                            new_frame.slots.borrow_mut()[0] = arg;
                            frame = new_frame;
                            code = &info.body;
                        }
                        _ => return Err(RtError::Fail("internal: application of a non-function".into())),
                    }
                }
                Code::Lam(info) => {
                    let captured: Vec<Cell> = info.captures.iter().map(|a| self.access(*a, &frame)).collect();
                    return Ok(Value::Fun(Rc::new(FunVal {
                        info,
                        free: Rc::new(captured),
                    })));
                }
                Code::Let(binds, body) => {
                    let mut cells = Vec::with_capacity(binds.len());
                    {
                        let mut slots = frame.slots.borrow_mut();
                        for (slot, _) in binds {
                            let cell = new_cell(Thunk::Blackhole);
                            slots[*slot as usize] = cell.clone();
                            cells.push(cell);
                        }
                    }
                    for ((_, bound), cell) in binds.iter().zip(&cells) {
                        *cell.borrow_mut() = Thunk::Pending(bound, frame.clone());
                    }
                    code = body;
                }
                Code::Case(scrut, bslot, alts) => {
                    let v = self.eval(scrut, &frame)?;
                    if *bslot != u32::MAX {
                        frame.slots.borrow_mut()[*bslot as usize] = new_cell(Thunk::Done(v.clone()));
                    }
                    for alt in alts {
                        match alt {
                            CAlt::Con(name, slots, body) => {
                                if let Value::Con(c) = &v {
                                    if c.name == *name {
                                        let mut fs = frame.slots.borrow_mut();
                                        for (slot, cell) in slots.iter().zip(&c.args) {
                                            fs[*slot as usize] = cell.clone();
                                        }
                                        drop(fs);
                                        code = body;
                                        continue 'outer;
                                    }
                                }
                            }
                            CAlt::Lit(l, body) => {
                                if lit_matches(&v, l) {
                                    code = body;
                                    continue 'outer;
                                }
                            }
                            CAlt::Default(body) => {
                                code = body;
                                continue 'outer;
                            }
                        }
                    }
                    return Err(RtError::Fail("Non-exhaustive patterns in case".into()));
                }
                Code::Con(name, args) => {
                    let cells: Vec<Cell> = args.iter().map(|a| self.delay(a, &frame)).collect();
                    return Ok(Value::Con(Rc::new(ConVal { name, args: cells })));
                }
                Code::Prim(name, args) => {
                    if name == "ratioLit" {
                        if let [Code::Frac(s)] = args.as_slice() {
                            return Ok(prims::ratio_literal(s));
                        }
                    }
                    if name == "seq" {
                        // `seq a b` forces `a`, then continues with `b` in tail position.
                        if let [a, b] = args.as_slice() {
                            self.eval(a, &frame)?;
                            code = b;
                            continue 'outer;
                        }
                    }
                    let cells: Vec<Cell> = args.iter().map(|a| self.delay(a, &frame)).collect();
                    return prims::call(self, name, &cells);
                }
                Code::Fail(msg) => return Err(RtError::Fail(msg.clone())),
            }
        }
    }

    pub fn apply(&self, f: Value, arg: Cell) -> RR {
        match f {
            Value::Fun(fun) => {
                let info = fun.info;
                let frame = blank_frame(info.nslots, fun.free.clone());
                frame.slots.borrow_mut()[0] = arg;
                self.eval(&info.body, &frame)
            }
            _ => Err(RtError::Fail("internal: application of a non-function".into())),
        }
    }

    /// Forces a Haskell list of characters into a Rust string.
    pub fn force_string(&self, cell: &Cell) -> Result<String, RtError> {
        let mut out = String::new();
        let mut cur = self.force(cell)?;
        loop {
            match cur {
                Value::Con(c) if c.name == "Builtin.:" => {
                    match self.force(&c.args[0])? {
                        Value::Char(ch) => out.push(ch),
                        _ => return Err(RtError::Fail("internal: string of non-characters".into())),
                    }
                    cur = self.force(&c.args[1])?;
                }
                _ => return Ok(out),
            }
        }
    }
}

pub fn lit_value(l: &Lit) -> RR {
    Ok(match l {
        Lit::Integer(s) => Value::Integer(Rc::new(BigInt::parse(s).ok_or_else(|| RtError::Fail("bad integer literal".into()))?)),
        Lit::Frac(s) => Value::Double(s.parse().unwrap_or(0.0)),
        Lit::Int(i) => Value::Int(*i),
        Lit::Double(d) => Value::Double(*d),
        Lit::Char(c) => Value::Char(*c),
        Lit::Str(s) => string_value(s),
        Lit::Addr(s) => Value::Addr(Rc::new(s.chars().map(|c| c as u32 as u8).chain(std::iter::once(0)).collect()), 0),
    })
}

/// A Haskell string value: a fully built list of characters.
pub fn string_value(s: &str) -> Value {
    let mut v = Value::Con(Rc::new(ConVal {
        name: "Builtin.[]",
        args: vec![],
    }));
    for ch in s.chars().rev() {
        v = Value::Con(Rc::new(ConVal {
            name: "Builtin.:",
            args: vec![new_cell(Thunk::Done(Value::Char(ch))), new_cell(Thunk::Done(v))],
        }));
    }
    v
}

fn lit_matches(v: &Value, l: &Lit) -> bool {
    match (v, l) {
        (Value::Char(a), Lit::Char(b)) => a == b,
        (Value::Int(a), Lit::Int(b)) => a == b,
        (Value::Int(a), Lit::Integer(s)) => s.parse::<i64>().ok() == Some(*a),
        (Value::Integer(a), Lit::Integer(s)) => BigInt::parse(s).is_some_and(|b| **a == b),
        _ => false,
    }
}

/// Runs `Main.main`, returning the process exit code.
pub fn run_main(rt: &Runtime) -> i32 {
    conc::gil_acquire();
    let Some(main_cell) = rt.global("Main.main") else {
        eprintln!("{}: The IO action ‘main’ is not defined in module ‘Main’", rt.prog_name);
        return 1;
    };
    let result = (|| -> Result<(), RtError> {
        let io = rt.force(&main_cell)?;
        // `main :: IO ()` is `IO (State# RealWorld -> (# State# RealWorld, () #))`.
        let run = match io {
            other_io @ Value::Con(_) | other_io @ Value::Fun(_) => other_io,
            _ => return Err(RtError::Fail("internal: main is not an IO action".into())),
        };
        let unit = new_cell(Thunk::Done(unit_value()));
        rt.apply(run, unit)?;
        Ok(())
    })();
    let code = match result {
        Ok(()) => 0,
        Err(RtError::Exit(code)) => code,
        Err(RtError::ErrorLoc(msg, site)) => {
            prims::flush_stdout();
            eprintln!(
                "{}: Uncaught exception ghc-internal:GHC.Internal.Exception.ErrorCall:\n\n{}\n\nHasCallStack backtrace:\n  {}\n",
                rt.prog_name, msg, site
            );
            1
        }
        Err(RtError::Error(msg)) | Err(RtError::Fail(msg)) => {
            prims::flush_stdout();
            eprintln!("{}: {}", rt.prog_name, msg);
            1
        }
        Err(RtError::Thrown(exc)) | Err(RtError::Async(exc)) => {
            prims::flush_stdout();
            let text = match &exc {
                Value::Con(c) if c.args.len() == 3 => rt.force_string(&c.args[2]).unwrap_or_default(),
                _ => String::from("uncaught exception"),
            };
            eprintln!("{}: {}", rt.prog_name, text);
            1
        }
    };
    prims::flush_stdout();
    code
}

pub fn bool_value(b: bool) -> Value {
    Value::Con(Rc::new(ConVal {
        name: if b { "GHC.Types.True" } else { "GHC.Types.False" },
        args: vec![],
    }))
}

pub fn unit_value() -> Value {
    Value::Con(Rc::new(ConVal {
        name: "Builtin.()",
        args: vec![],
    }))
}

pub fn con_value(name: &'static str, args: Vec<Value>) -> Value {
    Value::Con(Rc::new(ConVal {
        name,
        args: args.into_iter().map(|v| new_cell(Thunk::Done(v))).collect(),
    }))
}
