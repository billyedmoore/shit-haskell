//! The simplifier: local rewrites applied to every binding for a few rounds, until none applies.
//!
//! - A non-recursive local binding used exactly once, outside any lambda in its scope, moves to
//!   its use (no work is duplicated); an unused one is dropped.
//! - A small non-recursive global function applied to enough arguments is inlined: a copy with
//!   fresh binders replaces the call, and beta reduction binds the arguments.
//! - `(\x -> b) a` binds `a` to `x` (substituting a variable or number, otherwise as a let).
//! - A case of a constructor application, a literal, or a global known to be a nullary
//!   constructor (`otherwise`) selects its alternative.
//! - `case tagToEnum# e of {True -> ..; False -> ..}` becomes a case on the `Int#` itself.
//!
//! Every rewrite relies on the binders of a body being unique: bodies where they are not (some
//! matcher alternatives clone them) are left alone, to rule out capture.

use std::collections::{HashMap, HashSet};

use super::{Alt, Core, Lit, TopBind};

/// The largest body (in nodes, under its parameters) of a global function that is inlined.
const INLINE_SIZE: usize = 40;
/// The same for a function with an (active) INLINE pragma: GHC inlines those whatever their
/// size; this bounds the code a pragma can multiply (an inlined body is simplified again, so
/// INLINE functions calling INLINE functions compound).
const INLINE_PRAGMA_SIZE: usize = 120;
/// The rounds of rewriting of one binding.
const ROUNDS: usize = 8;

/// Primitives that evaluate all their arguments first: an argument that would be a thunk is
/// evaluated in place instead.
const STRICT_PRIMS: &[&str] = &[
    "+#", "-#", "*#", "quotInt#", "remInt#", "andI#", "orI#", "xorI#", "notI#", "negateInt#",
    "==#", "/=#", "<#", "<=#", ">#", ">=#", "uncheckedIShiftL#", "uncheckedIShiftRA#",
    "uncheckedIShiftRL#", "tagToEnum#", "eqChar#", "neChar#", "ltChar#", "leChar#", "gtChar#",
    "geChar#", "ord#", "chr#", "plusWord#", "minusWord#", "timesWord#", "and#", "or#", "xor#",
    "eqWord#", "neWord#", "ltWord#", "leWord#", "gtWord#", "geWord#", "uncheckedShiftL#",
    "uncheckedShiftRL#",
];
/// The calls inlined into one binding in one round.
const INLINE_BUDGET: usize = 64;

/// The fresh-name counter, kept across passes so that names made in one are not made again.
static FRESH: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

const TRUE: &str = "GHC.Types.True";
const FALSE: &str = "GHC.Types.False";

pub fn simplify(binds: &mut Vec<TopBind>) {
    float_con_fields(binds);
    // The rewrite rules, taken before any pass changes their sides, and the functions they
    // mention, which are not inlined while rules may still need to see them (phases 2 and 1).
    // HSC_NO_RULES: without them (to compare, or to rule them out).
    let enabled = std::env::var_os("HSC_NO_RULES").is_none();
    let mut rules = if enabled { Rules::new(extract_rules(binds)) } else { Rules { by_head: HashMap::new(), protected: HashSet::new(), inlines: HashMap::new() } };
    if std::env::var_os("HSC_NO_INLINE_PRAGMAS").is_none() {
        rules.inlines = extract_inlines(binds);
    }
    // The passes are GHC's phases 2, 1 and 0: the second inlines the bodies the first
    // simplified; the last cleans up after the calls by value.
    pass(binds, 2, &rules, None);
    pass(binds, 1, &rules, None);
    call_by_value(binds);
    pass(binds, 0, &rules, None);
    // Fusion in phase 0 makes local loops: their calls by value, and a clean-up of the
    // bindings that changed.
    // Loops made in phase 0 (`eftIntFB` inlined into a fused consumer) take their extra
    // parameters too (an IO loop its state token), in that clean-up.
    let mut changed = if std::env::var_os("HSC_NO_ETA").is_none() { eta_expand(binds, None, &rules) } else { HashSet::new() };
    changed.extend(call_by_value(binds));
    if !changed.is_empty() {
        pass(binds, 0, &rules, Some(&changed));
    }
    // `oneShot` was kept as a marker (see `into_one_shot`); it is the identity.
    for b in binds.iter_mut() {
        strip_one_shot(&mut b.body);
    }
}

/// Replaces `oneShot e` by `e`.
/// Bottom (a failing expression) as an arity: any number of arguments.
const BOTTOM: usize = usize::MAX;

/// Eta-expansion: a function whose body, after only cheap work (cases of variables and of
/// total primitives, lets of values), is a function in every alternative takes one more
/// parameter, `f = \xs -> \e -> body e` (the simplifier then pushes `e` into the alternatives,
/// where calls become saturated: an IO loop's `thenIO m (go xs)` becomes a call taking the
/// state token). Repeating the cheap work for each application of a partial application loses
/// nothing worth sharing. Globals and let-bound functions; one parameter per pass.
fn eta_expand(binds: &mut [TopBind], only: Option<&HashSet<String>>, rules: &Rules) -> HashSet<String> {
    let mut changed = HashSet::new();
    let globals: HashSet<String> = binds.iter().map(|b| b.name.clone()).collect();
    let bodies: HashMap<&str, &Core> = binds.iter().map(|b| (b.name.as_str(), &b.body)).collect();
    // Each global's parameters, through aliases.
    let mut arity: HashMap<String, usize> = HashMap::new();
    for b in binds.iter() {
        let mut cur = &b.body;
        for _ in 0..16 {
            match cur {
                Core::Var(v) => match bodies.get(v.as_str()) {
                    Some(next) => cur = next,
                    None => break,
                },
                _ => break,
            }
        }
        let n = value_arity(cur);
        if n > 0 {
            arity.insert(b.name.clone(), n);
        }
    }
    drop(bodies);
    let mut fresh = FRESH.load(std::sync::atomic::Ordering::Relaxed);
    for b in binds.iter_mut() {
        if is_rule_binding(&b.name) || b.name == ONE_SHOT || only.is_some_and(|o| !o.contains(&b.name)) {
            continue;
        }
        // An INLINE function keeps the parameters it is written with: it is inlined when
        // applied to them (as GHC inlines at the left side's arity), `(.) f g` say.
        let pragma = matches!(rules.inlines.get(&b.name), Some((true, _)));
        let before = fresh;
        let mut local = HashMap::new();
        if let Some(n) = expand_by_one(&b.body, &b.name, &arity, &mut local).filter(|_| !pragma) {
            arity.insert(b.name.clone(), n + 1);
            add_param(&mut b.body, &mut fresh);
        }
        // The local arities are by name: only where every binder is bound once.
        if unique_binders(&b.body, &globals, &mut HashSet::new()) {
            eta_locals(&mut b.body, &arity, &mut local, &mut fresh);
        }
        // Every new parameter takes a fresh name.
        if fresh != before {
            changed.insert(b.name.clone());
        }
    }
    FRESH.store(fresh, std::sync::atomic::Ordering::Relaxed);
    changed
}

/// The parameters of `f = rhs` when it can take one more (see `eta_expand`).
fn expand_by_one(rhs: &Core, f: &str, globals: &HashMap<String, usize>, local: &mut HashMap<String, usize>) -> Option<usize> {
    let (ps, body) = params(rhs);
    let n = ps.len();
    if n == 0 {
        return None;
    }
    // Recursive calls are taken to have the new parameter.
    let prev = local.insert(f.to_string(), n + 1);
    let k = cheap_arity(body, globals, local);
    match prev {
        Some(p) => local.insert(f.to_string(), p),
        None => local.remove(f),
    };
    (k >= 1 && k != BOTTOM).then_some(n)
}

/// `\xs -> body` (through `oneShot` markers) as `\xs -> \e -> body e`.
fn add_param(rhs: &mut Core, fresh: &mut u64) {
    let mut cur = rhs;
    loop {
        if matches!(cur, Core::App(f, a) if matches!(&**f, Core::Var(v) if v == ONE_SHOT) && matches!(**a, Core::Lam(..))) {
            let Core::App(_, a) = cur else { unreachable!() };
            cur = &mut **a;
        }
        match cur {
            Core::Lam(_, b) => cur = &mut **b,
            _ => break,
        }
    }
    *fresh += 1;
    let e = format!("eta~{fresh}");
    let body = take(cur);
    *cur = Core::Lam(e.clone(), Box::new(Core::App(Box::new(body), Box::new(Core::Var(e)))));
}

/// Eta-expansion of the let-bound functions of an expression.
fn eta_locals(c: &mut Core, globals: &HashMap<String, usize>, local: &mut HashMap<String, usize>, fresh: &mut u64) {
    match c {
        Core::Var(_) | Core::Lit(_) | Core::Fail(_) => {}
        Core::App(f, a) => {
            eta_locals(f, globals, local, fresh);
            eta_locals(a, globals, local, fresh);
        }
        Core::Lam(_, b) => eta_locals(b, globals, local, fresh),
        Core::Let(binds, body) => {
            for (x, r) in binds.iter() {
                // A function, or a partial application (`return ()`).
                let n = match value_arity(r) {
                    0 => cheap_arity(r, globals, local),
                    n => n,
                };
                if n > 0 && n != BOTTOM {
                    local.insert(x.clone(), n);
                }
            }
            for (x, r) in binds.iter_mut() {
                if let Some(n) = expand_by_one(r, x, globals, local) {
                    local.insert(x.clone(), n + 1);
                    add_param(r, fresh);
                }
                eta_locals(r, globals, local, fresh);
            }
            eta_locals(body, globals, local, fresh);
        }
        Core::Case(s, _, alts) => {
            eta_locals(s, globals, local, fresh);
            for a in alts {
                let (Alt::Con(_, _, b) | Alt::Lit(_, b) | Alt::Default(b)) = a;
                eta_locals(b, globals, local, fresh);
            }
        }
        Core::Con(_, args) | Core::Prim(_, args) => args.iter_mut().for_each(|a| eta_locals(a, globals, local, fresh)),
    }
}

/// How many arguments an expression takes after only cheap work (`BOTTOM` when it fails):
/// 0 when it does any real work first (a call, a case of a call, a let of a thunk).
fn cheap_arity(e: &Core, globals: &HashMap<String, usize>, local: &HashMap<String, usize>) -> usize {
    let arity_of = |v: &str| local.get(v).or_else(|| globals.get(v)).copied().unwrap_or(0);
    match e {
        Core::Fail(_) => BOTTOM,
        Core::Lam(_, b) => cheap_arity(b, globals, local).saturating_add(1),
        Core::Var(v) => arity_of(v),
        Core::App(..) => {
            if let Some(l) = one_shot_lambda(e) {
                return cheap_arity(l, globals, local);
            }
            // The head first (a saturated call does work), then the arguments.
            let mut n = 0;
            let mut h = e;
            while let Core::App(f, _) = h {
                n += 1;
                h = f;
            }
            let Core::Var(f) = h else { return 0 };
            let k = arity_of(f).saturating_sub(n);
            if k == 0 {
                return 0;
            }
            let mut h = e;
            while let Core::App(f, a) = h {
                if !is_cheap_value(a, globals, local) {
                    return 0;
                }
                h = f;
            }
            k
        }
        Core::Let(bs, body) => {
            if bs.iter().all(|(_, r)| is_cheap_value(r, globals, local)) {
                cheap_arity(body, globals, local)
            } else {
                0
            }
        }
        Core::Case(s, _, alts) => {
            let cheap = match &**s {
                Core::Var(_) => true,
                Core::Prim(p, args) => is_total_prim(p, args),
                _ => false,
            };
            if !cheap {
                return 0;
            }
            alts.iter()
                .map(|a| {
                    let (Alt::Con(_, _, b) | Alt::Lit(_, b) | Alt::Default(b)) = a;
                    cheap_arity(b, globals, local)
                })
                .min()
                .unwrap_or(BOTTOM)
        }
        Core::Lit(_) | Core::Con(..) | Core::Prim(..) => 0,
    }
}

/// An expression whose evaluation does no work worth sharing: an atom, a constructor of atoms,
/// or a function value (after cheap work).
fn is_cheap_value(e: &Core, globals: &HashMap<String, usize>, local: &HashMap<String, usize>) -> bool {
    match e {
        _ if is_atom(e) => true,
        Core::Lit(_) => true,
        Core::Con(_, args) => args.iter().all(is_atom),
        _ => {
            let k = cheap_arity(e, globals, local);
            k >= 1 && k != BOTTOM
        }
    }
}

/// A global bound to a constructor application with fields that are not atoms (a dictionary
/// with a default method, say) gets those fields as globals of their own, `g%f<i>`: then the
/// global is known (see `pass`), and cases of it select their field, as they do for
/// dictionaries of atoms. A top-level binding is as lazy and shared as the field was.
fn float_con_fields(binds: &mut Vec<TopBind>) {
    let mut extra = Vec::new();
    for b in binds.iter_mut() {
        if is_rule_binding(&b.name) {
            continue;
        }
        let Core::Con(_, fields) = &mut b.body else { continue };
        for (i, f) in fields.iter_mut().enumerate() {
            if is_atom(f) {
                continue;
            }
            let name = format!("{}%f{i}", b.name);
            extra.push(TopBind { name: name.clone(), body: std::mem::replace(f, Core::Var(name)) });
        }
    }
    binds.extend(extra);
}

