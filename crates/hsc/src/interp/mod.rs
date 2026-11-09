//! A lazy interpreter for Core. Thunks are shared mutable cells, global Core is leaked
//! so that thunks can refer to it by `'static` reference, and environments are linked
//! frames of cells.

pub mod bigint;
pub mod prims;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::core::{Alt, Core, Lit, TopBind};
use bigint::BigInt;

pub type Cell = Rc<RefCell<Thunk>>;

pub enum Thunk {
    Pending(&'static Core, Env),
    Done(Value),
    /// Under evaluation; re-entering it is an infinite loop.
    Blackhole,
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
}

pub struct ConVal {
    pub name: &'static str,
    pub args: Vec<Cell>,
}

pub struct FunVal {
    pub param: &'static str,
    pub body: &'static Core,
    pub env: Env,
}

/// A linked environment frame.
pub struct Frame {
    pub name: &'static str,
    pub cell: Cell,
    pub next: Env,
}

pub type Env = Option<Rc<Frame>>;

#[derive(Debug, Clone)]
pub enum RtError {
    /// A pattern-match failure or other compiler-generated failure.
    Fail(String),
    /// `error` called by the program.
    Error(String),
    /// Exit with a status code.
    Exit(i32),
}

pub type RR = Result<Value, RtError>;

pub struct Runtime {
    pub globals: HashMap<&'static str, Cell>,
    pub prog_name: String,
}

fn push(env: &Env, name: &'static str, cell: Cell) -> Env {
    Some(Rc::new(Frame {
        name,
        cell,
        next: env.clone(),
    }))
}

fn new_cell(t: Thunk) -> Cell {
    Rc::new(RefCell::new(t))
}

impl Runtime {
    /// Builds a runtime from the program's top-level bindings. Core is leaked so that it
    /// lives for the whole run.
    pub fn new(binds: Vec<TopBind>, prog_name: String) -> Runtime {
        let mut globals = HashMap::new();
        for b in binds {
            let name: &'static str = Box::leak(b.name.into_boxed_str());
            let body: &'static Core = Box::leak(Box::new(b.body));
            globals.insert(name, new_cell(Thunk::Pending(body, None)));
        }
        Runtime { globals, prog_name }
    }

    fn lookup(&self, name: &str, env: &Env) -> Result<Cell, RtError> {
        let mut cur = env;
        while let Some(frame) = cur {
            if frame.name == name {
                return Ok(frame.cell.clone());
            }
            cur = &frame.next;
        }
        self.globals
            .get(name)
            .cloned()
            .ok_or_else(|| RtError::Fail(format!("internal: unbound variable {name}")))
    }

    pub fn force(&self, cell: &Cell) -> RR {
        let (core, env) = {
            let mut t = cell.borrow_mut();
            match &*t {
                Thunk::Done(v) => return Ok(v.clone()),
                Thunk::Blackhole => return Err(RtError::Error("<<loop>>".into())),
                Thunk::Pending(core, env) => {
                    let (core, env) = (*core, env.clone());
                    *t = Thunk::Blackhole;
                    (core, env)
                }
            }
        };
        match self.eval(core, &env) {
            Ok(v) => {
                *cell.borrow_mut() = Thunk::Done(v.clone());
                Ok(v)
            }
            Err(e) => {
                *cell.borrow_mut() = Thunk::Pending(core, env);
                Err(e)
            }
        }
    }

    /// The cell for an expression: shared for variables, a value for literals, and a
    /// suspended computation otherwise.
    fn delay(&self, core: &'static Core, env: &Env) -> Result<Cell, RtError> {
        Ok(match core {
            Core::Var(x) => self.lookup(x, env)?,
            Core::Lit(Lit::Char(c)) => new_cell(Thunk::Done(Value::Char(*c))),
            _ => new_cell(Thunk::Pending(core, env.clone())),
        })
    }

