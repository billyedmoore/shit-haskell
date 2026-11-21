//! Lowering of Core to STG: every lambda and every delayed expression becomes a closure with its
//! own frame slots and the variables it captures; variables are slots of the current frame, captured
//! free variables, or globals. Arguments and constructor fields are atoms (variables or literals);
//! anything else is allocated as a thunk.

use std::collections::HashMap;

use crate::core::{Alt, Core, Lit, TopBind};

/// A value that can be passed without evaluating it.
#[derive(Clone, Debug, PartialEq)]
pub enum Atom {
    /// A slot of the current frame (slot 0 is the closure itself).
    Slot(u32),
    /// A variable captured by the current closure.
    Free(u32),
    /// A top-level binding.
    Global(u32),
    /// A literal, materialised as a static object (or a string for `Str`).
    Lit(SLit),
}

#[derive(Clone, Debug, PartialEq)]
pub enum SLit {
    Int(i64),
    Double(f64),
    Char(char),
    Integer(String),
    Str(String),
    Addr(String),
    Frac(String),
}

/// A heap object bound in `SExpr::Let`: either a closure (function/thunk) or a constructor
/// allocated directly in weak head normal form.
#[derive(Clone, Debug)]
pub enum SBind {
    Closure(SClosure),
    Con(String, Vec<Atom>),
}

impl SBind {
    pub fn atoms(&self) -> &[Atom] {
        match self {
            SBind::Closure(c) => &c.captures,
            SBind::Con(_, args) => args,
        }
    }
}

/// An expression of the body of a closure.
#[derive(Clone, Debug)]
pub enum SExpr {
    /// Evaluate an atom to weak head normal form and return it.
    Atom(Atom),
    /// Apply a function to atoms (evaluated when the function is), at its ordinary entry.
    App(Atom, Vec<Atom>),
    /// An exact call of a global's worker (see `workers`): its raw parameters' atoms are raw slots
    /// or number literals.
    CallWorker(u32, Vec<Atom>),
    /// Allocate a constructor and return it.
    Con(String, Vec<Atom>),
    /// A primitive operation on (lazy) atoms.
    Prim(String, Vec<Atom>),
    /// Allocate closures and constructors into slots, then continue.
    Let(Vec<(u32, SBind)>, Box<SExpr>),
    /// Evaluate the scrutinee, bind it to a slot, select an alternative.
    Case(Box<SExpr>, u32, Vec<SAlt>),
    /// Allocate a closure and return it (a lambda in a strict position).
    Closure(Box<SClosure>),
    Fail(String),
    /// A join point `id` with its parameter slots and code, in scope in the body: it runs in the
    /// frame of the enclosing closure, and is reached only by jumps in tail position.
    Join(u32, Vec<u32>, Box<SExpr>, Box<SExpr>),
    /// Stores the atoms in the join point's parameter slots and continues at its code.
    Jump(u32, Vec<Atom>),
}

#[derive(Clone, Debug)]
pub enum SAlt {
    /// A constructor with its field slots.
    Con(String, Vec<u32>, SExpr),
    Lit(Lit, SExpr),
    Default(SExpr),
}

/// Maximum number of raw worker arguments passed directly in `CACHE_REGS`.
pub const MAX_REG_RAW_ARGS: usize = 6;

/// A closure: a function of `arity` stack arguments, or a thunk (arity 0), with its captured atoms.
#[derive(Clone, Debug)]
pub struct SClosure {
    /// Stack arguments (boxed parameters, plus any raw parameters beyond `MAX_REG_RAW_ARGS`).
    pub arity: u32,
    /// The atoms, in the enclosing frame, that are captured, in capture order.
    pub captures: Vec<Atom>,
    /// The number of frame slots: slot 0 is the closure, then the stack arguments, then the locals.
    pub nslots: u32,
    /// The raw (unboxed) slots, numbered `RAW | i` (see `RAW`), apart from those.
    pub nraw: u32,
    /// A worker's raw parameters: `(arg_slot, raw_slot)`. `arg_slot == 0` means the raw parameter
    /// arrives directly in a cache register (`CACHE_REGS[i]`) rather than on the stack.
    pub raw_args: Vec<(u32, u32)>,
    /// The body returns its (numeric) result raw in R1, not a pointer (see `raw_returns`).
    pub ret_raw: bool,
    /// Whether the body (or nested closure captures) ever reads slot 0 (`Slot(0)` or `Free(_)`).
    pub uses_closure: bool,
    pub body: SExpr,
}

/// The bit that marks a slot number as a raw slot: it holds an unboxed `Int#`/`Word#`/`Char#`
/// (a number, not a pointer), which the collector does not read.
pub const RAW: u32 = 1 << 31;

/// Primitives whose result can be held raw: computed inline from raw or boxed operands.
pub const RAW_RESULT_PRIMS: &[&str] = &[
    "+#", "-#", "*#", "andI#", "orI#", "xorI#", "negateInt#", "notI#", "quotInt#", "remInt#",
    "uncheckedIShiftL#", "uncheckedIShiftRA#", "uncheckedIShiftRL#", "==#", "/=#", "<#", "<=#",
    ">#", ">=#", "plusWord#", "minusWord#", "timesWord#", "and#", "or#", "xor#", "eqWord#",
    "neWord#", "ltWord#", "leWord#", "gtWord#", "geWord#", "uncheckedShiftL#",
    "uncheckedShiftRL#", "eqChar#", "neChar#", "ltChar#", "leChar#", "gtChar#", "geChar#",
    "ord#", "chr#",
];

/// Primitives that can take raw operands (those, and `tagToEnum#`).
fn raw_operand_prim(p: &str) -> bool {
    p == "tagToEnum#" || RAW_RESULT_PRIMS.contains(&p)
}

/// A raw slot holding a `Char#` (its boxed form is a CHAR cell); others hold `Int#`/`Word#`.
pub const RAW_CHAR: u32 = 1 << 30;

/// The functions with a worker taking some parameters raw: by name, which parameters.
pub type Workers = HashMap<String, Vec<bool>>;

/// What the types say of each global function's parameters (in Core order, dictionaries
/// first): 0 not known to be a number, `NUM_INT` an `Int`/`Int#`/`Word`/`Word#`, `NUM_CHAR` a
/// `Char`/`Char#`. Core is untyped, so only these may be passed raw.
#[derive(Clone, Default, Debug)]
pub struct NumericParams {
    /// Each function's parameters' kinds.
    pub params: HashMap<String, Vec<u8>>,
    /// Functions whose result is a number when applied to all their parameters: their number of
    /// parameters (dictionaries included), and the result's kind.
    pub results: HashMap<String, (usize, u8)>,
}
pub const NUM_INT: u8 = 1;
pub const NUM_CHAR: u8 = 2;

/// Primitives in `RAW_RESULT_PRIMS` that cannot trap (excludes integer division/remainder by zero).
fn total_raw_prim(p: &str) -> bool {
    RAW_RESULT_PRIMS.contains(&p) && !matches!(p, "quotInt#" | "remInt#")
}

/// Whether `e` is guaranteed to evaluate to WHNF without diverging or throwing an exception,
/// given that every variable in `whnf` is already in WHNF (or raw).
fn is_whnf_expr(e: &Core, whnf: &std::collections::HashSet<String>) -> bool {
    match e {
        Core::Lit(Lit::Int(_) | Lit::Char(_)) => true,
        Core::Var(v) => whnf.contains(v.as_str()),
        Core::Prim(p, args) if total_raw_prim(p) => args.iter().all(|a| is_whnf_expr(a, whnf)),
        Core::Prim(p, args) if matches!(p.as_str(), "lazy" | "oneShot" | "inline" | "noinline") && args.len() == 1 => {
            is_whnf_expr(&args[0], whnf)
        }
        _ => false,
    }
}

/// Records the variables guaranteed to be in WHNF inside the alternatives of `case scrut as binder of ...`.
fn add_case_whnf(scrut: &Core, binder: &str, whnf: &mut std::collections::HashSet<String>) {
    whnf.insert(binder.to_string());
    match scrut {
        Core::Var(v) => {
            whnf.insert(v.clone());
        }
        Core::Prim(p, args) if RAW_RESULT_PRIMS.contains(&p.as_str()) => {
            for a in args {
                if let Core::Var(v) = a {
                    whnf.insert(v.clone());
                }
            }
        }
        _ => {}
    }
}

/// Whether an exact call `g args` may invoke `g`'s worker given `strict_map` and the currently
/// known-WHNF variables: every non-strict raw parameter must receive a guaranteed-WHNF argument.
fn can_call_worker(
    g: &str,
    args: &[&Core],
    w: &Workers,
    strict_map: &Workers,
    whnf: &std::collections::HashSet<String>,
) -> bool {
    let Some(raw) = w.get(g).filter(|r| r.len() == args.len()) else {
        return false;
    };
    let strict = strict_map.get(g);
    args.iter().enumerate().all(|(i, a)| {
        !raw[i] || strict.and_then(|s| s.get(i)).copied().unwrap_or(false) || is_whnf_expr(a, whnf)
    })
}

/// Whether every use of `x` in `e` can take it raw: as an operand of an inline primitive, as the
/// scrutinee of a case on literals whose binder is unused, as a raw argument of a worker call,
/// or as the body's result (boxed once, when returned).
fn raw_uses(
    e: &Core,
    x: &str,
    w: &Workers,
    strict_map: &Workers,
    whnf: &std::collections::HashSet<String>,
) -> bool {
    raw_uses_in(e, x, w, strict_map, whnf, true)
}

