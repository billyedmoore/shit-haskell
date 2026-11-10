//! Binding groups: dependency analysis, generalisation, and checking against signatures.

use std::collections::{HashMap, HashSet};

use super::expr::{qkey, wrap_let, STRICT_BIND};
use super::types::{subst_gen, Pred, Scheme, Ty};
use super::{Checker, Given, Wanted, TR};
use crate::core::matcher::{compile_body, compile_rows, CBody, Row};
use crate::core::Core;
use crate::syntax::ast::{Alt, Body, Clause, Decl, Expr, Loc, Pat, Rhs, Stmt};

enum Kind<'d> {
    Fun(&'d [Clause]),
    Pat { pat: Pat, rhs: Rhs },
}

struct Item<'d> {
    /// Binder keys defined by this item.
    keys: Vec<String>,
    /// The occurrence name, for messages.
    occ: String,
    kind: Kind<'d>,
    loc: Loc,
    /// Whether the item has a type signature.
    signed: bool,
}

impl Checker {
    /// Elaborates a binding group. For top-level groups, names are qualified with the
    /// module, and the returned bindings are global.
    pub fn infer_binds(&mut self, decls: &[Decl], top: bool) -> TR<Vec<(String, Core)>> {
        let module = self.module.clone();
        let key_of = |name: &str| {
            if top {
                format!("{module}.{name}")
            } else {
                name.to_string()
            }
        };

        // Signatures, by occurrence name.
        let mut sigs: HashMap<String, Scheme> = HashMap::new();
        for d in decls {
            if let Decl::Sig { names, ty, loc } = d {
                self.loc = *loc;
                let scheme = self.sig_scheme(ty, &[])?;
                let explicit = matches!(ty, crate::syntax::ast::Type::Forall(..));
                for n in names {
                    if explicit {
                        self.explicit_sigs.insert(if top { format!("{module}.{n}") } else { n.clone() });
                    }
                    sigs.insert(n.clone(), scheme.clone());
                }
            }
        }

        let mut items: Vec<Item> = Vec::new();
        for d in decls {
            match d {
                Decl::Fun { name, clauses, loc } => items.push(Item {
                    keys: vec![key_of(name)],
                    occ: name.clone(),
                    kind: Kind::Fun(clauses),
                    loc: *loc,
                    signed: sigs.contains_key(name),
                }),
                Decl::PatBind { pat, rhs, loc } => {
                    let pat = if top { qualify_pat(pat, &module) } else { pat.clone() };
                    let mut keys = Vec::new();
                    crate::rename::pat_vars(&pat)
                        .into_iter()
                        .for_each(|(v, _)| keys.push(v));
                    items.push(Item {
                        keys,
                        occ: String::new(),
                        kind: Kind::Pat {
                            pat,
                            rhs: rhs.clone(),
                        },
                        loc: *loc,
                        signed: false,
                    });
                }
                _ => {}
            }
        }

        // Signed functions are in scope at their declared type before any inference.
        for it in &items {
            if it.signed {
                let scheme = sigs[&it.occ].clone();
                self.vars.insert(it.keys[0].clone(), scheme);
            }
        }

        // Dependency graph among the unsigned items.
        let unsigned: Vec<usize> = (0..items.len()).filter(|&i| !items[i].signed).collect();
        let mut owner: HashMap<String, usize> = HashMap::new();
        for (pos, &i) in unsigned.iter().enumerate() {
            for k in &items[i].keys {
                owner.insert(k.clone(), pos);
            }
        }
        let mut edges: Vec<Vec<usize>> = vec![Vec::new(); unsigned.len()];
        for (pos, &i) in unsigned.iter().enumerate() {
            let mut refs = HashSet::new();
            match &items[i].kind {
                Kind::Fun(clauses) => clauses.iter().for_each(|c| collect_clause(c, &mut refs)),
                Kind::Pat { pat, rhs } => {
                    collect_pat(pat, &mut refs);
                    collect_rhs(rhs, &mut refs);
                }
            }
            for r in refs {
                if let Some(&target) = owner.get(&r) {
                    if !edges[pos].contains(&target) {
                        edges[pos].push(target);
                    }
                }
            }
        }

        let mut out = Vec::new();
        for scc in tarjan(unsigned.len(), &edges) {
            let group: Vec<&Item> = scc.iter().map(|&p| &items[unsigned[p]]).collect();
            out.extend(self.infer_scc(&group)?);
        }
        for it in &items {
            if let (true, Kind::Fun(clauses)) = (it.signed, &it.kind) {
                let scheme = sigs[&it.occ].clone();
                out.extend(self.check_signed(&it.keys[0], clauses, &scheme, it.loc)?);
            }
        }
        Ok(out)
    }