    /// Evaluates Core to weak head normal form. Calls and `let`/`case` continuations in
    /// tail position loop rather than recurse, so tail-recursive programs run in constant
    /// Rust stack.
    pub fn eval(&self, core: &'static Core, env: &Env) -> RR {
        let mut core = core;
        let mut env = env.clone();
        'outer: loop {
            match core {
                Core::Var(x) => {
                    let cell = self.lookup(x, &env)?;
                    return self.force(&cell);
                }
                Core::Lit(l) => return lit_value(l),
                Core::App(f, a) => {
                    let fv = self.eval(f, &env)?;
                    let arg = self.delay(a, &env)?;
                    match fv {
                        Value::Fun(fun) => {
                            env = push(&fun.env, fun.param, arg);
                            core = fun.body;
                        }
                        _ => return Err(RtError::Fail("internal: application of a non-function".into())),
                    }
                }
                Core::Lam(x, body) => {
                    return Ok(Value::Fun(Rc::new(FunVal {
                        param: x,
                        body,
                        env: env.clone(),
                    })))
                }
                Core::Let(binds, body) => {
                    env = self.bind_rec(binds, &env);
                    core = body;
                }
                Core::Case(scrut, b, alts) => {
                    let v = self.eval(scrut, &env)?;
                    let env2 = push(&env, leak_str(b), new_cell(Thunk::Done(v.clone())));
                    for alt in alts {
                        match alt {
                            Alt::Con(name, vars, body) => {
                                if let Value::Con(c) = &v {
                                    if c.name == name.as_str() {
                                        let mut env3 = env2.clone();
                                        for (var, cell) in vars.iter().zip(&c.args) {
                                            env3 = push(&env3, leak_str(var), cell.clone());
                                        }
                                        env = env3;
                                        core = body;
                                        continue 'outer;
                                    }
                                }
                            }
                            Alt::Lit(l, body) => {
                                if lit_matches(&v, l) {
                                    env = env2;
                                    core = body;
                                    continue 'outer;
                                }
                            }
                            Alt::Default(body) => {
                                env = env2;
                                core = body;
                                continue 'outer;
                            }
                        }
                    }
                    return Err(RtError::Fail("Non-exhaustive patterns in case".into()));
                }
                Core::Con(name, args) => {
                    let mut cells = Vec::with_capacity(args.len());
                    for a in args {
                        cells.push(self.delay(a, &env)?);
                    }
                    return Ok(Value::Con(Rc::new(ConVal {
                        name: leak_str(name),
                        args: cells,
                    })));
                }
                Core::Prim(name, args) => {
                    if name == "ratioLit" {
                        if let [Core::Lit(Lit::Frac(s))] = args.as_slice() {
                            return Ok(prims::ratio_literal(s));
                        }
                    }
                    if name == "seq" {
                        // `seq a b` forces `a`, then continues with `b` in tail position.
                        if let [a, b] = args.as_slice() {
                            self.eval(a, &env)?;
                            core = b;
                            continue 'outer;
                        }
                    }
                    let mut cells = Vec::with_capacity(args.len());
                    for a in args {
                        cells.push(self.delay(a, &env)?);
                    }
                    return prims::call(self, name, &cells);
                }
                Core::Fail(msg) => return Err(RtError::Fail(msg.clone())),
            }
        }
    }

    /// Binds a recursive group: every cell's environment includes the whole group.
    fn bind_rec(&self, binds: &'static [(String, Core)], env: &Env) -> Env {
        let cells: Vec<Cell> = binds.iter().map(|_| new_cell(Thunk::Blackhole)).collect();
        let mut env2 = env.clone();
        for ((name, _), cell) in binds.iter().zip(&cells) {
            env2 = push(&env2, leak_str(name), cell.clone());
        }
        for ((_, body), cell) in binds.iter().zip(&cells) {
            *cell.borrow_mut() = Thunk::Pending(body, env2.clone());
        }
        env2
    }

    pub fn apply(&self, f: Value, arg: Cell) -> RR {
        match f {
            Value::Fun(fv) => {
                let env = push(&fv.env, fv.param, arg);
                self.eval(fv.body, &env)
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

/// Interned static names. Names come from Core, which lives for the whole run.
fn leak_str(s: &str) -> &'static str {
    thread_local! {
        static INTERN: RefCell<HashMap<String, &'static str>> = RefCell::new(HashMap::new());
    }
    INTERN.with(|m| {
        let mut m = m.borrow_mut();
        if let Some(v) = m.get(s) {
            return *v;
        }
        let leaked: &'static str = Box::leak(s.to_string().into_boxed_str());
        m.insert(s.to_string(), leaked);
        leaked
    })
}

pub fn lit_value(l: &Lit) -> RR {
    Ok(match l {
        Lit::Integer(s) => Value::Integer(Rc::new(BigInt::parse(s).ok_or_else(|| RtError::Fail("bad integer literal".into()))?)),
        Lit::Frac(s) => Value::Double(s.parse().unwrap_or(0.0)),
        Lit::Char(c) => Value::Char(*c),
        Lit::Str(s) => string_value(s),
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
        (Value::Int(a), Lit::Integer(s)) => s.parse::<i64>().ok() == Some(*a),
        (Value::Integer(a), Lit::Integer(s)) => BigInt::parse(s).is_some_and(|b| **a == b),
        _ => false,
    }
}

/// Runs `Main.main`, returning the process exit code.
pub fn run_main(rt: &Runtime) -> i32 {
    let Some(main_cell) = rt.globals.get("Main.main").cloned() else {
        eprintln!("{}: The IO action ‘main’ is not defined in module ‘Main’", rt.prog_name);
        return 1;
    };
    let result = (|| -> Result<(), RtError> {
        let io = rt.force(&main_cell)?;
        // `main :: IO ()` is `IO (World -> IORes ())`.
        let run = match io {
            Value::Con(c) if c.name == "Base.IO" => rt.force(&c.args[0])?,
            _ => return Err(RtError::Fail("internal: main is not an IO action".into())),
        };
        let unit = new_cell(Thunk::Done(Value::Con(Rc::new(ConVal {
            name: "Builtin.()",
            args: vec![],
        }))));
        rt.apply(run, unit)?;
        Ok(())
    })();
    match result {
        Ok(()) => {
            prims::flush_stdout();
            0
        }
        Err(RtError::Exit(code)) => {
            prims::flush_stdout();
            code
        }
        Err(RtError::Error(msg)) => {
            prims::flush_stdout();
            eprintln!("{}: {}", rt.prog_name, msg);
            1
        }
        Err(RtError::Fail(msg)) => {
            prims::flush_stdout();
            eprintln!("{}: {}", rt.prog_name, msg);
            1
        }
    }
}

pub fn bool_value(b: bool) -> Value {
    Value::Con(Rc::new(ConVal {
        name: if b { "Base.True" } else { "Base.False" },
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