/// `raw_uses`; `result` says whether `e` is in a result position of the body. Elsewhere (inside a
/// scrutinee, whose own results become a join point's arguments, or an operand) `x` itself must
/// not be the value.
fn raw_uses_in(
    e: &Core,
    x: &str,
    w: &Workers,
    strict_map: &Workers,
    whnf: &std::collections::HashSet<String>,
    result: bool,
) -> bool {
    match e {
        Core::Var(v) => v != x || result,
        Core::Lit(_) | Core::Fail(_) => true,
        // An operand that is not an atom becomes a thunk: it must not mention the raw value.
        Core::Prim(p, args) if raw_operand_prim(p) => {
            args.iter().all(|a| matches!(a, Core::Var(v) if v == x) || !mentions(a, x))
        }
        Core::Prim(p, args) if p == "seq" && args.len() == 2 => {
            if mentions(&args[0], x) {
                return false;
            }
            if let Core::Var(v) = &args[0] {
                let mut next = whnf.clone();
                next.insert(v.clone());
                raw_uses_in(&args[1], x, w, strict_map, &next, result)
            } else {
                raw_uses_in(&args[1], x, w, strict_map, whnf, result)
            }
        }
        Core::Prim(p, args) if IDENTITY_PRIMS.contains(&p.as_str()) && args.len() == 1 => {
            raw_uses_in(&args[0], x, w, strict_map, whnf, result)
        }
        Core::Case(s, b, alts) if matches!(&**s, Core::Var(v) if v == x) => {
            let mut base_whnf = whnf.clone();
            add_case_whnf(s, b, &mut base_whnf);
            alts.iter().all(|a| match a {
                Alt::Lit(Lit::Int(_) | Lit::Char(_), body) | Alt::Default(body) => {
                    !mentions(body, b) && raw_uses_in(body, x, w, strict_map, &base_whnf, result)
                }
                _ => false,
            })
        }
        Core::App(..) => {
            let (f, args) = spine(e);
            if let Core::Var(g) = f {
                if can_call_worker(g.as_str(), &args, w, strict_map, whnf) {
                    let raw = &w[g.as_str()];
                    // A raw parameter's argument is evaluated in place (see `worker_call`); the others
                    // are passed or captured.
                    return g != x
                        && args.iter().zip(raw).all(|(a, r)| match a {
                            Core::Var(v) if v == x => *r,
                            a if *r => raw_uses_in(a, x, w, strict_map, whnf, false),
                            a => !mentions(a, x),
                        });
                }
            }
            // Otherwise the function and arguments are passed or captured (a non-atomic argument
            // becomes a thunk), never evaluated in place: they must not mention the raw value.
            !mentions(e, x)
        }
        Core::Lam(_, b) => !mentions(b, x),
        Core::Let(binds, body) => {
            let mut next = whnf.clone();
            for (n, _) in binds {
                next.remove(n);
            }
            binds.iter().all(|(_, r)| !mentions(r, x)) && raw_uses_in(body, x, w, strict_map, &next, result)
        }
        Core::Case(s, b, alts) => {
            if !raw_uses_in(s, x, w, strict_map, whnf, false) {
                return false;
            }
            let mut base_whnf = whnf.clone();
            add_case_whnf(s, b, &mut base_whnf);
            alts.iter().all(|a| match a {
                Alt::Con(_, vars, body) => {
                    let mut alt_whnf = base_whnf.clone();
                    for v in vars {
                        alt_whnf.remove(v);
                    }
                    raw_uses_in(body, x, w, strict_map, &alt_whnf, result)
                }
                Alt::Lit(_, body) | Alt::Default(body) => {
                    raw_uses_in(body, x, w, strict_map, &base_whnf, result)
                }
            })
        }
        Core::Con(_, args) | Core::Prim(_, args) => args.iter().all(|a| !mentions(a, x)),
    }
}

/// Whether `e` uses `x` as a number: an operand of an inline primitive, or the scrutinee of a
/// case with number or character literal alternatives.
fn numeric_use(e: &Core, x: &str) -> bool {
    match e {
        Core::Prim(p, args) if raw_operand_prim(p) && args.iter().any(|a| matches!(a, Core::Var(v) if v == x)) => true,
        Core::Case(s, _, alts)
            if matches!(&**s, Core::Var(v) if v == x)
                && alts.iter().any(|a| matches!(a, Alt::Lit(Lit::Int(_) | Lit::Char(_), _))) =>
        {
            true
        }
        Core::Var(_) | Core::Lit(_) | Core::Fail(_) => false,
        Core::App(f, a) => numeric_use(f, x) || numeric_use(a, x),
        Core::Lam(_, b) => numeric_use(b, x),
        Core::Let(binds, body) => binds.iter().any(|(_, r)| numeric_use(r, x)) || numeric_use(body, x),
        Core::Case(s, _, alts) => {
            numeric_use(s, x)
                || alts.iter().any(|a| match a {
                    Alt::Con(_, _, b) | Alt::Lit(_, b) | Alt::Default(b) => numeric_use(b, x),
                })
        }
        Core::Con(_, args) | Core::Prim(_, args) => args.iter().any(|a| numeric_use(a, x)),
    }
}

/// The workers that return their result raw: the type says the result of a full application
/// is a number, the body has exactly that many parameters, and no result is a constructor or a
/// function (each result is then a number to pass back as it is). Functions with no raw
/// parameters may also get a raw-returning worker (e.g. `size :: Tree -> Int`).
fn raw_returns(
    binds: &[TopBind],
    w: &Workers,
    strict_map: &Workers,
    numeric: &NumericParams,
) -> HashMap<String, u8> {
    // A result that is a call must be an exact call of another worker returning raw (so that a
    // tail call stays a jump: a call answering a boxed result would have to return here first).
    fn numeric_results(
        e: &Core,
        out: &HashMap<String, u8>,
        w_all: &Workers,
        strict_map: &Workers,
        whnf: &std::collections::HashSet<String>,
    ) -> bool {
        match e {
            Core::Con(..) | Core::Lam(..) => false,
            Core::Lit(l) => matches!(l, Lit::Int(_) | Lit::Char(_)),
            Core::Var(_) | Core::Fail(_) => true,
            Core::App(..) => {
                let (f, args) = spine(e);
                matches!(
                    f,
                    Core::Var(g)
                        if out.contains_key(g.as_str())
                            && can_call_worker(g.as_str(), &args, w_all, strict_map, whnf)
                )
            }
            Core::Prim(p, args) if p == "seq" && args.len() == 2 => {
                if let Core::Var(v) = &args[0] {
                    let mut next = whnf.clone();
                    next.insert(v.clone());
                    numeric_results(&args[1], out, w_all, strict_map, &next)
                } else {
                    numeric_results(&args[1], out, w_all, strict_map, whnf)
                }
            }
            Core::Prim(p, args) if IDENTITY_PRIMS.contains(&p.as_str()) && args.len() == 1 => {
                numeric_results(&args[0], out, w_all, strict_map, whnf)
            }
            Core::Prim(..) => true,
            Core::Let(binds, body) => {
                let mut body_whnf = whnf.clone();
                for (n, _) in binds {
                    body_whnf.remove(n);
                }
                let join_ok = if is_join(binds, body) {
                    let mut code_whnf = whnf.clone();
                    let mut cur = &binds[0].1;
                    while let Core::Lam(p, inner) = cur {
                        code_whnf.remove(p);
                        cur = inner;
                    }
                    numeric_results(cur, out, w_all, strict_map, &code_whnf)
                } else {
                    true
                };
                join_ok && numeric_results(body, out, w_all, strict_map, &body_whnf)
            }
            Core::Case(s, b, alts) => {
                let mut base_whnf = whnf.clone();
                add_case_whnf(s, b, &mut base_whnf);
                alts.iter().all(|a| match a {
                    Alt::Con(_, vars, body) => {
                        let mut alt_whnf = base_whnf.clone();
                        for v in vars {
                            alt_whnf.remove(v);
                        }
                        numeric_results(body, out, w_all, strict_map, &alt_whnf)
                    }
                    Alt::Lit(_, body) | Alt::Default(body) => {
                        numeric_results(body, out, w_all, strict_map, &base_whnf)
                    }
                })
            }
        }
    }
    let mut out = HashMap::new();
    let mut w_all = w.clone();
    let mut bodies = Vec::new();
    for b in binds {
        let Some((n, kind)) = numeric.results.get(&b.name) else { continue };
        if *n == 0 {
            continue;
        }
        let mut params = Vec::new();
        let mut inner = &b.body;
        while let Core::Lam(p, next) = inner {
            params.push(p.as_str());
            inner = next;
        }
        if params.len() == *n {
            out.insert(b.name.clone(), *kind);
            w_all.entry(b.name.clone()).or_insert_with(|| vec![false; *n]);
            let mut init_whnf = std::collections::HashSet::new();
            if let Some(raw) = w.get(&b.name) {
                for (p, r) in params.iter().zip(raw) {
                    if *r {
                        init_whnf.insert((*p).to_string());
                    }
                }
            }
            bodies.push((b.name.as_str(), inner, init_whnf));
        }
    }
    // From every candidate, weakened until each claim holds.
    loop {
        let failing: Vec<&str> = bodies
            .iter()
            .filter(|(name, inner, init_whnf)| {
                out.contains_key(*name) && !numeric_results(inner, &out, &w_all, strict_map, init_whnf)
            })
            .map(|(name, _, _)| *name)
            .collect();
        if failing.is_empty() {
            return out;
        }
        for name in failing {
            out.remove(name);
            if !w.contains_key(name) {
                w_all.remove(name);
            }
        }
    }
}

/// The global functions given workers: a parameter is passed raw when its type says it is a number,
/// the body uses it as a number, every use takes it raw, and either the body certainly evaluates
/// it (strict worker) or no strict numeric parameter exists (WHNF-guarded worker, invoked only at
/// call sites passing a guaranteed-WHNF argument).
pub fn workers(binds: &[TopBind], numeric: &NumericParams) -> Workers {
    workers_with_strict(binds, numeric).0
}