fn strip_one_shot(c: &mut Core) {
    match c {
        Core::Var(_) | Core::Lit(_) | Core::Fail(_) => {}
        Core::App(f, a) => {
            strip_one_shot(f);
            strip_one_shot(a);
            if matches!(&**f, Core::Var(v) if v == ONE_SHOT) {
                *c = take(a);
            }
        }
        Core::Lam(_, b) => strip_one_shot(b),
        Core::Let(binds, body) => {
            binds.iter_mut().for_each(|(_, r)| strip_one_shot(r));
            strip_one_shot(body);
        }
        Core::Case(s, _, alts) => {
            strip_one_shot(s);
            for a in alts {
                match a {
                    Alt::Con(_, _, b) | Alt::Lit(_, b) | Alt::Default(b) => strip_one_shot(b),
                }
            }
        }
        Core::Con(_, args) | Core::Prim(_, args) => args.iter_mut().for_each(strip_one_shot),
    }
}

/// One simplifier pass over the bindings (`only`: just those).
fn pass(binds: &mut [TopBind], phase: u32, rules: &Rules, only: Option<&HashSet<String>>) {
    if std::env::var_os("HSC_NO_ETA").is_none() {
        let _ = eta_expand(binds, only, rules);
    }
    let globals: HashSet<String> = binds.iter().map(|b| b.name.clone()).collect();
    let direct: HashMap<&str, &Core> = binds.iter().map(|b| (b.name.as_str(), &b.body)).collect();
    // Globals that are (aliases of) constructor applications to atoms, and global aliases.
    let mut known: HashMap<String, (String, Vec<Core>)> = HashMap::new();
    let mut aliases: HashMap<String, String> = HashMap::new();
    // Globals that are literals (a boxed Int or Char is its number in Core: `maxInt`).
    let literals: HashMap<String, Lit> = binds
        .iter()
        .filter_map(|b| {
            // Through the boxing identity `(\x -> x) lit` of a constructor not simplified yet.
            let mut e = &b.body;
            while let Core::App(f, a) = e {
                match &**f {
                    Core::Lam(x, body) if matches!(&**body, Core::Var(y) if y == x) => e = a,
                    _ => break,
                }
            }
            match e {
                Core::Lit(l @ (Lit::Int(_) | Lit::Char(_))) => Some((b.name.clone(), l.clone())),
                _ => None,
            }
        })
        .collect();
    for b in binds.iter() {
        let mut body = &b.body;
        for _ in 0..8 {
            match body {
                Core::Var(v) => match direct.get(v.as_str()) {
                    Some(next) => {
                        aliases.insert(b.name.clone(), v.clone());
                        body = next;
                    }
                    None => break,
                },
                Core::Con(con, args) if args.iter().all(is_atom) => {
                    known.insert(b.name.clone(), (con.clone(), args.clone()));
                    break;
                }
                _ => break,
            }
        }
    }
    // Each global function's number of parameters (for partial applications, see `let_values`).
    let arity_map: HashMap<String, usize> = binds
        .iter()
        .filter_map(|b| {
            let (n, _) = lambdas(&b.body);
            (n > 0).then(|| (b.name.clone(), n))
        })
        .collect();
    let arities = &arity_map;
    // The same, counting the parameters of a local function the body answers after its own
    // (`foldr k z = let go = \xs -> .. in go` takes three): applied to fewer, it only allocates.
    let cheap_arities: HashMap<&str, usize> = binds
        .iter()
        .filter_map(|b| {
            let (n, inner) = lambdas(&b.body);
            let extra = match inner {
                Core::Let(bs, body) if bs.iter().all(|(_, r)| matches!(r, Core::Lam(..))) => match &**body {
                    Core::Var(v) => bs.iter().find(|(x, _)| x == v).map_or(0, |(_, r)| lambdas(r).0),
                    _ => 0,
                },
                _ => 0,
            };
            (n + extra > 0).then_some((b.name.as_str(), n + extra))
        })
        .collect();
    let mut templates: HashMap<String, (usize, Core)> = HashMap::new();
    let mut fresh = FRESH.load(std::sync::atomic::Ordering::Relaxed);
    for b in binds.iter() {
        let (arity, inner) = lambdas(&b.body);
        // An INLINE pragma inlines a larger body, in its phases only; NOINLINE allows inlining
        // in its phases only.
        let limit = match rules.inlines.get(&b.name) {
            Some((true, act)) if act.active(phase) => INLINE_PRAGMA_SIZE,
            Some((_, act)) if !act.active(phase) => 0,
            _ => INLINE_SIZE,
        };
        // A partial application of a known function to atoms (`mconcat = foldr mappend mempty`)
        // inlines where it is applied, so that the call it makes is seen (and fuses).
        if arity == 0 && limit > 0 && !is_rule_binding(&b.name) && !rules.protected.contains(&b.name) {
            let mut args: Vec<&Core> = Vec::new();
            let mut f = &b.body;
            while let Core::App(g, a) = f {
                args.push(a);
                f = g;
            }
            let partial = matches!(f, Core::Var(g) if g != &b.name && cheap_arities.get(g.as_str()).is_some_and(|n| *n > args.len()));
            if partial && !args.is_empty() && args.iter().all(|a| is_atom(a)) {
                templates.insert(b.name.clone(), (1, b.body.clone()));
                continue;
            }
        }
        if arity == 0 || size(inner, limit + 1) > limit || mentions(&b.body, &b.name) {
            continue;
        }
        if is_rule_binding(&b.name) || b.name == ONE_SHOT || (phase > 0 && rules.protected.contains(&b.name)) {
            continue;
        }
        let mut names = HashSet::new();
        if unique_binders(&b.body, &globals, &mut names) {
            templates.insert(b.name.clone(), (arity, b.body.clone()));
        } else {
            // A body not simplified yet may bind a name twice: a renamed copy is the template.
            let mut copy = b.body.clone();
            uniquify(&mut copy, &globals, &mut HashSet::new(), &mut HashMap::new(), &mut fresh);
            if unique_binders(&copy, &globals, &mut HashSet::new()) {
                templates.insert(b.name.clone(), (arity, copy));
            }
        }
    }
    for bind in binds.iter_mut() {
        if is_rule_binding(&bind.name) || only.is_some_and(|o| !o.contains(&bind.name)) {
            continue;
        }
        let mut names = HashSet::new();
        if !unique_binders(&bind.body, &globals, &mut names) {
            // Rebound names (from pattern bindings and cloned alternatives) are renamed apart.
            let mut seen = HashSet::new();
            uniquify(&mut bind.body, &globals, &mut seen, &mut HashMap::new(), &mut fresh);
            let mut names = HashSet::new();
            if !unique_binders(&bind.body, &globals, &mut names) {
                continue;
            }
        }
        // HSC_TRACE_SIMPLIFY=<global>: that binding before simplification and after each round.
        constant_args(&mut bind.body, &globals);
        let trace = std::env::var("HSC_TRACE_SIMPLIFY").ok().filter(|t| *t == bind.name);
        if trace.is_some() {
            eprintln!("== start\n{}", super::pretty::show(&bind.body));
        }
        for _ in 0..ROUNDS {
            let mut occ = HashMap::new();
            let mut depth = HashMap::new();
            occurrences(&bind.body, 0, &mut depth, &mut occ);
            let mut once = HashSet::new();
            let mut dead = HashSet::new();
            // Single uses outside lambdas first; then values (see `let_values`) used once even
            // under a lambda: no work is repeated, and the call site can reduce them.
            for (n, o) in &occ {
                if o.count == 0 {
                    dead.insert(n.clone());
                } else if o.count == 1 && !o.under_lambda && !o.recursive {
                    once.insert(n.clone());
                }
            }
            let values = let_values(&bind.body, arities, &once);
            for (n, o) in occ {
                if o.count == 1 && o.under_lambda && !o.recursive && values.contains(&n) {
                    once.insert(n);
                }
            }
            let mut s = Simp {
                once: &once,
                dead: &dead,
                pending: HashMap::new(),
                known: &known,
                literals: &literals,
                aliases: &aliases,
                templates: &templates,
                this: &bind.name,
                rules,
                phase,
                rule_budget: RULE_BUDGET,
                atoms: HashMap::new(),
                evaluated: HashSet::new(),
                arities,
                local_arities: {
                    let mut a = HashMap::new();
                    local_arities(&bind.body, &mut a);
                    a
                },
                fresh: &mut fresh,
                budget: INLINE_BUDGET,
                changed: false,
            };
            s.expr(&mut bind.body);
            if trace.is_some() {
                eprintln!("== round\n{}", super::pretty::show(&bind.body));
            }
            if !s.changed {
                break;
            }
        }
    }
    FRESH.store(fresh, std::sync::atomic::Ordering::Relaxed);
}


struct Simp<'a> {
    once: &'a HashSet<String>,
    dead: &'a HashSet<String>,
    pending: HashMap<String, Core>,
    known: &'a HashMap<String, (String, Vec<Core>)>,
    /// Globals that are literals.
    literals: &'a HashMap<String, Lit>,
    aliases: &'a HashMap<String, String>,
    templates: &'a HashMap<String, (usize, Core)>,
    /// The binding being simplified (never inlined into itself).
    this: &'a str,
    /// The rewrite rules, and the phase (2, 1, 0) of this pass.
    rules: &'a Rules,
    phase: u32,
    /// The rewrites left to this binding in this round.
    rule_budget: usize,
    /// Locals bound to atoms, substituted at their uses.
    atoms: HashMap<String, Core>,
    /// Case binders: variables known to be evaluated.
    evaluated: HashSet<String>,
    /// The number of parameters of the global and let-bound functions in scope.
    arities: &'a HashMap<String, usize>,
    /// The same for the let-bound functions of the binding being simplified.
    local_arities: HashMap<String, usize>,
    fresh: &'a mut u64,
    budget: usize,
    changed: bool,
}

