//! Selective lambda-lifting for local functions before STG lowering.
//!
//! Local recursive loops (such as fused `forM_` / `foldl'` / list comprehension `go` bindings)
//! and closed local helpers are lifted to top-level bindings so they get direct calls, unboxed
//! register-argument workers, closure-less frames, and `.Lloop` self-tail-loops.
//!
//! Because `Core` is untyped, numeric parameter/result kinds (`NUM_INT` / `NUM_CHAR`) for lifted
//! bindings are inferred soundly by fixpoint over all call sites and known enclosing numeric
//! variables from `NumericParams`.

use std::collections::{HashMap, HashSet};

use crate::core::{Alt, Core, Lit, TopBind};

use super::stg::{raw_prim_result_kind, NumericParams, NUM_CHAR, NUM_DOUBLE, NUM_INT};

/// Maximum number of free variables captured by a lifted local function.
const MAX_LIFT_FV: usize = 6;

/// Type-preserving identity primitives (excludes `unsafeCoerce`/`unsafeCoerce#`).
const SAFE_IDENTITY_PRIMS: [&str; 4] = ["lazy", "oneShot", "inline", "noinline"];

/// Lifts eligible local functions into top-level bindings and records sound `NumericParams`
/// entries for them.
pub fn lambda_lift(binds: &mut Vec<TopBind>, numeric: &mut NumericParams) {
    let mut globals: HashSet<String> = binds.iter().map(|b| b.name.clone()).collect();
    let mut lifted: Vec<TopBind> = Vec::new();
    let mut next_id: usize = 0;

    // Process original top-level bindings first.
    for b in binds.iter_mut() {
        let mut lifter = Lifter {
            globals: &mut globals,
            numeric,
            lifted: &mut lifted,
            next_id: &mut next_id,
            owner: &b.name,
        };
        lifter.lift_top_bind(&mut b.body);
    }

    // Also process newly lifted bindings (BFS) so nested local loops inside lifted helpers
    // receive propagated `NumericParams` and get lifted too.
    let mut cursor = 0;
    while cursor < lifted.len() {
        let mut bind = lifted[cursor].clone();
        let owner = bind.name.clone();
        let mut more: Vec<TopBind> = Vec::new();
        {
            let mut lifter = Lifter {
                globals: &mut globals,
                numeric,
                lifted: &mut more,
                next_id: &mut next_id,
                owner: &owner,
            };
            lifter.lift_top_bind(&mut bind.body);
        }
        lifted[cursor] = bind;
        lifted.extend(more);
        cursor += 1;
    }

    binds.extend(lifted);
}

struct Lifter<'a> {
    globals: &'a mut HashSet<String>,
    numeric: &'a mut NumericParams,
    lifted: &'a mut Vec<TopBind>,
    next_id: &'a mut usize,
    owner: &'a str,
}

impl<'a> Lifter<'a> {
    fn lift_top_bind(&mut self, body: &mut Core) {
        let mut env: HashMap<String, u8> = HashMap::new();
        let kinds = self.numeric.params.get(self.owner).cloned();
        let mut cur = body;
        let mut idx = 0usize;
        while let Core::Lam(p, inner) = cur {
            if let Some(k) = kinds.as_ref().and_then(|ks| ks.get(idx)).copied().filter(|&k| k != 0) {
                env.insert(p.clone(), k);
            }
            idx += 1;
            cur = inner;
        }
        self.lift_expr(cur, &env);
    }

    fn lift_expr(&mut self, e: &mut Core, env: &HashMap<String, u8>) {
        match e {
            Core::Var(_) | Core::Lit(_) | Core::Fail(_) => {}
            Core::App(f, a) => {
                self.lift_expr(f, env);
                self.lift_expr(a, env);
            }
            Core::Lam(p, b) => {
                let mut inner_env = env.clone();
                inner_env.remove(p);
                self.lift_expr(b, &inner_env);
            }
            Core::Con(_, args) | Core::Prim(_, args) => {
                for a in args {
                    self.lift_expr(a, env);
                }
            }
            Core::Case(scrut, binder, alts) => {
                self.lift_expr(scrut, env);
                let sk = expr_kind(scrut, env, self.numeric);
                let mut base_env = env.clone();
                if sk != 0 {
                    base_env.insert(binder.clone(), sk);
                } else {
                    base_env.remove(binder);
                }
                for alt in alts {
                    match alt {
                        Alt::Con(_, fields, b) => {
                            let mut aenv = base_env.clone();
                            for f in fields {
                                aenv.remove(f);
                            }
                            self.lift_expr(b, &aenv);
                        }
                        Alt::Lit(lit, b) => {
                            let mut aenv = base_env.clone();
                            match lit {
                                Lit::Int(_) => {
                                    aenv.insert(binder.clone(), NUM_INT);
                                }
                                Lit::Char(_) => {
                                    aenv.insert(binder.clone(), NUM_CHAR);
                                }
                                _ => {}
                            }
                            self.lift_expr(b, &aenv);
                        }
                        Alt::Default(b) => {
                            self.lift_expr(b, &base_env);
                        }
                    }
                }
            }
            Core::Let(binds, body) => {
                // Iteratively peel off eligible local lambdas that do not depend on any other
                // binding in the same `Let` group.
                loop {
                    let mut lifted_idx = None;
                    for i in 0..binds.len() {
                        if self.try_lift_binding(binds, body, i, env) {
                            lifted_idx = Some(i);
                            break;
                        }
                    }
                    if let Some(i) = lifted_idx {
                        binds.remove(i);
                    } else {
                        break;
                    }
                }
                if binds.is_empty() {
                    let mut new_body = std::mem::replace(body.as_mut(), Core::Fail(String::new()));
                    self.lift_expr(&mut new_body, env);
                    *e = new_body;
                    return;
                }
                let mut body_env = env.clone();
                for (name, _) in binds.iter() {
                    body_env.remove(name);
                }
                for (name, rhs) in binds.iter_mut() {
                    self.lift_expr(rhs, &body_env);
                    if !mentions(rhs, name) {
                        let k = expr_kind(rhs, &body_env, self.numeric);
                        if k != 0 {
                            body_env.insert(name.clone(), k);
                        }
                    }
                }
                self.lift_expr(body, &body_env);
            }
        }
    }