fn workers_with_strict(binds: &[TopBind], numeric: &NumericParams) -> (Workers, Workers) {
    let mut w: Workers = HashMap::new();
    let mut strict_map: Workers = HashMap::new();
    let mut bodies: HashMap<&str, (Vec<&str>, &Core)> = HashMap::new();
    for b in binds {
        let mut params = Vec::new();
        let mut cur = &b.body;
        while let Core::Lam(x, inner) = cur {
            params.push(x.as_str());
            cur = inner;
        }
        if params.is_empty() {
            continue;
        }
        let evaluated = crate::core::simplify::certainly_evaluated(cur);
        // Core is untyped: a parameter is known to be an Int#/Word#/Char# only when the body
        // itself uses it as one (an identity like a newtype's constructor must stay boxed).
        // Both the types (a GADT match can refine a parameter's type in one alternative only) and
        // the body say it is a number.
        let types = numeric.params.get(&b.name);
        let strict_raw: Vec<bool> = params
            .iter()
            .enumerate()
            .map(|(i, p)| {
                types.and_then(|t| t.get(i)).is_some_and(|k| *k != 0) && evaluated.contains(*p) && numeric_use(cur, p)
            })
            .collect();
        if strict_raw.iter().any(|r| *r) {
            strict_map.insert(b.name.clone(), strict_raw.clone());
            w.insert(b.name.clone(), strict_raw);
            bodies.insert(b.name.as_str(), (params, cur));
        } else {
            // No strict numeric parameter: allow WHNF-guarded raw parameters (e.g. `member`,
            // `safe`, IO loops), invoked only at call sites passing guaranteed-WHNF values.
            let whnf_raw: Vec<bool> = params
                .iter()
                .enumerate()
                .map(|(i, p)| types.and_then(|t| t.get(i)).is_some_and(|k| *k != 0) && numeric_use(cur, p))
                .collect();
            if whnf_raw.iter().any(|r| *r) {
                strict_map.insert(b.name.clone(), vec![false; params.len()]);
                w.insert(b.name.clone(), whnf_raw);
                bodies.insert(b.name.as_str(), (params, cur));
            }
        }
    }
    loop {
        let mut changed = false;
        let names: Vec<String> = w.keys().cloned().collect();
        for name in names {
            let (params, body) = &bodies[name.as_str()];
            let mut raw = w[&name].clone();
            let mut init_whnf = std::collections::HashSet::new();
            for (p, r) in params.iter().zip(&raw) {
                if *r {
                    init_whnf.insert((*p).to_string());
                }
            }
            for (i, p) in params.iter().enumerate() {
                if raw[i] && !raw_uses(body, p, &w, &strict_map, &init_whnf) {
                    raw[i] = false;
                    changed = true;
                }
            }
            if raw.iter().any(|r| *r) {
                w.insert(name, raw);
            } else {
                w.remove(&name);
                strict_map.remove(&name);
            }
        }
        if !changed {
            return (w, strict_map);
        }
    }
}

/// A top-level binding: a closure with no captures (a function, or a CAF thunk).
pub struct SGlobal {
    pub name: String,
    pub closure: SClosure,
    /// The same function taking some parameters raw, for exact calls (see `workers`).
    pub worker: Option<(SClosure, Vec<bool>)>,
}

pub struct SProgram {
    pub globals: Vec<SGlobal>,
    pub index: HashMap<String, u32>,
}

/// The lowering state of one closure body being built.
struct Body {
    /// Local names bound in this body, to their slots.
    locals: HashMap<String, u32>,
    nslots: u32,
    /// Raw slots allocated.
    nraw: u32,
    /// Captured names, to their index, with the outer variable each is copied from.
    captures: Vec<(String, Atom)>,
    /// Captured names by index into `captures`.
    capture_index: HashMap<String, u32>,
    /// The join points in scope, to their id and arity.
    joins: HashMap<String, (u32, usize)>,
    /// Variables known to be in WHNF (or raw) at the current point in this body.
    whnf: std::collections::HashSet<String>,
    /// Where the expression being lowered is (see `Ctx`).
    ctx: Ctx,
    /// The kind of the raw result this body returns (a worker returning its number raw).
    ret_raw: Option<u8>,
}

/// Where an expression is lowered: its result returned from the body, a case scrutinee, or the
/// scrutinee of a case whose binder is raw (which takes a raw result as it is).
#[derive(Clone, Copy, PartialEq, Debug)]
enum Ctx {
    Tail,
    Scrut,
    RawScrut,
}

pub struct Lowering {
    index: HashMap<String, u32>,
    /// The stack of bodies being lowered: the innermost is last.
    bodies: Vec<Body>,
    /// Objects for the non-atomic arguments of the operation being lowered, to be allocated before it.
    pending: Vec<(u32, SBind)>,
    /// The next join point id (unique in the program).
    next_join: u32,
    /// The functions with workers (see `workers`).
    workers: Workers,
    /// For each worker, which raw parameters are certainly evaluated (vs WHNF-guarded).
    worker_strict: Workers,
    /// The parameters the types say are numbers (see `NumericParams`).
    numeric: NumericParams,
    /// The workers that return their result raw, with its kind (see `raw_returns`).
    raw_returns: HashMap<String, u8>,
}

impl Body {
    fn new() -> Body {
        // Slot 0 is the closure itself.
        Body {
            locals: HashMap::new(),
            nslots: 1,
            nraw: 0,
            captures: Vec::new(),
            capture_index: HashMap::new(),
            joins: HashMap::new(),
            whnf: std::collections::HashSet::new(),
            ctx: Ctx::Tail,
            ret_raw: None,
        }
    }

    fn fresh_slot(&mut self) -> u32 {
        let s = self.nslots;
        self.nslots += 1;
        s
    }
}

impl Lowering {
    /// Lowers a whole program. Global names are the top-level bindings.
    pub fn program(binds: &[TopBind], numeric: &NumericParams) -> SProgram {
        let index: HashMap<String, u32> = binds.iter().enumerate().map(|(i, b)| (b.name.clone(), i as u32)).collect();
        let no_workers = std::env::var_os("HSC_NO_WORKERS").is_some();
        let (workers, worker_strict) = if no_workers {
            (HashMap::new(), HashMap::new())
        } else {
            workers_with_strict(binds, numeric)
        };
        let mut lw = Lowering {
            index: index.clone(),
            bodies: Vec::new(),
            pending: Vec::new(),
            next_join: 0,
            workers,
            worker_strict,
            numeric: numeric.clone(),
            raw_returns: HashMap::new(),
        };
        if !no_workers {
            lw.raw_returns = raw_returns(binds, &lw.workers, &lw.worker_strict, numeric);
            for name in lw.raw_returns.keys() {
                if !lw.workers.contains_key(name) {
                    let n = numeric.results[name].0;
                    lw.workers.insert(name.clone(), vec![false; n]);
                    lw.worker_strict.insert(name.clone(), vec![false; n]);
                }
            }
        }
        let globals = binds
            .iter()
            .map(|b| {
                let closure = lw.closure_of(&b.body);
                let worker = lw.workers.get(&b.name).cloned().map(|raw| {
                    let kinds = lw.numeric.params.get(&b.name).cloned().unwrap_or_default();
                    let ret = lw.raw_returns.get(&b.name).copied();
                    (lw.closure_with(&b.body, Some((&raw, &kinds)), ret), raw)
                });
                SGlobal { name: b.name.clone(), closure, worker }
            })
            .collect();
        SProgram { globals, index }
    }

    /// The closure of a top-level or delayed expression: a lambda's parameters, or a thunk.
    fn closure_of(&mut self, e: &Core) -> SClosure {
        self.closure_with(e, None, None)
    }

    /// `closure_of`, with the parameters `raw` marks bound to raw slots (a worker): the first
    /// `MAX_REG_RAW_ARGS` raw parameters arrive in cache registers (`arg_slot == 0`), and any
    /// further ones arrive in stack argument slots.
    fn closure_with(&mut self, e: &Core, raw: Option<(&[bool], &[u8])>, ret_raw: Option<u8>) -> SClosure {
        let mut body = Body::new();
        body.ret_raw = ret_raw;
        self.bodies.push(body);
        let mut raw_args = Vec::new();
        let (arity, body_expr) = match e {
            Core::Lam(..) => {
                let mut params = Vec::new();
                let mut cur = e;
                while let Core::Lam(p, b) = cur {
                    params.push(p.clone());
                    cur = b;
                }
                let top = self.bodies.last_mut().expect("body");
                let mut stack_arity = 0u32;
                let mut reg_raws = 0usize;
                for (i, p) in params.iter().enumerate() {
                    if let Some((_, kinds)) = raw.filter(|(r, _)| r[i]) {
                        let kind = if kinds.get(i) == Some(&NUM_CHAR) { RAW_CHAR } else { 0 };
                        let r = RAW | kind | top.nraw;
                        top.nraw += 1;
                        top.locals.insert(p.clone(), r);
                        top.whnf.insert(p.clone());
                        if reg_raws < MAX_REG_RAW_ARGS {
                            reg_raws += 1;
                            raw_args.push((0, r));
                        } else {
                            let s = top.fresh_slot();
                            stack_arity += 1;
                            raw_args.push((s, r));
                        }
                    } else {
                        let s = top.fresh_slot();
                        stack_arity += 1;
                        top.locals.insert(p.clone(), s);
                    }
                }
                (stack_arity, cur)
            }
            other => (0, other),
        };
        let body = self.strict(body_expr);
        let b = self.bodies.pop().expect("body");
        let captures: Vec<Atom> = b.captures.iter().map(|(_, a)| a.clone()).collect();
        let uses_closure = !captures.is_empty() || expr_uses_closure(&body);
        SClosure { arity, captures, nslots: b.nslots, nraw: b.nraw, raw_args, ret_raw: b.ret_raw.is_some(), uses_closure, body }
    }

    /// Resolves a variable in the current body: a slot, a captured variable (captured now from
    /// the enclosing bodies), or a global.
    fn resolve(&mut self, name: &str) -> Atom {
        let depth = self.bodies.len() - 1;
        if let Some(a) = self.resolve_at(depth, name) {
            return a;
        }
        if let Some(&g) = self.index.get(name) {
            return Atom::Global(g);
        }
        panic!("internal: unbound variable {name} in lowering");
    }

    fn resolve_at(&mut self, depth: usize, name: &str) -> Option<Atom> {
        if let Some(&s) = self.bodies[depth].locals.get(name) {
            return Some(Atom::Slot(s));
        }
        if let Some(&i) = self.bodies[depth].capture_index.get(name) {
            return Some(Atom::Free(i));
        }
        if depth == 0 {
            return None;
        }
        // Capture from the enclosing body.
        let outer = self.resolve_at(depth - 1, name)?;
        let b = &mut self.bodies[depth];
        let i = b.captures.len() as u32;
        b.captures.push((name.to_string(), outer));
        b.capture_index.insert(name.to_string(), i);
        Some(Atom::Free(i))
    }