impl Simp<'_> {
    fn expr(&mut self, c: &mut Core) {
        match c {
            Core::Var(v) => {
                if let Some(e) = self.pending.remove(v.as_str()) {
                    *c = e;
                    self.changed = true;
                    self.expr(c);
                } else if let Some(a) = self.atoms.get(v.as_str()) {
                    *c = a.clone();
                    self.changed = true;
                }
            }
            Core::Lit(_) | Core::Fail(_) => {}
            Core::App(f, a) => {
                self.expr(f);
                self.expr(a);
                if self.rewrite(c) || self.app_of_case(c) {
                    self.changed = true;
                    self.expr(c);
                    return;
                }
                self.inline_call(c);
                self.reduce(c);
            }
            Core::Lam(_, b) => self.expr(b),
            Core::Let(bindings, _) => {
                for (x, r) in bindings.iter() {
                    let n = value_arity(r);
                    if n > 0 {
                        self.local_arities.insert(x.clone(), n);
                    }
                }
                if self.into_one_shot(c) {
                    self.changed = true;
                    self.expr(c);
                    return;
                }
                if bang_lets(c) {
                    self.changed = true;
                    self.expr(c);
                    return;
                }
                let Core::Let(bindings, body) = c else { unreachable!() };
                let mut kept = Vec::with_capacity(bindings.len());
                let group: HashSet<String> = bindings.iter().map(|(n, _)| n.clone()).collect();
                for (name, mut rhs) in std::mem::take(bindings) {
                    // A binding to a number, or to a variable bound outside its group that is not
                    // itself moving: substituted (an alias of a substituted variable takes its atom).
                    if let Core::Var(y) = &rhs {
                        if let Some(a) = self.atoms.get(y.as_str()) {
                            rhs = a.clone();
                        }
                    }
                    let atom = match &rhs {
                        Core::Lit(_) => is_atom(&rhs),
                        Core::Var(y) => !group.contains(y) && !self.once.contains(y),
                        _ => false,
                    };
                    if self.dead.contains(&name) {
                        self.changed = true;
                    } else if atom {
                        self.atoms.insert(name, rhs);
                        self.changed = true;
                    } else if self.once.contains(&name) {
                        self.pending.insert(name, rhs);
                    } else {
                        kept.push((name, rhs));
                    }
                }
                for (_, rhs) in kept.iter_mut() {
                    self.expr(rhs);
                }
                self.expr(body);
                if kept.is_empty() {
                    *c = take(body);
                } else {
                    *bindings = kept;
                    self.strict_let(c);
                }
            }
            Core::Case(s, binder, alts) => {
                self.expr(s);
                // A case of a variable already evaluated (a case binder), with one default
                // alternative, is that alternative.
                if let (Core::Var(v), [Alt::Default(_)]) = (&**s, &alts[..]) {
                    if self.evaluated.contains(v.as_str()) {
                        let v = Core::Var(v.clone());
                        let b = binder.clone();
                        let Some(Alt::Default(mut body)) = alts.pop() else { unreachable!() };
                        subst(&mut body, &b, &v);
                        *c = body;
                        self.changed = true;
                        self.expr(c);
                        return;
                    }
                }
                // A total primitive whose result is not used: dropped.
                if let (Core::Prim(p, args), [Alt::Default(body)]) = (&**s, &alts[..]) {
                    if is_total_prim(p, args) && !mentions(body, binder) {
                        let Some(Alt::Default(body)) = alts.pop() else { unreachable!() };
                        *c = body;
                        self.changed = true;
                        self.expr(c);
                        return;
                    }
                }
                // `case e as u of _ -> u` is `e`.
                if matches!(&alts[..], [Alt::Default(Core::Var(v))] if v == binder) {
                    *c = take(s);
                    self.changed = true;
                    return;
                }
                // A let in the scrutinee is floated out (the binders are unique).
                if let Core::Let(..) = &**s {
                    let Core::Case(s, binder, alts) = take(c) else { unreachable!() };
                    let Core::Let(bs, e) = *s else { unreachable!() };
                    let mut case = Core::Case(e, binder, alts);
                    self.changed = true;
                    self.expr(&mut case);
                    *c = Core::Let(bs, Box::new(case));
                    return;
                }
                // Case of a case with one alternative: the outer case moves into it.
                if let Core::Case(_, _, inner) = &mut **s {
                    if inner.len() == 1 {
                        let Core::Case(s, binder, alts) = take(c) else { unreachable!() };
                        let Core::Case(s2, b2, mut inner) = *s else { unreachable!() };
                        let body = match &mut inner[0] {
                            Alt::Con(_, _, b) | Alt::Lit(_, b) | Alt::Default(b) => b,
                        };
                        *body = Core::Case(Box::new(take(body)), binder, alts);
                        *c = Core::Case(s2, b2, inner);
                        self.changed = true;
                        self.expr(c);
                        return;
                    }
                }
                // Case of a case with several alternatives, the outer one small with a single
                // default alternative: a copy of it at each result of the inner one.
                if self.push_small_case(c) {
                    self.changed = true;
                    self.expr(c);
                    return;
                }
                // A value as a scrutinee (nothing to evaluate): its binder is the value.
                if self.case_of_value(c) {
                    self.changed = true;
                    self.expr(c);
                    return;
                }
                if let Some(selected) = self.select(c) {
                    *c = selected;
                    self.changed = true;
                    self.expr(c);
                    return;
                }
                self.tag_to_enum(c);
                let Core::Case(_, binder, alts) = c else { return };
                self.evaluated.insert(binder.clone());
                for a in alts {
                    match a {
                        Alt::Con(_, _, b) | Alt::Lit(_, b) | Alt::Default(b) => self.expr(b),
                    }
                }
            }
            // Representation identities (Int# and Word# share it; Core is untyped, so coercions
            // are nothing), and Integer conversions of small literals and of `IS`.
            Core::Prim(p, args)
                if args.len() == 1
                    && matches!(p.as_str(), "int2Word#" | "word2Int#" | "integerToInt" | "integerToWord" | "unsafeCoerce" | "unsafeCoerce#" | "coerce") =>
            {
                self.expr(&mut args[0]);
                let integer = p.starts_with("integer");
                let folded = match &mut args[0] {
                    _ if !integer => Some(take(&mut args[0])),
                    Core::Lit(Lit::Integer(t)) => t.parse::<i64>().ok().map(|i| Core::Lit(Lit::Int(i))),
                    Core::Con(k, fields) if k == "GHC.Num.Integer.IS" && fields.len() == 1 => fields.pop(),
                    _ => None,
                };
                if let Some(f) = folded {
                    *c = f;
                    self.changed = true;
                }
            }
            Core::Prim(p, args) if STRICT_PRIMS.contains(&p.as_str()) => {
                args.iter_mut().for_each(|a| self.expr(a));
                // Each argument that is not an atom is evaluated first, into a fresh variable.
                let mut cases = Vec::new();
                for a in args.iter_mut() {
                    if !is_atom(a) {
                        *self.fresh += 1;
                        let v = format!("s~{}", self.fresh);
                        cases.push((v.clone(), std::mem::replace(a, Core::Var(v))));
                    }
                }
                if !cases.is_empty() {
                    self.changed = true;
                } else if let Some(v) = fold(p, args) {
                    *c = Core::Lit(Lit::Int(v));
                    self.changed = true;
                    return;
                } else if let Some(a) = identity(p, args) {
                    *c = a;
                    self.changed = true;
                    return;
                }
                let mut e = take(c);
                for (v, a) in cases.into_iter().rev() {
                    e = Core::Case(Box::new(a), v, vec![Alt::Default(e)]);
                }
                *c = e;
            }
            Core::Con(_, args) | Core::Prim(_, args) => args.iter_mut().for_each(|a| self.expr(a)),
        }
    }

    /// `let x = e in case x as b of alts` (x not in e) as `case e as x of alts`, with `b` as `x`.
    fn strict_let(&mut self, c: &mut Core) {
        let Core::Let(bs, body) = c else { return };
        let [(x, e)] = &bs[..] else { return };
        if mentions(e, x) || self.is_value(e) {
            return;
        }
        let direct = matches!(&**body, Core::Case(s, _, _) if matches!(&**s, Core::Var(v) if v == x));
        if !direct {
            // The body certainly evaluates x: x is evaluated first instead of allocated.
            if !forced_total(body).contains(x.as_str()) {
                return;
            }
            let Core::Let(mut bs, body) = take(c) else { unreachable!() };
            let (x, e) = bs.pop().expect("one binding");
            *c = Core::Case(Box::new(e), x, vec![Alt::Default(*body)]);
            self.changed = true;
            return;
        }
        let Core::Let(mut bs, body) = take(c) else { unreachable!() };
        let (x, e) = bs.pop().expect("one binding");
        let Core::Case(_, b, mut alts) = *body else { unreachable!() };
        let var = Core::Var(x.clone());
        for a in alts.iter_mut() {
            match a {
                Alt::Con(_, _, body) | Alt::Lit(_, body) | Alt::Default(body) => subst(body, &b, &var),
            }
        }
        *c = Core::Case(Box::new(e), x, alts);
        self.changed = true;
    }

    /// `case (case s of alts1) as b of _ -> body` with `body` small: `body` (a fresh copy) after
    /// each result of the inner case, which then needs no join point.
    fn push_small_case(&mut self, c: &mut Core) -> bool {
        let Core::Case(s, _, alts) = c else { return false };
        let [Alt::Default(body)] = &alts[..] else { return false };
        if !matches!(**s, Core::Case(..)) || size(body, PUSH_SIZE + 1) > PUSH_SIZE || results(s, PUSH_RESULTS) > PUSH_RESULTS {
            return false;
        }
        // A single-use binding waiting for its occurrence (see `pending`) must keep one.
        if self.pending.keys().any(|x| mentions(body, x)) {
            return false;
        }
        let Core::Case(inner, b, mut alts) = take(c) else { unreachable!() };
        let Alt::Default(body) = alts.pop().expect("one alternative") else { unreachable!() };
        let mut inner = *inner;
        let fresh = &mut *self.fresh;
        at_results(&mut inner, &mut |r: &mut Core| {
            *fresh += 1;
            let tag = format!("~p{fresh}");
            let mut copy = body.clone();
            let mut map = HashMap::new();
            let b2 = format!("{b}{tag}");
            map.insert(b.clone(), b2.clone());
            freshen(&mut copy, &tag, &mut map);
            *r = Core::Case(Box::new(take(r)), b2, vec![Alt::Default(copy)]);
        });
        *c = inner;
        true
    }

    /// `case v as b of _ -> e` with `v` a value (a lambda, a global function, or a function
    /// applied to fewer arguments than it takes): `let b = v in e`.
    fn case_of_value(&mut self, c: &mut Core) -> bool {
        let Core::Case(s, _, alts) = c else { return false };
        if !matches!(&alts[..], [Alt::Default(_)]) || !self.is_value(s) {
            return false;
        }
        let Core::Case(s, b, mut alts) = take(c) else { unreachable!() };
        let Some(Alt::Default(body)) = alts.pop() else { unreachable!() };
        *c = Core::Let(vec![(b, *s)], Box::new(body));
        true
    }

    fn arity(&self, v: &str) -> usize {
        self.local_arities.get(v).or_else(|| self.arities.get(v)).copied().unwrap_or(0)
    }

    fn is_value(&self, e: &Core) -> bool {
        match e {
            Core::Lam(..) => true,
            Core::Var(v) => self.arity(v) > 0,
            Core::App(..) => {
                let mut n = 0;
                let mut h = e;
                while let Core::App(f, a) = h {
                    if !is_atom(a) {
                        return false;
                    }
                    n += 1;
                    h = f;
                }
                matches!(h, Core::Var(v) if self.arity(v) > n)
            }
            _ => false,
        }
    }

    /// `let bs in oneShot (\x -> e)` as `\x -> let bs in e`: the lambda runs at most once, so
    /// the bindings' work is not repeated, and the function it is the body of takes `x` too.
    fn into_one_shot(&mut self, c: &mut Core) -> bool {
        let Core::Let(_, body) = c else { return false };
        if one_shot_lambda(body).is_none() {
            return false;
        }
        let Core::Let(bs, body) = take(c) else { unreachable!() };
        let Core::App(marker, lam) = *body else { unreachable!() };
        let Core::Lam(x, inner) = *lam else { unreachable!() };
        // Still marked: an enclosing let moves in too.
        *c = Core::App(marker, Box::new(Core::Lam(x, Box::new(Core::Let(bs, inner)))));
        true
    }

    /// `(case s of alts) a` as the case applying each alternative to `a` (let-bound first when it
    /// is not an atom, so that it is not copied).
    fn app_of_case(&mut self, c: &mut Core) -> bool {
        let Core::App(f, _) = c else { return false };
        // `(seq a f) x` is `seq a (f x)`: both evaluate `a`, then `f`, then apply it.
        if matches!(&**f, Core::Prim(p, args) if p == "seq" && args.len() == 2) {
            let Core::App(f, a) = take(c) else { unreachable!() };
            let Core::Prim(p, mut args) = *f else { unreachable!() };
            let g = args.pop().expect("two arguments");
            args.push(Core::App(Box::new(g), a));
            *c = Core::Prim(p, args);
            return true;
        }
        if !matches!(**f, Core::Case(..)) {
            return false;
        }
        let Core::App(f, a) = take(c) else { unreachable!() };
        let Core::Case(s, b, mut alts) = *f else { unreachable!() };
        let (arg, bind) = if is_atom(&a) {
            (*a, None)
        } else {
            *self.fresh += 1;
            let v = format!("a~{}", self.fresh);
            (Core::Var(v.clone()), Some((v, *a)))
        };
        for alt in alts.iter_mut() {
            let body = match alt {
                Alt::Con(_, _, b) | Alt::Lit(_, b) | Alt::Default(b) => b,
            };
            *body = Core::App(Box::new(take(body)), Box::new(arg.clone()));
        }
        let case = Core::Case(s, b, alts);
        *c = match bind {
            Some((v, e)) => Core::Let(vec![(v, e)], Box::new(case)),
            None => case,
        };
        true
    }

    /// Rewrites a call that matches an active rule's left side by the rule's right side (a fresh
    /// copy, its variables bound to what they matched, further arguments applied after).
    fn rewrite(&mut self, c: &mut Core) -> bool {
        if self.rule_budget == 0 {
            return false;
        }
        let (head, args) = {
            let mut args = Vec::new();
            let mut h = &*c;
            while let Core::App(f, a) = h {
                args.push(&**a);
                h = f;
            }
            args.reverse();
            (h, args)
        };
        let Core::Var(g) = head else { return false };
        // An alias of `foldr` (`foldr = List.foldr` in the list instance of Foldable, which
        // `mapM_` and `forM_` use) is `foldr`, for "fold/build". (Not every alias: `length`
        // fused that way makes a loop that needs call-arity analysis to take its accumulator.)
        let mut g = g.as_str();
        let mut target = g;
        for _ in 0..8 {
            match self.aliases.get(target) {
                Some(next) => target = next.as_str(),
                None => break,
            }
        }
        if target == "GHC.Internal.Base.foldr" {
            g = target;
        }
        let Some(candidates) = self.rules.by_head.get(g) else { return false };
        for rule in candidates {
            if !rule.active(self.phase) || args.len() < rule.args.len() {
                continue;
            }
            let vars: HashSet<&str> = rule.vars.iter().map(String::as_str).collect();
            let mut sub: HashMap<String, Core> = HashMap::new();
            let mut bound = Vec::new();
            if !rule.args.iter().zip(&args).all(|(p, e)| match_core(p, e, &vars, &mut sub, &mut bound)) {
                continue;
            }
            if rule.vars.iter().any(|v| !sub.contains_key(v)) {
                continue;
            }
            // A fresh copy of the right side, its variables bound to their matches: atoms in
            // place, anything else shared through a let (a variable may occur more than once).
            *self.fresh += 1;
            let tag = format!("~r{}", self.fresh);
            let mut rhs = rule.rhs.clone();
            freshen(&mut rhs, &tag, &mut HashMap::new());
            let mut lets = Vec::new();
            for v in &rule.vars {
                let e = sub.remove(v).expect("matched");
                // An atom, or an expression the right side uses once (its lambdas are those of
                // `build`'s argument, called once): in place, so later rules see it.
                if is_atom(&e) || (occurrences_of(&rhs, v) == 1 && one_shot_use(&rhs, v, false)) {
                    subst(&mut rhs, v, &e);
                } else {
                    let name = format!("{v}{tag}");
                    subst(&mut rhs, v, &Core::Var(name.clone()));
                    lets.push((name, e));
                }
            }
            let mut out = rhs;
            for a in args.iter().skip(rule.args.len()) {
                out = Core::App(Box::new(out), Box::new((*a).clone()));
            }
            for (name, e) in lets.into_iter().rev() {
                out = Core::Let(vec![(name, e)], Box::new(out));
            }
            if std::env::var_os("HSC_DEBUG_RULES").is_some() {
                eprintln!("rule {} fired in {}", rule.name, self.this);
            }
            *c = out;
            self.rule_budget -= 1;
            return true;
        }
        false
    }

    /// Replaces the head of a saturated call of a small global function by a fresh copy of it.
    fn inline_call(&mut self, c: &mut Core) {
        if self.budget == 0 {
            return;
        }
        let mut n = 0;
        let mut head = &mut *c;
        while let Core::App(f, _) = head {
            n += 1;
            head = f;
        }
        let Core::Var(name) = head else { return };
        let mut target = name.as_str();
        for _ in 0..8 {
            match self.aliases.get(target) {
                Some(next) if self.templates.contains_key(next) || self.aliases.contains_key(next) => target = next,
                _ => break,
            }
        }
        if target == self.this {
            return;
        }
        let Some((arity, body)) = self.templates.get(target) else { return };
        if *arity > n {
            return;
        }
        let mut copy = body.clone();
        *self.fresh += 1;
        let mut map = HashMap::new();
        freshen(&mut copy, &format!("~{}", self.fresh), &mut map);
        *head = copy;
        self.budget -= 1;
        self.changed = true;
    }

    /// Beta reduction at the top of `c`: `(\x -> b) a`, and lets floated out of the function.
    fn reduce(&mut self, c: &mut Core) {
        // `coerce e` is `e` (Core is untyped).
        if matches!(c, Core::App(f, _) if matches!(&**f, Core::Var(v) if v == "GHC.Prim.coerce")) {
            let Core::App(_, a) = take(c) else { unreachable!() };
            *c = *a;
            self.changed = true;
            self.reduce(c);
            return;
        }
        let Core::App(f, _) = c else { return };
        // An applied `oneShot (\z -> e)`: the marker says nothing more, and the lambda reduces.
        if one_shot_lambda(f).is_some() {
            let Core::App(_, lam) = take(f) else { unreachable!() };
            **f = *lam;
            self.changed = true;
        }
        // The function of an application may itself be a redex, made by inlining.
        self.reduce(f);
        match &**f {
            Core::Lam(..) => {
                let Core::App(f, a) = take(c) else { unreachable!() };
                let Core::Lam(x, mut b) = *f else { unreachable!() };
                match *a {
                    a @ (Core::Var(_) | Core::Lit(Lit::Int(_) | Lit::Char(_) | Lit::Double(_))) => {
                        subst(&mut b, &x, &a);
                        *c = *b;
                    }
                    a => *c = Core::Let(vec![(x, a)], b),
                }
                self.changed = true;
                // `(let x = a in \y -> b) a2`, or a body that is itself an application to reduce.
                if let Core::Let(_, body) = c {
                    self.reduce(body);
                } else {
                    self.reduce(c);
                }
            }
            Core::Let(..) => {
                // `(let bs in e) a` as `let bs in e a`: the binders are unique, so `a` is not captured.
                let Core::App(f, a) = take(c) else { unreachable!() };
                let Core::Let(bs, e) = *f else { unreachable!() };
                let mut app = Core::App(e, a);
                self.reduce(&mut app);
                *c = Core::Let(bs, Box::new(app));
                self.changed = true;
            }
            _ => {}
        }
    }

    /// The alternative a case selects when its scrutinee is known, with the binders bound.
    fn select(&mut self, c: &mut Core) -> Option<Core> {
        let Core::Case(s, binder, alts) = c else { return None };
        match &**s {
            Core::Var(v) if self.known.contains_key(v.as_str()) => {
                let (con, fields) = &self.known[v.as_str()];
                let i = alts
                    .iter()
                    .position(|a| matches!(a, Alt::Con(k, vars, _) if k == con && vars.len() == fields.len()))
                    .or_else(|| alts.iter().position(|a| matches!(a, Alt::Default(_))))?;
                let v = Core::Var(v.clone());
                let (vars, mut body) = match alts.swap_remove(i) {
                    Alt::Con(_, vars, body) => (vars, body),
                    other => (Vec::new(), alt_body(other)),
                };
                for (x, f) in vars.iter().zip(fields) {
                    subst(&mut body, x, f);
                }
                subst(&mut body, binder, &v);
                Some(body)
            }
            Core::Con(con, args) => {
                let i = alts
                    .iter()
                    .position(|a| matches!(a, Alt::Con(k, vars, _) if k == con && vars.len() == args.len()))
                    .or_else(|| alts.iter().position(|a| matches!(a, Alt::Default(_))))?;
                let con = con.clone();
                let Core::Con(_, args) = take(s) else { unreachable!() };
                let (vars, body) = match alts.swap_remove(i) {
                    Alt::Con(_, vars, body) => (vars, body),
                    other => {
                        // A default: the fields get fresh names, for the binder's constructor.
                        *self.fresh += 1;
                        let vars = (0..args.len()).map(|k| format!("{binder}~f{}_{k}", self.fresh)).collect();
                        (vars, alt_body(other))
                    }
                };
                let mut binds: Vec<(String, Core)> = vars.iter().cloned().zip(args).collect();
                if mentions(&body, binder) {
                    binds.push((binder.clone(), Core::Con(con, vars.into_iter().map(Core::Var).collect())));
                }
                Some(if binds.is_empty() { body } else { Core::Let(binds, Box::new(body)) })
            }
            Core::Var(v) if self.literals.contains_key(v.as_str()) => {
                let l = self.literals[v.as_str()].clone();
                **s = Core::Lit(l);
                self.select(c)
            }
            Core::Lit(l @ (Lit::Int(_) | Lit::Char(_))) => {
                let i = alts
                    .iter()
                    .position(|a| matches!(a, Alt::Lit(k, _) if k == l))
                    .or_else(|| alts.iter().position(|a| matches!(a, Alt::Default(_))))?;
                let l = Core::Lit(l.clone());
                let mut body = alt_body(alts.swap_remove(i));
                subst(&mut body, binder, &l);
                Some(body)
            }
            _ => None,
        }
    }

    /// `case tagToEnum# e as b of` with Bool alternatives, as a case on the `Int#` `e`; the binder
    /// becomes the constructor each alternative selects.
    fn tag_to_enum(&mut self, c: &mut Core) {
        let Core::Case(s, binder, alts) = c else { return };
        let Core::Prim(p, args) = &**s else { return };
        if p != "tagToEnum#" || args.len() != 1 {
            return;
        }
        let bool_alt = |a: &Alt| match a {
            Alt::Con(k, vars, _) => vars.is_empty() && (k == TRUE || k == FALSE),
            Alt::Default(_) => true,
            Alt::Lit(..) => false,
        };
        if !alts.iter().all(bool_alt) || alts.is_empty() {
            return;
        }
        let explicit: Vec<&str> = alts
            .iter()
            .filter_map(|a| match a {
                Alt::Con(k, _, _) => Some(k.as_str()),
                _ => None,
            })
            .collect();
        let rest = match (explicit.contains(&TRUE), explicit.contains(&FALSE)) {
            (true, true) => None,
            (true, false) => Some(FALSE),
            (false, true) => Some(TRUE),
            (false, false) => return,
        };
        let mut new_alts = Vec::new();
        for a in std::mem::take(alts) {
            match a {
                Alt::Con(k, _, mut body) => {
                    subst(&mut body, binder, &Core::Con(k.clone(), Vec::new()));
                    new_alts.push(Alt::Lit(Lit::Int((k == TRUE) as i64), body));
                }
                Alt::Default(mut body) => {
                    if let Some(k) = rest {
                        subst(&mut body, binder, &Core::Con(k.to_string(), Vec::new()));
                        new_alts.push(Alt::Default(body));
                    }
                }
                Alt::Lit(..) => unreachable!(),
            }
        }
        // A True and a False alternative: the second becomes the default.
        if rest.is_none() {
            if let Some(Alt::Lit(_, body)) = new_alts.pop() {
                new_alts.push(Alt::Default(body));
            }
        }
        let Core::Prim(_, mut args) = take(s) else { unreachable!() };
        **s = args.pop().expect("one argument");
        *binder = format!("{binder}~t");
        *alts = new_alts;
        self.changed = true;
    }
}