    /// Infers a strongly connected group of unsigned bindings, and generalises it.
    fn infer_scc(&mut self, items: &[&Item]) -> TR<Vec<(String, Core)>> {
        let outer_level = self.level;
        self.level += 1;
        let outer_wanted = std::mem::take(&mut self.wanted);

        let mut mono: Vec<(String, Ty)> = Vec::new();
        for it in items {
            for k in &it.keys {
                let t = self.meta();
                self.vars.insert(k.clone(), Scheme::mono(t.clone()));
                mono.push((k.clone(), t));
            }
        }
        let mono_of = |k: &str| mono.iter().find(|(n, _)| n == k).expect("binder").1.clone();

        let mut cores: Vec<(String, Core)> = Vec::new();
        for it in items {
            self.loc = it.loc;
            match &it.kind {
                Kind::Fun(clauses) => {
                    let key = it.keys[0].clone();
                    let ty = mono_of(&key);
                    let core = self.fun_clauses(clauses, &ty, &it.occ)?;
                    cores.push((key, core));
                }
                Kind::Pat { pat, rhs } => {
                    let t = self.meta();
                    let body = self.rhs_body(rhs, &t)?;
                    let cpat = self.check_pat(pat, &t)?;
                    for k in &it.keys {
                        let bound = self.vars[k].ty.clone();
                        self.unify_at(&bound, &mono_of(k))?;
                    }
                    let s = self.fresh_name("pb");
                    let mut fresh = |b: &str| self.fresh_name(b);
                    let rhs_core = compile_body(&body, Core::Fail("Non-exhaustive guards in pattern binding".into()), &mut fresh);
                    cores.push((s.clone(), rhs_core));
                    // A banged pattern binding is evaluated before the body: its alias is forced
                    // by `wrap_let`, which is what makes `let !(# s, () #) = io s0` run its effect.
                    if matches!(pat, Pat::Bang(..)) {
                        cores.push((format!("{STRICT_BIND}{s}"), Core::Var(s.clone())));
                    }
                    for k in &it.keys {
                        let extract = compile_rows(
                            std::slice::from_ref(&s),
                            &[Row {
                                pats: vec![cpat.clone()],
                                body: CBody::Expr(Core::Var(k.clone())),
                            }],
                            Core::Fail("Irrefutable pattern failed".into()),
                            &mut fresh,
                        );
                        cores.push((k.clone(), extract));
                    }
                }
            }
        }

        let wanteds = std::mem::replace(&mut self.wanted, outer_wanted);
        self.level = outer_level;
        let (ev_binds, residual) = self.solve(wanteds)?;

        let restricted = items.iter().any(|it| match &it.kind {
            Kind::Pat { .. } => true,
            Kind::Fun(clauses) => clauses.first().is_some_and(|c| c.pats.is_empty()),
        });

        // Metas of the group's types that are local to it can be generalised.
        let mut qmetas: Vec<u32> = Vec::new();
        for (_, t) in &mono {
            let mut ms = Vec::new();
            self.metas.free_metas(t, &mut ms);
            for m in ms {
                if self.metas.levels[m as usize] > outer_level && !qmetas.contains(&m) {
                    qmetas.push(m);
                }
            }
        }

        let mut quantified: Vec<Wanted> = Vec::new();
        let mut floated: Vec<Wanted> = Vec::new();
        for w in residual {
            let p = self.metas_zonk_pred(&w.pred);
            let mut ms = Vec::new();
            for a in &p.args {
                self.metas.free_metas(a, &mut ms);
            }
            if restricted {
                // The monomorphism restriction: constrained variables are not generalised.
                for m in &ms {
                    self.metas.levels[*m as usize] = outer_level;
                    qmetas.retain(|x| x != m);
                }
                floated.push(w);
            } else if !ms.is_empty() && ms.iter().all(|m| qmetas.contains(m)) {
                quantified.push(w);
            } else {
                for m in &ms {
                    if self.metas.levels[*m as usize] > outer_level {
                        self.metas.levels[*m as usize] = outer_level;
                    }
                }
                floated.push(w);
            }
        }
        self.wanted.extend(floated);

        // Quantified predicates become dictionary parameters.
        let mut qpreds: Vec<Pred> = Vec::new();
        let mut dparams: Vec<String> = Vec::new();
        let mut ev_map: Vec<(String, Core)> = Vec::new();
        for w in quantified {
            let p = self.metas_zonk_pred(&w.pred);
            let idx = match qpreds.iter().position(|q| *q == p) {
                Some(i) => i,
                None => {
                    qpreds.push(p);
                    dparams.push(self.fresh_name("dq"));
                    qpreds.len() - 1
                }
            };
            ev_map.push((w.ev, Core::Var(dparams[idx].clone())));
        }

        let gen = |t: &Ty, metas: &Metas| -> Ty { generalise_ty(metas, t, &qmetas) };
        let names: Vec<String> = (0..qmetas.len()).map(|i| format!("t{i}")).collect();
        for (k, t) in &mono {
            let ty = gen(t, &self.metas);
            let preds: Vec<Pred> = qpreds
                .iter()
                .map(|p| Pred {
                    class: p.class.clone(),
                    args: p.args.iter().map(|a| generalise_ty(&self.metas, a, &qmetas)).collect(),
                })
                .collect();
            self.vars.insert(
                k.clone(),
                Scheme {
                    nvars: qmetas.len() as u32,
                    names: names.clone(),
                    preds,
                    ty,
                },
            );
        }

        let mut ev_binds = ev_binds;
        ev_binds.extend(ev_map);
        if qpreds.is_empty() {
            let mut all = cores;
            all.extend(ev_binds);
            return Ok(all);
        }
        // Dictionary abstraction: each binder becomes a function of the dictionaries,
        // whose body is a local recursive group with monomorphic recursion.
        let mut group = cores.clone();
        group.extend(ev_binds);
        let mut out = Vec::with_capacity(cores.len());
        for (k, _) in &cores {
            let body = Core::Let(group.clone(), Box::new(Core::Var(k.clone())));
            let lam = dparams.iter().rev().fold(body, |acc, d| Core::Lam(d.clone(), Box::new(acc)));
            out.push((k.clone(), lam));
        }
        Ok(out)
    }