    /// An expression evaluated in place (a case scrutinee, a body's result).
    /// The current body's position context.
    fn ctx(&self) -> Ctx {
        self.bodies.last().map_or(Ctx::Tail, |b| b.ctx)
    }

    fn set_ctx(&mut self, ctx: Ctx) -> Ctx {
        let b = self.bodies.last_mut().expect("body");
        std::mem::replace(&mut b.ctx, ctx)
    }

    /// Lowers `e` where `ctx` says (a scrutinee), restoring the context after.
    fn strict_in(&mut self, e: &Core, ctx: Ctx) -> SExpr {
        let prev = self.set_ctx(ctx);
        let out = self.strict(e);
        self.set_ctx(prev);
        out
    }

    /// The kind of the raw result returned from here, when the body returns raw and this is a
    /// result position.
    fn raw_result(&self) -> Option<u8> {
        let b = self.bodies.last()?;
        if b.ctx == Ctx::Tail { b.ret_raw } else { None }
    }

    /// Whether an exact call `g args` in the current body may invoke `g`'s worker.
    fn can_invoke_worker(&self, g: &str, args: &[&Core]) -> bool {
        if self.bodies.iter().any(|b| b.locals.contains_key(g) || b.capture_index.contains_key(g)) {
            return false;
        }
        let Some(body) = self.bodies.last() else { return false };
        can_call_worker(g, args, &self.workers, &self.worker_strict, &body.whnf)
    }

    /// The raw result kind of an exact call of a worker that returns raw.
    fn raw_call(&self, e: &Core) -> Option<u8> {
        let (f, args) = spine(e);
        let Core::Var(g) = f else { return None };
        if !self.can_invoke_worker(g.as_str(), &args) {
            return None;
        }
        self.raw_returns.get(g.as_str()).copied()
    }

    /// `case e as r of _ -> r` with a raw binder of the given kind: the number of `e` (unboxed,
    /// or taken raw from a worker returning raw), as the result of the case.
    fn through_raw(&mut self, e: SExpr, kind: u8) -> SExpr {
        let slot = self.bind_raw(&format!("r%{}", self.next_join), kind == NUM_CHAR);
        self.next_join += 1;
        SExpr::Case(Box::new(e), slot, vec![SAlt::Default(SExpr::Atom(Atom::Slot(slot)))])
    }

    fn strict(&mut self, e: &Core) -> SExpr {
        // Only a call directly in a raw binder's scrutinee may answer raw: anything with
        // alternatives or bindings there is an ordinary scrutinee (no mix of raw and boxed results).
        let direct = matches!(e, Core::App(..) | Core::Var(_) | Core::Lit(_))
            || matches!(e, Core::Prim(p, args) if IDENTITY_PRIMS.contains(&p.as_str()) && args.len() == 1);
        if self.ctx() == Ctx::RawScrut && !direct {
            return self.strict_in(e, Ctx::Scrut);
        }
        match e {
            Core::Var(v) => match self.join(v, 0) {
                Some(id) => SExpr::Jump(id, Vec::new()),
                None => {
                    let a = self.resolve(v);
                    match self.raw_result() {
                        // A boxed variable as a raw result: evaluated, then its number returned.
                        Some(kind) if !matches!(a, Atom::Slot(k) if k & RAW != 0) => {
                            self.through_raw(SExpr::Atom(a), kind)
                        }
                        _ => SExpr::Atom(a),
                    }
                }
            },
            Core::Lit(l) => SExpr::Atom(Atom::Lit(slit(l))),
            Core::App(..) => {
                let mark = self.pending.len();
                let (f, args) = spine(e);
                if let Some(id) = match f { Core::Var(v) => self.join(v, args.len()), _ => None } {
                    let aa = args.iter().map(|a| self.atom(a)).collect();
                    return self.allocating(mark, SExpr::Jump(id, aa));
                }
                let callee_raw = self.raw_call(e);
                let call = match self.worker_call(f, &args) {
                    Some(call) => call,
                    None => {
                        let fa = self.atom(f);
                        let aa = args.iter().map(|a| self.atom(a)).collect();
                        self.allocating(mark, SExpr::App(fa, aa))
                    }
                };
                let raw_here = self.ctx() == Ctx::RawScrut || self.raw_result().is_some();
                match (callee_raw, self.raw_result()) {
                    // A raw result where a raw one is taken: passed on as it is.
                    (Some(_), _) if raw_here => call,
                    // A raw result where a boxed one is expected: boxed after the return. (No tail
                    // cycle passes through here: a worker returning raw tail-calls only others that
                    // do, see `raw_returns`.)
                    (Some(kind), _) => self.through_raw(call, kind),
                    // A boxed result returned raw: unboxed after the return.
                    (None, Some(kind)) => self.through_raw(call, kind),
                    (None, None) => call,
                }
            }
            Core::Lam(..) => SExpr::Closure(Box::new(self.closure_of(e))),
            Core::Let(binds, body) if is_join(binds, body) => {
                let (name, rhs) = &binds[0];
                let saved_whnf = self.bodies.last().expect("body").whnf.clone();
                let mut params = Vec::new();
                let mut code = rhs;
                while let Core::Lam(x, b) = code {
                    self.bodies.last_mut().expect("body").whnf.remove(x);
                    params.push(self.bind_local(x));
                    code = b;
                }
                let code = self.strict(code);
                let id = self.next_join;
                self.next_join += 1;
                {
                    let top = self.bodies.last_mut().expect("body");
                    top.whnf = saved_whnf.clone();
                    top.whnf.remove(name);
                }
                let prev = self.bodies.last_mut().expect("body").joins.insert(name.clone(), (id, params.len()));
                let body = self.strict(body);
                let top = self.bodies.last_mut().expect("body");
                top.whnf = saved_whnf;
                match prev {
                    Some(p) => top.joins.insert(name.clone(), p),
                    None => top.joins.remove(name),
                };
                SExpr::Join(id, params, Box::new(code), Box::new(body))
            }
            Core::Let(binds, body) => {
                let bound = self.let_binds(binds);
                let saved_whnf = self.bodies.last().expect("body").whnf.clone();
                {
                    let top = self.bodies.last_mut().expect("body");
                    for (n, _) in binds {
                        top.whnf.remove(n);
                    }
                }
                let body = self.strict(body);
                self.bodies.last_mut().expect("body").whnf = saved_whnf;
                SExpr::Let(bound, Box::new(body))
            }
            Core::Case(scrut, binder, alts) => {
                // A nested case in a scrutinee becomes a join point, and a let is floated out of
                // the scrutinee. (A scrutinee can still be a case made by `strict`, from `seq` and
                // its identities: codegen gives its return address an empty frame.)
                match scrut.as_ref() {
                    Core::Let(binds, body) => {
                        let inner = Core::Let(
                            binds.clone(),
                            Box::new(Core::Case(body.clone(), binder.clone(), alts.clone())),
                        );
                        return self.strict(&inner);
                    }
                    Core::Case(..) => {
                        let join = join_point(scrut, binder, alts);
                        return self.strict(&join);
                    }
                    _ => {}
                }
                // The result of an inline primitive that is only used raw is kept unboxed.
                let raw_call = self.raw_call(scrut);
                let saved_whnf = self.bodies.last().expect("body").whnf.clone();
                let mut alt_base_whnf = saved_whnf.clone();
                add_case_whnf(scrut, binder, &mut alt_base_whnf);
                let raw = (matches!(scrut.as_ref(), Core::Prim(p, _) if RAW_RESULT_PRIMS.contains(&p.as_str())) || raw_call.is_some())
                    && matches!(&alts[..], [Alt::Default(body)] if raw_uses(body, binder, &self.workers, &self.worker_strict, &alt_base_whnf));
                let char = matches!(scrut.as_ref(), Core::Prim(p, _) if p == "chr#") || raw_call == Some(NUM_CHAR);
                let flat_alts = if !raw { flatten_case_alts(scrut, binder, alts) } else { None };
                let alts: &[Alt] = flat_alts.as_deref().unwrap_or(alts);
                let scrut = self.strict_in(scrut, if raw { Ctx::RawScrut } else { Ctx::Scrut });
                let bslot = if raw { self.bind_raw(binder, char) } else { self.bind_local(binder) };
                // Only one alternative runs, so their locals share the slots after the binder: the
                // frame needs the largest alternative's, not their sum.
                let (base, raw_base) = {
                    let b = self.bodies.last().expect("body");
                    (b.nslots, b.nraw)
                };
                let (mut top, mut raw_top) = (base, raw_base);
                let mut lowered = Vec::with_capacity(alts.len());
                for a in alts {
                    let b = self.bodies.last_mut().expect("body");
                    b.nslots = base;
                    b.nraw = raw_base;
                    b.whnf = alt_base_whnf.clone();
                    if let Alt::Con(_, vars, _) = a {
                        for v in vars {
                            b.whnf.remove(v);
                        }
                    }
                    lowered.push(self.alt(a));
                    let b = self.bodies.last().expect("body");
                    top = top.max(b.nslots);
                    raw_top = raw_top.max(b.nraw);
                }
                let b = self.bodies.last_mut().expect("body");
                b.nslots = top;
                b.nraw = raw_top;
                b.whnf = saved_whnf;
                SExpr::Case(Box::new(scrut), bslot, lowered)
            }
            Core::Con(name, args) => {
                let mark = self.pending.len();
                let atoms = args.iter().map(|a| self.atom(a)).collect();
                self.allocating(mark, SExpr::Con(name.clone(), atoms))
            }
            // `seq a b` evaluates a, then answers b; the identities answer their argument.
            Core::Prim(name, args) if name == "seq" && args.len() == 2 => {
                let slot = self.bind_local(&format!("seq%{}", self.bodies.len()));
                let scrut = self.strict_in(&args[0], Ctx::Scrut);
                let saved_whnf = self.bodies.last().expect("body").whnf.clone();
                if let Core::Var(v) = &args[0] {
                    self.bodies.last_mut().expect("body").whnf.insert(v.clone());
                }
                let body = self.strict(&args[1]);
                self.bodies.last_mut().expect("body").whnf = saved_whnf;
                SExpr::Case(Box::new(scrut), slot, vec![SAlt::Default(body)])
            }
            Core::Prim(name, args) if IDENTITY_PRIMS.contains(&name.as_str()) && args.len() == 1 => {
                self.strict(&args[0])
            }
            Core::Prim(name, args) => {
                let mark = self.pending.len();
                let atoms = args.iter().map(|a| self.atom(a)).collect();
                let prim = self.allocating(mark, SExpr::Prim(name.clone(), atoms));
                match self.raw_result() {
                    // A primitive's result returned raw: its number (stored raw, or unboxed).
                    Some(kind) => self.through_raw(prim, if name == "chr#" { NUM_CHAR } else { kind }),
                    None => prim,
                }
            }
            Core::Fail(msg) => SExpr::Fail(msg.clone()),
        }
    }