    /// Attempts to lift `binds[idx]` to top-level. Returns `true` if lifted (and substituted into
    /// all other `binds` and `body`).
    fn try_lift_binding(
        &mut self,
        binds: &mut [(String, Core)],
        body: &mut Core,
        idx: usize,
        env: &HashMap<String, u8>,
    ) -> bool {
        let name = &binds[idx].0;
        let rhs = &binds[idx].1;
        let (params, inner) = split_lambdas(rhs);
        let arity = params.len();
        if arity == 0 {
            return false;
        }
        // Must not mention any sibling binding in this `Let` group (siblings stay local unless
        // peeled first).
        for (j, (other, _)) in binds.iter().enumerate() {
            if j != idx && mentions(rhs, other) {
                return false;
            }
        }
        let is_rec = mentions(inner, name);
        // Never lift STG join points: join points stay in the caller's frame with zero call overhead.
        if !is_rec && binds.len() == 1 && is_join_point(name, arity, body) {
            return false;
        }

        // Collect free local variables of `rhs` (excluding `name`, its params, and top-level globals).
        let fvs = free_locals(rhs, name, self.globals);
        if fvs.len() > MAX_LIFT_FV {
            return false;
        }
        // If non-recursive, only lift closed local functions (0 free vars): avoids introducing
        // PAP allocations while turning closed helpers (e.g. `safe` in nqueens) into direct globals.
        if !is_rec && !fvs.is_empty() {
            return false;
        }

        // Guard against any shadowing of `name` or `fvs` inside `rhs`, siblings, or `body` so
        // substitution can never capture a variable.
        let mut guard_set: HashSet<&str> = fvs.iter().map(String::as_str).collect();
        guard_set.insert(name.as_str());
        if binds_any(rhs, &guard_set)
            || binds_any(body, &guard_set)
            || binds.iter().enumerate().any(|(j, (_, r))| j != idx && binds_any(r, &guard_set))
        {
            return false;
        }

        // Check call patterns to ensure we don't turn a reused closure into repeated PAP allocations.
        let mut rec_min_arity = usize::MAX;
        let mut rec_count = 0usize;
        scan_call_arities(inner, name, &mut rec_min_arity, &mut rec_count);
        let mut ext_min_arity = usize::MAX;
        let mut ext_count = 0usize;
        scan_call_arities(body, name, &mut ext_min_arity, &mut ext_count);
        for (j, (_, r)) in binds.iter().enumerate() {
            if j != idx {
                scan_call_arities(r, name, &mut ext_min_arity, &mut ext_count);
            }
        }
        if ext_count == 0 {
            return false;
        }
        if !fvs.is_empty() {
            // Every recursive call inside `rhs` must be saturated; externally either every call
            // is saturated or there is only a single initial reference (e.g. `go 1` returning `IO ()`).
            if rec_min_arity < arity {
                return false;
            }
            if ext_min_arity < arity && ext_count > 1 {
                return false;
            }
        }

        // Soundly infer numeric kinds for `params` (and optional return kind) before rewriting.
        let min_arity = rec_min_arity.min(ext_min_arity);
        let (param_kinds, ret_kind) = self.infer_numeric(name, &params, inner, &fvs, binds, idx, body, env, min_arity);

        // Allocate fresh unique top-level name.
        *self.next_id += 1;
        let lifted_name = self.fresh_global_name(name);
        self.globals.insert(lifted_name.clone());

        // Record inferred numeric kinds for the lifted top-level binding.
        let mut full_kinds: Vec<u8> = Vec::with_capacity(fvs.len() + arity);
        for fv in &fvs {
            full_kinds.push(env.get(fv).copied().unwrap_or(0));
        }
        full_kinds.extend_from_slice(&param_kinds);
        if full_kinds.iter().any(|&k| k != 0) {
            self.numeric.params.insert(lifted_name.clone(), full_kinds);
        }
        if ret_kind != 0 {
            self.numeric.results.insert(lifted_name.clone(), (fvs.len() + arity, ret_kind));
        }

        // Rewrite `name -> lifted_name fv0 ... fvk` in `rhs`, all siblings, and `body`.
        let name_owned = name.clone();
        let mut lifted_body = rhs.clone();
        subst_lifted(&mut lifted_body, &name_owned, &lifted_name, &fvs);
        for fv in fvs.iter().rev() {
            lifted_body = Core::Lam(fv.clone(), Box::new(lifted_body));
        }
        self.lifted.push(TopBind { name: lifted_name.clone(), body: lifted_body });

        for (j, (_, r)) in binds.iter_mut().enumerate() {
            if j != idx {
                subst_lifted(r, &name_owned, &lifted_name, &fvs);
            }
        }
        subst_lifted(body, &name_owned, &lifted_name, &fvs);
        true
    }