/// A let group with banged bindings, `let x = e; strict%x = x; .. in case strict%x of _ -> ..
/// body`, as its lazy bindings around a case for each banged one, in dependency order:
/// `let lazy in case e as x of _ -> .. body`. Every banged binding is evaluated before the body
/// either way. Left alone (false) when a lazy binding refers to a banged one, or the banged
/// ones depend on each other in a cycle.
fn bang_lets(c: &mut Core) -> bool {
    use crate::tc::expr::STRICT_BIND;
    let Core::Let(binds, body) = c else { return false };
    let names: HashSet<&str> = binds.iter().map(|(n, _)| n.as_str()).collect();
    // The chain of cases forcing the aliases.
    let mut forced = Vec::new();
    let mut cur = &**body;
    while let Core::Case(s, _, alts) = cur {
        let (Core::Var(v), [Alt::Default(next)]) = (&**s, &alts[..]) else { break };
        let Some(x) = v.strip_prefix(STRICT_BIND) else { break };
        if !names.contains(v.as_str()) || !binds.iter().any(|(n, r)| n == v && matches!(r, Core::Var(y) if y == x)) {
            break;
        }
        forced.push((v.clone(), x.to_string()));
        cur = next;
    }
    if forced.is_empty() {
        return false;
    }
    let strict: HashSet<&str> = forced.iter().map(|(_, x)| x.as_str()).collect();
    let aliases: HashSet<&str> = forced.iter().map(|(a, _)| a.as_str()).collect();
    if !strict.iter().all(|x| names.contains(x)) {
        return false;
    }
    // Every binding after those it mentions; aliases may not be mentioned at all.
    let members: Vec<&str> = binds.iter().map(|(n, _)| n.as_str()).filter(|n| !aliases.contains(n)).collect();
    let rhs: HashMap<&str, &Core> = binds.iter().map(|(n, r)| (n.as_str(), r)).collect();
    if members.iter().any(|n| aliases.iter().any(|a| mentions(rhs[n], a))) {
        return false;
    }
    let mut order: Vec<String> = Vec::new();
    let mut placed: HashSet<&str> = HashSet::new();
    while order.len() < members.len() {
        let next = members.iter().copied().find(|x| {
            !placed.contains(x) && members.iter().all(|y| placed.contains(y) || !mentions(rhs[x], y))
        });
        let Some(x) = next else { return false };
        placed.insert(x);
        order.push(x.to_string());
    }
    let Core::Let(binds, body) = take(c) else { unreachable!() };
    // The body under the chain of cases.
    let mut inner = *body;
    for _ in 0..forced.len() {
        let Core::Case(_, _, mut alts) = inner else { unreachable!() };
        inner = alt_body(alts.pop().expect("one alternative"));
    }
    let mut table: HashMap<String, Core> = binds.into_iter().collect();
    let mut e = inner;
    for x in order.into_iter().rev() {
        let r = table.remove(&x).expect("binding");
        e = if strict.contains(x.as_str()) {
            Core::Case(Box::new(r), x, vec![Alt::Default(e)])
        } else {
            Core::Let(vec![(x, r)], Box::new(e))
        };
    }
    *c = e;
    true
}

/// Specialises local functions on arguments that are the same global at every call (the
/// dictionaries of a local polymorphic function used at one type): `let f = \d x -> e in .. f
/// D a ..` becomes `let f = \x -> e[d := D] in .. f a ..`. Recursive calls may pass the
/// parameter itself. Every use of `f` must be a call with the parameter's argument.
fn constant_args(c: &mut Core, globals: &HashSet<String>) {
    match c {
        Core::Var(_) | Core::Lit(_) | Core::Fail(_) => {}
        Core::App(f, a) => {
            constant_args(f, globals);
            constant_args(a, globals);
        }
        Core::Lam(_, b) => constant_args(b, globals),
        Core::Let(binds, body) => {
            for (_, r) in binds.iter_mut() {
                constant_args(r, globals);
            }
            constant_args(body, globals);
            if binds.len() == 1 {
                specialise_let(binds, body, globals);
            }
        }
        Core::Case(s, _, alts) => {
            constant_args(s, globals);
            for a in alts {
                match a {
                    Alt::Con(_, _, b) | Alt::Lit(_, b) | Alt::Default(b) => constant_args(b, globals),
                }
            }
        }
        Core::Con(_, args) | Core::Prim(_, args) => args.iter_mut().for_each(|a| constant_args(a, globals)),
    }
}