    /// An exact call of a function with a worker: each raw parameter's argument is a raw slot or a
    /// number literal, evaluated first (into a fresh raw slot) otherwise; every raw parameter is
    /// either strict or receives a guaranteed-WHNF argument (checked by `can_invoke_worker`).
    fn worker_call(&mut self, f: &Core, args: &[&Core]) -> Option<SExpr> {
        let Core::Var(g) = f else { return None };
        if !self.can_invoke_worker(g.as_str(), args) {
            return None;
        }
        let raw = self.workers.get(g.as_str())?.clone();
        let mut evals: Vec<(u32, SExpr)> = Vec::new();
        let mut atoms: Vec<Option<Atom>> = vec![None; args.len()];
        for (i, a) in args.iter().enumerate() {
            if !raw[i] {
                continue;
            }
            let direct = match a {
                Core::Var(v) => match self.resolve(v) {
                    at @ Atom::Slot(k) if k & RAW != 0 => Some(at),
                    _ => None,
                },
                Core::Lit(l @ (Lit::Int(_) | Lit::Char(_))) => Some(Atom::Lit(slit(l))),
                _ => None,
            };
            match direct {
                Some(at) => atoms[i] = Some(at),
                None => {
                    let scrut = self.strict_in(a, Ctx::RawScrut);
                    let slot = self.bind_raw(&format!("w%{}", self.next_join), false);
                    self.next_join += 1;
                    evals.push((slot, scrut));
                    atoms[i] = Some(Atom::Slot(slot));
                }
            }
        }
        let mark = self.pending.len();
        let Atom::Global(gi) = self.atom(f) else { return None };
        let aa: Vec<Atom> = args.iter().zip(atoms).map(|(a, at)| at.unwrap_or_else(|| self.atom(a))).collect();
        let mut e = self.allocating(mark, SExpr::CallWorker(gi, aa));
        for (slot, scrut) in evals.into_iter().rev() {
            e = SExpr::Case(Box::new(scrut), slot, vec![SAlt::Default(e)]);
        }
        Some(e)
    }

    /// Wraps an operation with the allocations of the objects its arguments needed.
    fn allocating(&mut self, mark: usize, op: SExpr) -> SExpr {
        if self.pending.len() == mark {
            return op;
        }
        let bindings: Vec<(u32, SBind)> = self.pending.drain(mark..).collect();
        SExpr::Let(bindings, Box::new(op))
    }

    /// The join point a variable names in the current body, when it is applied to `n` arguments.
    fn join(&self, name: &str, n: usize) -> Option<u32> {
        let b = self.bodies.last()?;
        b.joins.get(name).filter(|(_, arity)| *arity == n).map(|(id, _)| *id)
    }

    /// Binds a name to a fresh raw slot.
    fn bind_raw(&mut self, name: &str, char: bool) -> u32 {
        let b = self.bodies.last_mut().expect("body");
        b.joins.remove(name);
        let s = RAW | if char { RAW_CHAR } else { 0 } | b.nraw;
        b.nraw += 1;
        b.locals.insert(name.to_string(), s);
        s
    }

    fn bind_local(&mut self, name: &str) -> u32 {
        let b = self.bodies.last_mut().expect("body");
        b.joins.remove(name);
        let s = b.fresh_slot();
        b.locals.insert(name.to_string(), s);
        s
    }

    /// Letrec bindings: slots are bound first (so the right-hand sides can refer to each other),
    /// then closures and direct constructors are lowered into one batched `SExpr::Let`.
    fn let_binds(&mut self, binds: &[(String, Core)]) -> Vec<(u32, SBind)> {
        let slots: Vec<u32> = binds.iter().map(|(n, _)| self.bind_local(n)).collect();
        let mark = self.pending.len();
        let mut out = Vec::with_capacity(binds.len());
        for (slot, (_, rhs)) in slots.into_iter().zip(binds) {
            if let Core::Con(con, args) = rhs {
                let atoms: Vec<Atom> = args.iter().map(|a| self.atom(a)).collect();
                out.push((slot, SBind::Con(con.clone(), atoms)));
            } else {
                out.push((slot, SBind::Closure(self.closure_of(rhs))));
            }
        }
        let mut all: Vec<(u32, SBind)> = self.pending.drain(mark..).collect();
        all.extend(out);
        all
    }

    fn alt(&mut self, a: &Alt) -> SAlt {
        match a {
            Alt::Con(name, vars, body) => {
                let slots = vars.iter().map(|v| self.bind_local(v)).collect();
                SAlt::Con(name.clone(), slots, self.strict(body))
            }
            Alt::Lit(l, body) => SAlt::Lit(l.clone(), self.strict(body)),
            Alt::Default(body) => SAlt::Default(self.strict(body)),
        }
    }

    /// An argument: a variable or literal is passed as it is; a constructor is allocated directly
    /// in WHNF (zero heap allocation for nullary constructors, exposing nested selector thunks);
    /// anything else becomes a closure.
    fn atom(&mut self, e: &Core) -> Atom {
        match e {
            Core::Var(v) => self.resolve(v),
            // Numbers and characters are static objects; the others are built when they are used.
            Core::Lit(l @ (Lit::Int(_) | Lit::Double(_) | Lit::Char(_))) => Atom::Lit(slit(l)),
            Core::Con(con, args) => {
                let atoms: Vec<Atom> = args.iter().map(|a| self.atom(a)).collect();
                let b = self.bodies.last_mut().expect("body");
                let s = b.fresh_slot();
                self.pending.push((s, SBind::Con(con.clone(), atoms)));
                Atom::Slot(s)
            }
            other => {
                // A delayed expression (or a lambda): a closure, allocated before the operation.
                let c = self.closure_of(other);
                let b = self.bodies.last_mut().expect("body");
                let s = b.fresh_slot();
                self.pending.push((s, SBind::Closure(c)));
                Atom::Slot(s)
            }
        }
    }
}

/// Whether `e` (or the capture list of any nested closure it allocates) reads slot 0 or a free variable.
fn expr_uses_closure(e: &SExpr) -> bool {
    let atom = |a: &Atom| matches!(a, Atom::Slot(0) | Atom::Free(_));
    match e {
        SExpr::Atom(a) => atom(a),
        SExpr::App(f, args) => atom(f) || args.iter().any(atom),
        SExpr::CallWorker(_, args) | SExpr::Con(_, args) | SExpr::Prim(_, args) | SExpr::Jump(_, args) => {
            args.iter().any(atom)
        }
        SExpr::Let(binds, body) => {
            binds.iter().any(|(_, b)| b.atoms().iter().any(atom)) || expr_uses_closure(body)
        }
        SExpr::Case(scrut, _, alts) => {
            expr_uses_closure(scrut)
                || alts.iter().any(|a| match a {
                    SAlt::Con(_, _, b) | SAlt::Lit(_, b) | SAlt::Default(b) => expr_uses_closure(b),
                })
        }
        SExpr::Closure(c) => c.captures.iter().any(atom),
        SExpr::Fail(_) => false,
        SExpr::Join(_, _, code, body) => expr_uses_closure(code) || expr_uses_closure(body),
    }
}

/// `case (case e of p -> r) of alts` as `let j = \x -> case x of alts in case e of p -> j r`: the
/// alternatives of the inner case end in calls of the join point `j`, so no case is a scrutinee.
fn join_point(scrut: &Core, binder: &str, alts: &[Alt]) -> Core {
    let j = format!("{binder}%join");
    let x = format!("{binder}%x");
    let body = Core::Case(Box::new(Core::Var(x.clone())), binder.to_string(), alts.to_vec());
    let join = Core::Lam(x, Box::new(body));
    let jumped = redirect(scrut, &j);
    Core::Let(vec![(j, join)], Box::new(jumped))
}

/// Replaces each result of `e` (the tail expressions of its cases and lets) with an application of
/// the join point `j` to it.
fn redirect(e: &Core, j: &str) -> Core {
    match e {
        Core::Let(binds, body) => Core::Let(binds.clone(), Box::new(redirect(body, j))),
        Core::Case(scrut, b, alts) => Core::Case(
            scrut.clone(),
            b.clone(),
            alts.iter()
                .map(|a| match a {
                    Alt::Con(c, vs, body) => Alt::Con(c.clone(), vs.clone(), redirect(body, j)),
                    Alt::Lit(l, body) => Alt::Lit(l.clone(), redirect(body, j)),
                    Alt::Default(body) => Alt::Default(redirect(body, j)),
                })
                .collect(),
        ),
        Core::Fail(_) => e.clone(),
        // The join point evaluates its argument at once, so a value that is not an atom is
        // evaluated here rather than passed as a thunk.
        other @ (Core::Var(_) | Core::Lit(_)) => Core::App(Box::new(Core::Var(j.to_string())), Box::new(other.clone())),
        other => {
            let x = format!("{j}%v");
            Core::Case(
                Box::new(other.clone()),
                x.clone(),
                vec![Alt::Default(Core::App(Box::new(Core::Var(j.to_string())), Box::new(Core::Var(x))))],
            )
        }
    }
}