    fn fresh_global_name(&mut self, local_name: &str) -> String {
        let base = local_name.split('%').next().unwrap_or("lift");
        loop {
            let cand = format!("{}$lift${base}${}", self.owner, *self.next_id);
            if !self.globals.contains(&cand) {
                return cand;
            }
            *self.next_id += 1;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn infer_numeric(
        &self,
        name: &str,
        params: &[&str],
        inner: &Core,
        fvs: &[String],
        binds: &[(String, Core)],
        idx: usize,
        body: &Core,
        env: &HashMap<String, u8>,
        min_arity: usize,
    ) -> (Vec<u8>, u8) {
        let arity = params.len();
        // Step 1: collect external call argument kinds (from `body` and sibling `binds`).
        let mut ext_kinds: Vec<Option<u8>> = vec![None; arity];
        let mut saw_ext = false;
        let mut collect_ext = |e: &Core| {
            collect_call_arg_kinds(e, name, env, self.numeric, arity, &mut ext_kinds, &mut saw_ext);
        };
        collect_ext(body);
        for (j, (_, r)) in binds.iter().enumerate() {
            if j != idx {
                collect_ext(r);
            }
        }

        // A parameter can only be inferred numeric if EVERY call supplies it (`j < min_arity`)
        // and every external call passes the same nonzero numeric kind.
        let mut cand: Vec<u8> = (0..arity)
            .map(|j| if j < min_arity && saw_ext { ext_kinds[j].unwrap_or(0) } else { 0 })
            .collect();

        // Step 2: greatest-fixpoint refinement against all recursive calls in `inner`.
        loop {
            let mut body_env: HashMap<String, u8> = HashMap::new();
            for fv in fvs {
                if let Some(&k) = env.get(fv) {
                    if k != 0 {
                        body_env.insert(fv.clone(), k);
                    }
                }
            }
            for (j, p) in params.iter().enumerate() {
                if cand[j] != 0 {
                    body_env.insert((*p).to_string(), cand[j]);
                }
            }
            let mut rec_kinds: Vec<Option<u8>> = vec![None; arity];
            let mut saw_rec = false;
            collect_call_arg_kinds(inner, name, &body_env, self.numeric, arity, &mut rec_kinds, &mut saw_rec);
            let mut changed = false;
            if saw_rec {
                for j in 0..arity {
                    if cand[j] != 0 {
                        if let Some(rk) = rec_kinds[j] {
                            if rk != cand[j] {
                                cand[j] = 0;
                                changed = true;
                            }
                        }
                    }
                }
            }
            if !changed {
                // Step 3: infer return kind of `inner` under the converged `body_env`.
                let ret_kind = infer_return_kind(inner, name, arity, &body_env, self.numeric);
                return (cand, ret_kind);
            }
        }
    }
}

/// Leading lambda parameters and inner body.
fn split_lambdas(e: &Core) -> (Vec<&str>, &Core) {
    let mut params = Vec::new();
    let mut cur = e;
    while let Core::Lam(p, b) = cur {
        params.push(p.as_str());
        cur = b;
    }
    (params, cur)
}

/// Application spine `(head, [arg0, arg1, ...])`.
fn spine(mut e: &Core) -> (&Core, Vec<&Core>) {
    let mut args = Vec::new();
    while let Core::App(f, a) = e {
        args.push(a.as_ref());
        e = f;
    }
    args.reverse();
    (e, args)
}

fn mentions(e: &Core, x: &str) -> bool {
    match e {
        Core::Var(v) => v == x,
        Core::Lit(_) | Core::Fail(_) => false,
        Core::App(f, a) => mentions(f, x) || mentions(a, x),
        Core::Lam(p, b) => p != x && mentions(b, x),
        Core::Let(binds, body) => {
            !binds.iter().any(|(n, _)| n == x)
                && (binds.iter().any(|(_, r)| mentions(r, x)) || mentions(body, x))
        }
        Core::Case(s, b, alts) => {
            mentions(s, x)
                || (b != x
                    && alts.iter().any(|a| match a {
                        Alt::Con(_, fs, body) => !fs.iter().any(|f| f == x) && mentions(body, x),
                        Alt::Lit(_, body) | Alt::Default(body) => mentions(body, x),
                    }))
        }
        Core::Con(_, args) | Core::Prim(_, args) => args.iter().any(|a| mentions(a, x)),
    }
}

/// Mirrors `stg::tail_calls` so we never lift a non-recursive STG join point.
fn is_join_point(j: &str, arity: usize, body: &Core) -> bool {
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
                if let [(jn, jrhs)] = &binds[..] {
                    let (jps, jcode) = split_lambdas(jrhs);
                    if !mentions(jrhs, jn) && tail_calls(body, jn, jps.len()) {
                        let shadowed = jps.iter().any(|p| *p == j);
                        return (shadowed || tail_calls(jcode, j, n)) && tail_calls(body, j, n);
                    }
                }
                binds.iter().all(|(_, r)| !mentions(r, j)) && tail_calls(body, j, n)
            }
            Core::Case(s, b, alts) => {
                !mentions(s, j)
                    && (b == j
                        || alts.iter().all(|a| match a {
                            Alt::Con(_, fs, body) => fs.iter().any(|f| f == j) || tail_calls(body, j, n),
                            Alt::Lit(_, body) | Alt::Default(body) => tail_calls(body, j, n),
                        }))
            }
            Core::Prim(p, args) if p == "seq" && args.len() == 2 => {
                !mentions(&args[0], j) && tail_calls(&args[1], j, n)
            }
            Core::Prim(p, args) if SAFE_IDENTITY_PRIMS.contains(&p.as_str()) && args.len() == 1 => {
                tail_calls(&args[0], j, n)
            }
            Core::Con(_, args) | Core::Prim(_, args) => args.iter().all(|a| !mentions(a, j)),
        }
    }
    tail_calls(body, j, arity)
}