    /// Checks a binding with a signature: its type variables are rigid, and its
    /// predicates are givens for the body.
    pub fn check_signed(&mut self, key: &str, clauses: &[Clause], scheme: &Scheme, loc: Loc) -> TR<Vec<(String, Core)>> {
        // GADT refinements made by the binding's patterns end with it.
        let refinements = self.metas.skol_binds.clone();
        let outer_flex = std::mem::take(&mut self.flex_kinds);
        let r = self.check_signed_body(key, clauses, scheme, loc);
        self.flex_kinds = outer_flex;
        self.metas.skol_binds = refinements;
        r
    }

    fn check_signed_body(&mut self, key: &str, clauses: &[Clause], scheme: &Scheme, loc: Loc) -> TR<Vec<(String, Core)>> {
        self.loc = loc;
        let outer_level = self.level;
        self.level += 1;
        // Kind variables (`k`, `k1`, `rep`, ...) are not tracked by kind inference, so they are
        // flexible here rather than rigid: a kind-polymorphic signature is then not refused.
        let skolems: Vec<Ty> = scheme
            .names
            .iter()
            .take(scheme.nvars as usize)
            .map(|n| {
                if is_kind_var_name(n) {
                    let m = self.metas.fresh(self.level);
                    if let Ty::Meta(i) = m {
                        self.flex_kinds.push(i);
                    }
                    m
                } else {
                    self.metas.fresh_skolem(n)
                }
            })
            .collect();
        let ty = subst_gen(&scheme.ty, &skolems);
        let preds: Vec<Pred> = scheme
            .preds
            .iter()
            .map(|p| Pred {
                class: p.class.clone(),
                args: p.args.iter().map(|a| subst_gen(a, &skolems)).collect(),
            })
            .collect();
        let dparams: Vec<String> = preds.iter().map(|_| self.fresh_name("dg")).collect();
        let givens_len = self.givens.len();
        for (p, d) in preds.iter().zip(&dparams) {
            self.add_given(Given {
                pred: p.clone(),
                ev: Core::Var(d.clone()),
            });
        }
        // With an explicit forall, the signature's variables scope over the body.
        let scope_mark = self.scoped.len();
        if self.explicit_sigs.contains(key) {
            for (n, sk) in scheme.names.iter().zip(&skolems) {
                self.scoped.push((n.clone(), sk.clone()));
            }
        }
        let outer_wanted = std::mem::take(&mut self.wanted);
        let body = self.fun_clauses(clauses, &ty, key)?;
        self.scoped.truncate(scope_mark);
        let wanted = std::mem::replace(&mut self.wanted, outer_wanted);
        self.level = outer_level;

        let (ev_binds, residual) = self.solve(wanted)?;
        for w in residual {
            let p = self.metas_zonk_pred(&w.pred);
            let mentions_skolem = p.args.iter().any(|a| mentions(a, &skolems));
            if mentions_skolem {
                self.givens.truncate(givens_len);
                return Err(super::TcError {
                    loc: w.loc,
                    msg: format!(
                        "Could not deduce ({}) from the context of the type signature for ‘{}’",
                        self.show_pred(&p),
                        super::types::unqualify(key)
                    ),
                });
            }
            self.wanted.push(w);
        }
        self.givens.truncate(givens_len);

        let inner = wrap_let(ev_binds, body);
        let core = dparams.into_iter().rev().fold(inner, |acc, d| Core::Lam(d, Box::new(acc)));
        Ok(vec![(key.to_string(), core)])
    }