/// Whether a let is lowered as a join point: one non-recursive binding all of whose uses in the
/// body are calls with as many arguments as it has parameters, in tail position.
fn is_join(binds: &[(String, Core)], body: &Core) -> bool {
    let [(name, rhs)] = binds else { return false };
    !mentions(rhs, name) && tail_calls(body, name, lambdas(rhs).0)
}

/// The leading parameters of an expression, and what is under them.
fn lambdas(e: &Core) -> (usize, &Core) {
    let mut n = 0;
    let mut cur = e;
    while let Core::Lam(_, b) = cur {
        n += 1;
        cur = b;
    }
    (n, cur)
}

const IDENTITY_PRIMS: [&str; 6] = ["unsafeCoerce", "unsafeCoerce#", "lazy", "oneShot", "inline", "noinline"];

/// Whether every use of `j` in `e` is a call `j a1 .. an` in tail position (its arguments not
/// mentioning `j`). Tail positions are those `strict` lowers in the frame and mode of `e`: case
/// alternatives, let bodies, the code of join points, and the result of `seq` and identities.
fn tail_calls(e: &Core, j: &str, n: usize) -> bool {
    match e {
        Core::Var(v) => v != j || n == 0,
        Core::Lit(_) | Core::Fail(_) => true,
        Core::App(..) => {
            let (f, args) = spine(e);
            match f {
                Core::Var(v) if v == j => args.len() == n && args.iter().all(|a| !mentions(a, j)),
                _ => !mentions(e, j),
            }
        }
        Core::Lam(x, b) => x == j || !mentions(b, j),
        Core::Let(binds, body) => {
            if binds.iter().any(|(x, _)| x == j) {
                return true;
            }
            if is_join(binds, body) {
                let (_, code) = lambdas(&binds[0].1);
                let shadowed = {
                    let mut cur = &binds[0].1;
                    let mut hit = false;
                    while let Core::Lam(x, b) = cur {
                        hit |= x == j;
                        cur = b;
                    }
                    hit
                };
                (shadowed || tail_calls(code, j, n)) && tail_calls(body, j, n)
            } else {
                binds.iter().all(|(_, rhs)| !mentions(rhs, j)) && tail_calls(body, j, n)
            }
        }
        Core::Case(scrut, binder, alts) => {
            !mentions(scrut, j)
                && (binder == j
                    || alts.iter().all(|a| match a {
                        Alt::Con(_, vars, b) => vars.iter().any(|v| v == j) || tail_calls(b, j, n),
                        Alt::Lit(_, b) | Alt::Default(b) => tail_calls(b, j, n),
                    }))
        }
        Core::Prim(p, args) if p == "seq" && args.len() == 2 => !mentions(&args[0], j) && tail_calls(&args[1], j, n),
        Core::Prim(p, args) if IDENTITY_PRIMS.contains(&p.as_str()) && args.len() == 1 => tail_calls(&args[0], j, n),
        Core::Con(_, args) | Core::Prim(_, args) => args.iter().all(|a| !mentions(a, j)),
    }
}

/// Whether a variable occurs in an expression (shadowing ignored: a conservative answer).
fn mentions(e: &Core, j: &str) -> bool {
    match e {
        Core::Var(v) => v == j,
        Core::Lit(_) | Core::Fail(_) => false,
        Core::App(f, a) => mentions(f, j) || mentions(a, j),
        Core::Lam(_, b) => mentions(b, j),
        Core::Let(binds, body) => binds.iter().any(|(_, r)| mentions(r, j)) || mentions(body, j),
        Core::Case(s, _, alts) => {
            mentions(s, j)
                || alts.iter().any(|a| match a {
                    Alt::Con(_, _, b) | Alt::Lit(_, b) | Alt::Default(b) => mentions(b, j),
                })
        }
        Core::Con(_, args) | Core::Prim(_, args) => args.iter().any(|a| mentions(a, j)),
    }
}

fn alts_mention(alts: &[Alt], v: &str) -> bool {
    alts.iter().any(|a| match a {
        Alt::Con(_, _, b) | Alt::Lit(_, b) | Alt::Default(b) => mentions(b, v),
    })
}

/// Flattens nested `Case` expressions on the same scrutinee inside a trailing `Alt::Default`,
/// e.g. `case x as cb1 of C1 -> e1; _ -> case x as cb2 of C2 -> e2; _ -> e3` becomes
/// `case x as cb1 of C1 -> e1; C2 -> e2; _ -> e3`.
fn flatten_case_alts(scrut: &Core, binder: &str, alts: &[Alt]) -> Option<Vec<Alt>> {
    let same_scrut = |inner: &Core| match inner {
        Core::Var(y) => y == binder || matches!(scrut, Core::Var(x) if y == x),
        _ => false,
    };
    let Some(Alt::Default(Core::Case(inner_scrut, b2, inner_alts))) = alts.last() else {
        return None;
    };
    if !same_scrut(inner_scrut)
        || (b2 != binder && alts_mention(inner_alts, b2))
        || inner_alts.is_empty()
    {
        return None;
    }
    // Preserve `[Default]` on `writeMutVar#` so codegen's inline MutVar fast path still matches.
    if alts.len() == 1 && matches!(scrut, Core::Prim(p, _) if p == "writeMutVar#") {
        return None;
    }
    if alts[..alts.len() - 1].iter().any(|a| matches!(a, Alt::Default(_))) {
        return None;
    }

    let mut out: Vec<Alt> = alts[..alts.len() - 1].to_vec();
    let mut cur_case: (&Core, &str, &[Alt]) = (inner_scrut, b2, inner_alts);
    let mut flattened = false;

    loop {
        let (is, ib, ialts) = cur_case;
        if !same_scrut(is) || (ib != binder && alts_mention(ialts, ib)) || ialts.is_empty() {
            out.push(Alt::Default(Core::Case(
                Box::new(is.clone()),
                ib.to_string(),
                ialts.to_vec(),
            )));
            break;
        }
        let has_early_default = ialts[..ialts.len() - 1].iter().any(|a| matches!(a, Alt::Default(_)));
        let overlaps = ialts.iter().any(|ia| match ia {
            Alt::Con(c, _, _) => out.iter().any(|oa| matches!(oa, Alt::Con(oc, _, _) if oc == c)),
            Alt::Lit(l, _) => out.iter().any(|oa| matches!(oa, Alt::Lit(ol, _) if ol == l)),
            Alt::Default(_) => false,
        });
        if has_early_default || overlaps {
            out.push(Alt::Default(Core::Case(
                Box::new(is.clone()),
                ib.to_string(),
                ialts.to_vec(),
            )));
            break;
        }
        flattened = true;
        match ialts.last() {
            Some(Alt::Default(Core::Case(ns, nb, nalts))) => {
                out.extend_from_slice(&ialts[..ialts.len() - 1]);
                cur_case = (ns, nb, nalts);
            }
            _ => {
                out.extend_from_slice(ialts);
                break;
            }
        }
    }

    flattened.then_some(out)
}

#[derive(Eq, Hash, PartialEq)]
enum EqLitKey {
    Int(i64),
    Char(char),
}

/// Extracts a single equality-comparison branch `case eqPrim x lit as b of 1# -> hit; _ -> miss`
/// (or the `0#` swapped polarity, or with an intermediate raw binder `case eqPrim x lit as b of _ -> case b of ...`).
fn match_eq_step(e: &Core) -> Option<(&str, Lit, bool, &Core, &Core, &str)> {
    let Core::Case(scrut, b1, alts) = e else { return None };
    let Core::Prim(p, args) = scrut.as_ref() else { return None };
    if args.len() != 2 {
        return None;
    }
    let (var, lit, is_char) = match (p.as_str(), &args[0], &args[1]) {
        ("eqChar#", Core::Var(x), Core::Lit(l @ Lit::Char(_)))
        | ("eqChar#", Core::Lit(l @ Lit::Char(_)), Core::Var(x)) => (x.as_str(), l.clone(), true),
        ("==#" | "eqWord#", Core::Var(x), Core::Lit(l @ Lit::Int(_)))
        | ("==#" | "eqWord#", Core::Lit(l @ Lit::Int(_)), Core::Var(x)) => (x.as_str(), l.clone(), false),
        _ => return None,
    };
    fn extract_bool_alts(alts: &[Alt]) -> Option<(&Core, &Core)> {
        match alts {
            [Alt::Lit(Lit::Int(1), hit), Alt::Default(miss)] => Some((hit, miss)),
            [Alt::Lit(Lit::Int(0), miss), Alt::Default(hit)] => Some((hit, miss)),
            _ => None,
        }
    }
    if let Some((hit, miss)) = extract_bool_alts(alts) {
        if !mentions(hit, b1) && !mentions(miss, b1) {
            return Some((var, lit, is_char, hit, miss, b1.as_str()));
        }
        return None;
    }
    if let [Alt::Default(Core::Case(s2, b2, alts2))] = &alts[..] {
        if matches!(s2.as_ref(), Core::Var(v) if v == b1) {
            if let Some((hit, miss)) = extract_bool_alts(alts2) {
                if !mentions(hit, b1)
                    && !mentions(miss, b1)
                    && !mentions(hit, b2)
                    && !mentions(miss, b2)
                {
                    return Some((var, lit, is_char, hit, miss, b1.as_str()));
                }
            }
        }
    }
    None
}