/// Free local variables of `e`, excluding `self_name` and `globals`, in deterministic order.
fn free_locals(e: &Core, self_name: &str, globals: &HashSet<String>) -> Vec<String> {
    fn walk<'a>(
        e: &'a Core,
        bound: &mut HashSet<&'a str>,
        globals: &HashSet<String>,
        seen: &mut HashSet<&'a str>,
        out: &mut Vec<String>,
    ) {
        match e {
            Core::Var(v) => {
                let s = v.as_str();
                if !bound.contains(s) && !globals.contains(s) && seen.insert(s) {
                    out.push(v.clone());
                }
            }
            Core::Lit(_) | Core::Fail(_) => {}
            Core::App(f, a) => {
                walk(f, bound, globals, seen, out);
                walk(a, bound, globals, seen, out);
            }
            Core::Lam(p, b) => {
                let inserted = bound.insert(p.as_str());
                walk(b, bound, globals, seen, out);
                if inserted {
                    bound.remove(p.as_str());
                }
            }
            Core::Let(binds, body) => {
                let mut added = Vec::new();
                for (n, _) in binds {
                    if bound.insert(n.as_str()) {
                        added.push(n.as_str());
                    }
                }
                for (_, r) in binds {
                    walk(r, bound, globals, seen, out);
                }
                walk(body, bound, globals, seen, out);
                for n in added {
                    bound.remove(n);
                }
            }
            Core::Case(s, b, alts) => {
                walk(s, bound, globals, seen, out);
                let b_ins = bound.insert(b.as_str());
                for alt in alts {
                    match alt {
                        Alt::Con(_, fs, body) => {
                            let mut added = Vec::new();
                            for f in fs {
                                if bound.insert(f.as_str()) {
                                    added.push(f.as_str());
                                }
                            }
                            walk(body, bound, globals, seen, out);
                            for f in added {
                                bound.remove(f);
                            }
                        }
                        Alt::Lit(_, body) | Alt::Default(body) => walk(body, bound, globals, seen, out),
                    }
                }
                if b_ins {
                    bound.remove(b.as_str());
                }
            }
            Core::Con(_, args) | Core::Prim(_, args) => {
                for a in args {
                    walk(a, bound, globals, seen, out);
                }
            }
        }
    }
    let mut bound = HashSet::new();
    bound.insert(self_name);
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    walk(e, &mut bound, globals, &mut seen, &mut out);
    out
}