    /// Core for a function's clauses against a type. Zero-argument bindings are values.
    pub fn fun_clauses(&mut self, clauses: &[Clause], ty: &Ty, name: &str) -> TR<Core> {
        let arity = clauses.first().map(|c| c.pats.len()).unwrap_or(0);
        if arity == 0 {
            let clause = clauses.first().expect("at least one clause");
            self.loc = clause.loc;
            let body = self.rhs_body(&clause.rhs, ty)?;
            let mut fresh = |b: &str| self.fresh_name(b);
            return Ok(compile_body(
                &body,
                Core::Fail(format!("Non-exhaustive guards in {}", super::types::unqualify(name))),
                &mut fresh,
            ));
        }
        let mut arg_tys = Vec::with_capacity(arity);
        let mut cur = ty.clone();
        for _ in 0..arity {
            let split = self.metas.prune(&cur).as_fun().map(|(a, r)| (a.clone(), r.clone()));
            let (a, r) = match split {
                Some(p) => p,
                None => {
                    let a = self.meta();
                    let r = self.meta();
                    self.unify_at(&cur, &Ty::fun(a.clone(), r.clone()))?;
                    (a, r)
                }
            };
            arg_tys.push(a);
            cur = r;
        }
        let res = cur;
        let scruts: Vec<String> = (0..arity).map(|_| self.fresh_name("arg")).collect();
        let mut rows = Vec::with_capacity(clauses.len());
        for c in clauses {
            self.loc = c.loc;
            let mark = self.alt_begin();
            let pats = self.check_pats(&c.pats, &arg_tys)?;
            let body = self.rhs_body(&c.rhs, &res)?;
            let body = self.alt_end(mark, body)?;
            for m in self.flex_kinds.clone() {
                self.metas.subst[m as usize] = None;
            }
            rows.push(Row { pats, body });
        }
        let mut fresh = |b: &str| self.fresh_name(b);
        let matched = compile_rows(
            &scruts,
            &rows,
            Core::Fail(format!(
                "Non-exhaustive patterns in function {}",
                super::types::unqualify(name)
            )),
            &mut fresh,
        );
        Ok(scruts.into_iter().rev().fold(matched, |acc, s| Core::Lam(s, Box::new(acc))))
    }
}

use super::types::Metas;

/// Replaces the listed metas with generic variables, in list order.
fn generalise_ty(metas: &Metas, t: &Ty, qmetas: &[u32]) -> Ty {
    match metas.prune(t) {
        Ty::Meta(m) => match qmetas.iter().position(|q| *q == m) {
            Some(i) => Ty::Gen(i as u32),
            None => Ty::Meta(m),
        },
        Ty::App(f, a) => Ty::app(generalise_ty(metas, &f, qmetas), generalise_ty(metas, &a, qmetas)),
        other => other,
    }
}

fn mentions(t: &Ty, skolems: &[Ty]) -> bool {
    match t {
        Ty::Skol(..) => skolems.contains(t),
        Ty::App(f, a) => mentions(f, skolems) || mentions(a, skolems),
        _ => false,
    }
}