/// Collapses chained equality-comparison cases (`eqChar#`, `==#`, `eqWord#`) and nested
/// same-scrutinee `Case` chains into flat multi-way `Case` expressions before worker analysis.
pub fn collapse_case_chains(binds: &mut [TopBind]) {
    fn walk(e: &mut Core) {
        // Iteratively peel chains of >= 2 equality comparisons on the same variable.
        if let Some((var, first_lit, is_char, _, _, b1)) = match_eq_step(e) {
            let var_owned = var.to_string();
            let b_fresh = format!("{b1}%eq");
            let mut steps: Vec<(Lit, Core)> = Vec::new();
            let mut seen = std::collections::HashSet::new();
            let mut cur_ref: &Core = e;
            let mut count = 0usize;
            while let Some((v, lit, ic, hit, miss, _)) = match_eq_step(cur_ref) {
                if v != var_owned || ic != is_char {
                    break;
                }
                count += 1;
                let key = match &lit {
                    Lit::Int(i) => EqLitKey::Int(*i),
                    Lit::Char(c) => EqLitKey::Char(*c),
                    _ => unreachable!(),
                };
                if seen.insert(key) {
                    steps.push((lit, hit.clone()));
                }
                cur_ref = miss;
            }
            let _ = first_lit;
            if count >= 2 {
                let mut alts: Vec<Alt> = steps.into_iter().map(|(l, h)| Alt::Lit(l, h)).collect();
                alts.push(Alt::Default(cur_ref.clone()));
                *e = Core::Case(Box::new(Core::Var(var_owned)), b_fresh, alts);
            }
        }
        match e {
            Core::Var(_) | Core::Lit(_) | Core::Fail(_) => {}
            Core::App(f, a) => {
                walk(f);
                walk(a);
            }
            Core::Lam(_, b) => walk(b),
            Core::Let(binds, body) => {
                for (_, r) in binds {
                    walk(r);
                }
                walk(body);
            }
            Core::Case(scrut, binder, alts) => {
                if matches!(scrut.as_ref(), Core::Var(_)) {
                    if let Some(flat) = flatten_case_alts(scrut, binder, alts) {
                        *alts = flat;
                    }
                }
                walk(scrut);
                for a in alts {
                    match a {
                        Alt::Con(_, _, b) | Alt::Lit(_, b) | Alt::Default(b) => walk(b),
                    }
                }
            }
            Core::Con(_, args) | Core::Prim(_, args) => {
                for a in args {
                    walk(a);
                }
            }
        }
    }
    for b in binds {
        walk(&mut b.body);
    }
}

/// The head and arguments of an application spine.
fn spine(e: &Core) -> (&Core, Vec<&Core>) {
    let mut args = Vec::new();
    let mut cur = e;
    while let Core::App(f, a) = cur {
        args.push(a.as_ref());
        cur = f;
    }
    args.reverse();
    (cur, args)
}

fn slit(l: &Lit) -> SLit {
    match l {
        Lit::Integer(s) => SLit::Integer(s.clone()),
        Lit::Int(i) => SLit::Int(*i),
        Lit::Double(d) => SLit::Double(*d),
        Lit::Frac(s) => SLit::Frac(s.clone()),
        Lit::Char(c) => SLit::Char(*c),
        Lit::Str(s) => SLit::Str(s.clone()),
        Lit::Addr(s) => SLit::Addr(s.clone()),
    }
}

#[cfg(test)]
mod raw_slot_tests {
    use super::*;

    fn var(v: &str) -> Core {
        Core::Var(v.into())
    }

    fn prim(p: &str, args: Vec<Core>) -> Core {
        Core::Prim(p.into(), args)
    }

    /// Every parameter typed as an Int (the types themselves are not under test here).
    fn numeric_all(binds: &[TopBind]) -> NumericParams {
        NumericParams { params: binds.iter().map(|b| (b.name.clone(), vec![NUM_INT; 8])).collect(), results: HashMap::new() }
    }

    fn case(s: Core, b: &str, body: Core) -> Core {
        Core::Case(Box::new(s), b.into(), vec![Alt::Default(body)])
    }

    /// Lowers `Test.f = \x -> body` beside `Test.k`, a CAF (an unknown function value).
    fn lower(body: Core) -> SClosure {
        let binds = vec![
            TopBind { name: "Test.f".into(), body: Core::Lam("x".into(), Box::new(body)) },
            TopBind { name: "Test.k".into(), body: Core::Var("Test.k".into()) },
        ];
        Lowering::program(&binds, &numeric_all(&binds)).globals.remove(0).closure
    }

    #[test]
    fn a_result_used_only_by_inline_primitives_is_raw() {
        // case x +# 1# as y of _ -> y *# y
        let c = lower(case(prim("+#", vec![var("x"), Core::Lit(Lit::Int(1))]), "y", prim("*#", vec![var("y"), var("y")])));
        assert_eq!(c.nraw, 1);
        let SExpr::Case(_, b, _) = &c.body else { panic!("{:?}", c.body) };
        assert_ne!(b & RAW, 0);
    }

    #[test]
    fn a_literal_case_or_a_result_keeps_it_raw() {
        let lits = Core::Case(
            Box::new(var("y")),
            "z".into(),
            vec![Alt::Lit(Lit::Int(0), Core::Lit(Lit::Int(1))), Alt::Default(var("y"))],
        );
        let c = lower(case(prim("remInt#", vec![var("x"), Core::Lit(Lit::Int(4))]), "y", lits));
        assert_eq!(c.nraw, 1);
    }

    #[test]
    fn results_that_escape_stay_boxed() {
        // Passed to an unknown function, stored in a constructor, captured, the case binder used,
        // in an argument expression, or a result of a scrutinee (a join point's argument).
        let escapes = vec![
            Core::App(Box::new(var("Test.k")), Box::new(var("y"))),
            Core::Con("Test.K".into(), vec![var("y")]),
            Core::Let(vec![("t".into(), prim("+#", vec![var("y"), var("y")]))], Box::new(var("t"))),
            Core::Case(Box::new(var("y")), "z".into(), vec![Alt::Default(var("z"))]),
            Core::App(Box::new(var("Test.k")), Box::new(prim("+#", vec![var("y"), Core::Lit(Lit::Int(1))]))),
            Core::Case(
                Box::new(Core::Case(Box::new(var("x")), "u".into(), vec![Alt::Default(var("y"))])),
                "s".into(),
                vec![Alt::Default(var("s"))],
            ),
        ];
        for body in escapes {
            let c = lower(case(prim("+#", vec![var("x"), var("x")]), "y", body.clone()));
            assert_eq!(c.nraw, 0, "{body:?}");
        }
    }

    #[test]
    fn strict_parameters_used_raw_get_a_worker() {
        // f x = case x ># 0# of 1# -> x; _ -> f (x +# 1#): x is evaluated and used raw.
        let body = Core::Case(
            Box::new(prim(">#", vec![var("x"), Core::Lit(Lit::Int(0))])),
            "c".into(),
            vec![Alt::Default(Core::Case(
                Box::new(var("c")),
                "d".into(),
                vec![
                    Alt::Lit(Lit::Int(1), var("x")),
                    Alt::Default(Core::App(Box::new(var("Test.f")), Box::new(prim("+#", vec![var("x"), Core::Lit(Lit::Int(1))])))),
                ],
            ))],
        );
        let binds = vec![TopBind { name: "Test.f".into(), body: Core::Lam("x".into(), Box::new(body)) }];
        let w = workers(&binds, &numeric_all(&binds));
        assert_eq!(w.get("Test.f"), Some(&vec![true]));
        let g = Lowering::program(&binds, &numeric_all(&binds)).globals.remove(0);
        let (worker, _) = g.worker.expect("a worker");
        assert_eq!(worker.raw_args.len(), 1);
    }

    #[test]
    fn a_parameter_the_types_do_not_make_a_number_gets_no_worker() {
        // A GADT match: `x` is a Char in one alternative only (its type is a variable).
        let body = Core::Case(
            Box::new(var("t")),
            "u".into(),
            vec![
                Alt::Con("Test.TChr".into(), vec![], case(prim("eqChar#", vec![var("x"), Core::Lit(Lit::Char('a'))]), "e", var("x"))),
                Alt::Con("Test.TStr".into(), vec![], var("x")),
            ],
        );
        let binds = vec![TopBind {
            name: "Test.f".into(),
            body: Core::Lam("t".into(), Box::new(Core::Lam("x".into(), Box::new(body)))),
        }];
        let untyped = NumericParams { params: [("Test.f".to_string(), vec![0, 0])].into_iter().collect(), results: HashMap::new() };
        assert!(workers(&binds, &untyped).is_empty());
    }

    #[test]
    fn a_parameter_not_used_as_a_number_gets_no_worker() {
        // A newtype's constructor (\x -> x) returns its evaluated parameter, of any type.
        let binds = vec![TopBind { name: "Test.K1".into(), body: Core::Lam("x".into(), Box::new(var("x"))) }];
        assert!(workers(&binds, &numeric_all(&binds)).is_empty());
    }

    #[test]
    fn nested_same_scrutinee_cases_flatten() {
        // case x as cb1 of A -> 1#; _ -> case x as cb2 of B -> 2#; _ -> case x as cb3 of C -> 3#; _ -> fail
        let inner3 = Core::Case(
            Box::new(var("x")),
            "cb3".into(),
            vec![Alt::Con("Test.C".into(), vec![], Core::Lit(Lit::Int(3))), Alt::Default(Core::Fail("no".into()))],
        );
        let inner2 = Core::Case(
            Box::new(var("x")),
            "cb2".into(),
            vec![Alt::Con("Test.B".into(), vec![], Core::Lit(Lit::Int(2))), Alt::Default(inner3)],
        );
        let outer = Core::Case(
            Box::new(var("x")),
            "cb1".into(),
            vec![Alt::Con("Test.A".into(), vec![], Core::Lit(Lit::Int(1))), Alt::Default(inner2)],
        );
        let c = lower(outer);
        let SExpr::Case(_, _, alts) = &c.body else { panic!("{:?}", c.body) };
        assert_eq!(alts.len(), 4);
        assert!(matches!(&alts[0], SAlt::Con(n, _, _) if n == "Test.A"));
        assert!(matches!(&alts[1], SAlt::Con(n, _, _) if n == "Test.B"));
        assert!(matches!(&alts[2], SAlt::Con(n, _, _) if n == "Test.C"));
        assert!(matches!(&alts[3], SAlt::Default(SExpr::Fail(_))));
    }