/// Whether `e` introduces a binder whose name is in `names`.
fn binds_any(e: &Core, names: &HashSet<&str>) -> bool {
    match e {
        Core::Var(_) | Core::Lit(_) | Core::Fail(_) => false,
        Core::App(f, a) => binds_any(f, names) || binds_any(a, names),
        Core::Lam(p, b) => names.contains(p.as_str()) || binds_any(b, names),
        Core::Let(binds, body) => {
            binds.iter().any(|(n, _)| names.contains(n.as_str()))
                || binds.iter().any(|(_, r)| binds_any(r, names))
                || binds_any(body, names)
        }
        Core::Case(s, b, alts) => {
            binds_any(s, names)
                || names.contains(b.as_str())
                || alts.iter().any(|a| match a {
                    Alt::Con(_, fs, body) => {
                        fs.iter().any(|f| names.contains(f.as_str())) || binds_any(body, names)
                    }
                    Alt::Lit(_, body) | Alt::Default(body) => binds_any(body, names),
                })
        }
        Core::Con(_, args) | Core::Prim(_, args) => args.iter().any(|a| binds_any(a, names)),
    }
}

/// Scans every occurrence of `target` in `e`, recording the minimum applied argument count and
/// total occurrence count.
fn scan_call_arities(e: &Core, target: &str, min_arity: &mut usize, count: &mut usize) {
    match e {
        Core::Var(v) if v == target => {
            *min_arity = 0;
            *count += 1;
        }
        Core::Var(_) | Core::Lit(_) | Core::Fail(_) => {}
        Core::App(..) => {
            let (head, args) = spine(e);
            if matches!(head, Core::Var(v) if v == target) {
                *min_arity = (*min_arity).min(args.len());
                *count += 1;
            } else {
                scan_call_arities(head, target, min_arity, count);
            }
            for a in args {
                scan_call_arities(a, target, min_arity, count);
            }
        }
        Core::Lam(p, b) => {
            if p != target {
                scan_call_arities(b, target, min_arity, count);
            }
        }
        Core::Let(binds, body) => {
            if binds.iter().any(|(n, _)| n == target) {
                return;
            }
            for (_, r) in binds {
                scan_call_arities(r, target, min_arity, count);
            }
            scan_call_arities(body, target, min_arity, count);
        }
        Core::Case(s, b, alts) => {
            scan_call_arities(s, target, min_arity, count);
            if b == target {
                return;
            }
            for alt in alts {
                match alt {
                    Alt::Con(_, fs, body) => {
                        if !fs.iter().any(|f| f == target) {
                            scan_call_arities(body, target, min_arity, count);
                        }
                    }
                    Alt::Lit(_, body) | Alt::Default(body) => {
                        scan_call_arities(body, target, min_arity, count);
                    }
                }
            }
        }
        Core::Con(_, args) | Core::Prim(_, args) => {
            for a in args {
                scan_call_arities(a, target, min_arity, count);
            }
        }
    }
}

/// Replaces every occurrence of `Core::Var(from)` with `Core::Var(to)` applied to `fvs`.
fn subst_lifted(e: &mut Core, from: &str, to: &str, fvs: &[String]) {
    match e {
        Core::Var(v) if v == from => {
            let mut app = Core::Var(to.to_string());
            for fv in fvs {
                app = Core::App(Box::new(app), Box::new(Core::Var(fv.clone())));
            }
            *e = app;
        }
        Core::Var(_) | Core::Lit(_) | Core::Fail(_) => {}
        Core::App(f, a) => {
            subst_lifted(f, from, to, fvs);
            subst_lifted(a, from, to, fvs);
        }
        Core::Lam(p, b) => {
            if p != from {
                subst_lifted(b, from, to, fvs);
            }
        }
        Core::Let(binds, body) => {
            if binds.iter().any(|(n, _)| n == from) {
                return;
            }
            for (_, r) in binds {
                subst_lifted(r, from, to, fvs);
            }
            subst_lifted(body, from, to, fvs);
        }
        Core::Case(s, b, alts) => {
            subst_lifted(s, from, to, fvs);
            if b == from {
                return;
            }
            for alt in alts {
                match alt {
                    Alt::Con(_, fs, body) => {
                        if !fs.iter().any(|f| f == from) {
                            subst_lifted(body, from, to, fvs);
                        }
                    }
                    Alt::Lit(_, body) | Alt::Default(body) => subst_lifted(body, from, to, fvs),
                }
            }
        }
        Core::Con(_, args) | Core::Prim(_, args) => {
            for a in args {
                subst_lifted(a, from, to, fvs);
            }
        }
    }
}