/// Rewrites the variables of a top-level pattern binding to their qualified names.
fn qualify_pat(p: &Pat, module: &str) -> Pat {
    let q = |v: &str| format!("{module}.{v}");
    match p {
        Pat::Var(v, l) => Pat::Var(q(v), *l),
        Pat::As(v, sub, l) => Pat::As(q(v), Box::new(qualify_pat(sub, module)), *l),
        Pat::Con(c, ps, l) => Pat::Con(c.clone(), ps.iter().map(|x| qualify_pat(x, module)).collect(), *l),
        Pat::Tuple(ps, l) => Pat::Tuple(ps.iter().map(|x| qualify_pat(x, module)).collect(), *l),
        Pat::List(ps, l) => Pat::List(ps.iter().map(|x| qualify_pat(x, module)).collect(), *l),
        Pat::Lazy(sub, l) => Pat::Lazy(Box::new(qualify_pat(sub, module)), *l),
        Pat::Bang(sub, l) => Pat::Bang(Box::new(qualify_pat(sub, module)), *l),
        Pat::Sig(sub, t, l) => Pat::Sig(Box::new(qualify_pat(sub, module)), t.clone(), *l),
        Pat::View(e, sub, l) => Pat::View(e.clone(), Box::new(qualify_pat(sub, module)), *l),
        Pat::Rec { con, fields, dotdot, loc } => Pat::Rec {
            con: con.clone(),
            fields: fields
                .iter()
                .map(|(f, s)| (f.clone(), s.as_ref().map(|x| qualify_pat(x, module))))
                .collect(),
            dotdot: *dotdot,
            loc: *loc,
        },
        other => other.clone(),
    }
}

/// Strongly connected components, dependencies first (Tarjan's algorithm).
fn tarjan(n: usize, edges: &[Vec<usize>]) -> Vec<Vec<usize>> {
    struct St {
        index: Vec<Option<usize>>,
        low: Vec<usize>,
        on: Vec<bool>,
        stack: Vec<usize>,
        next: usize,
        out: Vec<Vec<usize>>,
    }
    fn visit(v: usize, edges: &[Vec<usize>], s: &mut St) {
        s.index[v] = Some(s.next);
        s.low[v] = s.next;
        s.next += 1;
        s.stack.push(v);
        s.on[v] = true;
        for &w in &edges[v] {
            match s.index[w] {
                None => {
                    visit(w, edges, s);
                    s.low[v] = s.low[v].min(s.low[w]);
                }
                Some(iw) if s.on[w] => s.low[v] = s.low[v].min(iw),
                _ => {}
            }
        }
        if Some(s.low[v]) == s.index[v] {
            let mut comp = Vec::new();
            loop {
                let w = s.stack.pop().expect("stack");
                s.on[w] = false;
                comp.push(w);
                if w == v {
                    break;
                }
            }
            comp.sort_unstable();
            s.out.push(comp);
        }
    }
    let mut s = St {
        index: vec![None; n],
        low: vec![0; n],
        on: vec![false; n],
        stack: Vec::new(),
        next: 0,
        out: Vec::new(),
    };
    for v in 0..n {
        if s.index[v].is_none() {
            visit(v, edges, &mut s);
        }
    }
    s.out
}

// ----- free variables, for dependency analysis ------------------------------

fn collect_clause(c: &Clause, out: &mut HashSet<String>) {
    c.pats.iter().for_each(|p| collect_pat(p, out));
    collect_rhs(&c.rhs, out);
}

fn collect_rhs(rhs: &Rhs, out: &mut HashSet<String>) {
    rhs.wheres.iter().for_each(|d| collect_decl(d, out));
    match &rhs.body {
        Body::Plain(e) => collect_expr(e, out),
        Body::Guarded(alts) => {
            for (stmts, e) in alts {
                stmts.iter().for_each(|s| collect_stmt(s, out));
                collect_expr(e, out);
            }
        }
    }
}

fn collect_decl(d: &Decl, out: &mut HashSet<String>) {
    match d {
        Decl::Fun { clauses, .. } => clauses.iter().for_each(|c| collect_clause(c, out)),
        Decl::PatBind { pat, rhs, .. } => {
            collect_pat(pat, out);
            collect_rhs(rhs, out);
        }
        _ => {}
    }
}

fn collect_stmt(s: &Stmt, out: &mut HashSet<String>) {
    match s {
        Stmt::Bind { pat, expr, .. } => {
            collect_pat(pat, out);
            collect_expr(expr, out);
        }
        Stmt::Let { decls, .. } => decls.iter().for_each(|d| collect_decl(d, out)),
        Stmt::Expr { expr, .. } => collect_expr(expr, out),
    }
}

fn collect_alt(a: &Alt, out: &mut HashSet<String>) {
    collect_pat(&a.pat, out);
    collect_rhs(&a.rhs, out);
}