fn specialise_let(binds: &mut [(String, Core)], body: &mut Core, globals: &HashSet<String>) {
    let (f, rhs) = &binds[0];
    let params: Vec<String> = {
        let mut ps = Vec::new();
        let mut cur = rhs;
        while let Core::Lam(x, b) = cur {
            ps.push(x.clone());
            cur = b;
        }
        ps
    };
    if params.is_empty() {
        return;
    }
    // Every use: the variable names of its arguments (None for a non-variable), or None when
    // the use is not the head of a call.
    let mut calls: Vec<Option<Vec<Option<String>>>> = Vec::new();
    calls_of(rhs, f, &mut calls);
    calls_of(body, f, &mut calls);
    if calls.is_empty() || calls.iter().any(|c| c.is_none()) {
        return;
    }
    let calls: Vec<Vec<Option<String>>> = calls.into_iter().flatten().collect();
    let mut constant: Vec<Option<String>> = vec![None; params.len()];
    for (i, p) in params.iter().enumerate() {
        let mut g: Option<&str> = None;
        let mut ok = true;
        for call in &calls {
            match call.get(i) {
                Some(Some(v)) if v == p => {}
                Some(Some(v)) if globals.contains(v) && g.is_none_or(|g| g == v) => g = Some(v),
                _ => {
                    ok = false;
                    break;
                }
            }
        }
        if ok {
            constant[i] = g.map(str::to_string);
        }
    }
    // At least one parameter must remain: dropping them all would make `f` a shared thunk
    // where each call recomputed it (and could keep a large result alive).
    if constant.iter().all(|c| c.is_none()) || constant.iter().all(|c| c.is_some()) {
        return;
    }
    let f = f.clone();
    // The function without those parameters, the global in their place.
    let rhs = &mut binds[0].1;
    let mut lams = Vec::new();
    let mut cur = take(rhs);
    while let Core::Lam(x, b) = cur {
        lams.push(x);
        cur = *b;
    }
    let mut inner = cur;
    for (x, k) in lams.iter().zip(&constant) {
        if let Some(g) = k {
            subst(&mut inner, x, &Core::Var(g.clone()));
        }
    }
    for (x, k) in lams.into_iter().zip(&constant).rev() {
        if k.is_none() {
            inner = Core::Lam(x, Box::new(inner));
        }
    }
    *rhs = inner;
    drop_args(rhs, &f, &constant);
    drop_args(body, &f, &constant);
}

/// The uses of `f` in `e` (see `specialise_let`).
fn calls_of(e: &Core, f: &str, out: &mut Vec<Option<Vec<Option<String>>>>) {
    match e {
        Core::Var(v) => {
            if v == f {
                out.push(None);
            }
        }
        Core::Lit(_) | Core::Fail(_) => {}
        Core::App(..) => {
            let mut args = Vec::new();
            let mut head = e;
            while let Core::App(g, a) = head {
                args.push(&**a);
                head = g;
            }
            args.reverse();
            match head {
                Core::Var(v) if v == f => {
                    out.push(Some(args.iter().map(|a| match a { Core::Var(x) => Some(x.clone()), _ => None }).collect()));
                }
                other => calls_of(other, f, out),
            }
            for a in args {
                calls_of(a, f, out);
            }
        }
        Core::Lam(_, b) => calls_of(b, f, out),
        Core::Let(binds, body) => {
            binds.iter().for_each(|(_, r)| calls_of(r, f, out));
            calls_of(body, f, out);
        }
        Core::Case(s, _, alts) => {
            calls_of(s, f, out);
            for a in alts {
                match a {
                    Alt::Con(_, _, b) | Alt::Lit(_, b) | Alt::Default(b) => calls_of(b, f, out),
                }
            }
        }
        Core::Con(_, args) | Core::Prim(_, args) => args.iter().for_each(|a| calls_of(a, f, out)),
    }
}

/// Removes, from every call of `f`, the arguments at the positions `constant` marks. A spine
/// whose head is `f` is one call: only its arguments are walked, not its partial applications.
fn drop_args(c: &mut Core, f: &str, constant: &[Option<String>]) {
    let is_call = {
        let mut head = &*c;
        while let Core::App(g, _) = head {
            head = g;
        }
        matches!(c, Core::App(..)) && matches!(head, Core::Var(v) if v == f)
    };
    if is_call {
        let mut args = Vec::new();
        let mut cur = take(c);
        while let Core::App(g, a) = cur {
            args.push(*a);
            cur = *g;
        }
        args.reverse();
        let mut e = cur;
        for (i, mut a) in args.into_iter().enumerate() {
            if constant.get(i).is_some_and(|k| k.is_some()) {
                continue;
            }
            drop_args(&mut a, f, constant);
            e = Core::App(Box::new(e), Box::new(a));
        }
        *c = e;
        return;
    }
    match c {
        Core::Var(_) | Core::Lit(_) | Core::Fail(_) => {}
        Core::App(g, a) => {
            drop_args(g, f, constant);
            drop_args(a, f, constant);
        }
        Core::Lam(_, b) => drop_args(b, f, constant),
        Core::Let(binds, body) => {
            binds.iter_mut().for_each(|(_, r)| drop_args(r, f, constant));
            drop_args(body, f, constant);
        }
        Core::Case(s, _, alts) => {
            drop_args(s, f, constant);
            for a in alts {
                match a {
                    Alt::Con(_, _, b) | Alt::Lit(_, b) | Alt::Default(b) => drop_args(b, f, constant),
                }
            }
        }
        Core::Con(_, args) | Core::Prim(_, args) => args.iter_mut().for_each(|a| drop_args(a, f, constant)),
    }
}

/// The let-bound locals of an expression whose right sides are values: lambdas, or globals
/// applied to atoms (partial applications), whose copies cost no repeated work.
fn let_values(e: &Core, arity: &HashMap<String, usize>, moving: &HashSet<String>) -> HashSet<String> {
    // A lambda, or a global applied to fewer arguments than it takes (a partial application:
    // no work), each argument a value itself or an atom that is not moving (whose work would
    // otherwise move along).
    fn value(e: &Core, arity: &HashMap<String, usize>, moving: &HashSet<String>) -> bool {
        match e {
            Core::Lam(..) => true,
            Core::App(..) => {
                let mut h = e;
                let mut n = 0;
                while let Core::App(f, a) = h {
                    let ok = (is_atom(a) && !matches!(&**a, Core::Var(v) if moving.contains(v.as_str())))
                        || (!is_atom(a) && value(a, arity, moving));
                    if !ok {
                        return false;
                    }
                    n += 1;
                    h = f;
                }
                matches!(h, Core::Var(v) if arity.get(v.as_str()).is_some_and(|k| *k > n))
            }
            _ => false,
        }
    }
    let is_value = |e: &Core| value(e, arity, moving);
    fn go(e: &Core, is_value: &dyn Fn(&Core) -> bool, out: &mut HashSet<String>) {
        match e {
            Core::Var(_) | Core::Lit(_) | Core::Fail(_) => {}
            Core::App(f, a) => {
                go(f, is_value, out);
                go(a, is_value, out);
            }
            Core::Lam(_, b) => go(b, is_value, out),
            Core::Let(binds, body) => {
                for (x, r) in binds {
                    if is_value(r) {
                        out.insert(x.clone());
                    }
                    go(r, is_value, out);
                }
                go(body, is_value, out);
            }
            Core::Case(s, _, alts) => {
                go(s, is_value, out);
                for a in alts {
                    match a {
                        Alt::Con(_, _, b) | Alt::Lit(_, b) | Alt::Default(b) => go(b, is_value, out),
                    }
                }
            }
            Core::Con(_, args) | Core::Prim(_, args) => args.iter().for_each(|a| go(a, is_value, out)),
        }
    }
    let mut out = HashSet::new();
    go(e, &is_value, &mut out);
    out
}

/// Whether the use of `x` in `e` is under no lambda but those of `build`'s or `augment`'s
/// argument (which run once): moving an expression there repeats no work.
fn one_shot_use(e: &Core, x: &str, under: bool) -> bool {
    match e {
        Core::Var(v) => v != x || !under,
        Core::Lit(_) | Core::Fail(_) => true,
        Core::App(f, a) => {
            let builder = matches!(&**f, Core::Var(g) if g == "GHC.Internal.Base.build" || g == "GHC.Internal.Base.augment")
                || matches!(&**f, Core::App(ff, _) if matches!(&**ff, Core::Var(g) if g == "GHC.Internal.Base.augment"));
            if builder {
                // The argument's own lambdas run once.
                let mut body = &**a;
                while let Core::Lam(_, b) = body {
                    body = b;
                }
                return one_shot_use(f, x, under) && one_shot_use(body, x, under);
            }
            one_shot_use(f, x, under) && one_shot_use(a, x, under)
        }
        Core::Lam(_, b) => one_shot_use(b, x, true),
        Core::Let(binds, body) => binds.iter().all(|(_, r)| one_shot_use(r, x, true)) && one_shot_use(body, x, under),
        Core::Case(s, _, alts) => {
            one_shot_use(s, x, under)
                && alts.iter().all(|a| match a {
                    Alt::Con(_, _, b) | Alt::Lit(_, b) | Alt::Default(b) => one_shot_use(b, x, under),
                })
        }
        Core::Con(_, args) | Core::Prim(_, args) => args.iter().all(|a| one_shot_use(a, x, true)),
    }
}

/// The occurrences of a variable in an expression.
fn occurrences_of(e: &Core, x: &str) -> usize {
    match e {
        Core::Var(v) => (v == x) as usize,
        Core::Lit(_) | Core::Fail(_) => 0,
        Core::App(f, a) => occurrences_of(f, x) + occurrences_of(a, x),
        Core::Lam(_, b) => occurrences_of(b, x),
        Core::Let(binds, body) => binds.iter().map(|(_, r)| occurrences_of(r, x)).sum::<usize>() + occurrences_of(body, x),
        Core::Case(s, _, alts) => {
            occurrences_of(s, x)
                + alts
                    .iter()
                    .map(|a| match a {
                        Alt::Con(_, _, b) | Alt::Lit(_, b) | Alt::Default(b) => occurrences_of(b, x),
                    })
                    .sum::<usize>()
        }
        Core::Con(_, args) | Core::Prim(_, args) => args.iter().map(|a| occurrences_of(a, x)).sum(),
    }
}

/// `GHC.Magic.oneShot`: marks a lambda called at most once (never inlined, so the marker
/// stays for `into_one_shot`; removed after the last pass).
const ONE_SHOT: &str = "GHC.Magic.oneShot";

/// The lambda of `oneShot (\x -> e)`, if `e` is that.
fn one_shot_lambda(e: &Core) -> Option<&Core> {
    match e {
        Core::App(f, a) if matches!(&**f, Core::Var(v) if v == ONE_SHOT) && matches!(**a, Core::Lam(..)) => Some(a),
        _ => None,
    }
}

/// The number of parameters of a function, counting those of lambdas under `oneShot`.
fn value_arity(mut e: &Core) -> usize {
    let mut n = 0;
    loop {
        if let Some(l) = one_shot_lambda(e) {
            e = l;
        }
        let Core::Lam(_, b) = e else { return n };
        n += 1;
        e = b;
    }
}

/// The let-bound lambdas of an expression, with their numbers of parameters.
fn local_arities(e: &Core, out: &mut HashMap<String, usize>) {
    match e {
        Core::Var(_) | Core::Lit(_) | Core::Fail(_) => {}
        Core::App(f, a) => {
            local_arities(f, out);
            local_arities(a, out);
        }
        Core::Lam(_, b) => local_arities(b, out),
        Core::Let(binds, body) => {
            for (x, r) in binds {
                let n = value_arity(r);
                if n > 0 {
                    out.insert(x.clone(), n);
                }
                local_arities(r, out);
            }
            local_arities(body, out);
        }
        Core::Case(s, _, alts) => {
            local_arities(s, out);
            for a in alts {
                match a {
                    Alt::Con(_, _, b) | Alt::Lit(_, b) | Alt::Default(b) => local_arities(b, out),
                }
            }
        }
        Core::Con(_, args) | Core::Prim(_, args) => args.iter().for_each(|a| local_arities(a, out)),
    }
}

/// The largest outer case body copied to each result of an inner case, and the most results.
const PUSH_SIZE: usize = 30;
const PUSH_RESULTS: usize = 4;

/// The results of an expression (the ends of its cases and lets), counted until past `limit`.
fn results(e: &Core, limit: usize) -> usize {
    match e {
        Core::Case(_, _, alts) => {
            let mut n = 0;
            for a in alts {
                let (Alt::Con(_, _, b) | Alt::Lit(_, b) | Alt::Default(b)) = a;
                n += results(b, limit - n.min(limit));
                if n > limit {
                    break;
                }
            }
            n
        }
        Core::Let(_, b) => results(b, limit),
        Core::Fail(_) => 0,
        _ => 1,
    }
}

/// Applies `f` at each result of an expression (failures are left).
fn at_results(e: &mut Core, f: &mut dyn FnMut(&mut Core)) {
    match e {
        Core::Case(_, _, alts) => {
            for a in alts {
                match a {
                    Alt::Con(_, _, b) | Alt::Lit(_, b) | Alt::Default(b) => at_results(b, f),
                }
            }
        }
        Core::Let(_, b) => at_results(b, f),
        Core::Fail(_) => {}
        _ => f(e),
    }
}

/// The rewrites a binding may take in one round (rules could otherwise rewrite each other's
/// results forever).
const RULE_BUDGET: usize = 256;

/// Whether a top-level binding is a rewrite rule (see `rename::RULE_PREFIX`).
fn is_rule_binding(name: &str) -> bool {
    name.contains(".$rule%")
}

/// When a rule is active, in the phases 2, 1, 0 (see `syntax::ast::Activation`).
#[derive(Clone, Copy, Debug, PartialEq)]
enum Act {
    Always,
    From(u32),
    Before(u32),
    Never,
}

/// A rewrite rule: `head args = rhs`, with its variables (free in `args` and `rhs`).
#[derive(Clone, Debug)]
struct RuleDef {
    name: String,
    act: Act,
    vars: Vec<String>,
    head: String,
    args: Vec<Core>,
    rhs: Core,
}

impl RuleDef {
    fn active(&self, phase: u32) -> bool {
        self.act.active(phase)
    }
}

impl Act {
    fn active(self, phase: u32) -> bool {
        match self {
            Act::Always => true,
            Act::From(n) => phase <= n,
            Act::Before(n) => phase > n,
            Act::Never => false,
        }
    }