/// Soundly determines whether `e` is guaranteed to evaluate to an `INT`/`WORD` (`NUM_INT`),
/// `CHAR` (`NUM_CHAR`), or `DOUBLE` (`NUM_DOUBLE`) value, or `0` if unknown.
fn expr_kind(e: &Core, env: &HashMap<String, u8>, numeric: &NumericParams) -> u8 {
    match e {
        Core::Lit(Lit::Int(_)) => NUM_INT,
        Core::Lit(Lit::Char(_)) => NUM_CHAR,
        Core::Lit(Lit::Double(_)) => NUM_DOUBLE,
        Core::Lit(_) | Core::Fail(_) | Core::Con(..) | Core::Lam(..) => 0,
        Core::Var(v) => env.get(v).copied().unwrap_or(0),
        Core::Prim(p, args) => {
            if let Some(k) = raw_prim_result_kind(p) {
                k
            } else if p == "seq" && args.len() == 2 {
                expr_kind(&args[1], env, numeric)
            } else if SAFE_IDENTITY_PRIMS.contains(&p.as_str()) && args.len() == 1 {
                expr_kind(&args[0], env, numeric)
            } else {
                0
            }
        }
        Core::App(..) => {
            let (head, args) = spine(e);
            if let Core::Var(g) = head {
                if let Some(&(arity, kind)) = numeric.results.get(g.as_str()) {
                    if args.len() == arity {
                        return kind;
                    }
                }
            }
            0
        }
        Core::Let(binds, body) => {
            let mut inner_env = env.clone();
            for (n, _) in binds {
                inner_env.remove(n);
            }
            for (n, r) in binds {
                if !mentions(r, n) {
                    let k = expr_kind(r, &inner_env, numeric);
                    if k != 0 {
                        inner_env.insert(n.clone(), k);
                    }
                }
            }
            expr_kind(body, &inner_env, numeric)
        }
        Core::Case(scrut, binder, alts) => {
            let sk = expr_kind(scrut, env, numeric);
            let mut base_env = env.clone();
            if sk != 0 {
                base_env.insert(binder.clone(), sk);
            } else {
                base_env.remove(binder);
            }
            let mut merged: Option<u8> = None;
            for alt in alts {
                let (b, aenv) = match alt {
                    Alt::Con(_, fs, b) => {
                        let mut e = base_env.clone();
                        for f in fs {
                            e.remove(f);
                        }
                        (b, e)
                    }
                    Alt::Lit(lit, b) => {
                        let mut e = base_env.clone();
                        match lit {
                            Lit::Int(_) => {
                                e.insert(binder.clone(), NUM_INT);
                            }
                            Lit::Char(_) => {
                                e.insert(binder.clone(), NUM_CHAR);
                            }
                            _ => {}
                        }
                        (b, e)
                    }
                    Alt::Default(b) => (b, base_env.clone()),
                };
                if matches!(b, Core::Fail(_)) {
                    continue;
                }
                let k = expr_kind(b, &aenv, numeric);
                if k == 0 {
                    return 0;
                }
                match merged {
                    None => merged = Some(k),
                    Some(prev) if prev == k => {}
                    Some(_) => return 0,
                }
            }
            merged.unwrap_or(0)
        }
    }
}

/// Walks `e` tracking local numeric binders, merging the `expr_kind` of each argument passed to
/// `target` into `kinds` (`Some(0)` means conflicting or non-numeric).
fn collect_call_arg_kinds(
    e: &Core,
    target: &str,
    env: &HashMap<String, u8>,
    numeric: &NumericParams,
    arity: usize,
    kinds: &mut [Option<u8>],
    saw_call: &mut bool,
) {
    match e {
        Core::Var(_) | Core::Lit(_) | Core::Fail(_) => {}
        Core::App(..) => {
            let (head, args) = spine(e);
            if matches!(head, Core::Var(v) if v == target) {
                *saw_call = true;
                for (j, arg) in args.iter().take(arity).enumerate() {
                    let k = expr_kind(arg, env, numeric);
                    kinds[j] = match kinds[j] {
                        None => Some(k),
                        Some(prev) if prev == k => Some(prev),
                        Some(_) => Some(0),
                    };
                }
            } else {
                collect_call_arg_kinds(head, target, env, numeric, arity, kinds, saw_call);
            }
            for a in args {
                collect_call_arg_kinds(a, target, env, numeric, arity, kinds, saw_call);
            }
        }
        Core::Lam(p, b) => {
            if p == target {
                return;
            }
            let mut inner_env = env.clone();
            inner_env.remove(p);
            collect_call_arg_kinds(b, target, &inner_env, numeric, arity, kinds, saw_call);
        }
        Core::Let(binds, body) => {
            if binds.iter().any(|(n, _)| n == target) {
                return;
            }
            let mut inner_env = env.clone();
            for (n, _) in binds {
                inner_env.remove(n);
            }
            for (n, r) in binds {
                collect_call_arg_kinds(r, target, &inner_env, numeric, arity, kinds, saw_call);
                if !mentions(r, n) {
                    let k = expr_kind(r, &inner_env, numeric);
                    if k != 0 {
                        inner_env.insert(n.clone(), k);
                    }
                }
            }
            collect_call_arg_kinds(body, target, &inner_env, numeric, arity, kinds, saw_call);
        }
        Core::Case(scrut, binder, alts) => {
            collect_call_arg_kinds(scrut, target, env, numeric, arity, kinds, saw_call);
            if binder == target {
                return;
            }
            let sk = expr_kind(scrut, env, numeric);
            let mut base_env = env.clone();
            if sk != 0 {
                base_env.insert(binder.clone(), sk);
            } else {
                base_env.remove(binder);
            }
            for alt in alts {
                match alt {
                    Alt::Con(_, fs, b) => {
                        if fs.iter().any(|f| f == target) {
                            continue;
                        }
                        let mut aenv = base_env.clone();
                        for f in fs {
                            aenv.remove(f);
                        }
                        collect_call_arg_kinds(b, target, &aenv, numeric, arity, kinds, saw_call);
                    }
                    Alt::Lit(lit, b) => {
                        let mut aenv = base_env.clone();
                        match lit {
                            Lit::Int(_) => {
                                aenv.insert(binder.clone(), NUM_INT);
                            }
                            Lit::Char(_) => {
                                aenv.insert(binder.clone(), NUM_CHAR);
                            }
                            _ => {}
                        }
                        collect_call_arg_kinds(b, target, &aenv, numeric, arity, kinds, saw_call);
                    }
                    Alt::Default(b) => {
                        collect_call_arg_kinds(b, target, &base_env, numeric, arity, kinds, saw_call);
                    }
                }
            }
        }
        Core::Con(_, args) | Core::Prim(_, args) => {
            for a in args {
                collect_call_arg_kinds(a, target, env, numeric, arity, kinds, saw_call);
            }
        }
    }
}