fn collect_pat(p: &Pat, out: &mut HashSet<String>) {
    match p {
        Pat::View(e, sub, _) => {
            collect_expr(e, out);
            collect_pat(sub, out);
        }
        Pat::Con(_, ps, _) | Pat::Tuple(ps, _) | Pat::List(ps, _) => ps.iter().for_each(|x| collect_pat(x, out)),
        Pat::As(_, sub, _) | Pat::Lazy(sub, _) | Pat::Bang(sub, _) | Pat::Sig(sub, _, _) => collect_pat(sub, out),
        Pat::Rec { fields, .. } => {
            for (_, s) in fields {
                if let Some(s) = s {
                    collect_pat(s, out);
                }
            }
        }
        _ => {}
    }
}

fn collect_expr(e: &Expr, out: &mut HashSet<String>) {
    match e {
        Expr::Var(q, _) => {
            out.insert(qkey(q));
        }
        Expr::IPVar(..) => {}
        Expr::Con(..) | Expr::Lit(..) | Expr::Wild(_) => {}
        Expr::App(f, a) => {
            collect_expr(f, out);
            collect_expr(a, out);
        }
        Expr::TyApp(e, _, _) | Expr::Sig(e, _, _) | Expr::AsPat(_, e, _) | Expr::LazyPat(e, _) | Expr::BangPat(e, _) => {
            collect_expr(e, out)
        }
        Expr::ViewPat(f, p, _) => {
            collect_expr(f, out);
            collect_expr(p, out);
        }
        Expr::OpChain(items, _) => {
            for it in items {
                match it {
                    crate::syntax::ast::OpItem::Operand(x) => collect_expr(x, out),
                    crate::syntax::ast::OpItem::Op(q, _) => {
                        out.insert(qkey(q));
                    }
                    crate::syntax::ast::OpItem::Neg(_) => {}
                }
            }
        }
        Expr::Lam(pats, body, _) => {
            pats.iter().for_each(|p| collect_pat(p, out));
            collect_expr(body, out);
        }
        Expr::Let(decls, body, _) => {
            decls.iter().for_each(|d| collect_decl(d, out));
            collect_expr(body, out);
        }
        Expr::If(c, t, f, _) => {
            collect_expr(c, out);
            collect_expr(t, out);
            collect_expr(f, out);
        }
        Expr::MultiIf(alts, _) => {
            for (stmts, e) in alts {
                stmts.iter().for_each(|s| collect_stmt(s, out));
                collect_expr(e, out);
            }
        }
        Expr::Case(s, alts, _) => {
            collect_expr(s, out);
            alts.iter().for_each(|a| collect_alt(a, out));
        }
        Expr::LamCase(alts, _) => alts.iter().for_each(|a| collect_alt(a, out)),
        Expr::Do(stmts, _) => stmts.iter().for_each(|s| collect_stmt(s, out)),
        Expr::Tuple(elems, _) => elems.iter().flatten().for_each(|x| collect_expr(x, out)),
        Expr::List(elems, _) => elems.iter().for_each(|x| collect_expr(x, out)),
        Expr::ArithSeq { from, then, to, .. } => {
            collect_expr(from, out);
            if let Some(t) = then {
                collect_expr(t, out);
            }
            if let Some(t) = to {
                collect_expr(t, out);
            }
        }
        Expr::ListComp(e, branches, _) => {
            collect_expr(e, out);
            for b in branches {
                b.iter().for_each(|s| collect_stmt(s, out));
            }
        }
        Expr::LeftSection(e, op, _) | Expr::RightSection(op, e, _) => {
            out.insert(qkey(op));
            collect_expr(e, out);
        }
        Expr::RecCon { fields, .. } => {
            for (f, v) in fields {
                match v {
                    Some(x) => collect_expr(x, out),
                    None => {
                        out.insert(f.name.clone());
                    }
                }
            }
        }
        Expr::RecUpd(e, fields, _) => {
            collect_expr(e, out);
            for (f, v) in fields {
                match v {
                    Some(x) => collect_expr(x, out),
                    None => {
                        out.insert(f.name.clone());
                    }
                }
            }
        }
    }
}

/// Names conventionally given to kind and representation variables in the libraries.
fn is_kind_var_name(n: &str) -> bool {
    let digits = |rest: &str| rest.chars().all(|c| c.is_ascii_digit());
    match n.chars().next() {
        Some('k') => digits(&n[1..]),
        Some('r') => n == "rep" || n == "rr" || (n.len() > 1 && digits(&n[1..])),
        _ => false,
    }
}