    /// The activation a marker name ends with (see `rename::activation_code`).
    fn parse(code: &str) -> Option<Act> {
        match code {
            "A" => Some(Act::Always),
            "N" => Some(Act::Never),
            t if t.starts_with('F') => t[1..].parse().ok().map(Act::From),
            t if t.starts_with('B') => t[1..].parse().ok().map(Act::Before),
            _ => None,
        }
    }
}

/// The program's INLINE and NOINLINE pragmas, by function: INLINE (true) with the phases it
/// inlines in, or NOINLINE with the phases the function may be inlined in as usual (from the
/// marker bindings `$rule%inl<i>%<target>%<kind>%<activation>`, see `rename::inline_binding`).
fn extract_inlines(binds: &[TopBind]) -> HashMap<String, (bool, Act)> {
    let mut out = HashMap::new();
    for b in binds {
        let Some((_, rest)) = b.name.split_once(".$rule%inl") else { continue };
        let mut parts = rest.rsplitn(3, '%');
        let (Some(act), Some(kind), Some(target)) = (parts.next(), parts.next(), parts.next()) else { continue };
        let Some(target) = target.split_once('%').map(|(_, t)| t) else { continue };
        let Some(act) = Act::parse(act) else { continue };
        match kind {
            "I" => out.insert(target.to_string(), (true, act)),
            // A plain NOINLINE (written without phases) is never inlined.
            "N" => out.insert(target.to_string(), (false, if act == Act::Always { Act::Never } else { act })),
            _ => None,
        };
    }
    out
}

/// The rules by the function their left sides apply, and every global those sides mention.
struct Rules {
    by_head: HashMap<String, Vec<RuleDef>>,
    protected: HashSet<String>,
    /// INLINE and NOINLINE pragmas (see `extract_inlines`).
    inlines: HashMap<String, (bool, Act)>,
}

impl Rules {
    fn new(mut rules: Vec<RuleDef>) -> Rules {
        rules.extend(builtin_rules());
        let mut by_head: HashMap<String, Vec<RuleDef>> = HashMap::new();
        let mut protected = HashSet::new();
        for r in rules {
            protected.insert(r.head.clone());
            for a in &r.args {
                globals_in(a, &r.vars, &mut protected);
            }
            by_head.entry(r.head.clone()).or_default().push(r);
        }
        Rules { by_head, protected, inlines: HashMap::new() }
    }
}

/// The global variables an expression mentions (those not among `locals`, nor bound in it).
fn globals_in(e: &Core, locals: &[String], out: &mut HashSet<String>) {
    match e {
        Core::Var(v) => {
            if v.contains('.') && !locals.contains(v) {
                out.insert(v.clone());
            }
        }
        Core::Lit(_) | Core::Fail(_) => {}
        Core::App(f, a) => {
            globals_in(f, locals, out);
            globals_in(a, locals, out);
        }
        Core::Lam(_, b) => globals_in(b, locals, out),
        Core::Let(binds, body) => {
            binds.iter().for_each(|(_, r)| globals_in(r, locals, out));
            globals_in(body, locals, out);
        }
        Core::Case(s, _, alts) => {
            globals_in(s, locals, out);
            for a in alts {
                match a {
                    Alt::Con(_, _, b) | Alt::Lit(_, b) | Alt::Default(b) => globals_in(b, locals, out),
                }
            }
        }
        Core::Con(_, args) | Core::Prim(_, args) => args.iter().for_each(|a| globals_in(a, locals, out)),
    }
}

/// The rules among the bindings: `\vars -> [lhs, rhs]` (the parameters' alias lets resolved),
/// with a left side that applies a global.
fn extract_rules(binds: &[TopBind]) -> Vec<RuleDef> {
    let mut out = Vec::new();
    for b in binds {
        if !is_rule_binding(&b.name) {
            continue;
        }
        let act = match b.name.rsplit('%').next() {
            Some("A") => Act::Always,
            Some("N") => Act::Never,
            Some(t) if t.starts_with('F') => t[1..].parse().map(Act::From).unwrap_or(Act::Never),
            Some(t) if t.starts_with('B') => t[1..].parse().map(Act::Before).unwrap_or(Act::Never),
            _ => continue,
        };
        let mut vars = Vec::new();
        let mut cur = b.body.clone();
        while let Core::Lam(x, inner) = cur {
            vars.push(x);
            cur = *inner;
        }
        // `let k = arg in ..`: the clause's names for its parameters.
        while let Core::Let(bs, body) = &cur {
            match &bs[..] {
                [(x, Core::Var(y))] if vars.contains(y) => {
                    let (x, y) = (x.clone(), y.clone());
                    let Core::Let(_, mut body) = cur else { unreachable!() };
                    subst(&mut body, &x, &Core::Var(y));
                    cur = *body;
                }
                _ => break,
            }
        }
        let Core::Con(cons, sides) = &cur else { continue };
        let (lhs, rhs) = match (cons.as_str(), &sides[..]) {
            ("Builtin.:", [lhs, rest]) => match rest {
                Core::Con(c2, more) if c2 == "Builtin.:" && more.len() == 2 => (lhs.clone(), more[0].clone()),
                _ => continue,
            },
            _ => continue,
        };
        let mut args = Vec::new();
        let mut h = &lhs;
        while let Core::App(f, a) = h {
            args.push((**a).clone());
            h = f;
        }
        args.reverse();
        let Core::Var(head) = h else { continue };
        if vars.contains(head) || args.is_empty() {
            continue;
        }
        out.push(RuleDef { name: b.name.clone(), act, vars, head: head.clone(), args, rhs });
    }
    out
}

/// "fold/build" and "foldr/augment", whose `g` has a rank-2 type the checker cannot give a rule
/// binder: built in, as base states them.
fn builtin_rules() -> Vec<RuleDef> {
    let v = |n: &str| Core::Var(n.to_string());
    let app = |f: Core, a: Core| Core::App(Box::new(f), Box::new(a));
    let foldr = "GHC.Internal.Base.foldr";
    vec![
        RuleDef {
            name: "fold/build".into(),
            act: Act::Always,
            vars: vec!["k%rule".into(), "z%rule".into(), "g%rule".into()],
            head: foldr.into(),
            args: vec![v("k%rule"), v("z%rule"), app(v("GHC.Internal.Base.build"), v("g%rule"))],
            rhs: app(app(v("g%rule"), v("k%rule")), v("z%rule")),
        },
        RuleDef {
            name: "foldr/augment".into(),
            act: Act::Always,
            vars: vec!["k%rule".into(), "z%rule".into(), "g%rule".into(), "xs%rule".into()],
            head: foldr.into(),
            args: vec![v("k%rule"), v("z%rule"), app(app(v("GHC.Internal.Base.augment"), v("g%rule")), v("xs%rule"))],
            rhs: app(app(v("g%rule"), v("k%rule")), app(app(app(v(foldr), v("k%rule")), v("z%rule")), v("xs%rule"))),
        },
        // GHC.List's "foldr2/left" (its `g` has a rank-2 type, which a rule binding cannot
        // check): `zip [1 ..] ys` and the like fuse with their left list.
        RuleDef {
            name: "foldr2/left".into(),
            act: Act::Always,
            vars: vec!["k%rule".into(), "z%rule".into(), "g%rule".into(), "ys%rule".into()],
            head: "GHC.Internal.List.foldr2".into(),
            args: vec![v("k%rule"), v("z%rule"), app(v("GHC.Internal.Base.build"), v("g%rule")), v("ys%rule")],
            rhs: app(
                app(
                    app(v("g%rule"), app(app(v("GHC.Internal.List.foldr2_left"), v("k%rule")), v("z%rule"))),
                    Core::Lam("_%rule".into(), Box::new(v("z%rule"))),
                ),
                v("ys%rule"),
            ),
        },
    ]
}

/// Matches a rule's argument pattern against an expression: the rule's variables match
/// anything (the same expression at each occurrence), lambdas up to the names they bind.
fn match_core(
    p: &Core,
    e: &Core,
    vars: &HashSet<&str>,
    sub: &mut HashMap<String, Core>,
    bound: &mut Vec<(String, String)>,
) -> bool {
    match (p, e) {
        (Core::Var(v), _) if vars.contains(v.as_str()) => {
            // An expression mentioning a variable the pattern's lambdas bind cannot be a match.
            if bound.iter().any(|(_, ev)| mentions(e, ev)) {
                return false;
            }
            match sub.get(v.as_str()) {
                Some(prev) => prev == e,
                None => {
                    sub.insert(v.clone(), e.clone());
                    true
                }
            }
        }
        (Core::Var(v), Core::Var(w)) => match bound.iter().rev().find(|(pv, _)| pv == v) {
            Some((_, ev)) => ev == w,
            None => v == w && !bound.iter().any(|(_, ev)| ev == w),
        },
        (Core::Lit(a), Core::Lit(b)) => a == b,
        (Core::App(f1, a1), Core::App(f2, a2)) => match_core(f1, f2, vars, sub, bound) && match_core(a1, a2, vars, sub, bound),
        (Core::Lam(x, b1), Core::Lam(y, b2)) => {
            bound.push((x.clone(), y.clone()));
            let ok = match_core(b1, b2, vars, sub, bound);
            bound.pop();
            ok
        }
        (Core::Con(c1, a1), Core::Con(c2, a2)) | (Core::Prim(c1, a1), Core::Prim(c2, a2)) => {
            c1 == c2 && a1.len() == a2.len() && a1.iter().zip(a2).all(|(x, y)| match_core(x, y, vars, sub, bound))
        }
        _ => false,
    }
}

/// The value of an `Int#` primitive applied to literals (as the runtime computes it).
fn fold(p: &str, args: &[Core]) -> Option<i64> {
    let lit = |k: usize| match args.get(k) {
        Some(Core::Lit(Lit::Int(i))) => Some(*i),
        _ => None,
    };
    let a = lit(0)?;
    if args.len() == 1 {
        return match p {
            "negateInt#" => Some(a.wrapping_neg()),
            "notI#" => Some(!a),
            _ => None,
        };
    }
    let b = lit(1)?;
    if args.len() != 2 {
        return None;
    }
    Some(match p {
        "+#" => a.wrapping_add(b),
        "-#" => a.wrapping_sub(b),
        "*#" => a.wrapping_mul(b),
        "quotInt#" if b != 0 => a.wrapping_div(b),
        "remInt#" if b != 0 => a.wrapping_rem(b),
        "andI#" => a & b,
        "orI#" => a | b,
        "xorI#" => a ^ b,
        "==#" => (a == b) as i64,
        "/=#" => (a != b) as i64,
        "<#" => (a < b) as i64,
        "plusWord#" => (a as u64).wrapping_add(b as u64) as i64,
        "minusWord#" => (a as u64).wrapping_sub(b as u64) as i64,
        "timesWord#" => (a as u64).wrapping_mul(b as u64) as i64,
        "and#" => a & b,
        "or#" => a | b,
        "xor#" => a ^ b,
        "eqWord#" => (a == b) as i64,
        "neWord#" => (a != b) as i64,
        "ltWord#" => ((a as u64) < (b as u64)) as i64,
        "leWord#" => ((a as u64) <= (b as u64)) as i64,
        "gtWord#" => ((a as u64) > (b as u64)) as i64,
        "geWord#" => ((a as u64) >= (b as u64)) as i64,
        "<=#" => (a <= b) as i64,
        ">#" => (a > b) as i64,
        ">=#" => (a >= b) as i64,
        _ => return None,
    })
}

/// A primitive without effects that cannot fail, on atoms: its result unused, it can go.
fn is_total_prim(p: &str, args: &[Core]) -> bool {
    const TOTAL: &[&str] = &[
        "+#", "-#", "*#", "andI#", "orI#", "xorI#", "notI#", "negateInt#", "==#", "/=#", "<#", "<=#", ">#", ">=#",
        "plusWord#", "minusWord#", "timesWord#", "and#", "or#", "xor#", "not#", "eqWord#", "neWord#", "ltWord#",
        "leWord#", "gtWord#", "geWord#", "eqChar#", "neChar#", "ltChar#", "leChar#", "gtChar#", "geChar#", "ord#",
        "int2Word#", "word2Int#", "uncheckedIShiftL#", "uncheckedIShiftRA#", "uncheckedIShiftRL#",
        "uncheckedShiftL#", "uncheckedShiftRL#",
    ];
    let divides = matches!(p, "quotInt#" | "remInt#")
        && matches!(args.get(1), Some(Core::Lit(Lit::Int(d))) if *d != 0 && *d != -1);
    (TOTAL.contains(&p) || divides) && args.iter().all(is_atom)
}

