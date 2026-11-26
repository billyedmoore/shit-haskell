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

/// A heap object bound in `SExpr::Let`: a closure (function/thunk), a constructor allocated
/// directly in weak head normal form, or a boxed cell materialized from a raw slot.
#[derive(Clone, Debug)]
pub enum SBind {
    Closure(SClosure),
    Con(String, Vec<Atom>),
    BoxRaw(u32),
}

impl SBind {
    pub fn atoms(&self) -> &[Atom] {
        match self {
            SBind::Closure(c) => &c.captures,
            SBind::Con(_, args) => args,
            SBind::BoxRaw(_) => &[],
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

/// Sentinel slot index for an unused (dead) `Case` binder or `SAlt::Con` field.
/// High bits (`RAW`, `RAW_CHAR`) are clear so `slot & RAW == 0` stays false.
pub const UNUSED_SLOT: u32 = 0x3fff_ffff;

/// Whether a Core application of `g` to `nargs` arguments is a known diverging error call.
fn is_diverging_call(g: &str, nargs: usize) -> bool {
    let Some((mod_name, base)) = g.rsplit_once('.') else { return false };
    let std_err_mod = matches!(
        mod_name,
        "GHC.Internal.Control.Exception.Base"
            | "Control.Exception.Base"
            | "GHC.Control.Exception.Base"
            | "GHC.Internal.Err"
            | "GHC.Err"
            | "Prelude"
    );
    if !std_err_mod {
        return false;
    }
    match base {
        "patError"
        | "irrefutPatError"
        | "recSelError"
        | "recConError"
        | "recUpdError"
        | "nonExhaustiveGuardsError"
        | "noMethodBindingError"
        | "absentError"
        | "typeError"
        | "errorWithoutStackTrace" => nargs >= 1,
        "error" => nargs >= 2,
        "undefined" => nargs >= 1,
        _ => false,
    }
}

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
            let body_shadowed = binds.iter().any(|(n, _)| n == x);
            if result && is_join(binds, body) {
                let mut code_whnf = whnf.clone();
                let mut cur = &binds[0].1;
                let mut shadowed = false;
                while let Core::Lam(p, inner) = cur {
                    if p == x {
                        shadowed = true;
                    }
                    code_whnf.remove(p);
                    cur = inner;
                }
                (shadowed || raw_uses_in(cur, x, w, strict_map, &code_whnf, result))
                    && (body_shadowed || raw_uses_in(body, x, w, strict_map, &next, result))
            } else {
                binds.iter().all(|(_, r)| !mentions(r, x))
                    && (body_shadowed || raw_uses_in(body, x, w, strict_map, &next, result))
            }
        }
        Core::Case(s, b, alts) => {
            if !raw_uses_in(s, x, w, strict_map, whnf, false) {
                return false;
            }
            if b == x {
                return true;
            }
            let mut base_whnf = whnf.clone();
            add_case_whnf(s, b, &mut base_whnf);
            alts.iter().all(|a| match a {
                Alt::Con(_, vars, body) => {
                    if vars.iter().any(|v| v == x) {
                        return true;
                    }
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
                        if is_diverging_call(g.as_str(), args.len())
                            || (out.contains_key(g.as_str())
                                && can_call_worker(g.as_str(), &args, w_all, strict_map, whnf))
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
                if raw[i]
                    && !raw_uses(body, p, &w, &strict_map, &init_whnf)
                    && !(mentions(body, &name)
                        && no_bad_raw_case(body, p)
                        && has_unboxed_recursive_path(body, p, &name, &w, &strict_map, &init_whnf))
                {
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

/// Whether `e` never scrutinizes `x` with a `Case` whose alternatives cannot match a raw slot
/// (anything other than `Int`/`Char` literals or `Default`).
fn no_bad_raw_case(e: &Core, x: &str) -> bool {
    match e {
        Core::Var(_) | Core::Lit(_) | Core::Fail(_) => true,
        Core::App(f, a) => no_bad_raw_case(f, x) && no_bad_raw_case(a, x),
        Core::Lam(p, b) => p == x || no_bad_raw_case(b, x),
        Core::Let(binds, body) => {
            if binds.iter().any(|(n, _)| n == x) {
                return true;
            }
            binds.iter().all(|(_, r)| no_bad_raw_case(r, x)) && no_bad_raw_case(body, x)
        }
        Core::Case(s, b, alts) => {
            let mut cur = s.as_ref();
            while let Core::Prim(p, args) = cur {
                if IDENTITY_PRIMS.contains(&p.as_str()) && args.len() == 1 {
                    cur = &args[0];
                } else {
                    break;
                }
            }
            if matches!(cur, Core::Var(v) if v == x) {
                let alts_ok = alts
                    .iter()
                    .all(|a| matches!(a, Alt::Lit(Lit::Int(_) | Lit::Char(_), _) | Alt::Default(_)));
                if !alts_ok {
                    return false;
                }
            } else if !no_bad_raw_case(s, x) {
                return false;
            }
            b == x
                || alts.iter().all(|a| match a {
                    Alt::Con(_, vars, body) => vars.iter().any(|v| v == x) || no_bad_raw_case(body, x),
                    Alt::Lit(_, body) | Alt::Default(body) => no_bad_raw_case(body, x),
                })
        }
        Core::Con(_, args) | Core::Prim(_, args) => args.iter().all(|a| no_bad_raw_case(a, x)),
    }
}

/// Whether `e` contains an exact worker call to `target` along an evaluated path.
fn has_self_raw_call(
    e: &Core,
    target: &str,
    w: &Workers,
    strict_map: &Workers,
    whnf: &std::collections::HashSet<String>,
) -> bool {
    match e {
        Core::App(..) => {
            let (f, args) = spine(e);
            matches!(f, Core::Var(g) if g == target && can_call_worker(g.as_str(), &args, w, strict_map, whnf))
        }
        Core::Prim(p, args) if p == "seq" && args.len() == 2 => {
            if let Core::Var(v) = &args[0] {
                let mut next = whnf.clone();
                next.insert(v.clone());
                has_self_raw_call(&args[1], target, w, strict_map, &next)
            } else {
                has_self_raw_call(&args[1], target, w, strict_map, whnf)
            }
        }
        Core::Prim(p, args) if IDENTITY_PRIMS.contains(&p.as_str()) && args.len() == 1 => {
            has_self_raw_call(&args[0], target, w, strict_map, whnf)
        }
        Core::Let(binds, body) => {
            let mut next = whnf.clone();
            for (n, _) in binds {
                next.remove(n);
            }
            has_self_raw_call(body, target, w, strict_map, &next)
        }
        Core::Case(s, b, alts) => {
            if has_self_raw_call(s, target, w, strict_map, whnf) {
                return true;
            }
            let mut base_whnf = whnf.clone();
            add_case_whnf(s, b, &mut base_whnf);
            alts.iter().any(|a| match a {
                Alt::Con(_, vars, body) => {
                    let mut alt_whnf = base_whnf.clone();
                    for v in vars {
                        alt_whnf.remove(v);
                    }
                    has_self_raw_call(body, target, w, strict_map, &alt_whnf)
                }
                Alt::Lit(_, body) | Alt::Default(body) => has_self_raw_call(body, target, w, strict_map, &base_whnf),
            })
        }
        _ => false,
    }
}

/// Whether `e` keeps `x` unboxed along at least one recursive path back to `target` without
/// boxing `x` before the branch, allowing colder alternatives (e.g. constructing a solution in
/// backtracking search) to box `x` on demand via `SBind::BoxRaw`.
fn has_unboxed_recursive_path(
    e: &Core,
    x: &str,
    target: &str,
    w: &Workers,
    strict_map: &Workers,
    whnf: &std::collections::HashSet<String>,
) -> bool {
    match e {
        Core::Prim(p, args) if p == "seq" && args.len() == 2 => {
            if mentions(&args[0], x) {
                return false;
            }
            if let Core::Var(v) = &args[0] {
                let mut next = whnf.clone();
                next.insert(v.clone());
                has_unboxed_recursive_path(&args[1], x, target, w, strict_map, &next)
            } else {
                has_unboxed_recursive_path(&args[1], x, target, w, strict_map, whnf)
            }
        }
        Core::Prim(p, args) if IDENTITY_PRIMS.contains(&p.as_str()) && args.len() == 1 => {
            has_unboxed_recursive_path(&args[0], x, target, w, strict_map, whnf)
        }
        Core::Let(binds, body) => {
            if binds.iter().any(|(n, _)| n == x) {
                return false;
            }
            let mut next = whnf.clone();
            for (n, _) in binds {
                next.remove(n);
            }
            if is_join(binds, body) {
                let mut code_whnf = whnf.clone();
                let mut cur = &binds[0].1;
                let mut shadowed = false;
                while let Core::Lam(p, inner) = cur {
                    if p == x {
                        shadowed = true;
                    }
                    code_whnf.remove(p);
                    cur = inner;
                }
                (shadowed || raw_uses_in(cur, x, w, strict_map, &code_whnf, true))
                    && has_unboxed_recursive_path(body, x, target, w, strict_map, &next)
            } else {
                binds.iter().all(|(_, r)| !mentions(r, x))
                    && has_unboxed_recursive_path(body, x, target, w, strict_map, &next)
            }
        }
        Core::Case(s, b, alts) => {
            let scrut_is_x = matches!(&**s, Core::Var(v) if v == x);
            if !scrut_is_x && !raw_uses_in(s, x, w, strict_map, whnf, false) {
                return false;
            }
            if b == x {
                return false;
            }
            let mut base_whnf = whnf.clone();
            add_case_whnf(s, b, &mut base_whnf);
            alts.iter().any(|a| match a {
                Alt::Con(_, vars, body) => {
                    if scrut_is_x || vars.iter().any(|v| v == x) {
                        return false;
                    }
                    let mut alt_whnf = base_whnf.clone();
                    for v in vars {
                        alt_whnf.remove(v);
                    }
                    (raw_uses_in(body, x, w, strict_map, &alt_whnf, true)
                        && has_self_raw_call(body, target, w, strict_map, &alt_whnf))
                        || has_unboxed_recursive_path(body, x, target, w, strict_map, &alt_whnf)
                }
                Alt::Lit(l, body) => {
                    if scrut_is_x && (!matches!(l, Lit::Int(_) | Lit::Char(_)) || mentions(body, b)) {
                        return false;
                    }
                    (raw_uses_in(body, x, w, strict_map, &base_whnf, true)
                        && has_self_raw_call(body, target, w, strict_map, &base_whnf))
                        || has_unboxed_recursive_path(body, x, target, w, strict_map, &base_whnf)
                }
                Alt::Default(body) => {
                    if scrut_is_x && mentions(body, b) {
                        return false;
                    }
                    (raw_uses_in(body, x, w, strict_map, &base_whnf, true)
                        && has_self_raw_call(body, target, w, strict_map, &base_whnf))
                        || has_unboxed_recursive_path(body, x, target, w, strict_map, &base_whnf)
                }
            })
        }
        _ => false,
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
    /// Objects for the non-atomic arguments of the operation being lowered in this body.
    pending: Vec<(u32, SBind)>,
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
            pending: Vec::new(),
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
        debug_assert!(b.pending.is_empty(), "internal: unconsumed pending bindings");
        let captures: Vec<Atom> = b.captures.iter().map(|(_, a)| a.clone()).collect();
        let uses_closure = !captures.is_empty() || expr_uses_closure(&body);
        let mut closure = SClosure {
            arity,
            captures,
            nslots: b.nslots,
            nraw: b.nraw,
            raw_args,
            ret_raw: b.ret_raw.is_some(),
            uses_closure,
            body,
        };
        compact_raw_slots(&mut closure);
        closure
    }

    fn pending_len(&self) -> usize {
        self.bodies.last().expect("body").pending.len()
    }

    /// Boxes raw slot `r` in body `depth` by scheduling `SBind::BoxRaw(r)` on its pending list.
    fn box_raw_at(&mut self, depth: usize, r: u32) -> Atom {
        let b = &mut self.bodies[depth];
        if let Some((s, _)) = b.pending.iter().find(|(_, bind)| matches!(bind, SBind::BoxRaw(prev) if *prev == r)) {
            return Atom::Slot(*s);
        }
        let s = b.fresh_slot();
        b.pending.push((s, SBind::BoxRaw(r)));
        Atom::Slot(s)
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
        // Capture from the enclosing body; raw slots cannot be captured directly by a closure
        // (the collector traces closure captures as heap pointers), so box on demand in the outer body.
        let mut outer = self.resolve_at(depth - 1, name)?;
        if let Atom::Slot(r) = outer {
            if r & RAW != 0 {
                outer = self.box_raw_at(depth - 1, r);
            }
        }
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
                let mark = self.pending_len();
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
                let diverging = matches!(f, Core::Var(g) if is_diverging_call(g.as_str(), args.len()));
                match (callee_raw, self.raw_result()) {
                    // A raw result where a raw one is taken: passed on as it is.
                    (Some(_), _) if raw_here => call,
                    // A raw result where a boxed one is expected: boxed after the return. (No tail
                    // cycle passes through here: a worker returning raw tail-calls only others that
                    // do, see `raw_returns`.)
                    (Some(kind), _) => self.through_raw(call, kind),
                    // A diverging call never returns, so no unboxing continuation is needed.
                    (None, Some(_)) if diverging => call,
                    // A boxed result returned raw: unboxed after the return.
                    (None, Some(kind)) => self.through_raw(call, kind),
                    (None, None) => call,
                }
            }
            Core::Lam(..) => {
                let mark = self.pending_len();
                let c = self.closure_of(e);
                self.allocating(mark, SExpr::Closure(Box::new(c)))
            }
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
                // Temporaries allocated only to evaluate the scrutinee are dead once it returns.
                let (scrut_base, scrut_raw_base) = {
                    let b = self.bodies.last().expect("body");
                    (b.nslots, b.nraw)
                };
                let scrut = self.strict_in(scrut, if raw { Ctx::RawScrut } else { Ctx::Scrut });
                let (scrut_top, scrut_raw_top) = {
                    let b = self.bodies.last_mut().expect("body");
                    let t = (b.nslots, b.nraw);
                    b.nslots = scrut_base;
                    b.nraw = scrut_raw_base;
                    t
                };
                let scrut_raw_slot = match &scrut {
                    SExpr::Atom(Atom::Slot(r)) if r & RAW != 0 => Some(*r),
                    _ => None,
                };
                let needs_bslot = raw
                    || alts_free_mention(alts, binder)
                    || alts.iter().any(|a| {
                        matches!(a, Alt::Lit(l, _) if !matches!(l, Lit::Int(_) | Lit::Char(_) | Lit::Double(_)))
                    });
                let mut saved_binder_local: Option<Option<u32>> = None;
                let bslot = if let Some(r) = scrut_raw_slot {
                    // Codegen skips writing `bslot` when scrutinizing a raw atom; alias `binder`
                    // directly to the raw slot so any use in the alts reads (or boxes) `r`.
                    if alts_free_mention(alts, binder) {
                        let b = self.bodies.last_mut().expect("body");
                        b.joins.remove(binder);
                        saved_binder_local = Some(b.locals.insert(binder.clone(), r));
                    }
                    UNUSED_SLOT
                } else if raw {
                    self.bind_raw(binder, char)
                } else if needs_bslot {
                    self.bind_local(binder)
                } else {
                    UNUSED_SLOT
                };
                // Only one alternative runs, so their locals share the slots after the binder: the
                // frame needs the largest alternative's, not their sum.
                let (base, raw_base) = {
                    let b = self.bodies.last().expect("body");
                    (b.nslots, b.nraw)
                };
                let (mut top, mut raw_top) = (scrut_top.max(base), scrut_raw_top.max(raw_base));
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
                if let Some(prev) = saved_binder_local {
                    match prev {
                        Some(s) => {
                            b.locals.insert(binder.clone(), s);
                        }
                        None => {
                            b.locals.remove(binder);
                        }
                    }
                }
                b.nslots = top;
                b.nraw = raw_top;
                b.whnf = saved_whnf;
                SExpr::Case(Box::new(scrut), bslot, lowered)
            }
            Core::Con(name, args) => {
                let mark = self.pending_len();
                let atoms = args.iter().map(|a| self.atom(a)).collect();
                self.allocating(mark, SExpr::Con(name.clone(), atoms))
            }
            // `seq a b` evaluates a, then answers b; the identities answer their argument.
            Core::Prim(name, args) if name == "seq" && args.len() == 2 => {
                let (base, raw_base) = {
                    let b = self.bodies.last().expect("body");
                    (b.nslots, b.nraw)
                };
                let scrut = self.strict_in(&args[0], Ctx::Scrut);
                let (scrut_top, scrut_raw_top) = {
                    let b = self.bodies.last_mut().expect("body");
                    let t = (b.nslots, b.nraw);
                    b.nslots = base;
                    b.nraw = raw_base;
                    t
                };
                let saved_whnf = self.bodies.last().expect("body").whnf.clone();
                if let Core::Var(v) = &args[0] {
                    self.bodies.last_mut().expect("body").whnf.insert(v.clone());
                }
                let body = self.strict(&args[1]);
                let b = self.bodies.last_mut().expect("body");
                b.nslots = b.nslots.max(scrut_top);
                b.nraw = b.nraw.max(scrut_raw_top);
                b.whnf = saved_whnf;
                SExpr::Case(Box::new(scrut), UNUSED_SLOT, vec![SAlt::Default(body)])
            }
            Core::Prim(name, args) if IDENTITY_PRIMS.contains(&name.as_str()) && args.len() == 1 => {
                self.strict(&args[0])
            }
            Core::Prim(name, args)
                if strict_integer_arity(name).is_some_and(|n| n == args.len()) =>
            {
                let mut new_args = args.to_vec();
                let mut evals: Vec<(String, Core)> = Vec::new();
                for a in &mut new_args {
                    let ready = match a {
                        Core::Lit(Lit::Integer(s)) => s.parse::<i64>().is_ok(),
                        Core::Lit(Lit::Int(_)) if name == "intToInteger" => true,
                        Core::Var(v) => v.starts_with("ip%"),
                        _ => false,
                    };
                    if !ready {
                        let fresh = format!("ip%{}", self.next_join);
                        self.next_join += 1;
                        let orig = std::mem::replace(a, Core::Var(fresh.clone()));
                        evals.push((fresh, orig));
                    }
                }
                if !evals.is_empty() {
                    let mut wrapped = Core::Prim(name.clone(), new_args);
                    for (fresh, orig) in evals.into_iter().rev() {
                        wrapped = Core::Case(Box::new(orig), fresh, vec![Alt::Default(wrapped)]);
                    }
                    return self.strict(&wrapped);
                }
                let mark = self.pending_len();
                let atoms = args.iter().map(|a| self.atom(a)).collect();
                let prim = self.allocating(mark, SExpr::Prim(name.clone(), atoms));
                match self.raw_result() {
                    Some(kind) => self.through_raw(prim, kind),
                    None => prim,
                }
            }
            Core::Prim(name, args) => {
                let mark = self.pending_len();
                let accepts_raw = raw_operand_prim(name);
                let atoms = args
                    .iter()
                    .map(|a| if accepts_raw { self.raw_or_boxed_atom(a) } else { self.atom(a) })
                    .collect();
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
        let mark = self.pending_len();
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
        let b = self.bodies.last_mut().expect("body");
        if b.pending.len() == mark {
            return op;
        }
        let bindings: Vec<(u32, SBind)> = b.pending.drain(mark..).collect();
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
        let mark = self.pending_len();
        let mut out = Vec::with_capacity(binds.len());
        for (slot, (_, rhs)) in slots.into_iter().zip(binds) {
            if let Core::Con(con, args) = rhs {
                let atoms: Vec<Atom> = args.iter().map(|a| self.atom(a)).collect();
                out.push((slot, SBind::Con(con.clone(), atoms)));
            } else {
                out.push((slot, SBind::Closure(self.closure_of(rhs))));
            }
        }
        let mut all: Vec<(u32, SBind)> = self.bodies.last_mut().expect("body").pending.drain(mark..).collect();
        all.extend(out);
        all
    }

    fn alt(&mut self, a: &Alt) -> SAlt {
        match a {
            Alt::Con(name, vars, body) => {
                let slots = vars
                    .iter()
                    .map(|v| {
                        if free_mentions(body, v) {
                            self.bind_local(v)
                        } else {
                            UNUSED_SLOT
                        }
                    })
                    .collect();
                SAlt::Con(name.clone(), slots, self.strict(body))
            }
            Alt::Lit(l, body) => SAlt::Lit(l.clone(), self.strict(body)),
            Alt::Default(body) => SAlt::Default(self.strict(body)),
        }
    }

    /// An operand for a primitive that can accept either a raw slot or a boxed atom.
    fn raw_or_boxed_atom(&mut self, e: &Core) -> Atom {
        match e {
            Core::Var(v) => self.resolve(v),
            other => self.atom(other),
        }
    }

    /// A boxed argument: a variable or literal is passed as it is (boxing a raw slot on demand);
    /// a constructor is allocated directly in WHNF (zero heap allocation for nullary constructors,
    /// exposing nested selector thunks); anything else becomes a closure.
    fn atom(&mut self, e: &Core) -> Atom {
        match e {
            Core::Var(v) => match self.resolve(v) {
                Atom::Slot(r) if r & RAW != 0 => {
                    let depth = self.bodies.len() - 1;
                    self.box_raw_at(depth, r)
                }
                other => other,
            },
            // Numbers, small Integers and characters are static objects; the others are built when they are used.
            Core::Lit(l @ (Lit::Int(_) | Lit::Double(_) | Lit::Char(_))) => Atom::Lit(slit(l)),
            Core::Lit(Lit::Integer(s)) if s.parse::<i64>().is_ok() => Atom::Lit(SLit::Integer(s.clone())),
            Core::Con(con, args) => {
                let atoms: Vec<Atom> = args.iter().map(|a| self.atom(a)).collect();
                let b = self.bodies.last_mut().expect("body");
                let s = b.fresh_slot();
                b.pending.push((s, SBind::Con(con.clone(), atoms)));
                Atom::Slot(s)
            }
            other => {
                // A delayed expression (or a lambda): a closure, allocated before the operation.
                let c = self.closure_of(other);
                let b = self.bodies.last_mut().expect("body");
                let s = b.fresh_slot();
                b.pending.push((s, SBind::Closure(c)));
                Atom::Slot(s)
            }
        }
    }
}

/// Compacts raw slot indices in `c` via backward liveness + interference-graph greedy coloring
/// so dead sequential raw temporaries reuse slots and shrink the frame size (e.g. `fib`: 7 -> 2).
fn compact_raw_slots(c: &mut SClosure) {
    let n = c.nraw as usize;
    if n <= 1 || n > 64 {
        return;
    }
    let all_mask: u64 = if n == 64 { u64::MAX } else { (1u64 << n) - 1 };
    let mut adj = [0u64; 64];
    let mut active = 0u64;

    let raw_bit = |s: u32| -> u64 {
        if s & RAW != 0 {
            1u64 << (s & !(RAW | RAW_CHAR))
        } else {
            0
        }
    };
    let atom_bit = |a: &Atom| -> u64 {
        match a {
            Atom::Slot(s) => raw_bit(*s),
            _ => 0,
        }
    };
    let interfere = |adj: &mut [u64; 64], def: usize, live: u64| {
        let m = live & !(1u64 << def);
        adj[def] |= m;
        let mut rem = m;
        while rem != 0 {
            let j = rem.trailing_zeros() as usize;
            adj[j] |= 1u64 << def;
            rem &= rem - 1;
        }
    };

    fn liveness(
        e: &SExpr,
        live_after: u64,
        all_mask: u64,
        joins: &mut HashMap<u32, u64>,
        adj: &mut [u64; 64],
        active: &mut u64,
        atom_bit: &impl Fn(&Atom) -> u64,
        interfere: &impl Fn(&mut [u64; 64], usize, u64),
    ) -> u64 {
        match e {
            SExpr::Atom(a) => {
                let u = atom_bit(a);
                *active |= u;
                live_after | u
            }
            SExpr::App(f, args) => {
                let u = args.iter().fold(atom_bit(f), |m, a| m | atom_bit(a));
                *active |= u;
                live_after | u
            }
            SExpr::CallWorker(_, args) | SExpr::Con(_, args) | SExpr::Prim(_, args) => {
                let u = args.iter().fold(0u64, |m, a| m | atom_bit(a));
                *active |= u;
                live_after | u
            }
            SExpr::Let(binds, body) => {
                let body_in = liveness(body, live_after, all_mask, joins, adj, active, atom_bit, interfere);
                let mut u = 0u64;
                for (_, b) in binds {
                    match b {
                        SBind::BoxRaw(r) => u |= 1u64 << (*r & !(RAW | RAW_CHAR)),
                        _ => {
                            for a in b.atoms() {
                                u |= atom_bit(a);
                            }
                        }
                    }
                }
                *active |= u;
                body_in | u
            }
            SExpr::Case(scrut, bslot, alts) => {
                let mut alts_in = 0u64;
                for a in alts {
                    let b = match a {
                        SAlt::Con(_, _, b) | SAlt::Lit(_, b) | SAlt::Default(b) => b,
                    };
                    alts_in |= liveness(b, live_after, all_mask, joins, adj, active, atom_bit, interfere);
                }
                if *bslot & RAW != 0 {
                    let d = (*bslot & !(RAW | RAW_CHAR)) as usize;
                    *active |= 1u64 << d;
                    interfere(adj, d, alts_in);
                    alts_in &= !(1u64 << d);
                }
                liveness(scrut, alts_in, all_mask, joins, adj, active, atom_bit, interfere)
            }
            SExpr::Closure(cl) => {
                let u = cl.captures.iter().fold(0u64, |m, a| m | atom_bit(a));
                *active |= u;
                live_after | u
            }
            SExpr::Fail(_) => live_after,
            SExpr::Join(id, _, code, body) => {
                let code_in = liveness(code, live_after, all_mask, joins, adj, active, atom_bit, interfere);
                joins.insert(*id, code_in);
                liveness(body, live_after, all_mask, joins, adj, active, atom_bit, interfere)
            }
            SExpr::Jump(id, args) => {
                let u = args.iter().fold(0u64, |m, a| m | atom_bit(a));
                *active |= u;
                let j = joins.get(id).copied().unwrap_or(all_mask);
                j | u
            }
        }
    }

    let mut joins = HashMap::new();
    let body_in = liveness(&c.body, 0, all_mask, &mut joins, &mut adj, &mut active, &atom_bit, &interfere);
    let mut raw_args_mask = 0u64;
    for &(_, r) in &c.raw_args {
        let d = (r & !(RAW | RAW_CHAR)) as usize;
        raw_args_mask |= 1u64 << d;
        active |= 1u64 << d;
    }
    for &(_, r) in &c.raw_args {
        let d = (r & !(RAW | RAW_CHAR)) as usize;
        interfere(&mut adj, d, body_in | raw_args_mask);
    }

    let mut color = [u32::MAX; 64];
    let mut max_color = 0u32;
    let mut colored_any = false;
    // Keep raw_args in their original 0.. order when possible.
    for &(_, r) in &c.raw_args {
        let i = (r & !(RAW | RAW_CHAR)) as usize;
        if color[i] == u32::MAX {
            let mut used = 0u64;
            let mut nb = adj[i];
            while nb != 0 {
                let j = nb.trailing_zeros() as usize;
                if color[j] != u32::MAX {
                    used |= 1u64 << color[j];
                }
                nb &= nb - 1;
            }
            let col = (!used).trailing_zeros();
            color[i] = col;
            max_color = max_color.max(col);
            colored_any = true;
        }
    }
    for i in 0..n {
        if (active & (1u64 << i)) == 0 || color[i] != u32::MAX {
            continue;
        }
        let mut used = 0u64;
        let mut nb = adj[i];
        while nb != 0 {
            let j = nb.trailing_zeros() as usize;
            if color[j] != u32::MAX {
                used |= 1u64 << color[j];
            }
            nb &= nb - 1;
        }
        let col = (!used).trailing_zeros();
        color[i] = col;
        max_color = max_color.max(col);
        colored_any = true;
    }

    let new_nraw = if colored_any { max_color + 1 } else { 0 };
    if new_nraw >= c.nraw {
        return;
    }

    let remap = |s: &mut u32| {
        if *s & RAW != 0 {
            let idx = (*s & !(RAW | RAW_CHAR)) as usize;
            let col = color[idx];
            debug_assert!(col != u32::MAX, "internal: uncolored raw slot {idx}");
            *s = RAW | (*s & RAW_CHAR) | col;
        }
    };
    let remap_atom = |a: &mut Atom| {
        if let Atom::Slot(s) = a {
            remap(s);
        }
    };
    fn rewrite_expr(e: &mut SExpr, remap: &impl Fn(&mut u32), remap_atom: &impl Fn(&mut Atom)) {
        match e {
            SExpr::Atom(a) => remap_atom(a),
            SExpr::App(f, args) => {
                remap_atom(f);
                args.iter_mut().for_each(remap_atom);
            }
            SExpr::CallWorker(_, args) | SExpr::Con(_, args) | SExpr::Prim(_, args) | SExpr::Jump(_, args) => {
                args.iter_mut().for_each(remap_atom);
            }
            SExpr::Let(binds, body) => {
                for (_, b) in binds {
                    match b {
                        SBind::Closure(cl) => cl.captures.iter_mut().for_each(remap_atom),
                        SBind::Con(_, args) => args.iter_mut().for_each(remap_atom),
                        SBind::BoxRaw(r) => remap(r),
                    }
                }
                rewrite_expr(body, remap, remap_atom);
            }
            SExpr::Case(scrut, bslot, alts) => {
                rewrite_expr(scrut, remap, remap_atom);
                remap(bslot);
                for a in alts {
                    match a {
                        SAlt::Con(_, _, b) | SAlt::Lit(_, b) | SAlt::Default(b) => rewrite_expr(b, remap, remap_atom),
                    }
                }
            }
            SExpr::Closure(cl) => cl.captures.iter_mut().for_each(remap_atom),
            SExpr::Fail(_) => {}
            SExpr::Join(_, _, code, body) => {
                rewrite_expr(code, remap, remap_atom);
                rewrite_expr(body, remap, remap_atom);
            }
        }
    }

    for (_, r) in &mut c.raw_args {
        remap(r);
    }
    rewrite_expr(&mut c.body, &remap, &remap_atom);
    c.nraw = new_nraw;
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
    let mut memo = HashMap::new();
    is_join_memo(binds, body, &mut memo)
}

fn is_join_memo(binds: &[(String, Core)], body: &Core, memo: &mut HashMap<usize, bool>) -> bool {
    let [(name, rhs)] = binds else { return false };
    let key = binds.as_ptr() as usize;
    if let Some(&res) = memo.get(&key) {
        return res;
    }
    let res = !mentions(rhs, name) && tail_calls_memo(body, name, lambdas(rhs).0, memo);
    memo.insert(key, res);
    res
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

/// Expected arity of `Integer` primitives whose arguments are strictly evaluated left-to-right.
fn strict_integer_arity(name: &str) -> Option<usize> {
    match name {
        "integerAdd" | "integerSub" | "integerMul" | "integerQuot" | "integerRem" | "integerDiv"
        | "integerMod" | "integerQuotRem" | "integerDivMod" | "integerEq" | "integerLt"
        | "integerLe" => Some(2),
        "integerNegate" | "integerAbs" | "integerSignum" | "integerToInt" | "intToInteger" => Some(1),
        _ => None,
    }
}

/// Whether every use of `j` in `e` is a call `j a1 .. an` in tail position (its arguments not
/// mentioning `j`). Tail positions are those `strict` lowers in the frame and mode of `e`: case
/// alternatives, let bodies, the code of join points, and the result of `seq` and identities.
fn tail_calls_memo(e: &Core, j: &str, n: usize, memo: &mut HashMap<usize, bool>) -> bool {
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
            if binds.iter().all(|(_, rhs)| !mentions(rhs, j)) {
                return tail_calls_memo(body, j, n, memo);
            }
            if !tail_calls_memo(body, j, n, memo) {
                return false;
            }
            if is_join_memo(binds, body, memo) {
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
                shadowed || tail_calls_memo(code, j, n, memo)
            } else {
                false
            }
        }
        Core::Case(scrut, binder, alts) => {
            !mentions(scrut, j)
                && (binder == j
                    || alts.iter().all(|a| match a {
                        Alt::Con(_, vars, b) => vars.iter().any(|v| v == j) || tail_calls_memo(b, j, n, memo),
                        Alt::Lit(_, b) | Alt::Default(b) => tail_calls_memo(b, j, n, memo),
                    }))
        }
        Core::Prim(p, args) if p == "seq" && args.len() == 2 => !mentions(&args[0], j) && tail_calls_memo(&args[1], j, n, memo),
        Core::Prim(p, args) if IDENTITY_PRIMS.contains(&p.as_str()) && args.len() == 1 => tail_calls_memo(&args[0], j, n, memo),
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

/// Whether `v` occurs free in `e` (respecting shadowing by `Lam`, `Let`, `Case`, and `Alt::Con`).
fn free_mentions(e: &Core, v: &str) -> bool {
    match e {
        Core::Var(x) => x == v,
        Core::Lit(_) | Core::Fail(_) => false,
        Core::App(f, a) => free_mentions(f, v) || free_mentions(a, v),
        Core::Lam(x, b) => x != v && free_mentions(b, v),
        Core::Let(binds, body) => {
            if binds.iter().any(|(n, _)| n == v) {
                false
            } else {
                binds.iter().any(|(_, r)| free_mentions(r, v)) || free_mentions(body, v)
            }
        }
        Core::Case(s, b, alts) => {
            free_mentions(s, v) || (b != v && alts_free_mention(alts, v))
        }
        Core::Con(_, args) | Core::Prim(_, args) => args.iter().any(|a| free_mentions(a, v)),
    }
}

fn alts_free_mention(alts: &[Alt], v: &str) -> bool {
    alts.iter().any(|a| match a {
        Alt::Con(_, vars, b) => !vars.iter().any(|x| x == v) && free_mentions(b, v),
        Alt::Lit(_, b) | Alt::Default(b) => free_mentions(b, v),
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

/// Extracts `(false_rhs, true_rhs)` from alternatives covering `GHC.Types.False` and `GHC.Types.True`.
fn extract_bool_con_alts(alts: &[Alt]) -> Option<(&Core, &Core)> {
    let mut false_rhs: Option<&Core> = None;
    let mut true_rhs: Option<&Core> = None;
    let mut saw_bool_con = false;
    for a in alts {
        match a {
            Alt::Con(c, vars, rhs) if c == "GHC.Types.False" && vars.is_empty() => {
                saw_bool_con = true;
                false_rhs.get_or_insert(rhs);
            }
            Alt::Con(c, vars, rhs) if c == "GHC.Types.True" && vars.is_empty() => {
                saw_bool_con = true;
                true_rhs.get_or_insert(rhs);
            }
            Alt::Default(rhs) => {
                false_rhs.get_or_insert(rhs);
                true_rhs.get_or_insert(rhs);
                break;
            }
            _ => return None,
        }
    }
    if saw_bool_con {
        Some((false_rhs?, true_rhs?))
    } else {
        None
    }
}

/// Builds `case arg of { 0# -> false_rhs; _ -> true_rhs }`, wrapping primitive/call scrutinees
/// in a raw-eligible `[Default(Case(Var(b_raw), ...))]` shell.
fn mk_tag_to_enum_case(arg: Core, b_prefix: &str, false_rhs: Core, true_rhs: Core) -> Core {
    let b_inner = format!("{b_prefix}%ti");
    let bool_alts = vec![Alt::Lit(Lit::Int(0), false_rhs), Alt::Default(true_rhs)];
    let needs_raw_wrapper =
        matches!(&arg, Core::Prim(p, _) if RAW_RESULT_PRIMS.contains(&p.as_str())) || matches!(&arg, Core::App(..));
    if needs_raw_wrapper {
        let b_raw = format!("{b_prefix}%te");
        Core::Case(
            Box::new(arg),
            b_raw.clone(),
            vec![Alt::Default(Core::Case(Box::new(Core::Var(b_raw)), b_inner, bool_alts))],
        )
    } else {
        Core::Case(Box::new(arg), b_inner, bool_alts)
    }
}

/// Recognizes `GHC.Types.True` and `GHC.Types.False` represented either as nullary `Core::Con`
/// or as top-level `Core::Var`.
fn bool_const(e: &Core) -> Option<bool> {
    match e {
        Core::Con(c, args) if args.is_empty() && c == "GHC.Types.True" => Some(true),
        Core::Con(c, args) if args.is_empty() && c == "GHC.Types.False" => Some(false),
        Core::Var(v) if v == "GHC.Types.True" => Some(true),
        Core::Var(v) if v == "GHC.Types.False" => Some(false),
        _ => None,
    }
}

/// Rewrites `case tagToEnum# x of { False -> f; True -> t }` (and the two-step variant through a
/// case binder, optionally wrapped in a non-capturing join-point `Let`) into an unboxed `Int#`
/// case on `x` (`0# -> f; _ -> t`), eliminating `Bool` materialization and exposing comparisons
/// to `match_eq_step` and `compare_and_branch`.
fn rewrite_tag_to_enum_bool(e: &mut Core) {
    let Core::Case(scrut, b1, alts) = e else { return };
    let Core::Prim(p, args) = scrut.as_ref() else { return };
    if p != "tagToEnum#" || args.len() != 1 {
        return;
    }
    let matched = if let Some((f, t)) = extract_bool_con_alts(alts) {
        if !mentions(f, b1) && !mentions(t, b1) {
            Some((Vec::new(), f.clone(), t.clone()))
        } else {
            None
        }
    } else if let [Alt::Default(def_body)] = &alts[..] {
        // Peel any leading non-capturing `Let` bindings (e.g. join points created by Bool
        // Case-of-Case fusion) to reach `case b1 as b2 of { True -> t; False -> f }`.
        let mut let_layers: Vec<Vec<(String, Core)>> = Vec::new();
        let mut cur = def_body;
        let mut ok = true;
        while let Core::Let(binds, inner) = cur {
            if binds.iter().any(|(n, rhs)| n == b1 || mentions(rhs, b1)) {
                ok = false;
                break;
            }
            let_layers.push(binds.clone());
            cur = inner;
        }
        if ok {
            if let Core::Case(s2, b2, alts2) = cur {
                if matches!(s2.as_ref(), Core::Var(v) if v == b1) {
                    if let Some((f, t)) = extract_bool_con_alts(alts2) {
                        if !mentions(f, b1) && !mentions(t, b1) && !mentions(f, b2) && !mentions(t, b2) {
                            Some((let_layers, f.clone(), t.clone()))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };
    let Some((let_layers, mut false_rhs, mut true_rhs)) = matched else { return };
    let arg = args[0].clone();
    let b_prefix = b1.clone();
    let mut outer_layers: Vec<Vec<(String, Core)>> = Vec::new();
    for binds in let_layers.into_iter().rev() {
        if !outer_layers.is_empty() {
            outer_layers.push(binds);
            continue;
        }
        let in_f = binds.iter().any(|(n, _)| mentions(&false_rhs, n));
        let in_t = binds.iter().any(|(n, _)| mentions(&true_rhs, n));
        match (in_f, in_t) {
            (false, true) => true_rhs = Core::Let(binds, Box::new(true_rhs)),
            (true, false) => false_rhs = Core::Let(binds, Box::new(false_rhs)),
            (false, false) => {}
            (true, true) => outer_layers.push(binds),
        }
    }
    let mut out = mk_tag_to_enum_case(arg, &b_prefix, false_rhs, true_rhs);
    for binds in outer_layers {
        out = Core::Let(binds, Box::new(out));
    }
    *e = out;
}

/// Checks whether `e` is a tree of `Case` expressions whose leaves are all `True`, `False`,
/// `tagToEnum#`, or `Fail`, with no inner binder capturing variables free in `f` or `t`.
fn check_bool_case_tree(e: &Core, f: &Core, t: &Core, true_uses: &mut usize, false_uses: &mut usize) -> bool {
    if let Some(val) = bool_const(e) {
        if val {
            *true_uses += 1;
        } else {
            *false_uses += 1;
        }
        return true;
    }
    match e {
        Core::Prim(p, args) if p == "tagToEnum#" && args.len() == 1 => {
            *true_uses += 1;
            *false_uses += 1;
            true
        }
        Core::Fail(_) => true,
        Core::Case(_, ib, alts) => {
            if mentions(f, ib) || mentions(t, ib) {
                return false;
            }
            alts.iter().all(|a| match a {
                Alt::Con(_, vars, body) => {
                    vars.iter().all(|v| !mentions(f, v) && !mentions(t, v))
                        && check_bool_case_tree(body, f, t, true_uses, false_uses)
                }
                Alt::Lit(_, body) | Alt::Default(body) => check_bool_case_tree(body, f, t, true_uses, false_uses),
            })
        }
        _ => false,
    }
}

/// Replaces leaves of a checked Bool case tree with `f_ref` / `t_ref`.
fn rewrite_bool_case_tree(e: Core, f_ref: &Core, t_ref: &Core, b_prefix: &str, counter: &mut usize) -> Core {
    if let Some(val) = bool_const(&e) {
        return if val { t_ref.clone() } else { f_ref.clone() };
    }
    match e {
        Core::Prim(p, mut args) if p == "tagToEnum#" && args.len() == 1 => {
            let n = *counter;
            *counter += 1;
            mk_tag_to_enum_case(args.remove(0), &format!("{b_prefix}%{n}"), f_ref.clone(), t_ref.clone())
        }
        Core::Fail(msg) => Core::Fail(msg),
        Core::Case(s, ib, alts) => Core::Case(
            s,
            ib,
            alts.into_iter()
                .map(|a| match a {
                    Alt::Con(c, vs, body) => {
                        Alt::Con(c, vs, rewrite_bool_case_tree(body, f_ref, t_ref, b_prefix, counter))
                    }
                    Alt::Lit(l, body) => Alt::Lit(l, rewrite_bool_case_tree(body, f_ref, t_ref, b_prefix, counter)),
                    Alt::Default(body) => Alt::Default(rewrite_bool_case_tree(body, f_ref, t_ref, b_prefix, counter)),
                })
                .collect(),
        ),
        other => other,
    }
}

/// Floats 1-alt `Case`-of-`Case` (`case (case s1 as r of _ -> e1) as b of alts` ->
/// `case s1 as r of _ -> case e1 as b of alts`) and fuses Bool `Case`-of-`Case` trees (e.g.
/// `&&` / `||` in guards) directly into branches with zero-allocation join points.
fn simplify_case_of_case(e: &mut Core) {
    // 1. Float 1-alt Default Case out of a Case scrutinee.
    let float_one_alt = match e {
        Core::Case(scrut, b, alts) => match scrut.as_ref() {
            Core::Case(s1, r, inner_alts)
                if matches!(&inner_alts[..], [Alt::Default(_)])
                    && r != b
                    && !alts_mention(alts, r)
                    && !matches!(s1.as_ref(), Core::Prim(p, _) if p == "writeMutVar#") =>
            {
                true
            }
            _ => false,
        },
        _ => false,
    };
    if float_one_alt {
        let Core::Case(scrut, b, alts) = std::mem::replace(e, Core::Fail(String::new())) else { unreachable!() };
        let Core::Case(s1, r, mut inner_alts) = *scrut else { unreachable!() };
        let Alt::Default(e1) = inner_alts.remove(0) else { unreachable!() };
        let mut inner = Core::Case(Box::new(e1), b, alts);
        if let Core::Case(is, ib, ialts) = &mut inner {
            if may_flatten_case(is, ialts) {
                if let Some(flat) = flatten_case_alts(is, ib, ialts) {
                    *ialts = flat;
                }
            }
        }
        simplify_case_of_case(&mut inner);
        rewrite_tag_to_enum_bool(&mut inner);
        *e = Core::Case(s1, r, vec![Alt::Default(inner)]);
        return;
    }

    // 2. Fuse Bool Case-of-Case when the inner Case tree only returns True/False/tagToEnum#.
    let fuse_bool = match e {
        Core::Case(scrut, b, alts) if matches!(scrut.as_ref(), Core::Case(..)) => {
            extract_bool_con_alts(alts).and_then(|(f, t)| {
                if mentions(f, b)
                    || mentions(t, b)
                    || matches!(f, Core::Lam(..))
                    || matches!(t, Core::Lam(..))
                {
                    return None;
                }
                let (mut t_uses, mut f_uses) = (0usize, 0usize);
                check_bool_case_tree(scrut, f, t, &mut t_uses, &mut f_uses)
                    .then(|| (f.clone(), t.clone(), f_uses, t_uses))
            })
        }
        _ => None,
    };
    if let Some((f, t, f_uses, t_uses)) = fuse_bool {
        let Core::Case(scrut, b, _) = std::mem::replace(e, Core::Fail(String::new())) else { unreachable!() };
        let is_tiny = |x: &Core| match x {
            Core::Var(_) | Core::Lit(_) | Core::Fail(_) => true,
            Core::Con(_, args) => args.is_empty(),
            _ => false,
        };
        let jt_name = format!("{b}%jt");
        let jf_name = format!("{b}%jf");
        let bind_t = t_uses >= 2
            && !is_tiny(&t)
            && !mentions(&t, &jt_name)
            && !mentions(&f, &jt_name)
            && !mentions(&scrut, &jt_name);
        let bind_f = f_uses >= 2
            && !is_tiny(&f)
            && !mentions(&f, &jf_name)
            && !mentions(&t, &jf_name)
            && !mentions(&scrut, &jf_name);
        let t_ref = if bind_t { Core::Var(jt_name.clone()) } else { t.clone() };
        let f_ref = if bind_f { Core::Var(jf_name.clone()) } else { f.clone() };
        let mut counter = 0usize;
        let mut out = rewrite_bool_case_tree(*scrut, &f_ref, &t_ref, &b, &mut counter);
        if bind_f {
            out = Core::Let(vec![(jf_name, f)], Box::new(out));
        }
        if bind_t {
            out = Core::Let(vec![(jt_name, t)], Box::new(out));
        }
        *e = out;
    }
}

/// Whether `e` is a small pure expression safe to float into case alternatives without code bloat.
fn small_pure_rhs(e: &Core, budget: &mut usize) -> bool {
    if *budget == 0 {
        return false;
    }
    *budget -= 1;
    match e {
        Core::Var(_) | Core::Lit(_) => true,
        Core::App(f, a) => small_pure_rhs(f, budget) && small_pure_rhs(a, budget),
        Core::Con(_, args) => args.iter().all(|a| small_pure_rhs(a, budget)),
        Core::Prim(p, args) if RAW_RESULT_PRIMS.contains(&p.as_str()) => {
            args.iter().all(|a| small_pure_rhs(a, budget))
        }
        Core::Case(s, _, alts) if alts.len() <= 3 => {
            small_pure_rhs(s, budget)
                && alts.iter().all(|a| match a {
                    Alt::Con(_, _, b) | Alt::Lit(_, b) | Alt::Default(b) => small_pure_rhs(b, budget),
                })
        }
        _ => false,
    }
}

/// Whether floating `r = rhs` into `e` eliminates a thunk allocation on at least one branch
/// (either `e` does not use `r`, returns `r` directly in tail position, or is a `Case` whose
/// alternatives do).
fn alt_benefits_from_float(e: &Core, r: &str, rhs: &Core) -> bool {
    if !mentions(e, r) || matches!(e, Core::Var(v) if v == r) {
        return true;
    }
    if let Core::Case(s, b, alts) = e {
        if !mentions(s, r) && b != r && !mentions(rhs, b) {
            return alts.iter().any(|a| match a {
                Alt::Con(_, vars, body) => {
                    let sh = vars.iter().any(|v| v == r);
                    sh || (!vars.iter().any(|v| mentions(rhs, v)) && alt_benefits_from_float(body, r, rhs))
                }
                Alt::Lit(_, body) | Alt::Default(body) => alt_benefits_from_float(body, r, rhs),
            });
        }
    }
    false
}

/// Floats a small non-recursive `let r = rhs in case scrut of alts` into `alts` when at least one
/// alternative returns `r` directly (turning it into a tail call with zero thunk allocation) or
/// does not use `r`, preserving laziness and sharing in all other alternatives.
fn float_let_into_case(e: &mut Core) {
    let Core::Let(binds, body) = e else { return };
    let [(r, rhs)] = &binds[..] else { return };
    let Core::Case(scrut, binder, alts) = body.as_ref() else { return };
    if alts.is_empty()
        || mentions(rhs, r)
        || mentions(scrut, r)
        || binder == r
        || mentions(rhs, binder)
        || is_join(binds, body)
    {
        return;
    }
    let mut budget = 20usize;
    if !small_pure_rhs(rhs, &mut budget) {
        return;
    }
    let mut used_alts = 0usize;
    let mut has_benefit = false;
    for a in alts {
        let (shadowed, alt_body, capture) = match a {
            Alt::Con(_, vars, b) => {
                let sh = vars.iter().any(|v| v == r);
                let cap = !sh && mentions(b, r) && vars.iter().any(|v| mentions(rhs, v));
                (sh, b, cap)
            }
            Alt::Lit(_, b) | Alt::Default(b) => (false, b, false),
        };
        if capture {
            return;
        }
        if shadowed || !mentions(alt_body, r) {
            has_benefit = true;
        } else {
            used_alts += 1;
            if alt_benefits_from_float(alt_body, r, rhs) {
                has_benefit = true;
            }
        }
    }
    if !has_benefit || used_alts > 3 {
        return;
    }
    let r_owned = r.clone();
    let rhs_owned = rhs.clone();
    let scrut_owned = scrut.clone();
    let binder_owned = binder.clone();
    let new_alts: Vec<Alt> = alts
        .iter()
        .map(|a| {
            let transform = |b: &Core, shadowed: bool| -> Core {
                if shadowed || !mentions(b, &r_owned) {
                    b.clone()
                } else if matches!(b, Core::Var(v) if v == &r_owned) {
                    rhs_owned.clone()
                } else {
                    let mut wrapped = Core::Let(vec![(r_owned.clone(), rhs_owned.clone())], Box::new(b.clone()));
                    float_let_into_case(&mut wrapped);
                    wrapped
                }
            };
            match a {
                Alt::Con(c, vars, b) => {
                    let sh = vars.iter().any(|v| v == &r_owned);
                    Alt::Con(c.clone(), vars.clone(), transform(b, sh))
                }
                Alt::Lit(l, b) => Alt::Lit(l.clone(), transform(b, false)),
                Alt::Default(b) => Alt::Default(transform(b, false)),
            }
        })
        .collect();
    *e = Core::Case(scrut_owned, binder_owned, new_alts);
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

/// Whether flattening nested cases on `binder` is safe without destroying a 1-alt raw-eligible
/// case shell (`case rawPrim ... as r of _ -> case r of ...`).
fn may_flatten_case(scrut: &Core, alts: &[Alt]) -> bool {
    let raw_shell = matches!(alts, [Alt::Default(_)])
        && (matches!(scrut, Core::Prim(p, _) if RAW_RESULT_PRIMS.contains(&p.as_str()))
            || matches!(scrut, Core::App(..)));
    !raw_shell
}

/// Simplifies `tagToEnum#` Bool cases, collapses chained equality-comparison cases (`eqChar#`,
/// `==#`, `eqWord#`) and nested same-scrutinee `Case` chains into flat multi-way `Case`
/// expressions, and floats single-use tail `Let` bindings into `Case` alternatives before worker
/// analysis.
pub fn collapse_case_chains(binds: &mut [TopBind]) {
    fn rewrite_bools(e: &mut Core) {
        match e {
            Core::Var(_) | Core::Lit(_) | Core::Fail(_) => {}
            Core::App(f, a) => {
                rewrite_bools(f);
                rewrite_bools(a);
            }
            Core::Lam(_, b) => rewrite_bools(b),
            Core::Let(binds, body) => {
                for (_, r) in binds {
                    rewrite_bools(r);
                }
                rewrite_bools(body);
            }
            Core::Case(scrut, binder, alts) => {
                if may_flatten_case(scrut, alts) {
                    if let Some(flat) = flatten_case_alts(scrut, binder, alts) {
                        *alts = flat;
                    }
                }
                rewrite_bools(scrut);
                for a in alts {
                    match a {
                        Alt::Con(_, _, b) | Alt::Lit(_, b) | Alt::Default(b) => rewrite_bools(b),
                    }
                }
            }
            Core::Con(_, args) | Core::Prim(_, args) => {
                for a in args {
                    rewrite_bools(a);
                }
            }
        }
        if let Core::Case(scrut, binder, alts) = e {
            if may_flatten_case(scrut, alts) {
                if let Some(flat) = flatten_case_alts(scrut, binder, alts) {
                    *alts = flat;
                }
            }
            let already_whnf = matches!(scrut.as_ref(), Core::Con(..))
                || matches!(scrut.as_ref(), Core::Prim(p, args) if p == "tagToEnum#" && args.len() == 1);
            if already_whnf && matches!(&alts[..], [Alt::Default(Core::Var(v))] if v == binder) {
                let s = std::mem::replace(scrut.as_mut(), Core::Fail(String::new()));
                *e = s;
            }
        }
        simplify_case_of_case(e);
        rewrite_tag_to_enum_bool(e);
    }

    fn walk(e: &mut Core) {
        // Iteratively peel chains of >= 2 equality comparisons on the same variable (top-down so
        // earlier branches take precedence over shadowed duplicates).
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
        float_let_into_case(e);
    }
    for b in binds {
        rewrite_bools(&mut b.body);
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
            Core::Let(vec![("t".into(), prim("+#", vec![var("y"), var("y")]))], Box::new(Core::Con("Test.K".into(), vec![var("t")]))),
            Core::Case(Box::new(var("y")), "z".into(), vec![Alt::Default(var("z"))]),
            Core::App(Box::new(var("Test.k")), Box::new(prim("+#", vec![var("y"), Core::Lit(Lit::Int(1))]))),
            Core::Case(
                Box::new(Core::Case(Box::new(var("x")), "u".into(), vec![Alt::Default(var("y"))])),
                "s".into(),
                vec![Alt::Default(var("s"))],
            ),
            Core::Case(
                Box::new(Core::Let(vec![("t".into(), prim("+#", vec![var("y"), var("y")]))], Box::new(var("t")))),
                "s".into(),
                vec![Alt::Default(var("s"))],
            ),
        ];
        for body in escapes {
            let c = lower(case(prim("+#", vec![var("x"), var("x")]), "y", body.clone()));
            assert_eq!(c.nraw, 0, "{body:?}");
        }
        // A join point runs in the same stack frame and does not capture `y`, so `y` stays raw.
        let join_body = Core::Let(vec![("t".into(), prim("+#", vec![var("y"), var("y")]))], Box::new(var("t")));
        let c = lower(case(prim("+#", vec![var("x"), var("x")]), "y", join_body));
        assert_eq!(c.nraw, 1);
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

    #[test]
    fn small_integer_literals_are_static_atoms_and_strict_integer_prims_force_args() {
        // f x = integerAdd (integerAdd x 1) 2
        // Small i64 Integer literals should stay Atom::Lit(SLit::Integer) without wrapper thunks,
        // and the nested `integerAdd x 1` should be forced strictly via SExpr::Case (not thunked).
        let inner = prim("integerAdd", vec![var("x"), Core::Lit(Lit::Integer("1".into()))]);
        let outer = prim("integerAdd", vec![inner, Core::Lit(Lit::Integer("2".into()))]);
        let binds = vec![TopBind { name: "Test.f".into(), body: Core::Lam("x".into(), Box::new(outer)) }];
        let prog = Lowering::program(&binds, &NumericParams::default());
        let SExpr::Case(scrut, _, alts) = &prog.globals[0].closure.body else {
            panic!("expected strict Case forcing nested integerAdd, got {:?}", prog.globals[0].closure.body);
        };
        let SExpr::Case(_, _, inner_alts) = scrut.as_ref() else {
            panic!("expected inner strict Case forcing x, got {scrut:?}");
        };
        assert!(
            matches!(&inner_alts[0], SAlt::Default(SExpr::Prim(p, args)) if p == "integerAdd" && matches!(&args[1], Atom::Lit(SLit::Integer(s)) if s == "1")),
            "expected inner integerAdd with static Integer literal, got {inner_alts:?}"
        );
        let SAlt::Default(SExpr::Prim(p, args)) = &alts[0] else {
            panic!("expected outer integerAdd prim, got {:?}", alts[0]);
        };
        assert_eq!(p, "integerAdd");
        assert!(matches!(&args[1], Atom::Lit(SLit::Integer(s)) if s == "2"), "{args:?}");
    }

    #[test]
    fn collapse_case_chains_fuses_bool_trees_and_preserves_raw_comparison_shells() {
        // 1. Matcher-style nested Bool Case-of-Case (`c /= q && d /= 0`) with Core::Var True/False:
        //    should fuse into raw comparison shells with zero `tagToEnum#` remaining.
        let inner_bool = Core::Case(
            Box::new(prim("/=#", vec![var("c"), var("q")])),
            "u1".into(),
            vec![Alt::Default(Core::Case(
                Box::new(prim("tagToEnum#", vec![var("u1")])),
                "b1".into(),
                vec![Alt::Default(Core::Case(
                    Box::new(Core::Case(
                        Box::new(var("b1")),
                        "cb1".into(),
                        vec![
                            Alt::Con(
                                "GHC.Types.True".into(),
                                vec![],
                                Core::Case(
                                    Box::new(prim("/=#", vec![var("d"), Core::Lit(Lit::Int(0))])),
                                    "u2".into(),
                                    vec![Alt::Default(prim("tagToEnum#", vec![var("u2")]))],
                                ),
                            ),
                            Alt::Default(Core::Case(
                                Box::new(var("b1")),
                                "cb2".into(),
                                vec![
                                    Alt::Con("GHC.Types.False".into(), vec![], var("GHC.Types.False")),
                                    Alt::Default(Core::Fail("&&".into())),
                                ],
                            )),
                        ],
                    )),
                    "bout".into(),
                    vec![
                        Alt::Con("GHC.Types.True".into(), vec![], Core::Lit(Lit::Int(10))),
                        Alt::Default(Core::Case(
                            Box::new(var("bout")),
                            "cb3".into(),
                            vec![
                                Alt::Con("GHC.Types.False".into(), vec![], Core::Lit(Lit::Int(20))),
                                Alt::Default(Core::Fail("&&".into())),
                            ],
                        )),
                    ],
                ))],
            ))],
        );
        // 2. Raw comparison shell (`case (n <# 2#) as r of _ -> case r of { 1# -> n; _ -> 0# }`):
        //    must NOT be flattened into a multi-alt Case on `<#` (which would lose raw binder).
        let raw_shell = case(
            prim("<#", vec![var("n"), Core::Lit(Lit::Int(2))]),
            "r",
            Core::Case(
                Box::new(var("r")),
                "rb".into(),
                vec![Alt::Lit(Lit::Int(1), var("n")), Alt::Default(Core::Lit(Lit::Int(0)))],
            ),
        );
        // 3. `let r = (case x ==# 0# of 1# -> 1; _ -> 2) in case flag of True -> Con r; _ -> r`:
        //    should float small Case RHS into alternatives, turning the Default branch into a tail Case.
        let small_case_rhs = Core::Case(
            Box::new(prim("==#", vec![var("x"), Core::Lit(Lit::Int(0))])),
            "eqb".into(),
            vec![Alt::Lit(Lit::Int(1), Core::Lit(Lit::Int(1))), Alt::Default(Core::Lit(Lit::Int(2)))],
        );
        let float_body = Core::Let(
            vec![("r".into(), small_case_rhs)],
            Box::new(Core::Case(
                Box::new(var("flag")),
                "fb".into(),
                vec![
                    Alt::Con("GHC.Types.True".into(), vec![], Core::Con("Test.K".into(), vec![var("r")])),
                    Alt::Default(var("r")),
                ],
            )),
        );
        let mut binds = vec![
            TopBind { name: "Test.f_bool".into(), body: inner_bool },
            TopBind { name: "Test.f_raw".into(), body: raw_shell },
            TopBind { name: "Test.f_float".into(), body: float_body },
        ];
        collapse_case_chains(&mut binds);
        fn has_tag_to_enum(e: &Core) -> bool {
            match e {
                Core::Prim(p, args) => p == "tagToEnum#" || args.iter().any(has_tag_to_enum),
                Core::App(f, a) => has_tag_to_enum(f) || has_tag_to_enum(a),
                Core::Lam(_, b) => has_tag_to_enum(b),
                Core::Let(bs, b) => bs.iter().any(|(_, r)| has_tag_to_enum(r)) || has_tag_to_enum(b),
                Core::Case(s, _, alts) => {
                    has_tag_to_enum(s)
                        || alts.iter().any(|a| match a {
                            Alt::Con(_, _, b) | Alt::Lit(_, b) | Alt::Default(b) => has_tag_to_enum(b),
                        })
                }
                Core::Con(_, args) => args.iter().any(has_tag_to_enum),
                Core::Var(_) | Core::Lit(_) | Core::Fail(_) => false,
            }
        }
        assert!(!has_tag_to_enum(&binds[0].body), "tagToEnum# should be eliminated: {:?}", binds[0].body);
        // Raw shell must remain `[Alt::Default(Case(Var(r), ...))]`.
        let Core::Case(_, _, ref raw_alts) = binds[1].body else { panic!("{:?}", binds[1].body) };
        assert!(matches!(&raw_alts[..], [Alt::Default(Core::Case(..))]), "raw shell was flattened: {raw_alts:?}");
        // Let with small Case RHS should have floated into the Case alternatives.
        let Core::Case(_, _, ref float_alts) = binds[2].body else {
            panic!("expected outer Case after float_let_into_case, got {:?}", binds[2].body);
        };
        assert!(matches!(&float_alts[1], Alt::Default(Core::Case(..))), "Default branch should inline small Case RHS: {:?}", float_alts[1]);
    }

    fn app(f: Core, a: Core) -> Core {
        Core::App(Box::new(f), Box::new(a))
    }

    #[test]
    fn cold_boxing_promotes_backtracking_loop_but_not_all_branch_boxing() {
        // 1. Backtracking loop (`go n q = case q ># n of 1# -> Nil; _ -> case flag of 0# -> go n (q +# 1#); _ -> Cons q (go n (q +# 1#))`):
        //    `n` and `q` stay unboxed on the recursive `0#` branch and box on demand (`SBind::BoxRaw`)
        //    on the cold `Cons` branch -> gets raw worker `[true, true]`.
        let step = app(app(var("Test.go"), var("n")), prim("+#", vec![var("q"), Core::Lit(Lit::Int(1))]));
        let cold_cons = Core::Con("Builtin.:".into(), vec![var("q"), step.clone()]);
        let go_body = Core::Lam(
            "n".into(),
            Box::new(Core::Lam(
                "q".into(),
                Box::new(case(
                    prim(">#", vec![var("q"), var("n")]),
                    "cmp",
                    Core::Case(
                        Box::new(var("cmp")),
                        "cb1".into(),
                        vec![
                            Alt::Lit(Lit::Int(1), Core::Con("Builtin.[]".into(), vec![])),
                            Alt::Default(Core::Case(
                                Box::new(var("Test.flag")),
                                "fb".into(),
                                vec![
                                    Alt::Lit(Lit::Int(0), step),
                                    Alt::Default(cold_cons),
                                ],
                            )),
                        ],
                    ),
                )),
            )),
        );
        // 2. Tree insert (`ins k t = case k <# 0# of 1# -> Node k (ins k t); _ -> Node k (ins k t)`):
        //    every branch boxes `k` into a constructor/thunk -> must NOT get a raw worker.
        let rec_node = Core::Con("Test.Node".into(), vec![var("k"), app(app(var("Test.ins"), var("k")), var("t"))]);
        let ins_body = Core::Lam(
            "k".into(),
            Box::new(Core::Lam(
                "t".into(),
                Box::new(case(
                    prim("<#", vec![var("k"), Core::Lit(Lit::Int(0))]),
                    "cmp2",
                    Core::Case(
                        Box::new(var("cmp2")),
                        "cb2".into(),
                        vec![
                            Alt::Lit(Lit::Int(1), rec_node.clone()),
                            Alt::Default(rec_node),
                        ],
                    ),
                )),
            )),
        );
        let binds = vec![
            TopBind { name: "Test.flag".into(), body: Core::Lit(Lit::Int(0)) },
            TopBind { name: "Test.go".into(), body: go_body },
            TopBind { name: "Test.ins".into(), body: ins_body },
        ];
        let numeric = NumericParams {
            params: [
                ("Test.go".to_string(), vec![NUM_INT, NUM_INT]),
                ("Test.ins".to_string(), vec![NUM_INT, 0]),
            ]
            .into_iter()
            .collect(),
            results: HashMap::new(),
        };
        let prog = Lowering::program(&binds, &numeric);
        let (go_worker, go_raw) = prog.globals[1].worker.as_ref().expect("go should get raw worker");
        assert_eq!(go_raw, &vec![true, true]);
        fn has_box_raw(e: &SExpr) -> bool {
            match e {
                SExpr::Let(bs, b) => bs.iter().any(|(_, bind)| matches!(bind, SBind::BoxRaw(_))) || has_box_raw(b),
                SExpr::Case(s, _, alts) => {
                    has_box_raw(s)
                        || alts.iter().any(|a| match a {
                            SAlt::Con(_, _, b) | SAlt::Lit(_, b) | SAlt::Default(b) => has_box_raw(b),
                        })
                }
                SExpr::Join(_, _, c, b) => has_box_raw(c) || has_box_raw(b),
                _ => false,
            }
        }
        assert!(has_box_raw(&go_worker.body), "expected SBind::BoxRaw on cold branch: {:?}", go_worker.body);
        assert!(prog.globals[2].worker.is_none(), "ins boxes on every branch and should not get raw worker");
    }

    #[test]
    fn compact_raw_slots_shrinks_sequential_fib_temporaries() {
        // fib n = case n <# 2# of 1# -> n; _ -> case fib (n -# 1#) of a -> case fib (n -# 2#) of b -> a +# b
        let fib_body = Core::Lam(
            "n".into(),
            Box::new(case(
                prim("<#", vec![var("n"), Core::Lit(Lit::Int(2))]),
                "c",
                Core::Case(
                    Box::new(var("c")),
                    "cb".into(),
                    vec![
                        Alt::Lit(Lit::Int(1), var("n")),
                        Alt::Default(case(
                            prim("-#", vec![var("n"), Core::Lit(Lit::Int(1))]),
                            "n1",
                            case(
                                app(var("Test.fib"), var("n1")),
                                "a",
                                case(
                                    prim("-#", vec![var("n"), Core::Lit(Lit::Int(2))]),
                                    "n2",
                                    case(
                                        app(var("Test.fib"), var("n2")),
                                        "b",
                                        prim("+#", vec![var("a"), var("b")]),
                                    ),
                                ),
                            ),
                        )),
                    ],
                ),
            )),
        );
        let binds = vec![TopBind { name: "Test.fib".into(), body: fib_body }];
        let numeric = NumericParams {
            params: [("Test.fib".to_string(), vec![NUM_INT])].into_iter().collect(),
            results: [("Test.fib".to_string(), (1, NUM_INT))].into_iter().collect(),
        };
        let prog = Lowering::program(&binds, &numeric);
        let (worker, _) = prog.globals[0].worker.as_ref().expect("fib worker");
        assert!(worker.nraw <= 2, "expected compacted nraw <= 2 for fib, got {}", worker.nraw);
    }

    #[test]
    fn box_raw_at_deduplicates_pending_box_in_same_batch() {
        // go q n = case q <# 0# of 1# -> 0#; _ -> case n ==# 0# of 1# -> Pair q q; _ -> go (q +# 1#) (n -# 1#)
        // Hot path uses `q` raw, cold branch uses `q` twice in `Pair q q`: must emit only ONE `SBind::BoxRaw`.
        let inner = case(
            prim("==#", vec![var("n"), Core::Lit(Lit::Int(0))]),
            "c",
            Core::Case(
                Box::new(var("c")),
                "cb".into(),
                vec![
                    Alt::Lit(Lit::Int(1), Core::Con("Test.Pair".into(), vec![var("q"), var("q")])),
                    Alt::Default(case(
                        prim("+#", vec![var("q"), Core::Lit(Lit::Int(1))]),
                        "q1",
                        case(
                            prim("-#", vec![var("n"), Core::Lit(Lit::Int(1))]),
                            "n1",
                            app(app(var("Test.go"), var("q1")), var("n1")),
                        ),
                    )),
                ],
            ),
        );
        let go_body = Core::Lam(
            "q".into(),
            Box::new(Core::Lam(
                "n".into(),
                Box::new(case(
                    prim("<#", vec![var("q"), Core::Lit(Lit::Int(0))]),
                    "c0",
                    Core::Case(
                        Box::new(var("c0")),
                        "cb0".into(),
                        vec![Alt::Lit(Lit::Int(1), Core::Lit(Lit::Int(0))), Alt::Default(inner)],
                    ),
                )),
            )),
        );
        let binds = vec![TopBind { name: "Test.go".into(), body: go_body }];
        let numeric = NumericParams {
            params: [("Test.go".to_string(), vec![NUM_INT, NUM_INT])].into_iter().collect(),
            results: HashMap::new(),
        };
        let prog = Lowering::program(&binds, &numeric);
        let (worker, _) = prog.globals[0].worker.as_ref().expect("go should get raw worker");
        fn count_box_raw(e: &SExpr) -> usize {
            match e {
                SExpr::Let(bs, b) => {
                    bs.iter().filter(|(_, bind)| matches!(bind, SBind::BoxRaw(_))).count() + count_box_raw(b)
                }
                SExpr::Case(s, _, alts) => {
                    count_box_raw(s)
                        + alts
                            .iter()
                            .map(|a| match a {
                                SAlt::Con(_, _, b) | SAlt::Lit(_, b) | SAlt::Default(b) => count_box_raw(b),
                            })
                            .sum::<usize>()
                }
                SExpr::Join(_, _, c, b) => count_box_raw(c) + count_box_raw(b),
                _ => 0,
            }
        }
        assert_eq!(count_box_raw(&worker.body), 1, "expected deduplicated single SBind::BoxRaw: {:?}", worker.body);
    }

    #[test]
    fn is_join_handles_deeply_nested_lets_without_exponential_blowup() {
        let mut body = Core::Var("x39".into());
        for i in (1..40).rev() {
            let name = format!("x{i}");
            let prev = format!("x{}", i - 1);
            body = Core::Let(vec![(name, Core::Var(prev))], Box::new(body));
        }
        let binds = vec![("x0".to_string(), Core::Lit(Lit::Int(1)))];
        assert!(is_join(&binds, &body));
    }
}