/// Infers whether every non-self-call, non-failing return path of `e` produces the same numeric
/// kind (`NUM_INT` or `NUM_CHAR`).
fn infer_return_kind(
    e: &Core,
    self_name: &str,
    self_arity: usize,
    env: &HashMap<String, u8>,
    numeric: &NumericParams,
) -> u8 {
    enum Ret {
        SelfOrFail,
        Kind(u8),
        Unknown,
    }
    fn walk(e: &Core, self_name: &str, self_arity: usize, env: &HashMap<String, u8>, numeric: &NumericParams) -> Ret {
        match e {
            Core::Fail(_) => Ret::SelfOrFail,
            Core::App(..) => {
                let (head, args) = spine(e);
                if matches!(head, Core::Var(v) if v == self_name) && args.len() == self_arity {
                    return Ret::SelfOrFail;
                }
                let k = expr_kind(e, env, numeric);
                if k != 0 { Ret::Kind(k) } else { Ret::Unknown }
            }
            Core::Prim(p, args) if p == "seq" && args.len() == 2 => {
                walk(&args[1], self_name, self_arity, env, numeric)
            }
            Core::Prim(p, args) if SAFE_IDENTITY_PRIMS.contains(&p.as_str()) && args.len() == 1 => {
                walk(&args[0], self_name, self_arity, env, numeric)
            }
            Core::Let(binds, body) => {
                let mut inner_env = env.clone();
                for (n, _) in binds {
                    inner_env.remove(n);
                }
                for (n, r) in binds {
                    if !mentions(r, n) {
                        let k = expr_kind(r, &inner_env, numeric);
                        if k != 0 {
                            inner_env.insert(n.clone(), k);
                        }
                    }
                }
                walk(body, self_name, self_arity, &inner_env, numeric)
            }
            Core::Case(scrut, binder, alts) => {
                let sk = expr_kind(scrut, env, numeric);
                let mut base_env = env.clone();
                if sk != 0 {
                    base_env.insert(binder.clone(), sk);
                } else {
                    base_env.remove(binder);
                }
                let mut merged: Option<u8> = None;
                for alt in alts {
                    let (b, aenv) = match alt {
                        Alt::Con(_, fs, b) => {
                            let mut e = base_env.clone();
                            for f in fs {
                                e.remove(f);
                            }
                            (b, e)
                        }
                        Alt::Lit(lit, b) => {
                            let mut e = base_env.clone();
                            match lit {
                                Lit::Int(_) => {
                                    e.insert(binder.clone(), NUM_INT);
                                }
                                Lit::Char(_) => {
                                    e.insert(binder.clone(), NUM_CHAR);
                                }
                                _ => {}
                            }
                            (b, e)
                        }
                        Alt::Default(b) => (b, base_env.clone()),
                    };
                    match walk(b, self_name, self_arity, &aenv, numeric) {
                        Ret::SelfOrFail => {}
                        Ret::Unknown => return Ret::Unknown,
                        Ret::Kind(k) => match merged {
                            None => merged = Some(k),
                            Some(prev) if prev == k => {}
                            Some(_) => return Ret::Unknown,
                        },
                    }
                }
                match merged {
                    Some(k) => Ret::Kind(k),
                    None => Ret::SelfOrFail,
                }
            }
            _ => {
                let k = expr_kind(e, env, numeric);
                if k != 0 { Ret::Kind(k) } else { Ret::Unknown }
            }
        }
    }
    match walk(e, self_name, self_arity, env, numeric) {
        Ret::Kind(k) => k,
        Ret::SelfOrFail | Ret::Unknown => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(f: Core, args: Vec<Core>) -> Core {
        args.into_iter().fold(f, |acc, a| Core::App(Box::new(acc), Box::new(a)))
    }

    #[test]
    fn lifts_local_recursive_loop_with_numeric_params_and_return() {
        // Test.sum n = let go = \i acc -> case i <=# n of 1# -> case acc +# i of acc' -> go (i +# 1#) acc'; _ -> acc in go 1# 0#
        let go_body = Core::Case(
            Box::new(Core::Prim("<=#".into(), vec![Core::Var("i%2".into()), Core::Var("n%1".into())])),
            "c%4".into(),
            vec![
                Alt::Lit(
                    Lit::Int(1),
                    Core::Case(
                        Box::new(Core::Prim("+#".into(), vec![Core::Var("acc%5".into()), Core::Var("i%2".into())])),
                        "acc'%6".into(),
                        vec![Alt::Default(app(
                            Core::Var("go%3".into()),
                            vec![
                                Core::Prim("+#".into(), vec![Core::Var("i%2".into()), Core::Lit(Lit::Int(1))]),
                                Core::Var("acc'%6".into()),
                            ],
                        ))],
                    ),
                ),
                Alt::Default(Core::Var("acc%5".into())),
            ],
        );
        let top = TopBind {
            name: "Test.sum".into(),
            body: Core::Lam(
                "n%1".into(),
                Box::new(Core::Let(
                    vec![(
                        "go%3".into(),
                        Core::Lam("i%2".into(), Box::new(Core::Lam("acc%5".into(), Box::new(go_body)))),
                    )],
                    Box::new(app(Core::Var("go%3".into()), vec![Core::Lit(Lit::Int(1)), Core::Lit(Lit::Int(0))])),
                )),
            ),
        };
        let mut binds = vec![top];
        let mut numeric = NumericParams::default();
        numeric.params.insert("Test.sum".into(), vec![NUM_INT]);
        numeric.results.insert("Test.sum".into(), (1, NUM_INT));

        lambda_lift(&mut binds, &mut numeric);

        assert_eq!(binds.len(), 2);
        let lifted = &binds[1];
        assert!(lifted.name.starts_with("Test.sum$lift$go$"));
        assert_eq!(numeric.params.get(&lifted.name), Some(&vec![NUM_INT, NUM_INT, NUM_INT]));
        assert_eq!(numeric.results.get(&lifted.name), Some(&(3, NUM_INT)));
        // Lowering to STG should give the lifted loop a 3-raw-param worker returning raw.
        let prog = crate::native::stg::Lowering::program(&binds, &numeric);
        let (wc, raw) = prog.globals[1].worker.as_ref().expect("lifted loop gets worker");
        assert_eq!(raw, &vec![true, true, true]);
        assert!(wc.ret_raw);
        assert_eq!(wc.arity, 0);
        assert_eq!(wc.raw_args.len(), 3);
    }

    #[test]
    fn non_numeric_recursive_argument_prevents_unsound_numeric_inference() {
        // go is initially called with 1#, but recursively passes an unknown variable `u%9`.
        let go_body = Core::Case(
            Box::new(Core::Var("x%2".into())),
            "b%3".into(),
            vec![
                Alt::Lit(Lit::Int(0), Core::Lit(Lit::Int(0))),
                Alt::Default(app(Core::Var("go%1".into()), vec![Core::Var("u%9".into())])),
            ],
        );
        let top = TopBind {
            name: "Test.bad".into(),
            body: Core::Lam(
                "u%9".into(),
                Box::new(Core::Let(
                    vec![("go%1".into(), Core::Lam("x%2".into(), Box::new(go_body)))],
                    Box::new(app(Core::Var("go%1".into()), vec![Core::Lit(Lit::Int(1))])),
                )),
            ),
        };
        let mut binds = vec![top];
        let mut numeric = NumericParams::default();

        lambda_lift(&mut binds, &mut numeric);

        assert_eq!(binds.len(), 2);
        let lifted = &binds[1];
        assert!(!numeric.params.contains_key(&lifted.name));
    }

    #[test]
    fn join_points_are_not_lifted() {
        let top = TopBind {
            name: "Test.jp".into(),
            body: Core::Lam(
                "x%1".into(),
                Box::new(Core::Let(
                    vec![("j%2".into(), Core::Lam("y%3".into(), Box::new(Core::Var("y%3".into()))))],
                    Box::new(Core::Case(
                        Box::new(Core::Var("x%1".into())),
                        "b%4".into(),
                        vec![
                            Alt::Lit(Lit::Int(0), app(Core::Var("j%2".into()), vec![Core::Lit(Lit::Int(1))])),
                            Alt::Default(app(Core::Var("j%2".into()), vec![Core::Lit(Lit::Int(2))])),
                        ],
                    )),
                )),
            ),
        };
        let mut binds = vec![top];
        let mut numeric = NumericParams::default();
        lambda_lift(&mut binds, &mut numeric);
        assert_eq!(binds.len(), 1);
    }
}