/// An `Int#` or `Word#` primitive on atoms that answers one of them, or a literal, whatever
/// the other is: `x +# 0#` is `x`, `andI# x 0#` is `0#`, `orI# x x` is `x`.
fn identity(p: &str, args: &[Core]) -> Option<Core> {
    let [a, b] = args else { return None };
    let lit = |c: &Core| match c {
        Core::Lit(Lit::Int(i)) => Some(*i),
        _ => None,
    };
    let same = matches!((a, b), (Core::Var(x), Core::Var(y)) if x == y);
    // `x op k`, either way round for the commutative ones.
    let with = |k: i64, commutes: bool| -> Option<&Core> {
        if lit(b) == Some(k) {
            Some(a)
        } else if commutes && lit(a) == Some(k) {
            Some(b)
        } else {
            None
        }
    };
    let zero = Core::Lit(Lit::Int(0));
    match p {
        "+#" | "plusWord#" | "orI#" | "or#" | "xorI#" | "xor#" if with(0, true).is_some() => with(0, true).cloned(),
        "-#" | "minusWord#" | "uncheckedIShiftL#" | "uncheckedIShiftRA#" | "uncheckedIShiftRL#" | "uncheckedShiftL#" | "uncheckedShiftRL#"
            if with(0, false).is_some() =>
        {
            with(0, false).cloned()
        }
        "*#" | "timesWord#" if with(1, true).is_some() => with(1, true).cloned(),
        "*#" | "timesWord#" | "andI#" | "and#" if with(0, true).is_some() => Some(zero),
        "andI#" | "and#" if with(-1, true).is_some() => with(-1, true).cloned(),
        "orI#" | "or#" if with(-1, true).is_some() => Some(Core::Lit(Lit::Int(-1))),
        "andI#" | "and#" | "orI#" | "or#" if same => Some(a.clone()),
        "xorI#" | "xor#" | "-#" | "minusWord#" if same => Some(zero),
        _ => None,
    }
}

/// A variable or a number or character literal: passed and substituted as it is.
fn is_atom(c: &Core) -> bool {
    matches!(c, Core::Var(_) | Core::Lit(Lit::Int(_) | Lit::Char(_) | Lit::Double(_)))
}

fn take(c: &mut Core) -> Core {
    std::mem::replace(c, Core::Fail(String::new()))
}