    #[test]
    fn nested_case_with_used_inner_binder_or_overlap_does_not_flatten() {
        // Inner binder `cb2` is referenced in inner alt body -> must not flatten.
        let inner_used = Core::Case(
            Box::new(var("x")),
            "cb2".into(),
            vec![Alt::Con("Test.B".into(), vec![], var("cb2")), Alt::Default(Core::Lit(Lit::Int(0)))],
        );
        let outer_used = Core::Case(
            Box::new(var("x")),
            "cb1".into(),
            vec![Alt::Con("Test.A".into(), vec![], Core::Lit(Lit::Int(1))), Alt::Default(inner_used)],
        );
        let c = lower(outer_used);
        let SExpr::Case(_, _, alts) = &c.body else { panic!("{:?}", c.body) };
        assert_eq!(alts.len(), 2);
        assert!(matches!(&alts[1], SAlt::Default(SExpr::Case(..))));

        // Overlapping constructor `Test.A` -> must not flatten.
        let inner_dup = Core::Case(
            Box::new(var("x")),
            "cb2".into(),
            vec![Alt::Con("Test.A".into(), vec![], Core::Lit(Lit::Int(2))), Alt::Default(Core::Lit(Lit::Int(0)))],
        );
        let outer_dup = Core::Case(
            Box::new(var("x")),
            "cb1".into(),
            vec![Alt::Con("Test.A".into(), vec![], Core::Lit(Lit::Int(1))), Alt::Default(inner_dup)],
        );
        let c2 = lower(outer_dup);
        let SExpr::Case(_, _, alts2) = &c2.body else { panic!("{:?}", c2.body) };
        assert_eq!(alts2.len(), 2);
        assert!(matches!(&alts2[1], SAlt::Default(SExpr::Case(..))));
    }

    #[test]
    fn collapse_case_chains_folds_eq_chains_and_deduplicates() {
        // case eqChar# c 'a' of 1# -> 1; _ -> case eqChar# c 'b' of 1# -> 2; _ -> case eqChar# c 'a' of 1# -> 9; _ -> 0
        let step3 = Core::Case(
            Box::new(prim("eqChar#", vec![var("c"), Core::Lit(Lit::Char('a'))])),
            "e3".into(),
            vec![Alt::Lit(Lit::Int(1), Core::Lit(Lit::Int(9))), Alt::Default(Core::Lit(Lit::Int(0)))],
        );
        let step2 = Core::Case(
            Box::new(prim("eqChar#", vec![var("c"), Core::Lit(Lit::Char('b'))])),
            "e2".into(),
            vec![Alt::Lit(Lit::Int(1), Core::Lit(Lit::Int(2))), Alt::Default(step3)],
        );
        let step1 = Core::Case(
            Box::new(prim("eqChar#", vec![var("c"), Core::Lit(Lit::Char('a'))])),
            "e1".into(),
            vec![Alt::Lit(Lit::Int(1), Core::Lit(Lit::Int(1))), Alt::Default(step2)],
        );
        let mut binds = vec![TopBind { name: "Test.f".into(), body: Core::Lam("c".into(), Box::new(step1)) }];
        collapse_case_chains(&mut binds);
        let Core::Lam(_, body) = &binds[0].body else { panic!("expected Lam") };
        let Core::Case(scrut, _, alts) = body.as_ref() else { panic!("expected flat Case, got {body:?}") };
        assert!(matches!(scrut.as_ref(), Core::Var(v) if v == "c"));
        assert_eq!(alts.len(), 3);
        assert!(matches!(&alts[0], Alt::Lit(Lit::Char('a'), Core::Lit(Lit::Int(1)))));
        assert!(matches!(&alts[1], Alt::Lit(Lit::Char('b'), Core::Lit(Lit::Int(2)))));
        assert!(matches!(&alts[2], Alt::Default(Core::Lit(Lit::Int(0)))));
    }

    #[test]
    fn zero_raw_param_function_can_get_raw_returning_worker() {
        // size t = case t of Tip -> 0#; Bin l r -> case size l as a of _ -> case size r as b of _ -> a +# b
        let body = Core::Case(
            Box::new(var("t")),
            "u".into(),
            vec![
                Alt::Con("Test.Tip".into(), vec![], Core::Lit(Lit::Int(0))),
                Alt::Con(
                    "Test.Bin".into(),
                    vec!["l".into(), "r".into()],
                    case(
                        Core::App(Box::new(var("Test.size")), Box::new(var("l"))),
                        "a",
                        case(
                            Core::App(Box::new(var("Test.size")), Box::new(var("r"))),
                            "b",
                            prim("+#", vec![var("a"), var("b")]),
                        ),
                    ),
                ),
            ],
        );
        let binds = vec![TopBind { name: "Test.size".into(), body: Core::Lam("t".into(), Box::new(body)) }];
        let numeric = NumericParams {
            params: [("Test.size".to_string(), vec![0])].into_iter().collect(),
            results: [("Test.size".to_string(), (1, NUM_INT))].into_iter().collect(),
        };
        let prog = Lowering::program(&binds, &numeric);
        let (worker, raw) = prog.globals[0].worker.as_ref().expect("0-raw-param raw-returning worker");
        assert_eq!(raw, &vec![false]);
        assert!(worker.ret_raw);
        assert!(worker.raw_args.is_empty());
        // Recursive call inside worker uses CallWorker into a raw slot.
        let SExpr::Case(_, _, alts) = &worker.body else { panic!("{:?}", worker.body) };
        let SAlt::Con(_, _, bin_body) = &alts[1] else { panic!("{:?}", alts[1]) };
        let SExpr::Case(scrut, b, _) = bin_body else { panic!("{bin_body:?}") };
        assert!(matches!(scrut.as_ref(), SExpr::CallWorker(0, _)), "{scrut:?}");
        assert_ne!(b & RAW, 0);
    }

    #[test]
    fn whnf_guarded_worker_called_only_on_whnf_arguments() {
        // member k t = case t of Tip -> 0#; Bin x l r -> case k <# x of 1# -> member k l; _ -> member k r
        // `k` is NOT certainly evaluated (Tip doesn't touch `k`), so `member` gets a WHNF-guarded worker.
        let member_body = Core::Case(
            Box::new(var("t")),
            "u".into(),
            vec![
                Alt::Con("Test.Tip".into(), vec![], Core::Lit(Lit::Int(0))),
                Alt::Con(
                    "Test.Bin".into(),
                    vec!["x".into(), "l".into(), "r".into()],
                    case(
                        prim("<#", vec![var("k"), var("x")]),
                        "c",
                        Core::Case(
                            Box::new(var("c")),
                            "d".into(),
                            vec![
                                Alt::Lit(
                                    Lit::Int(1),
                                    Core::App(Box::new(Core::App(Box::new(var("Test.member")), Box::new(var("k")))), Box::new(var("l"))),
                                ),
                                Alt::Default(Core::App(
                                    Box::new(Core::App(Box::new(var("Test.member")), Box::new(var("k")))),
                                    Box::new(var("r")),
                                )),
                            ],
                        ),
                    ),
                ),
            ],
        );
        // Caller 1: `caller_lit t = member 7# t` -> literal 7# is WHNF -> CallWorker.
        let caller_lit = Core::Lam(
            "t".into(),
            Box::new(Core::App(
                Box::new(Core::App(Box::new(var("Test.member")), Box::new(Core::Lit(Lit::Int(7))))),
                Box::new(var("t")),
            )),
        );
        // Caller 2: `caller_lazy k t = member k t` -> unforced `k` -> must NOT call worker!
        let caller_lazy = Core::Lam(
            "k".into(),
            Box::new(Core::Lam(
                "t".into(),
                Box::new(Core::App(
                    Box::new(Core::App(Box::new(var("Test.member")), Box::new(var("k")))),
                    Box::new(var("t")),
                )),
            )),
        );
        // Caller 3: `caller_div k t = case k of _ -> member (k quotInt# 0#) t` -> quotInt# can trap -> boxed App!
        let caller_div = Core::Lam(
            "k".into(),
            Box::new(Core::Lam(
                "t".into(),
                Box::new(case(
                    var("k"),
                    "kb",
                    Core::App(
                        Box::new(Core::App(
                            Box::new(var("Test.member")),
                            Box::new(prim("quotInt#", vec![var("k"), Core::Lit(Lit::Int(0))])),
                        )),
                        Box::new(var("t")),
                    ),
                )),
            )),
        );
        let binds = vec![
            TopBind {
                name: "Test.member".into(),
                body: Core::Lam("k".into(), Box::new(Core::Lam("t".into(), Box::new(member_body)))),
            },
            TopBind { name: "Test.caller_lit".into(), body: caller_lit },
            TopBind { name: "Test.caller_lazy".into(), body: caller_lazy },
            TopBind { name: "Test.caller_div".into(), body: caller_div },
        ];
        let numeric = NumericParams {
            params: [("Test.member".to_string(), vec![NUM_INT, 0])].into_iter().collect(),
            results: [("Test.member".to_string(), (2, NUM_INT))].into_iter().collect(),
        };
        let prog = Lowering::program(&binds, &numeric);
        let (worker, raw) = prog.globals[0].worker.as_ref().expect("WHNF-guarded worker");
        assert_eq!(raw, &vec![true, false]);
        assert!(worker.ret_raw);
        // caller_lit invokes worker (wrapped in through_raw to box the raw return).
        let SExpr::Case(scrut, _, _) = &prog.globals[1].closure.body else {
            panic!("expected through_raw Case, got {:?}", prog.globals[1].closure.body);
        };
        assert!(matches!(scrut.as_ref(), SExpr::CallWorker(0, _)), "{scrut:?}");
        // caller_lazy must stay a boxed App so `member undefined Tip` does not diverge.
        assert!(matches!(prog.globals[2].closure.body, SExpr::App(..)), "{:?}", prog.globals[2].closure.body);
        // caller_div must stay a boxed App because `quotInt#` can divide by zero.
        let SExpr::Case(_, _, alts) = &prog.globals[3].closure.body else { panic!("expected Case") };
        let SAlt::Default(div_body) = &alts[0] else { panic!("expected Default") };
        assert!(matches!(div_body, SExpr::Let(_, b) if matches!(b.as_ref(), SExpr::App(..))), "{div_body:?}");
    }
}

