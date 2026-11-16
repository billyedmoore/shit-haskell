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

pub fn simplify(binds: &mut [TopBind]) {
    // The second pass inlines the bodies the first simplified; the last cleans up after the
    // calls by value.
    pass(binds);
    pass(binds);
    call_by_value(binds);
    pass(binds);
}

fn pass(binds: &mut [TopBind]) {
    let globals: HashSet<String> = binds.iter().map(|b| b.name.clone()).collect();
    let direct: HashMap<&str, &Core> = binds.iter().map(|b| (b.name.as_str(), &b.body)).collect();
    // Globals that are (aliases of) constructor applications to atoms, and global aliases.
    let mut known: HashMap<String, (String, Vec<Core>)> = HashMap::new();
    let mut aliases: HashMap<String, String> = HashMap::new();
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
    let mut templates: HashMap<String, (usize, Core)> = HashMap::new();
    for b in binds.iter() {
        let (arity, inner) = lambdas(&b.body);
        if arity == 0 || size(inner, INLINE_SIZE + 1) > INLINE_SIZE || mentions(&b.body, &b.name) {
            continue;
        }
        let mut names = HashSet::new();
        if unique_binders(&b.body, &globals, &mut names) {
            templates.insert(b.name.clone(), (arity, b.body.clone()));
        }
    }
    let mut fresh = FRESH.load(std::sync::atomic::Ordering::Relaxed);
    for bind in binds.iter_mut() {
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
            for (n, o) in occ {
                if o.count == 0 {
                    dead.insert(n);
                } else if o.count == 1 && !o.under_lambda && !o.recursive {
                    once.insert(n);
                }
            }
            let mut s = Simp {
                once: &once,
                dead: &dead,
                pending: HashMap::new(),
                known: &known,
                aliases: &aliases,
                templates: &templates,
                this: &bind.name,
                atoms: HashMap::new(),
                evaluated: HashSet::new(),
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
    aliases: &'a HashMap<String, String>,
    templates: &'a HashMap<String, (usize, Core)>,
    /// The binding being simplified (never inlined into itself).
    this: &'a str,
    /// Locals bound to atoms, substituted at their uses.
    atoms: HashMap<String, Core>,
    /// Case binders: variables known to be evaluated.
    evaluated: HashSet<String>,
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
                self.inline_call(c);
                self.reduce(c);
            }
            Core::Lam(_, b) => self.expr(b),
            Core::Let(bindings, body) => {
                let _ = (bindings, body);
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
            // Representation identities (Int# and Word# share it), and Integer conversions of
            // small literals and of `IS`.
            Core::Prim(p, args) if args.len() == 1 && matches!(p.as_str(), "int2Word#" | "word2Int#" | "integerToInt" | "integerToWord") => {
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
        if mentions(e, x) || matches!(e, Core::Lam(..)) {
            return;
        }
        let direct = matches!(&**body, Core::Case(s, _, _) if matches!(&**s, Core::Var(v) if v == x));
        if !direct {
            // The body certainly evaluates x: x is evaluated first instead of allocated.
            if !forced(body, &HashMap::new()).contains(x.as_str()) {
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
        let Core::App(f, _) = c else { return };
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

/// Strictness of global functions, and calls by value: a parameter is strict when the function's
/// body evaluates it on every path that returns (failures and calls of failing functions count as
/// evaluating nothing, so no exception changes). The analysis starts with every parameter
/// strict and weakens to a fixpoint. A saturated call's non-atomic argument for a strict
/// parameter is then evaluated before the call (`case a as v of _ -> f .. v ..`) instead of
/// being allocated as a thunk.
fn call_by_value(binds: &mut [TopBind]) {
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
    for b in binds.iter_mut() {
        by_value(&mut b.body, &strict, &mut fresh);
    }
    FRESH.store(fresh, std::sync::atomic::Ordering::Relaxed);
}

/// The variables an expression certainly evaluates when its value is demanded.
fn forced(e: &Core, strict: &HashMap<String, Vec<bool>>) -> HashSet<String> {
    let mut out = HashSet::new();
    match e {
        Core::Var(v) => {
            out.insert(v.clone());
        }
        Core::Lit(_) | Core::Fail(_) | Core::Lam(..) | Core::Con(..) => {}
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
            out = forced(body, strict);
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
            out = forced(s, strict);
        }
        Core::Case(s, b, alts) => {
            let mut common: Option<HashSet<String>> = None;
            for a in alts {
                let (vars, body): (&[String], &Core) = match a {
                    Alt::Con(_, vars, body) => (vars, body),
                    Alt::Lit(_, body) | Alt::Default(body) => (&[], body),
                };
                let mut f = forced(body, strict);
                for v in vars {
                    f.remove(v);
                }
                common = Some(match common {
                    None => f,
                    Some(c) => c.intersection(&f).cloned().collect(),
                });
            }
            out = common.unwrap_or_default();
            out.remove(b);
            out.extend(forced(s, strict));
        }
        Core::Prim(p, args) if p == "seq" && args.len() == 2 => {
            out = forced(&args[0], strict);
            out.extend(forced(&args[1], strict));
        }
        Core::Prim(p, args) if STRICT_PRIMS.contains(&p.as_str()) => {
            for a in args {
                out.extend(forced(a, strict));
            }
        }
        Core::Prim(..) => {}
    }
    out
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