fn alt_body(a: Alt) -> Core {
    match a {
        Alt::Con(_, _, b) | Alt::Lit(_, b) | Alt::Default(b) => b,
    }
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

/// The number of nodes of an expression, counting up to `limit`.
fn size(e: &Core, limit: usize) -> usize {
    fn go(e: &Core, n: &mut usize, limit: usize) {
        *n += 1;
        if *n > limit {
            return;
        }
        match e {
            Core::Var(_) | Core::Lit(_) | Core::Fail(_) => {}
            Core::App(f, a) => {
                go(f, n, limit);
                go(a, n, limit);
            }
            Core::Lam(_, b) => go(b, n, limit),
            Core::Let(binds, body) => {
                binds.iter().for_each(|(_, r)| go(r, n, limit));
                go(body, n, limit);
            }
            Core::Case(s, _, alts) => {
                go(s, n, limit);
                alts.iter().for_each(|a| match a {
                    Alt::Con(_, _, b) | Alt::Lit(_, b) | Alt::Default(b) => go(b, n, limit),
                });
            }
            Core::Con(_, args) | Core::Prim(_, args) => args.iter().for_each(|a| go(a, n, limit)),
        }
    }
    let mut n = 0;
    go(e, &mut n, limit);
    n
}

/// Whether a variable occurs in an expression (shadowing ignored: a conservative answer).
fn mentions(e: &Core, x: &str) -> bool {
    match e {
        Core::Var(v) => v == x,
        Core::Lit(_) | Core::Fail(_) => false,
        Core::App(f, a) => mentions(f, x) || mentions(a, x),
        Core::Lam(_, b) => mentions(b, x),
        Core::Let(binds, body) => binds.iter().any(|(_, r)| mentions(r, x)) || mentions(body, x),
        Core::Case(s, _, alts) => {
            mentions(s, x)
                || alts.iter().any(|a| match a {
                    Alt::Con(_, _, b) | Alt::Lit(_, b) | Alt::Default(b) => mentions(b, x),
                })
        }
        Core::Con(_, args) | Core::Prim(_, args) => args.iter().any(|a| mentions(a, x)),
    }
}

/// Replaces the occurrences of a (uniquely bound) local by an atom.
fn subst(c: &mut Core, x: &str, by: &Core) {
    match c {
        Core::Var(v) => {
            if v == x {
                *c = by.clone();
            }
        }
        Core::Lit(_) | Core::Fail(_) => {}
        Core::App(f, a) => {
            subst(f, x, by);
            subst(a, x, by);
        }
        Core::Lam(_, b) => subst(b, x, by),
        Core::Let(binds, body) => {
            binds.iter_mut().for_each(|(_, r)| subst(r, x, by));
            subst(body, x, by);
        }
        Core::Case(s, _, alts) => {
            subst(s, x, by);
            for a in alts {
                match a {
                    Alt::Con(_, _, b) | Alt::Lit(_, b) | Alt::Default(b) => subst(b, x, by),
                }
            }
        }
        Core::Con(_, args) | Core::Prim(_, args) => args.iter_mut().for_each(|a| subst(a, x, by)),
    }
}

/// Renames every binder of a closed expression (with unique binders) by adding a suffix, and
/// its occurrences with it.
fn freshen(c: &mut Core, suffix: &str, map: &mut HashMap<String, String>) {
    let rename = |x: &mut String, map: &mut HashMap<String, String>| {
        let new = format!("{x}{suffix}");
        map.insert(x.clone(), new.clone());
        *x = new;
    };
    match c {
        Core::Var(v) => {
            if let Some(n) = map.get(v.as_str()) {
                *v = n.clone();
            }
        }
        Core::Lit(_) | Core::Fail(_) => {}
        Core::App(f, a) => {
            freshen(f, suffix, map);
            freshen(a, suffix, map);
        }
        Core::Lam(x, b) => {
            rename(x, map);
            freshen(b, suffix, map);
        }
        Core::Let(binds, body) => {
            for (x, _) in binds.iter_mut() {
                rename(x, map);
            }
            binds.iter_mut().for_each(|(_, r)| freshen(r, suffix, map));
            freshen(body, suffix, map);
        }
        Core::Case(s, b, alts) => {
            freshen(s, suffix, map);
            rename(b, map);
            for a in alts {
                match a {
                    Alt::Con(_, vars, body) => {
                        vars.iter_mut().for_each(|v| rename(v, map));
                        freshen(body, suffix, map);
                    }
                    Alt::Lit(_, body) | Alt::Default(body) => freshen(body, suffix, map),
                }
            }
        }
        Core::Con(_, args) | Core::Prim(_, args) => args.iter_mut().for_each(|a| freshen(a, suffix, map)),
    }
}

/// Renames binders apart: a binder whose name was already bound in the body (or is a global)
/// gets a fresh name in its scope. `env` maps the names renamed in the current scope.
fn uniquify(c: &mut Core, globals: &HashSet<String>, seen: &mut HashSet<String>, env: &mut HashMap<String, String>, fresh: &mut u64) {
    // Binds a name: answers the previous mapping, to restore at the end of the scope.
    fn bind(x: &mut String, globals: &HashSet<String>, seen: &mut HashSet<String>, env: &mut HashMap<String, String>, fresh: &mut u64) -> (String, Option<String>) {
        let old = x.clone();
        let prev = if globals.contains(x.as_str()) || !seen.insert(x.clone()) {
            *fresh += 1;
            let new = format!("{x}~u{fresh}");
            seen.insert(new.clone());
            *x = new.clone();
            env.insert(old.clone(), new)
        } else {
            env.remove(&old)
        };
        (old, prev)
    }
    fn restore(env: &mut HashMap<String, String>, saved: Vec<(String, Option<String>)>) {
        for (old, prev) in saved.into_iter().rev() {
            match prev {
                Some(p) => env.insert(old, p),
                None => env.remove(&old),
            };
        }
    }
    match c {
        Core::Var(v) => {
            if let Some(n) = env.get(v.as_str()) {
                *v = n.clone();
            }
        }
        Core::Lit(_) | Core::Fail(_) => {}
        Core::App(f, a) => {
            uniquify(f, globals, seen, env, fresh);
            uniquify(a, globals, seen, env, fresh);
        }
        Core::Lam(x, b) => {
            let saved = vec![bind(x, globals, seen, env, fresh)];
            uniquify(b, globals, seen, env, fresh);
            restore(env, saved);
        }
        Core::Let(binds, body) => {
            let saved: Vec<_> = binds.iter_mut().map(|(x, _)| bind(x, globals, seen, env, fresh)).collect();
            binds.iter_mut().for_each(|(_, r)| uniquify(r, globals, seen, env, fresh));
            uniquify(body, globals, seen, env, fresh);
            restore(env, saved);
        }
        Core::Case(s, b, alts) => {
            uniquify(s, globals, seen, env, fresh);
            let saved = vec![bind(b, globals, seen, env, fresh)];
            for a in alts.iter_mut() {
                match a {
                    Alt::Con(_, vars, body) => {
                        let inner: Vec<_> = vars.iter_mut().map(|v| bind(v, globals, seen, env, fresh)).collect();
                        uniquify(body, globals, seen, env, fresh);
                        restore(env, inner);
                    }
                    Alt::Lit(_, body) | Alt::Default(body) => uniquify(body, globals, seen, env, fresh),
                }
            }
            restore(env, saved);
        }
        Core::Con(_, args) | Core::Prim(_, args) => args.iter_mut().for_each(|a| uniquify(a, globals, seen, env, fresh)),
    }
}

#[derive(Default)]
struct Occ {
    count: u32,
    under_lambda: bool,
    recursive: bool,
}

fn unique_binders(c: &Core, globals: &HashSet<String>, names: &mut HashSet<String>) -> bool {
    fn bind(name: &String, globals: &HashSet<String>, names: &mut HashSet<String>) -> bool {
        !globals.contains(name) && names.insert(name.clone())
    }
    match c {
        Core::Var(_) | Core::Lit(_) | Core::Fail(_) => true,
        Core::App(f, a) => unique_binders(f, globals, names) && unique_binders(a, globals, names),
        Core::Lam(name, body) => bind(name, globals, names) && unique_binders(body, globals, names),
        Core::Let(bindings, body) => {
            bindings.iter().all(|(name, _)| bind(name, globals, names))
                && bindings.iter().all(|(_, rhs)| unique_binders(rhs, globals, names))
                && unique_binders(body, globals, names)
        }
        Core::Case(scrut, binder, alts) => {
            unique_binders(scrut, globals, names)
                && bind(binder, globals, names)
                && alts.iter().all(|alt| match alt {
                    Alt::Con(_, vars, body) => {
                        vars.iter().all(|v| bind(v, globals, names)) && unique_binders(body, globals, names)
                    }
                    Alt::Lit(_, body) | Alt::Default(body) => unique_binders(body, globals, names),
                })
        }
        Core::Con(_, args) | Core::Prim(_, args) => args.iter().all(|a| unique_binders(a, globals, names)),
    }
}

/// Counts the uses of each let-bound local. `depth` maps a let-bound name in scope to the lambda
/// depth of its binding, and whether its own group's right-hand sides are being walked.
fn occurrences(c: &Core, lam: i32, depth: &mut HashMap<String, (i32, bool)>, occ: &mut HashMap<String, Occ>) {
    match c {
        Core::Var(v) => {
            if let Some(&(d, in_rhs)) = depth.get(v) {
                let o = occ.entry(v.clone()).or_default();
                o.count += 1;
                o.under_lambda |= lam > d;
                o.recursive |= in_rhs;
            }
        }
        Core::Lit(_) | Core::Fail(_) => {}
        Core::App(f, a) => {
            occurrences(f, lam, depth, occ);
            occurrences(a, lam, depth, occ);
        }
        Core::Lam(_, b) => occurrences(b, lam + 1, depth, occ),
        Core::Let(bindings, body) => {
            for (name, _) in bindings {
                depth.insert(name.clone(), (lam, false));
                occ.entry(name.clone()).or_default();
            }
            for (name, rhs) in bindings {
                depth.insert(name.clone(), (lam, true));
                occurrences(rhs, lam, depth, occ);
                depth.insert(name.clone(), (lam, false));
            }
            occurrences(body, lam, depth, occ);
        }
        Core::Case(s, _, alts) => {
            occurrences(s, lam, depth, occ);
            for a in alts {
                match a {
                    Alt::Con(_, _, b) | Alt::Lit(_, b) | Alt::Default(b) => occurrences(b, lam, depth, occ),
                }
            }
        }
        Core::Con(_, args) | Core::Prim(_, args) => args.iter().for_each(|a| occurrences(a, lam, depth, occ)),
    }
}

/// Strictness of global functions, and calls by value: a parameter is strict when every
/// alternative of the function's body that returns evaluates it. A pattern-match failure is
/// bottom (it constrains nothing, as GHC takes `patError`), so an argument may be evaluated
/// before a call that would fail; calls of functions that raise count as evaluating nothing.
/// The analysis starts with every parameter strict and weakens to a fixpoint. A saturated
/// call's non-atomic argument for a strict parameter is then evaluated before the call
/// (`case a as v of _ -> f .. v ..`) instead of being allocated as a thunk.
/// Answers the bindings it changed.
fn call_by_value(binds: &mut [TopBind]) -> HashSet<String> {
    let mut strict: HashMap<String, Vec<bool>> = HashMap::new();
    for b in binds.iter() {
        let (n, _) = lambdas(&b.body);
        if n > 0 {
            strict.insert(b.name.clone(), vec![true; n]);
        }
    }
    // Marks only weaken, so this ends; stopping early would keep unproven marks.
    loop {
        let mut changed = false;
        for b in binds.iter() {
            let Some(old) = strict.get(&b.name) else { continue };
            let mut params = Vec::new();
            let mut cur = &b.body;
            while let Core::Lam(x, inner) = cur {
                params.push(x.as_str());
                cur = inner;
            }
            let forced = forced(cur, &strict);
            let new: Vec<bool> = params.iter().zip(old).map(|(p, o)| *o && forced.contains(*p)).collect();
            if &new != old {
                strict.insert(b.name.clone(), new);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    strict.retain(|_, s| s.iter().any(|x| *x));
    let mut fresh = FRESH.load(std::sync::atomic::Ordering::Relaxed);
    let mut changed = HashSet::new();
    for b in binds.iter_mut() {
        // The body's local functions (this relies on its binders being unique, as `uniquify`
        // makes them; a function name bound twice, or named like a global, is left out): their strictness, with the globals', to a fixpoint.
        let mut locals = Vec::new();
        local_functions(&b.body, &mut locals);
        let mut count: HashMap<&str, usize> = HashMap::new();
        for (x, _) in &locals {
            *count.entry(x).or_default() += 1;
        }
        locals.retain(|(x, _)| count[x] == 1 && !strict.contains_key(*x));
        let names: Vec<String> = locals.iter().map(|(x, _)| x.to_string()).collect();
        for (x, r) in &locals {
            strict.insert(x.to_string(), vec![true; params(r).0.len()]);
        }
        loop {
            let mut changed = false;
            for (x, r) in &locals {
                let (ps, body) = params(r);
                let forced = forced(body, &strict);
                let old = &strict[*x];
                let new: Vec<bool> = ps.iter().zip(old).map(|(p, o)| *o && forced.contains(*p)).collect();
                if &new != old {
                    strict.insert(x.to_string(), new);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        for x in &names {
            if !strict[x].iter().any(|s| *s) {
                strict.remove(x);
            }
        }
        let before = fresh;
        by_value(&mut b.body, &strict, &mut fresh);
        if fresh != before {
            changed.insert(b.name.clone());
        }
        for x in &names {
            strict.remove(x);
        }
    }
    FRESH.store(fresh, std::sync::atomic::Ordering::Relaxed);
    changed
}

/// The let-bound functions of an expression, with their right sides.
fn local_functions<'a>(e: &'a Core, out: &mut Vec<(&'a str, &'a Core)>) {
    match e {
        Core::Var(_) | Core::Lit(_) | Core::Fail(_) => {}
        Core::App(f, a) => {
            local_functions(f, out);
            local_functions(a, out);
        }
        Core::Lam(_, b) => local_functions(b, out),
        Core::Let(binds, body) => {
            for (x, r) in binds {
                if value_arity(r) > 0 {
                    out.push((x, r));
                }
                local_functions(r, out);
            }
            local_functions(body, out);
        }
        Core::Case(s, _, alts) => {
            local_functions(s, out);
            for a in alts {
                match a {
                    Alt::Con(_, _, b) | Alt::Lit(_, b) | Alt::Default(b) => local_functions(b, out),
                }
            }
        }
        Core::Con(_, args) | Core::Prim(_, args) => args.iter().for_each(|a| local_functions(a, out)),
    }
}

/// A function's parameters (those of lambdas under `oneShot` too) and its body.
fn params(mut e: &Core) -> (Vec<&str>, &Core) {
    let mut ps = Vec::new();
    loop {
        if let Some(l) = one_shot_lambda(e) {
            e = l;
        }
        let Core::Lam(x, b) = e else { return (ps, e) };
        ps.push(x.as_str());
        e = b;
    }
}

/// The variables an expression certainly evaluates when its value is demanded.
fn forced(e: &Core, strict: &HashMap<String, Vec<bool>>) -> HashSet<String> {
    forced_d(e, strict, true).0
}

/// The variables an expression certainly evaluates when its value is demanded (failures are
/// bottom, calls of unknown functions evaluate only the function): for the native back end's
/// choice of parameters to pass unboxed.
pub fn certainly_evaluated(e: &Core) -> HashSet<String> {
    forced(e, &HashMap::new())
}

/// What `forced` answers when a failing alternative counts as evaluating nothing: used where
/// an earlier evaluation should not even precede a pattern failure (`strict_let`).
fn forced_total(e: &Core) -> HashSet<String> {
    forced_d(e, &HashMap::new(), false).0
}

/// What `forced` answers, and whether the expression certainly fails (a pattern-match failure
/// is bottom, as GHC takes it: a failing alternative constrains nothing). Calls of functions that
/// raise are not counted as failing.
fn forced_d(e: &Core, strict: &HashMap<String, Vec<bool>>, bottom: bool) -> (HashSet<String>, bool) {
    let mut out = HashSet::new();
    let mut fails = false;
    match e {
        Core::Var(v) => {
            out.insert(v.clone());
        }
        Core::Fail(_) => fails = bottom,
        Core::Lit(_) | Core::Lam(..) | Core::Con(..) => {}
        Core::App(..) => {
            let mut args = Vec::new();
            let mut head = e;
            while let Core::App(f, a) = head {
                args.push(&**a);
                head = f;
            }
            args.reverse();
            if let Core::Var(f) = head {
                out.insert(f.clone());
                if let Some(s) = strict.get(f) {
                    if args.len() >= s.len() {
                        for (a, st) in args.iter().zip(s) {
                            if *st {
                                out.extend(forced(a, strict));
                            }
                        }
                    }
                }
            }
        }
        Core::Let(binds, body) => {
            (out, fails) = forced_d(body, strict, bottom);
            for (x, _) in binds {
                out.remove(x);
            }
        }
        // After an effect (a primitive with one, or an application answering a pair, as IO
        // actions do), what the continuation evaluates is not certain to be evaluated: it may
        // raise or loop first. Only the scrutinee counts (GHC's "IO hack").
        Core::Case(s, _, alts)
            if matches!(&**s, Core::Prim(p, _) if !STRICT_PRIMS.contains(&p.as_str()))
                || (matches!(&**s, Core::App(..))
                    && alts.iter().any(|a| matches!(a, Alt::Con(k, _, _) if k == "Builtin.(,)"))) =>
        {
            (out, fails) = forced_d(s, strict, bottom);
        }
        Core::Case(s, b, alts) => {
            let mut common: Option<HashSet<String>> = None;
            for a in alts {
                let (vars, body): (&[String], &Core) = match a {
                    Alt::Con(_, vars, body) => (vars, body),
                    Alt::Lit(_, body) | Alt::Default(body) => (&[], body),
                };
                let (mut f, alt_fails) = forced_d(body, strict, bottom);
                if alt_fails {
                    continue;
                }
                for v in vars {
                    f.remove(v);
                }
                common = Some(match common {
                    None => f,
                    Some(c) => c.intersection(&f).cloned().collect(),
                });
            }
            // Every alternative fails: so does the case.
            fails = common.is_none() && !alts.is_empty();
            out = common.unwrap_or_default();
            out.remove(b);
            let (sf, s_fails) = forced_d(s, strict, bottom);
            out.extend(sf);
            fails |= s_fails;
        }
        Core::Prim(p, args) if p == "seq" && args.len() == 2 => {
            let (a, fa) = forced_d(&args[0], strict, bottom);
            let (b, fb) = forced_d(&args[1], strict, bottom);
            out = a;
            out.extend(b);
            fails = fa || fb;
        }
        Core::Prim(p, args) if STRICT_PRIMS.contains(&p.as_str()) => {
            for a in args {
                let (f, fa) = forced_d(a, strict, bottom);
                out.extend(f);
                fails |= fa;
            }
        }
        Core::Prim(..) => {}
    }
    (out, fails)
}

fn by_value(c: &mut Core, strict: &HashMap<String, Vec<bool>>, fresh: &mut u64) {
    match c {
        Core::Var(_) | Core::Lit(_) | Core::Fail(_) => return,
        Core::App(f, a) => {
            by_value(f, strict, fresh);
            by_value(a, strict, fresh);
        }
        Core::Lam(_, b) => by_value(b, strict, fresh),
        Core::Let(binds, body) => {
            binds.iter_mut().for_each(|(_, r)| by_value(r, strict, fresh));
            by_value(body, strict, fresh);
        }
        Core::Case(s, _, alts) => {
            by_value(s, strict, fresh);
            for a in alts {
                match a {
                    Alt::Con(_, _, b) | Alt::Lit(_, b) | Alt::Default(b) => by_value(b, strict, fresh),
                }
            }
        }
        Core::Con(_, args) | Core::Prim(_, args) => args.iter_mut().for_each(|a| by_value(a, strict, fresh)),
    }
    let Core::App(..) = c else { return };
    let mut n = 0;
    let mut head = &*c;
    while let Core::App(f, _) = head {
        n += 1;
        head = f;
    }
    let Core::Var(f) = head else { return };
    let Some(s) = strict.get(f) else { return };
    if n < s.len() {
        return;
    }
    // The arguments, first to last, of the spine.
    let mut spine: Vec<&mut Core> = Vec::with_capacity(n);
    let mut cur = &mut *c;
    while let Core::App(f, a) = cur {
        spine.push(&mut **a);
        cur = &mut **f;
    }
    spine.reverse();
    let mut cases = Vec::new();
    for (a, st) in spine.into_iter().zip(s) {
        if *st && !is_atom(a) && !matches!(a, Core::Lam(..)) {
            *fresh += 1;
            let v = format!("v~{fresh}");
            cases.push((v.clone(), std::mem::replace(a, Core::Var(v))));
        }
    }
    let mut e = take(c);
    for (v, a) in cases.into_iter().rev() {
        e = Core::Case(Box::new(a), v, vec![Alt::Default(e)]);
    }
    *c = e;
}

#[cfg(test)]
mod rule_tests {
    use super::*;

    fn v(n: &str) -> Core {
        Core::Var(n.into())
    }

    fn app(f: Core, args: Vec<Core>) -> Core {
        args.into_iter().fold(f, |f, a| Core::App(Box::new(f), Box::new(a)))
    }

    fn rule(vars: &[&str], head: &str, args: Vec<Core>, rhs: Core, act: Act) -> RuleDef {
        RuleDef { name: "t".into(), act, vars: vars.iter().map(|s| s.to_string()).collect(), head: head.into(), args, rhs }
    }

    fn rewrite_with(rules: Vec<RuleDef>, phase: u32, mut e: Core) -> Option<Core> {
        let rules = Rules { by_head: rules.into_iter().fold(HashMap::new(), |mut m: HashMap<String, Vec<RuleDef>>, r| { m.entry(r.head.clone()).or_default().push(r); m }), protected: HashSet::new(), inlines: HashMap::new() };
        let (once, dead, known, aliases, templates) = (HashSet::new(), HashSet::new(), HashMap::new(), HashMap::new(), HashMap::new());
        let literals = HashMap::new();
        let mut fresh = 0;
        let no_arities = HashMap::new();
        let mut s = Simp {
            once: &once, dead: &dead, pending: HashMap::new(), known: &known, literals: &literals, aliases: &aliases, templates: &templates,
            this: "Test.x", rules: &rules, phase, rule_budget: RULE_BUDGET, atoms: HashMap::new(), evaluated: HashSet::new(), arities: &no_arities, local_arities: HashMap::new(),
            fresh: &mut fresh, budget: 0, changed: false,
        };
        s.rewrite(&mut e).then_some(e)
    }

    #[test]
    fn fold_build_rewrites_with_further_arguments() {
        let r = builtin_rules().remove(0);
        let e = app(v("GHC.Internal.Base.foldr"), vec![v("k"), v("z"), app(v("GHC.Internal.Base.build"), vec![v("g")]), v("extra")]);
        let out = rewrite_with(vec![r], 2, e).expect("fires");
        assert_eq!(out, app(v("g"), vec![v("k"), v("z"), v("extra")]));
    }

    #[test]
    fn repeated_variables_must_match_the_same_expression() {
        // f x x = x
        let r = rule(&["x"], "T.f", vec![v("x"), v("x")], v("x"), Act::Always);
        assert!(rewrite_with(vec![r.clone()], 0, app(v("T.f"), vec![v("a"), v("a")])).is_some());
        assert!(rewrite_with(vec![r], 0, app(v("T.f"), vec![v("a"), v("b")])).is_none());
    }

    #[test]
    fn lambda_patterns_match_up_to_renaming_and_activation_is_respected() {
        // mapFB c (\y -> y) = c, active before phase 1 only
        let lam = |x: &str| Core::Lam(x.into(), Box::new(v(x)));
        let r = rule(&["c"], "T.mapFB", vec![v("c"), lam("y")], v("c"), Act::Before(1));
        let e = app(v("T.mapFB"), vec![v("k"), lam("q")]);
        assert_eq!(rewrite_with(vec![r.clone()], 2, e.clone()), Some(v("k")));
        assert!(rewrite_with(vec![r.clone()], 1, e.clone()).is_none());
        // A lambda that is not the identity does not match.
        let other = app(v("T.mapFB"), vec![v("k"), Core::Lam("q".into(), Box::new(v("w")))]);
        assert!(rewrite_with(vec![r], 2, other).is_none());
    }

    #[test]
    fn non_atomic_matches_are_shared_through_a_let() {
        // dup x = pair x x, applied to a call: the call is bound once.
        let pair = |a: Core, b: Core| Core::Con("T.P".into(), vec![a, b]);
        let r = rule(&["x"], "T.dup", vec![v("x")], pair(v("x"), v("x")), Act::Always);
        let out = rewrite_with(vec![r], 0, app(v("T.dup"), vec![app(v("T.g"), vec![v("a")])])).expect("fires");
        let Core::Let(bs, body) = out else { panic!("{out:?}") };
        assert_eq!(bs[0].1, app(v("T.g"), vec![v("a")]));
        assert_eq!(*body, pair(v(&bs[0].0), v(&bs[0].0)));
    }
}
