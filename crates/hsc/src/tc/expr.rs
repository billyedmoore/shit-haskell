//! Expression and pattern checking, with elaboration to Core.

use std::collections::HashMap;

use super::types::{subst_gen, subst_gen_pred, Pred, Scheme, Ty};
use super::{Checker, TR, Wanted};
use crate::core::matcher::{compile_body, compile_rows, CBody, CGuard, CPat, Row};
use crate::core::{Alt, Core, Lit as CLit};
use crate::syntax::ast::{Alt as AstAlt, Body, Clause, Decl, Expr, Lit, Loc, Pat, QName, Rhs, Stmt, Type};

/// The qualified key of a renamed name: `M.x` for globals, the unique name for locals.
pub fn qkey(q: &QName) -> String {
    match &q.qual {
        Some(m) => format!("{m}.{}", q.name),
        None => q.name.clone(),
    }
}

fn prelude(name: &str, loc: Loc) -> Expr {
    Expr::Var(
        QName {
            qual: Some(super::PRELUDE_MODULE.into()),
            name: name.into(),
        },
        loc,
    )
}

fn var(name: &str, loc: Loc) -> Expr {
    Expr::Var(QName::unqual(name), loc)
}

fn app(f: Expr, a: Expr) -> Expr {
    Expr::App(Box::new(f), Box::new(a))
}

fn lam1(p: Pat, body: Expr, loc: Loc) -> Expr {
    Expr::Lam(vec![p], Box::new(body), loc)
}

impl Checker {
    /// Checks an expression against a known type.
    pub fn check(&mut self, e: &Expr, ty: &Ty) -> TR<Core> {
        // The expected type reaches the alternatives, so that a GADT match can refine it there.
        match e {
            Expr::Case(scrut, alts, loc) => {
                self.loc = *loc;
                let (cs, ts) = self.infer(scrut)?;
                let s = self.fresh_name("scrut");
                let rows = self.alt_rows(alts, &ts, ty)?;
                let mut fresh = |b: &str| self.fresh_name(b);
                let matched = compile_rows(
                    std::slice::from_ref(&s),
                    &rows,
                    Core::Fail("Non-exhaustive patterns in case".into()),
                    &mut fresh,
                );
                Ok(self.bind_scrutinee(&ts, s, cs, matched))
            }
            // `\case` against a function type: its argument and result types reach the
            // alternatives (a GADT or data-family pattern needs them).
            Expr::LamCase(alts, loc) if self.metas.zonk(ty).as_fun().is_some() => {
                self.loc = *loc;
                let zt = self.metas.zonk(ty);
                let (a, r) = zt.as_fun().map(|(a, r)| (a.clone(), r.clone())).expect("a function type");
                let x = self.fresh_name("lc");
                let rows = self.alt_rows(alts, &a, &r)?;
                let mut fresh = |b: &str| self.fresh_name(b);
                let matched = compile_rows(
                    std::slice::from_ref(&x),
                    &rows,
                    Core::Fail("Non-exhaustive patterns in \\case".into()),
                    &mut fresh,
                );
                Ok(Core::Lam(x, Box::new(matched)))
            }
            Expr::Let(decls, body, loc) if !decls.iter().any(|d| matches!(d, Decl::IPBind { .. })) => {
                self.loc = *loc;
                let (mut binds, deferred) = self.infer_local_deferring(decls)?;
                let cb = self.check(body, ty)?;
                binds.extend(self.finish_deferred(deferred)?);
                Ok(self.wrap_let_forcing(binds, cb))
            }
            Expr::App(..) => Ok(self.infer_spine(e, Some(ty))?.0),
            // A record construction against its expected type: the result type is known before
            // the fields are checked, so that a field's family type can reduce.
            Expr::RecCon { con, fields, loc, .. } if self.env.datacons.contains_key(&qkey(con)) => {
                self.loc = *loc;
                let (core, t) = self.infer_rec_con_at(con, fields, *loc, Some(ty))?;
                self.unify_at(ty, &t)?;
                Ok(core)
            }
            // A comprehension is checked at its expected type, so that the refinement of a generator's
            // pattern (a GADT match) reaches the element expression.
            Expr::ListComp(body, branches, loc) if branches.len() == 1 => {
                self.loc = *loc;
                let desugared = self.desugar_comp(body, &branches[0], *loc);
                self.check(&desugared, ty)
            }
            Expr::Lam(pats, body, loc) => Ok(self.lambda(pats, body, *loc, Some(ty))?.0),
            _ => {
                let (c, t) = self.infer(e)?;
                self.unify_at(&t, ty)?;
                Ok(c)
            }
        }
    }

    /// A `let`: the bindings of an unlifted type (`s2 = poke p w s1 :: State# s`) are evaluated
    /// before the body, in order, as GHC evaluates them; the others stay lazy.
    fn wrap_let_forcing(&mut self, binds: Vec<(String, Core)>, body: Core) -> Core {
        let unlifted: Vec<String> = binds
            .iter()
            .filter(|(n, _)| !n.starts_with(STRICT_BIND))
            .filter(|(n, _)| {
                // Only a state token carries an effect; other unlifted values are pure.
                self.vars.get(n).cloned().is_some_and(|s| {
                    s.nvars == 0
                        && s.preds.is_empty()
                        && matches!(self.metas.zonk(&s.ty).split_apps().0, Ty::Con(c) if c == "GHC.Prim.State#")
                })
            })
            .map(|(n, _)| n.clone())
            .collect();
        if unlifted.is_empty() {
            return wrap_let(binds, body);
        }
        // Scheduled by their dependencies: the lazy bindings that need no unlifted one not yet
        // bound form a `let`, then each unlifted one whose dependencies are bound is `case rhs of
        // n -> ..`, and so on.
        let names: std::collections::HashSet<String> = binds.iter().map(|(n, _)| n.clone()).collect();
        let deps: std::collections::HashMap<String, Vec<String>> = binds
            .iter()
            .map(|(n, rhs)| {
                let mut out = Vec::new();
                crate::native::mentions(rhs, &mut out);
                let ds: Vec<String> = out.into_iter().filter(|m| names.contains(*m) && *m != n.as_str()).map(str::to_string).collect();
                (n.clone(), ds)
            })
            .collect();
        enum Step {
            Lazy(Vec<(String, Core)>),
            Strict(String, Core),
        }
        let mut remaining = binds;
        let mut steps: Vec<Step> = Vec::new();
        let mut bound: std::collections::HashSet<String> = std::collections::HashSet::new();
        while !remaining.is_empty() {
            // The lazy bindings that do not need an unbound unlifted one, directly or through others.
            let mut blocked: std::collections::HashSet<String> =
                remaining.iter().filter(|(n, _)| unlifted.contains(n)).map(|(n, _)| n.clone()).collect();
            loop {
                let more: Vec<String> = remaining
                    .iter()
                    .filter(|(n, _)| !blocked.contains(n) && deps[n].iter().any(|d| blocked.contains(d)))
                    .map(|(n, _)| n.clone())
                    .collect();
                if more.is_empty() {
                    break;
                }
                blocked.extend(more);
            }
            let (ready, rest): (Vec<_>, Vec<_>) = remaining.into_iter().partition(|(n, _)| !blocked.contains(n));
            remaining = rest;
            if !ready.is_empty() {
                bound.extend(ready.iter().map(|(n, _)| n.clone()));
                steps.push(Step::Lazy(ready));
            }
            // The unlifted bindings whose dependencies are all bound now.
            let mut progressed = false;
            let mut i = 0;
            while i < remaining.len() {
                let n = remaining[i].0.clone();
                if unlifted.contains(&n) && deps[&n].iter().all(|d| bound.contains(d)) {
                    let (n, rhs) = remaining.remove(i);
                    bound.insert(n.clone());
                    steps.push(Step::Strict(n, rhs));
                    progressed = true;
                } else {
                    i += 1;
                }
            }
            if !progressed && !remaining.is_empty() {
                // A cycle through an unlifted binding: left lazy.
                steps.push(Step::Lazy(std::mem::take(&mut remaining)));
            }
        }
        let mut body = body;
        for st in steps.into_iter().rev() {
            body = match st {
                Step::Strict(n, rhs) => Core::Case(Box::new(rhs), n, vec![Alt::Default(body)]),
                Step::Lazy(bs) => wrap_let(bs, body),
            };
        }
        body
    }

    /// Binds a case scrutinee to its variable. A scrutinee of an unlifted type (`State#`, `Int#`, ...)
    /// is evaluated first, even when the alternatives only bind it: the primitive it comes from has
    /// its effect when it is run, as in GHC. A lifted scrutinee stays lazy.
    fn bind_scrutinee(&mut self, ts: &Ty, s: String, cs: Core, body: Core) -> Core {
        if self.is_unlifted(ts) {
            Core::Case(Box::new(cs), s, vec![crate::core::Alt::Default(body)])
        } else {
            Core::Let(vec![(s, cs)], Box::new(body))
        }
    }

    /// A lambda. With an expected type its parameters and body are checked against the parts of
    /// that type, so that a GADT pattern refines the known types rather than fixing new ones.
    fn lambda(&mut self, pats: &[Pat], body: &Expr, loc: Loc, expected: Option<&Ty>) -> TR<(Core, Ty)> {
        self.loc = loc;
        let mark = self.alt_begin();
        let mut arg_tys: Vec<Ty> = Vec::with_capacity(pats.len());
        let mut rest: Option<Ty> = expected.cloned();
        // A polymorphic or constrained type between the parameters (`\a b -> ..` at
        // `a -> forall b. C b => b -> r`): its variables are rigid in the body and its dictionaries
        // are parameters of the lambda at that place.
        let mut binders: Vec<Option<usize>> = Vec::new();
        let mut dict_names: Vec<String> = Vec::new();
        let mut given_dicts: Vec<(Pred, String)> = Vec::new();
        for (k, _) in pats.iter().enumerate() {
            if k > 0 {
                if let Some(t) = rest.take() {
                    let before = dict_names.len();
                    let mut names = Vec::new();
                    let t = self.peel_polymorphic(t, &mut names, &mut given_dicts);
                    dict_names.extend(names);
                    for _ in before..dict_names.len() {
                        binders.push(None);
                    }
                    rest = Some(t);
                }
            }
            binders.push(Some(k));
            let split = rest.as_ref().and_then(|t| self.metas.prune(t).as_fun().map(|(x, y)| (x.clone(), y.clone())));
            let (a, r) = match split {
                Some(p) => p,
                None => {
                    let x = self.meta();
                    let y = self.meta();
                    if let Some(t) = &rest {
                        self.unify_at(t, &Ty::fun(x.clone(), y.clone()))?;
                    }
                    (x, y)
                }
            };
            arg_tys.push(a);
            rest = Some(r);
        }
        // And one after the last parameter: the body is checked at its rigid instance.
        if !pats.is_empty() {
            if let Some(t) = rest.take() {
                let before = dict_names.len();
                let mut names = Vec::new();
                let t = self.peel_polymorphic(t, &mut names, &mut given_dicts);
                dict_names.extend(names);
                for _ in before..dict_names.len() {
                    binders.push(None);
                }
                rest = Some(t);
            }
        }
        let cpats = self.check_pats(pats, &arg_tys)?;
        let givens_len = self.givens.len();
        for (p, d) in &given_dicts {
            self.add_given(super::Given { pred: p.clone(), ev: Core::Var(d.clone()) });
        }
        let outer = if given_dicts.is_empty() { None } else { Some(std::mem::take(&mut self.wanted)) };
        let (cbody, tbody) = match &rest {
            Some(r) => (self.check(body, r)?, r.clone()),
            None => self.infer(body)?,
        };
        let cbody = match outer {
            Some(outer) => {
                let inner = std::mem::replace(&mut self.wanted, outer);
                let (binds, residual) = self.solve(inner)?;
                self.wanted.extend(residual);
                wrap_let(binds, cbody)
            }
            None => cbody,
        };
        self.givens.truncate(givens_len);
        let cbody = match self.alt_end(mark, CBody::Expr(cbody))? {
            CBody::Expr(c) => c,
            other => {
                let mut fresh = |b: &str| self.fresh_name(b);
                compile_body(&other, Core::Fail("guard".into()), &mut fresh)
            }
        };
        let scruts: Vec<String> = pats.iter().map(|_| self.fresh_name("arg")).collect();
        let rows = vec![Row {
            pats: cpats,
            body: CBody::Expr(cbody),
        }];
        let fail = Core::Fail("Non-exhaustive patterns in lambda".into());
        let mut fresh = |b: &str| self.fresh_name(b);
        let matched = compile_rows(&scruts, &rows, fail, &mut fresh);
        let mut core = matched;
        let mut dicts = dict_names.iter().rev();
        for b in binders.iter().rev() {
            let name = match b {
                Some(k) => scruts[*k].clone(),
                None => dicts.next().cloned().unwrap_or_default(),
            };
            core = Core::Lam(name, Box::new(core));
        }
        Ok((core, Ty::funs(arg_tys, tbody)))
    }

    /// An application spine `f a1 .. an`: the head is inferred and each argument is checked
    /// against its parameter type. With an expected type, the result is unified with it before
    /// the arguments are checked, so that their constraints are solved at the known type.
    /// Returns the Core and the result type.
    fn infer_spine(&mut self, e: &Expr, expected: Option<&Ty>) -> TR<(Core, Ty)> {
        let mut spine: Vec<&Expr> = Vec::new();
        let mut head = e;
        while let Expr::App(f, a) = head {
            spine.push(a.as_ref());
            head = f.as_ref();
        }
        spine.reverse();
        // `f $ x` is `f x`, as GHC types and desugars it: no thunk for the argument of `$`, and
        // `f` may be polymorphic (`runST $ do ..`).
        if let Expr::Var(q, _) = head {
            if spine.len() >= 2 && qkey(q) == "GHC.Internal.Base.$" {
                let mut direct = Expr::App(Box::new(spine[0].clone()), Box::new(spine[1].clone()));
                for a in &spine[2..] {
                    direct = Expr::App(Box::new(direct), Box::new((*a).clone()));
                }
                return self.infer_spine(&direct, expected);
            }
        }
        let (mut core, mut tf) = self.infer(head)?;
        let mut params = Vec::with_capacity(spine.len());
        // Dictionaries of polymorphic results met in the middle of the spine: (number of arguments
        // applied before them, predicates).
        let mut mid_dicts: Vec<(usize, Vec<Pred>)> = Vec::new();
        for _ in &spine {
            // A polymorphic result applied to further arguments (`unPut p k` with a rank-N field) is
            // instantiated first.
            let mut found: Vec<Pred> = Vec::new();
            loop {
                match self.metas.prune(&tf) {
                    Ty::Forall(ids, ps, body) => {
                        let args: Vec<Ty> = ids.iter().map(|_| self.meta()).collect();
                        found.extend(ps.iter().map(|p| super::types::subst_bound_pred(p, &ids, &args)));
                        tf = super::types::subst_bound(&body, &ids, &args);
                    }
                    // A constrained result (a field `Render doc => Renderer doc -> Int -> doc`)
                    // takes its dictionaries before the next argument.
                    Ty::Qual(ps, body) => {
                        found.extend(ps);
                        tf = *body;
                    }
                    _ => break,
                }
            }
            if !found.is_empty() {
                mid_dicts.push((params.len(), found));
            }
            let (arg, res) = match self.metas.prune(&tf).as_fun() {
                Some((x, y)) => (x.clone(), y.clone()),
                None => {
                    let x = self.meta();
                    let y = self.meta();
                    self.unify_at(&tf, &Ty::fun(x.clone(), y.clone()))?;
                    (x, y)
                }
            };
            params.push(arg);
            tf = res;
        }
        // A result of a polymorphic type (a rank-N field, `unQ q`) is instantiated where it is used.
        let mut pending_dicts: Vec<Pred> = Vec::new();
        loop {
            match self.metas.prune(&tf) {
                Ty::Forall(ids, ps, body) => {
                    let args: Vec<Ty> = ids.iter().map(|_| self.meta()).collect();
                    pending_dicts.extend(ps.iter().map(|p| super::types::subst_bound_pred(p, &ids, &args)));
                    tf = super::types::subst_bound(&body, &ids, &args);
                }
                Ty::Qual(ps, body) if !spine.is_empty() => {
                    pending_dicts.extend(ps);
                    tf = *body;
                }
                _ => break,
            }
        }
        if let Some(ty) = expected {
            self.unify_at(&tf, ty)?;
        }
        // Lambdas are checked after the other arguments: their parameter types are often fixed
        // by those, and a GADT pattern in a lambda must not fix them first.
        let mut checked: Vec<Option<Core>> = (0..spine.len()).map(|_| None).collect();
        for lambdas in [false, true] {
            for (i, (a, p)) in spine.iter().zip(&params).enumerate() {
                if matches!(a, Expr::Lam(..)) == lambdas {
                    checked[i] = Some(self.check_arg(a, p)?);
                }
            }
        }
        // An argument of an unlifted type is evaluated before the call, as in GHC: the primitive it
        // comes from has its effect then. It is bound by a case around the application.
        let mut eager: Vec<(String, Core)> = Vec::new();
        let loc0 = self.loc;
        for (i, (c, p)) in checked.into_iter().flatten().zip(&params).enumerate() {
            if let Some(pos) = mid_dicts.iter().position(|(k, _)| *k == i) {
                let (_, ps) = mid_dicts.remove(pos);
                core = self.apply_dicts(core, ps, loc0);
            }
            let c = if !matches!(c, Core::Var(_) | Core::Lit(_)) && self.is_unlifted(p) {
                let v = self.fresh_name("u");
                eager.push((v.clone(), c));
                Core::Var(v)
            } else {
                c
            };
            core = Core::App(Box::new(core), Box::new(c));
        }
        for (v, c) in eager.into_iter().rev() {
            core = Core::Case(Box::new(c), v, vec![crate::core::Alt::Default(core)]);
        }
        let loc = self.loc;
        let core = self.apply_dicts(core, pending_dicts, loc);
        Ok((core, tf))
    }

    /// Whether a type is known to be unlifted (`State#`, `Int#`, ...): its values are never thunks.
    fn is_unlifted(&self, t: &Ty) -> bool {
        let head = self.metas.zonk(t);
        matches!(head.split_apps().0, Ty::Con(c) if c.ends_with('#'))
    }

    /// Checks an argument against its parameter type. A polymorphic or constrained parameter
    /// is checked under its rigid variables and dictionaries.
    fn check_arg(&mut self, a: &Expr, arg: &Ty) -> TR<Core> {
        if let Ty::Forall(ids, preds, body) = self.metas.prune(arg) {
            // A polymorphic argument: its variables are rigid while it is checked.
            let sk: Vec<Ty> = ids.iter().map(|_| self.metas.fresh_skolem("r")).collect();
            let preds: Vec<Pred> = preds.iter().map(|p| super::types::subst_bound_pred(p, &ids, &sk)).collect();
            let body = super::types::subst_bound(&body, &ids, &sk);
            return self.check_qual_arg(a, &preds, &body);
        }
        if let Ty::Qual(preds, body) = self.metas.prune(arg) {
            // A rank-N argument: checked under the dictionaries it takes.
            return self.check_qual_arg(a, &preds, &body);
        }
        self.check(a, arg)
    }

    /// Infers the type of an expression, producing Core.
    pub fn infer(&mut self, e: &Expr) -> TR<(Core, Ty)> {
        self.loc = e.loc();
        match e {
            Expr::Var(q, loc) => {
                self.loc = *loc;
                let key = self.resolve_key(&qkey(q));
                let scheme = self
                    .lookup_var(&key)
                    .ok_or_else(|| super::TcError {
                        loc: *loc,
                        msg: format!("Variable not in scope: {key}"),
                    })?;
                let (core, ty) = self.inst_var(&key, &scheme, *loc);
                Ok((core, ty))
            }
            Expr::Con(q, loc) => {
                self.loc = *loc;
                let key = self.resolve_key(&qkey(q));
                if let Some((prim, boxed)) = boxing_types(&key) {
                    // I#, D#, C#: boxing is the identity on the runtime representation.
                    let f = Core::Lam("box%".into(), Box::new(Core::Var("box%".into())));
                    return Ok((f, Ty::fun(Ty::con(prim), Ty::con(boxed))));
                }
                let scheme = self.lookup_var(&key).ok_or_else(|| super::TcError {
                    loc: *loc,
                    msg: format!("Data constructor not in scope: {key}"),
                })?;
                Ok(self.inst_var(&key, &scheme, *loc))
            }
            Expr::Lit(l, loc) => {
                self.loc = *loc;
                self.infer_lit(l)
            }
            Expr::App(..) => self.infer_spine(e, None),
            Expr::TyApp(..) => {
                // `f @T1 @T2`: the explicit types instantiate the leading quantified variables.
                let mut tys: Vec<&Type> = Vec::new();
                let mut head = e;
                while let Expr::TyApp(f, t, _) = head {
                    tys.push(t);
                    head = f;
                }
                tys.reverse();
                let (q, loc) = match head {
                    Expr::Var(q, loc) | Expr::Con(q, loc) => (q, *loc),
                    _ => {
                        return self.err("Visible type application is supported only on variables and constructors")
                    }
                };
                self.loc = loc;
                let key = self.resolve_key(&qkey(q));
                let scheme = self.lookup_var(&key).ok_or_else(|| super::TcError {
                    loc,
                    msg: format!("Variable not in scope: {key}"),
                })?;
                let mut explicit = Vec::with_capacity(tys.len());
                for t in tys {
                    if matches!(t, Type::Wild(_)) {
                        explicit.push(self.metas.fresh(self.level));
                        continue;
                    }
                    let mut tvs = Vec::new();
                    let ty = self.convert_type(t, &mut tvs)?;
                    if !tvs.is_empty() {
                        return self.err(format!("Not in scope: type variable ‘{}’", tvs[0]));
                    }
                    explicit.push(ty);
                }
                if explicit.len() > scheme.nvars as usize {
                    return self.err(format!(
                        "Cannot apply expression of type ‘{}’ to {} visible type argument(s)",
                        key,
                        explicit.len()
                    ));
                }
                Ok(self.inst_var_explicit(&key, &scheme, loc, &explicit))
            }
            Expr::OpChain(_, loc) => {
                self.loc = *loc;
                self.err("internal: unresolved operator chain")
            }
            Expr::Lam(pats, body, loc) => self.lambda(pats, body, *loc, None),
            Expr::Splice(true, e, loc) => {
                // `$$(e)` in a typed quotation: `e :: Code m t` stands for a `t`.
                self.loc = *loc;
                let (_, te) = self.infer(e)?;
                let m = self.quote_monads.last().cloned().unwrap_or_else(|| self.meta());
                let t = self.meta();
                self.unify_at(&te, &Ty::apps(Ty::con("GHC.Internal.TH.Syntax.Code"), vec![m, t.clone()]))?;
                Ok((Core::Fail("hsc: Template Haskell splices cannot be run yet".into()), t))
            }
            Expr::Splice(false, _, loc) => {
                self.loc = *loc;
                self.err("Template Haskell splices are not supported yet")
            }
            Expr::NameQuote(_, _, loc) | Expr::QuoteType(_, loc) | Expr::QuotePat(_, loc) => {
                self.loc = *loc;
                self.err("internal: a quotation reached the type checker")
            }
            Expr::Quote(kind, body, loc) => {
                // A quotation has the type GHC gives it: `Quote m => m Exp` (or `m [Dec]`, `m Type`,
                // `m Pat`), and `Quote m => Code m t` for a typed quotation of an expression of type
                // `t`. Running Template Haskell code is not supported yet: the value is an error.
                self.loc = *loc;
                const TH: &str = "GHC.Internal.TH.Syntax";
                if self.env.classes.get(&format!("{TH}.Quote")).is_none() {
                    return self.err("Template Haskell quotations need GHC.Internal.TH.Syntax (import Language.Haskell.TH.Syntax)");
                }
                let m = self.meta();
                let ev = self.fresh_name("quote");
                self.wanted.push(super::Wanted {
                    pred: super::types::Pred { class: format!("{TH}.Quote"), args: vec![m.clone()] },
                    ev,
                    loc: *loc,
                });
                let ty = match (*kind, body) {
                    ('T', Some(b)) => {
                        self.quote_monads.push(m.clone());
                        let r = self.infer(b);
                        self.quote_monads.pop();
                        let (_, t) = r?;
                        Ty::apps(Ty::con(&format!("{TH}.Code")), vec![m, t])
                    }
                    ('e', Some(b)) => {
                        // An untyped quotation's body is not type-checked by GHC either; its names
                        // are resolved.
                        let _ = b;
                        Ty::app(m, Ty::con(&format!("{TH}.Exp")))
                    }
                    ('d', _) => Ty::app(m, Ty::list(Ty::con(&format!("{TH}.Dec")))),
                    ('t', _) => Ty::app(m, Ty::con(&format!("{TH}.Type"))),
                    _ => Ty::app(m, Ty::con(&format!("{TH}.Pat"))),
                };
                Ok((Core::Fail("hsc: Template Haskell quotations cannot be run yet".into()), ty))
            }
            Expr::IPVar(name, loc) => {
                // An implicit parameter is a wanted constraint whose evidence is its value.
                self.loc = *loc;
                let t = self.meta();
                let ev = self.fresh_name("ip");
                self.wanted.push(super::Wanted {
                    pred: super::types::Pred { class: format!("?{name}"), args: vec![t.clone()] },
                    ev: ev.clone(),
                    loc: *loc,
                });
                Ok((Core::Var(ev), t))
            }
            Expr::Let(decls, body, loc) if decls.iter().any(|d| matches!(d, Decl::IPBind { .. })) => {
                // `let ?x = e in body`: the bindings are givens for the body.
                self.loc = *loc;
                let mut bindings: Vec<(String, Core)> = Vec::new();
                let mut givens: Vec<super::Given> = Vec::new();
                for d in decls {
                    if let Decl::IPBind { name, expr, loc } = d {
                        self.loc = *loc;
                        let (ce, te) = self.infer(expr)?;
                        let ev = self.fresh_name("ip");
                        bindings.push((ev.clone(), ce));
                        givens.push(super::Given {
                            pred: super::types::Pred { class: format!("?{name}"), args: vec![te] },
                            ev: Core::Var(ev),
                        });
                    }
                }
                let givens_len = self.givens.len();
                self.givens.extend(givens);
                let outer = std::mem::take(&mut self.wanted);
                let r = self.infer(body);
                let inner = std::mem::replace(&mut self.wanted, outer);
                let (cb, tb) = match r {
                    Ok(x) => x,
                    Err(e) => {
                        self.givens.truncate(givens_len);
                        return Err(e);
                    }
                };
                let (ev_binds, residual) = self.solve(inner)?;
                self.wanted.extend(residual);
                self.givens.truncate(givens_len);
                Ok((wrap_let(bindings, wrap_let(ev_binds, cb)), tb))
            }
            Expr::Let(decls, body, loc) => {
                self.loc = *loc;
                let (mut binds, deferred) = self.infer_local_deferring(decls)?;
                let (cb, tb) = self.infer(body)?;
                binds.extend(self.finish_deferred(deferred)?);
                Ok((self.wrap_let_forcing(binds, cb), tb))
            }
            Expr::If(c, t, f, loc) => {
                self.loc = *loc;
                let bool_ty = self.bool_ty();
                let cc = self.check(c, &bool_ty)?;
                let (ct, tt) = self.infer(t)?;
                let cf = self.check(f, &tt)?;
                let b = self.fresh_name("if");
                Ok((
                    Core::Case(
                        Box::new(cc),
                        b,
                        vec![
                            Alt::Con(crate::core::matcher::TRUE.into(), vec![], ct),
                            Alt::Default(cf),
                        ],
                    ),
                    tt,
                ))
            }
            Expr::MultiIf(alts, loc) => {
                self.loc = *loc;
                let r = self.meta();
                let cgs = self.guard_alts(alts, &r)?;
                let body = CBody::Guards(cgs);
                let mut fresh = |b: &str| self.fresh_name(b);
                let core = compile_body(&body, Core::Fail("Non-exhaustive guards in multi-way if".into()), &mut fresh);
                Ok((core, r))
            }
            Expr::Case(scrut, alts, loc) => {
                self.loc = *loc;
                let (cs, ts) = self.infer(scrut)?;
                let r = self.meta();
                let s = self.fresh_name("scrut");
                let rows = self.alt_rows(alts, &ts, &r)?;
                let mut fresh = |b: &str| self.fresh_name(b);
                let matched = compile_rows(
                    std::slice::from_ref(&s),
                    &rows,
                    Core::Fail("Non-exhaustive patterns in case".into()),
                    &mut fresh,
                );
                Ok((self.bind_scrutinee(&ts, s, cs, matched), r))
            }
            Expr::LamCase(alts, loc) => {
                self.loc = *loc;
                let a = self.meta();
                let r = self.meta();
                let x = self.fresh_name("lc");
                let rows = self.alt_rows(alts, &a, &r)?;
                let mut fresh = |b: &str| self.fresh_name(b);
                let matched = compile_rows(
                    std::slice::from_ref(&x),
                    &rows,
                    Core::Fail("Non-exhaustive patterns in \\case".into()),
                    &mut fresh,
                );
                Ok((Core::Lam(x, Box::new(matched)), Ty::fun(a, r)))
            }
            Expr::Do(stmts, loc) => {
                self.loc = *loc;
                if self.applicative_do {
                    if let Some(e) = applicative_do(stmts, *loc) {
                        return self.infer(&e);
                    }
                }
                let desugared = self.desugar_do(stmts, *loc)?;
                self.infer(&desugared)
            }
            Expr::Tuple(elems, loc) => {
                self.loc = *loc;
                if elems.iter().all(|e| e.is_some()) {
                    let mut cores = Vec::with_capacity(elems.len());
                    let mut tys = Vec::with_capacity(elems.len());
                    for e in elems.iter().flatten() {
                        let (c, t) = self.infer(e)?;
                        cores.push(c);
                        tys.push(t);
                    }
                    // A component of an unlifted type (the `State#` of `(# copy# .. s, () #)`) is
                    // evaluated when the tuple is built, as GHC evaluates unlifted expressions: it
                    // is where the primitive has its effect.
                    let mut eager: Vec<(String, Core)> = Vec::new();
                    for (c, t) in cores.iter_mut().zip(&tys) {
                        if !matches!(c, Core::Var(_) | Core::Lit(_)) && self.is_unlifted(t) {
                            let v = self.fresh_name("ut");
                            eager.push((v.clone(), std::mem::replace(c, Core::Var(v))));
                        }
                    }
                    let name = Ty::tuple_con(elems.len());
                    let mut core = Core::Con(name, cores);
                    for (v, c) in eager.into_iter().rev() {
                        core = Core::Case(Box::new(c), v, vec![crate::core::Alt::Default(core)]);
                    }
                    Ok((core, Ty::tuple(tys)))
                } else {
                    // A tuple section: the holes become lambda parameters.
                    let mut params = Vec::new();
                    let mut items = Vec::with_capacity(elems.len());
                    for el in elems {
                        match el {
                            Some(x) => items.push(Some(x.clone())),
                            None => {
                                let name = self.fresh_name("ts");
                                params.push(Pat::Var(name.clone(), *loc));
                                items.push(Some(var(&name, *loc)));
                            }
                        }
                    }
                    let body = Expr::Tuple(items, *loc);
                    self.infer(&Expr::Lam(params, Box::new(body), *loc))
                }
            }
            Expr::List(elems, loc) => {
                self.loc = *loc;
                let elem = self.meta();
                let mut cores = Vec::with_capacity(elems.len());
                for e in elems {
                    cores.push(self.check(e, &elem)?);
                }
                let n = cores.len();
                let mut core = Core::Con("Builtin.[]".into(), vec![]);
                for c in cores.into_iter().rev() {
                    core = Core::Con("Builtin.:".into(), vec![c, core]);
                }
                if self.overloaded_lists {
                    // `fromListN n [..]` at any IsList type.
                    let (f, fty) = self.lookup_inst("GHC.Internal.IsList.fromListN")?;
                    let (len_ty, rest) = split_fun_ty(&fty);
                    let (list_ty, res) = split_fun_ty(&rest);
                    self.unify_at(&len_ty, &Ty::con("GHC.Types.Int"))?;
                    self.unify_at(&list_ty, &Ty::list(elem))?;
                    let len = Core::Con("GHC.Types.I#".into(), vec![Core::Lit(CLit::Int(n as i64))]);
                    return Ok((Core::App(Box::new(Core::App(Box::new(f), Box::new(len))), Box::new(core)), res));
                }
                Ok((core, Ty::list(elem)))
            }
            Expr::ArithSeq { from, then, to, loc } => {
                self.loc = *loc;
                let (name, args) = match (then, to) {
                    (None, None) => ("enumFrom", vec![from.as_ref().clone()]),
                    (Some(t), None) => ("enumFromThen", vec![from.as_ref().clone(), t.as_ref().clone()]),
                    (None, Some(t)) => ("enumFromTo", vec![from.as_ref().clone(), t.as_ref().clone()]),
                    (Some(t), Some(u)) => (
                        "enumFromThenTo",
                        vec![from.as_ref().clone(), t.as_ref().clone(), u.as_ref().clone()],
                    ),
                };
                let call = args.into_iter().fold(prelude(name, *loc), app);
                if self.overloaded_lists {
                    let (seq, seq_ty) = self.infer(&call)?;
                    let (f, fty) = self.lookup_inst("GHC.Internal.IsList.fromList")?;
                    let (list_ty, res) = split_fun_ty(&fty);
                    self.unify_at(&list_ty, &seq_ty)?;
                    return Ok((Core::App(Box::new(f), Box::new(seq)), res));
                }
                self.infer(&call)
            }
            Expr::ListComp(body, branches, loc) => {
                self.loc = *loc;
                if branches.len() != 1 {
                    let desugared = self.desugar_parallel_comp(body, branches, *loc);
                    return self.infer(&desugared);
                }
                let desugared = self.desugar_comp(body, &branches[0], *loc);
                self.infer(&desugared)
            }
            Expr::LeftSection(e, op, loc) => {
                self.loc = *loc;
                let (cf, tf) = self.infer_op(op, *loc)?;
                let (ca, ta) = self.infer(e)?;
                let r = self.meta();
                self.unify_at(&tf, &Ty::fun(ta, r.clone()))?;
                Ok((Core::App(Box::new(cf), Box::new(ca)), r))
            }
            Expr::RightSection(op, e, loc) => {
                self.loc = *loc;
                let (cf, tf) = self.infer_op(op, *loc)?;
                let (ce, te) = self.infer(e)?;
                let x = self.meta();
                let r = self.meta();
                self.unify_at(&tf, &Ty::fun(x.clone(), Ty::fun(te, r.clone())))?;
                let xv = self.fresh_name("sx");
                let ev = self.fresh_name("se");
                let body = Core::App(
                    Box::new(Core::App(Box::new(cf), Box::new(Core::Var(xv.clone())))),
                    Box::new(Core::Var(ev.clone())),
                );
                Ok((
                    Core::Let(vec![(ev, ce)], Box::new(Core::Lam(xv, Box::new(body)))),
                    Ty::fun(x, r),
                ))
            }
            Expr::RecCon {
                con,
                fields,
                loc,
                ..
            } => {
                self.loc = *loc;
                self.infer_rec_con(con, fields, *loc)
            }
            Expr::RecUpd(e, fields, loc) => {
                self.loc = *loc;
                let desugared = self.desugar_rec_upd(e, fields, *loc)?;
                self.infer(&desugared)
            }
            Expr::Sig(e, t, loc) => {
                self.loc = *loc;
                let scheme = self.sig_scheme(t, &[])?;
                if scheme.nvars != 0 || !scheme.preds.is_empty() {
                    // `(e :: forall a. C a => t)` is `let v :: forall a. C a => t; v = e in v`.
                    let v = self.fresh_name("sig");
                    let binding = crate::syntax::ast::Decl::Fun {
                        name: v.clone(),
                        clauses: vec![crate::syntax::ast::Clause {
                            pats: Vec::new(),
                            rhs: crate::syntax::ast::Rhs {
                                body: crate::syntax::ast::Body::Plain((**e).clone()),
                                wheres: Vec::new(),
                            },
                            loc: *loc,
                            end: *loc,
                        }],
                        loc: *loc,
                    };
                    let sig = crate::syntax::ast::Decl::Sig {
                        names: vec![v.clone()],
                        ty: t.clone(),
                        loc: *loc,
                    };
                    let body = Expr::Var(QName::unqual(v), *loc);
                    return self.infer(&Expr::Let(vec![sig, binding], Box::new(body), *loc));
                }
                let c = self.check(e, &scheme.ty)?;
                Ok((c, scheme.ty))
            }
            Expr::Wild(loc) => {
                self.loc = *loc;
                self.err("Found hole: _")
            }
            Expr::AsPat(_, _, loc) | Expr::LazyPat(_, loc) | Expr::BangPat(_, loc) | Expr::ViewPat(_, _, loc) => {
                self.loc = *loc;
                self.err("Pattern syntax in expression context")
            }
        }
    }

    /// Instantiates a variable's scheme, creating wanted evidence for its predicates.
    pub fn inst_var(&mut self, name: &str, scheme: &Scheme, loc: Loc) -> (Core, Ty) {
        self.inst_var_explicit(name, scheme, loc, &[])
    }

    /// Instantiates a variable's scheme; the leading quantified variables take the explicit
    /// types given by visible type application, the rest are fresh unification variables.
    /// Checks an argument of a constrained (rank-N) parameter type: its predicates are
    /// givens, and the argument becomes a function of their dictionaries.
    fn check_qual_arg(&mut self, a: &Expr, preds: &[Pred], body: &Ty) -> TR<Core> {
        let dparams: Vec<String> = preds.iter().map(|_| self.fresh_name("dq")).collect();
        let givens_len = self.givens.len();
        for (p, d) in preds.iter().zip(&dparams) {
            self.add_given(super::Given { pred: p.clone(), ev: Core::Var(d.clone()) });
        }
        let outer = std::mem::take(&mut self.wanted);
        // Checked against the parameter type (a lambda takes its argument types from it).
        let result = self.check(a, body);
        let inner = std::mem::replace(&mut self.wanted, outer);
        let ca = match result {
            Ok(ca) => ca,
            Err(e) => {
                self.givens.truncate(givens_len);
                return Err(e);
            }
        };
        let (binds, residual) = self.solve(inner)?;
        self.wanted.extend(residual);
        self.givens.truncate(givens_len);
        let mut core = wrap_let(binds, ca);
        for d in dparams.into_iter().rev() {
            core = Core::Lam(d, Box::new(core));
        }
        Ok(core)
    }

    pub fn inst_var_explicit(&mut self, name: &str, scheme: &Scheme, loc: Loc, explicit: &[Ty]) -> (Core, Ty) {
        let args: Vec<Ty> = (0..scheme.nvars as usize)
            .map(|i| match explicit.get(i) {
                Some(t) => t.clone(),
                None => self.metas.fresh(self.level),
            })
            .collect();
        let ty = subst_gen(&scheme.ty, &args);
        // `coerce :: a -> b` (a primitive, its `Coercible a b` not checked): the two types are
        // kept to resolve a variable otherwise ambiguous (see `coercible_partner`).
        if name == "GHC.Prim.coerce" {
            if let Some((a, b)) = ty.as_fun() {
                self.coercions.push((a.clone(), b.clone()));
            }
        }
        let preds: Vec<Pred> = scheme.preds.iter().map(|p| subst_gen_pred(p, &args)).collect();
        let mut core = Core::Var(name.to_string());
        for p in preds {
            let ev = self.fresh_name("d");
            if p.class == "?callStack" {
                // The call pushes its frame onto the enclosing stack when the wanted is solved.
                self.callsites.insert(ev.clone(), (super::types::unqualify(name).to_string(), loc));
            }
            self.wanted.push(Wanted {
                pred: p,
                ev: ev.clone(),
                loc,
            });
            core = Core::App(Box::new(core), Box::new(Core::Var(ev)));
        }
        // A variable whose type is constrained (a rank-N parameter) takes the dictionaries now.
        let mut ty = ty;
        loop {
            match self.metas.prune(&ty) {
                Ty::Qual(ps, body) => {
                    core = self.apply_dicts(core, ps, loc);
                    ty = *body;
                }
                Ty::Forall(ids, ps, body) => {
                    // A polymorphic variable: instantiated at every use.
                    let args: Vec<Ty> = ids.iter().map(|_| self.meta()).collect();
                    let ps: Vec<Pred> = ps.iter().map(|p| super::types::subst_bound_pred(p, &ids, &args)).collect();
                    core = self.apply_dicts(core, ps, loc);
                    ty = super::types::subst_bound(&body, &ids, &args);
                }
                _ => break,
            }
        }
        (core, ty)
    }

    /// Applies a Core expression to the dictionaries of predicates, requesting them as wanted.
    fn apply_dicts(&mut self, mut core: Core, preds: Vec<Pred>, loc: Loc) -> Core {
        for p in preds {
            let ev = self.fresh_name("d");
            self.wanted.push(Wanted { pred: p, ev: ev.clone(), loc });
            core = Core::App(Box::new(core), Box::new(Core::Var(ev)));
        }
        core
    }

    fn infer_op(&mut self, op: &QName, loc: Loc) -> TR<(Core, Ty)> {
        let key = self.resolve_key(&qkey(op));
        let scheme = self.lookup_var(&key).ok_or_else(|| super::TcError {
            loc,
            msg: format!("Variable not in scope: {key}"),
        })?;
        Ok(self.inst_var(&key, &scheme, loc))
    }

    fn infer_lit(&mut self, l: &Lit) -> TR<(Core, Ty)> {
        match l {
            Lit::Int(s) => {
                let (fi, fty) = self.lookup_inst("Base.fromInteger")?;
                let (arg, res) = split_fun_ty(&fty);
                self.unify_at(&arg, &Ty::con("GHC.Num.Integer.Integer"))?;
                Ok((Core::App(Box::new(fi), Box::new(Core::Lit(CLit::Integer(s.clone())))), res))
            }
            Lit::Frac(s) => {
                let (fr, fty) = self.lookup_inst("Base.fromRational")?;
                let (arg, res) = split_fun_ty(&fty);
                self.unify_at(&arg, &Ty::apps(Ty::Con(self.resolve_type("Base.Ratio")), vec![Ty::con("GHC.Num.Integer.Integer")]))?;
                let lit = Core::Prim("ratioLit".into(), vec![Core::Lit(CLit::Frac(s.clone()))]);
                Ok((Core::App(Box::new(fr), Box::new(lit)), res))
            }
            Lit::Char(c) => Ok((Core::Lit(CLit::Char(*c)), Ty::con("GHC.Types.Char"))),
            Lit::Str(s) if self.overloaded_strings => {
                // OverloadedStrings: `fromString "s"` at any IsString type.
                let (fs, fty) = self.lookup_inst("GHC.Internal.Data.String.fromString")?;
                let (arg, res) = split_fun_ty(&fty);
                self.unify_at(&arg, &Ty::list(Ty::con("GHC.Types.Char")))?;
                Ok((Core::App(Box::new(fs), Box::new(Core::Lit(CLit::Str(s.clone())))), res))
            }
            Lit::Str(s) => Ok((Core::Lit(CLit::Str(s.clone())), Ty::list(Ty::con("GHC.Types.Char")))),
            Lit::Prim(p) => {
                let (core, ty) = prim_lit(p, false);
                Ok((core, Ty::con(ty)))
            }
        }
    }

    /// Looks up a Prelude function and instantiates it (the caller adds the wanteds).
    fn lookup_inst(&mut self, name: &str) -> TR<(Core, Ty)> {
        let name = self.resolve_key(name);
        let name = name.as_str();
        let scheme = self.lookup_var(name).ok_or_else(|| super::TcError {
            loc: self.loc,
            msg: format!("internal: {name} is not defined in the Prelude"),
        })?;
        Ok(self.inst_var(name, &scheme, self.loc))
    }

    /// Checks a list of patterns against types, binding their variables.
    pub fn check_pats(&mut self, pats: &[Pat], tys: &[Ty]) -> TR<Vec<CPat>> {
        let mut out = Vec::with_capacity(pats.len());
        for (p, t) in pats.iter().zip(tys) {
            out.push(self.check_pat(p, t)?);
        }
        Ok(out)
    }

    /// Checks a pattern against a type, binding its variables with monomorphic types.
    pub fn check_pat(&mut self, p: &Pat, ty: &Ty) -> TR<CPat> {
        // A structured pattern against a polymorphic type instantiates it (`runST (ST s) = ..`).
        if !matches!(p, Pat::Var(..) | Pat::Wild(_)) {
            if let Ty::Forall(ids, preds, body) = self.metas.prune(ty) {
                if preds.is_empty() {
                    let args: Vec<Ty> = ids.iter().map(|_| self.meta()).collect();
                    let inst = super::types::subst_bound(&body, &ids, &args);
                    return self.check_pat(p, &inst);
                }
            }
        }
        match p {
            Pat::Var(name, _) => {
                self.vars.insert(name.clone(), Scheme::mono(ty.clone()));
                Ok(CPat::Var(name.clone()))
            }
            Pat::Wild(_) => Ok(CPat::Wild),
            Pat::Lit(l, loc) => {
                self.loc = *loc;
                self.lit_pat(l, false, ty)
            }
            Pat::NegLit(l, loc) => {
                self.loc = *loc;
                self.lit_pat(l, true, ty)
            }
            Pat::Con(q, args, loc) => {
                self.loc = *loc;
                let key = self.resolve_key(&qkey(q));
                if let (Some((prim, boxed)), 1) = (boxing_types(&key), args.len()) {
                    self.unify_at(ty, &Ty::con(boxed))?;
                    return self.check_pat(&args[0], &Ty::con(prim));
                }
                let (arg_tys, dict_pats) = self.con_pattern_types(&key, args.len(), ty)?;
                let newtype = self.env.datacons.get(&key).is_some_and(|d| d.newtype);
                let mut subs = dict_pats;
                for (a, t) in args.iter().zip(&arg_tys) {
                    subs.push(self.check_pat(a, t)?);
                }
                if newtype {
                    // Newtypes are erased: matching the constructor matches its field.
                    return Ok(subs.pop().expect("newtype has one field"));
                }
                Ok(CPat::Con(key, subs))
            }
            Pat::Rec { con, fields, loc, .. } => {
                self.loc = *loc;
                let key = qkey(con);
                let occs = self
                    .env
                    .datacons
                    .get(&key)
                    .map(|d| d.fields.clone())
                    .unwrap_or_default();
                // A constructor without field names (`C{}` on a positional one) has its arity.
                let n = if occs.is_empty() {
                    self.env.datacons.get(&key).map(|d| d.arity).unwrap_or(0)
                } else {
                    occs.len()
                };
                let mut args: Vec<Pat> = (0..n).map(|_| Pat::Wild(*loc)).collect();
                // The sub-patterns start with the context dictionaries, so a field's position among
                // them is offset by their number.
                let ndicts = self.env.datacons.get(&key).map(|d| d.scheme.preds.len()).unwrap_or(0);
                let mut order = Vec::new();
                for (f, sub) in fields {
                    let pos = occs.iter().position(|o| *o == f.name).ok_or_else(|| super::TcError {
                        loc: *loc,
                        msg: format!("Constructor ‘{}’ does not have field ‘{}’", con.name, f.name),
                    })?;
                    args[pos] = match sub {
                        Some(p) => p.clone(),
                        None => Pat::Var(f.name.clone(), *loc),
                    };
                    order.push(pos + ndicts);
                }
                let checked = self.check_pat(&Pat::Con(con.clone(), args, *loc), ty)?;
                // Matching follows the order in which the fields are written.
                Ok(match checked {
                    CPat::Con(name, subs) => CPat::Ordered(name, subs, order),
                    other => other,
                })
            }
            Pat::Tuple(ps, loc) => {
                self.loc = *loc;
                let tys: Vec<Ty> = ps.iter().map(|_| self.meta()).collect();
                self.unify_at(&Ty::tuple(tys.clone()), ty)?;
                let mut subs = Vec::with_capacity(ps.len());
                for (p, t) in ps.iter().zip(&tys) {
                    subs.push(self.check_pat(p, t)?);
                }
                Ok(CPat::Con(Ty::tuple_con(ps.len()), subs))
            }
            Pat::List(ps, loc) => {
                self.loc = *loc;
                let elem = self.meta();
                if self.overloaded_lists {
                    // `toList -> [..]` at any IsList type.
                    let (f, fty) = self.lookup_inst("GHC.Internal.IsList.toList")?;
                    let (arg, res) = split_fun_ty(&fty);
                    self.unify_at(&arg, ty)?;
                    self.unify_at(&res, &Ty::list(elem.clone()))?;
                    let mut subs = Vec::with_capacity(ps.len());
                    for p in ps {
                        subs.push(self.check_pat(p, &elem)?);
                    }
                    let mut core = CPat::Con("Builtin.[]".into(), vec![]);
                    for s in subs.into_iter().rev() {
                        core = CPat::Con("Builtin.:".into(), vec![s, core]);
                    }
                    return Ok(CPat::View(f, Box::new(core)));
                }
                self.unify_at(&Ty::list(elem.clone()), ty)?;
                let mut subs = Vec::with_capacity(ps.len());
                for p in ps {
                    subs.push(self.check_pat(p, &elem)?);
                }
                let mut core = CPat::Con("Builtin.[]".into(), vec![]);
                for s in subs.into_iter().rev() {
                    core = CPat::Con("Builtin.:".into(), vec![s, core]);
                }
                Ok(core)
            }
            Pat::As(name, sub, _) => {
                self.vars.insert(name.clone(), Scheme::mono(ty.clone()));
                let inner = self.check_pat(sub, ty)?;
                Ok(CPat::As(name.clone(), Box::new(inner)))
            }
            Pat::Lazy(sub, _) => Ok(CPat::Lazy(Box::new(self.check_pat(sub, ty)?))),
            Pat::Bang(sub, _) => Ok(CPat::Bang(Box::new(self.check_pat(sub, ty)?))),
            Pat::Sig(sub, t, loc) => {
                self.loc = *loc;
                // ScopedTypeVariables: a type variable the pattern signature mentions for the
                // first time is bound by it, to a fresh type, for the rest of the binding.
                let free = self.sig_scheme(t, &[])?.names;
                for name in free {
                    if !self.scoped.iter().any(|(n, _)| *n == name) {
                        let m = self.meta();
                        self.scoped.push((name, m));
                    }
                }
                let scheme = self.sig_scheme(t, &[])?;
                let (sty, _, _) = self.metas.instantiate(&scheme, self.level);
                self.unify_at(&sty, ty)?;
                self.check_pat(sub, ty)
            }
            Pat::View(e, sub, loc) => {
                self.loc = *loc;
                let r = self.meta();
                let ce = self.check(e, &Ty::fun(ty.clone(), r.clone()))?;
                Ok(CPat::View(ce, Box::new(self.check_pat(sub, &r)?)))
            }
            Pat::OpChain(..) => self.err("internal: unresolved pattern operator"),
        }
    }

    /// The field types of a constructor pattern, after unifying its result type with the
    /// scrutinee type. Variables the scrutinee does not determine are existential: they
    /// become rigid. Context dictionaries are bound by pattern variables and become givens.
    fn con_pattern_types(&mut self, key: &str, n: usize, ty: &Ty) -> TR<(Vec<Ty>, Vec<CPat>)> {
        let con = self.env.datacons.get(key).cloned().ok_or_else(|| super::TcError {
            loc: self.loc,
            msg: format!("Data constructor not in scope: {key}"),
        })?;
        if con.arity != n {
            return self.err(format!(
                "The constructor ‘{}’ should have {} argument{}, but has been given {}",
                super::types::unqualify(key),
                con.arity,
                if con.arity == 1 { "" } else { "s" },
                n
            ));
        }
        let (cty, preds, vars) = self.metas.instantiate(&con.scheme, self.level);
        let mut arg_tys = Vec::with_capacity(n);
        let mut cur = cty;
        for _ in 0..n {
            let split = self.metas.prune(&cur).as_fun().map(|(a, r)| (a.clone(), r.clone()));
            match split {
                Some((a, r)) => {
                    arg_tys.push(a);
                    cur = r;
                }
                None => return self.err("internal: constructor type is not a function"),
            }
        }
        // Refinement: the result type may be more specific than the scrutinee (GADTs). Families
        // are reduced first, so that a reducible family application refines like its value.
        // A data instance's constructor against an unknown applied (`t r` for `V Any`): the unknown
        // is the family (see `align_data_families`).
        if self.env.families.values().any(|f| !f.equations.is_empty()) {
            let (c0, t0) = (self.metas.zonk(&cur), self.metas.zonk(ty));
            let _ = self.align_data_families(&c0, &t0, 0, false)?;
        }
        let cur_n = self.normalize(&cur);
        let ty_n = self.normalize(ty);
        let refinements = self.metas.skol_binds.len();
        let refined = self.metas.unify_refine(&cur_n, &ty_n);
        // A refinement can make a family given reduce to more equalities.
        if self.metas.skol_binds.len() != refinements {
            self.refine_family_givens();
        }
        match refined {
            Ok(()) => {}
            Err(_) => {
                let (r, t) = (self.show(&cur), self.show(ty));
                return self.err(format!("Couldn't match expected type ‘{t}’ with actual type ‘{r}’"));
            }
        }
        // A scrutinee type stuck on a family of unknowns (`XRec pass (HsType pass)` in a local
        // function without a signature) is not refined: its equality with the pattern's type
        // waits as a wanted, as GHC has it.
        let deferred_from = self.deferred_eqs.len();
        if self.has_family(&ty_n) && self.has_metas(&self.metas.zonk(&ty_n)) {
            self.unify_at(&cur_n, &ty_n)?;
        }
        // Variables that do not occur in the scrutinee type are existential: rigid.
        let mut scrut_metas = Vec::new();
        self.metas.free_metas(&self.metas.zonk(ty), &mut scrut_metas);
        for (a, b, _) in self.deferred_eqs[deferred_from..].to_vec() {
            self.metas.free_metas(&self.metas.zonk(&a), &mut scrut_metas);
            self.metas.free_metas(&self.metas.zonk(&b), &mut scrut_metas);
        }
        for v in &vars {
            if let Ty::Meta(m) = self.metas.prune(v) {
                if !scrut_metas.contains(&m) {
                    self.metas.skolems += 1;
                    let sk = Ty::Skol(self.metas.skolems, format!("x{}", self.metas.skolems));
                    self.metas.subst[m as usize] = Some(sk);
                }
            }
        }
        let mut dict_pats = Vec::with_capacity(preds.len());
        for p in preds {
            let dv = self.fresh_name("dx");
            self.add_given(super::Given {
                pred: p,
                ev: Core::Var(dv.clone()),
            });
            dict_pats.push(CPat::Var(dv));
        }
        Ok((arg_tys, dict_pats))
    }

    /// A literal pattern: a predicate `\x -> x == lit` used as a test.
    fn lit_pat(&mut self, l: &Lit, negative: bool, ty: &Ty) -> TR<CPat> {
        let loc = self.loc;
        if let Lit::Prim(p) = l {
            // A primitive literal is compared directly: its type has no `Eq` instance.
            let (core, tyname) = prim_lit(p, negative);
            self.unify_at(&Ty::con(tyname), ty)?;
            let Core::Lit(l) = core else { unreachable!("prim_lit yields a literal") };
            return Ok(CPat::Lit(l));
        }
        let x = self.fresh_name("lx");
        let (lit_core, eq_ty, num_needed) = match l {
            Lit::Prim(_) => unreachable!("handled above"),
            Lit::Int(s) => {
                let s = if negative { format!("-{s}") } else { s.clone() };
                let (fi, fty) = self.lookup_inst("Base.fromInteger")?;
                let (arg, res) = split_fun_ty(&fty);
                self.unify_at(&arg, &Ty::con("GHC.Num.Integer.Integer"))?;
                self.unify_at(&res, ty)?;
                (Core::App(Box::new(fi), Box::new(Core::Lit(CLit::Integer(s)))), ty.clone(), true)
            }
            Lit::Frac(s) => {
                let s = if negative { format!("-{s}") } else { s.clone() };
                let (fr, fty) = self.lookup_inst("Base.fromRational")?;
                let (arg, res) = split_fun_ty(&fty);
                self.unify_at(&arg, &Ty::apps(Ty::Con(self.resolve_type("Base.Ratio")), vec![Ty::con("GHC.Num.Integer.Integer")]))?;
                self.unify_at(&res, ty)?;
                let lit = Core::Prim("ratioLit".into(), vec![Core::Lit(CLit::Frac(s))]);
                (Core::App(Box::new(fr), Box::new(lit)), ty.clone(), true)
            }
            Lit::Char(c) => {
                self.unify_at(&Ty::con("GHC.Types.Char"), ty)?;
                (Core::Lit(CLit::Char(*c)), ty.clone(), false)
            }
            Lit::Str(s) if self.overloaded_strings => {
                let (fs, fty) = self.lookup_inst("GHC.Internal.Data.String.fromString")?;
                let (arg, res) = split_fun_ty(&fty);
                self.unify_at(&arg, &Ty::list(Ty::con("GHC.Types.Char")))?;
                self.unify_at(&res, ty)?;
                (Core::App(Box::new(fs), Box::new(Core::Lit(CLit::Str(s.clone())))), ty.clone(), false)
            }
            Lit::Str(s) => {
                let str_ty = Ty::list(Ty::con("GHC.Types.Char"));
                self.unify_at(&str_ty, ty)?;
                (Core::Lit(CLit::Str(s.clone())), str_ty, false)
            }
        };
        // Equality: `==` from an Eq dictionary for the scrutinee type.
        let (eq, eq_fty) = self.lookup_inst("GHC.Classes.==")?;
        let (eq_arg, _) = split_fun_ty(&eq_fty);
        self.unify_at(&eq_arg, &eq_ty)?;
        let _ = num_needed;
        let test = Core::Lam(
            x.clone(),
            Box::new(Core::App(
                Box::new(Core::App(Box::new(eq), Box::new(Core::Var(x)))),
                Box::new(lit_core),
            )),
        );
        let _ = loc;
        Ok(CPat::Test(test))
    }

    /// Checks the alternatives of a `case`, returning one row per alternative.
    fn alt_rows(&mut self, alts: &[AstAlt], scrut_ty: &Ty, res: &Ty) -> TR<Vec<Row>> {
        let mut rows = Vec::with_capacity(alts.len());
        for a in alts {
            self.loc = a.loc;
            let mark = self.alt_begin();
            let pat = self.check_pat(&a.pat, scrut_ty)?;
            let body = self.rhs_body(&a.rhs, res)?;
            let body = self.alt_end(mark, body)?;
            rows.push(Row {
                pats: vec![pat],
                body,
            });
        }
        Ok(rows)
    }

    /// Checks a right-hand side. `where` bindings scope over guards and bodies.
    pub fn rhs_body(&mut self, rhs: &Rhs, ty: &Ty) -> TR<CBody> {
        // ImplicitParams: `where ?x = e` gives the right-hand side a binding for `?x`.
        if rhs.wheres.iter().any(|d| matches!(d, Decl::IPBind { .. })) {
            let mark = self.alt_begin();
            let mut binds: Vec<(String, Core)> = Vec::new();
            for d in &rhs.wheres {
                if let Decl::IPBind { name, expr, loc } = d {
                    self.loc = *loc;
                    let (ce, te) = self.infer(expr)?;
                    let ev = self.fresh_name("ip");
                    binds.push((ev.clone(), ce));
                    self.add_given(super::Given {
                        pred: super::types::Pred { class: format!("?{name}"), args: vec![te] },
                        ev: Core::Var(ev),
                    });
                }
            }
            let others: Vec<Decl> = rhs.wheres.iter().filter(|d| !matches!(d, Decl::IPBind { .. })).cloned().collect();
            let inner = self.rhs_body(&Rhs { body: rhs.body.clone(), wheres: others }, ty)?;
            let body = self.alt_end(mark, inner)?;
            return Ok(CBody::Where(binds, Box::new(body)));
        }
        let (mut wheres, deferred) = if rhs.wheres.is_empty() {
            (Vec::new(), Vec::new())
        } else {
            self.infer_local_deferring(&rhs.wheres)?
        };
        let body = match &rhs.body {
            Body::Plain(e) => CBody::Expr(self.check(e, ty)?),
            Body::Guarded(alts) => CBody::Guards(self.guard_alts(alts, ty)?),
        };
        wheres.extend(self.finish_deferred(deferred)?);
        Ok(if wheres.is_empty() {
            body
        } else {
            CBody::Where(wheres, Box::new(body))
        })
    }

    fn guard_alts(&mut self, alts: &[(Vec<Stmt>, Expr)], ty: &Ty) -> TR<Vec<(Vec<CGuard>, Core)>> {
        let mut out = Vec::with_capacity(alts.len());
        for (conds, e) in alts {
            let mut gs = Vec::with_capacity(conds.len());
            // A pattern guard whose pattern gives constraints (`| Dict <- d`) scopes them over
            // the guards after it and the body: their evidence is bound right after it.
            let mut scopes: Vec<(super::AltMark, usize)> = Vec::new();
            for s in conds {
                if let Stmt::Bind { pat, expr, loc } = s {
                    self.loc = *loc;
                    let (ce, te) = self.infer(expr)?;
                    let mark = self.alt_begin();
                    let cp = self.check_pat(pat, &te)?;
                    gs.push(CGuard::Pat(cp, ce));
                    if self.alt_introduced(&mark) {
                        scopes.push((mark, gs.len()));
                    } else {
                        self.alt_merge(mark);
                    }
                    continue;
                }
                gs.push(self.guard(s)?);
            }
            let c = self.check(e, ty)?;
            while let Some((mark, pos)) = scopes.pop() {
                let binds = self.alt_end_binds(mark)?;
                if !binds.is_empty() {
                    gs.insert(pos, CGuard::Let(binds));
                }
            }
            out.push((gs, c));
        }
        Ok(out)
    }

    fn guard(&mut self, s: &Stmt) -> TR<CGuard> {
        match s {
            Stmt::Expr { expr, loc } => {
                self.loc = *loc;
                let b = self.bool_ty();
                Ok(CGuard::Bool(self.check(expr, &b)?))
            }
            Stmt::Bind { pat, expr, loc } => {
                self.loc = *loc;
                let (ce, te) = self.infer(expr)?;
                let cp = self.check_pat(pat, &te)?;
                Ok(CGuard::Pat(cp, ce))
            }
            Stmt::Let { decls, loc } => {
                self.loc = *loc;
                Ok(CGuard::Let(self.infer_local(decls)?))
            }
        }
    }

    // ----- desugaring to AST ------------------------------------------------

    fn desugar_do(&mut self, stmts: &[Stmt], loc: Loc) -> TR<Expr> {
        let Some((first, rest)) = stmts.split_first() else {
            return self.err("Empty 'do' block");
        };
        if rest.is_empty() {
            return match first {
                Stmt::Expr { expr, .. } => Ok(expr.clone()),
                _ => self.err("The last statement in a 'do' block must be an expression"),
            };
        }
        match first {
            Stmt::Expr { expr, loc } => {
                let r = self.desugar_do(rest, loc.clone())?;
                Ok(app(app(prelude(">>", *loc), expr.clone()), r))
            }
            Stmt::Bind { pat, expr, loc } => {
                let r = self.desugar_do(rest, *loc)?;
                let lam = if self.irrefutable(pat) {
                    lam1(pat.clone(), r, *loc)
                } else {
                    let x = self.fresh_name("dox");
                    let msg = format!("Pattern match failure in `do' block at {}:{}", loc.line, loc.col);
                    let fail_call = app(prelude("fail", *loc), Expr::Lit(Lit::Str(msg), *loc));
                    let case = Expr::Case(
                        Box::new(var(&x, *loc)),
                        vec![
                            AstAlt {
                                pat: pat.clone(),
                                rhs: plain_rhs(r),
                                loc: *loc,
                            },
                            AstAlt {
                                pat: Pat::Wild(*loc),
                                rhs: plain_rhs(fail_call),
                                loc: *loc,
                            },
                        ],
                        *loc,
                    );
                    lam1(Pat::Var(x, *loc), case, *loc)
                };
                Ok(app(app(prelude(">>=", *loc), expr.clone()), lam))
            }
            Stmt::Let { decls, loc } => {
                let r = self.desugar_do(rest, *loc)?;
                Ok(Expr::Let(decls.clone(), Box::new(r), *loc))
            }
        }
    }

    /// Patterns that cannot fail, and so need no `fail` in a `do` block.
    fn irrefutable(&self, p: &Pat) -> bool {
        match p {
            Pat::Var(..) | Pat::Wild(_) | Pat::Lazy(..) => true,
            Pat::Bang(sub, _) | Pat::Sig(sub, _, _) | Pat::As(_, sub, _) => self.irrefutable(sub),
            Pat::Tuple(ps, _) => ps.iter().all(|p| self.irrefutable(p)),
            Pat::Rec { con, .. } => self
                .env
                .datacons
                .get(&qkey(con))
                .and_then(|d| self.env.tycons.get(&d.tycon))
                .is_some_and(|t| t.cons.len() == 1),
            Pat::Con(q, args, _) => {
                let single = self
                    .env
                    .datacons
                    .get(&qkey(q))
                    .and_then(|d| self.env.tycons.get(&d.tycon))
                    .is_some_and(|t| t.cons.len() == 1);
                single && args.iter().all(|a| self.irrefutable(a))
            }
            _ => false,
        }
    }

    /// A parallel comprehension `[e | qs1 | qs2 | ..]` (ParallelListComp): each branch is the
    /// list of the variables it binds, the lists are zipped, and `e` is drawn from the zip
    /// (`[e | ((x1, ..), ((y1, ..), ..)) <- zip [(x1, ..) | qs1] (zip [(y1, ..) | qs2] ..)]`).
    fn desugar_parallel_comp(&mut self, body: &Expr, branches: &[Vec<Stmt>], loc: Loc) -> Expr {
        let mut lists: Vec<(Pat, Expr)> = Vec::new();
        for b in branches {
            let mut vars: Vec<String> = Vec::new();
            for st in b {
                match st {
                    Stmt::Bind { pat, .. } => vars.extend(crate::rename::pat_vars(pat).into_iter().map(|(v, _)| v)),
                    Stmt::Let { decls, .. } => {
                        for d in decls {
                            match d {
                                Decl::Fun { name, .. } => vars.push(name.clone()),
                                Decl::PatBind { pat, .. } => vars.extend(crate::rename::pat_vars(pat).into_iter().map(|(v, _)| v)),
                                _ => {}
                            }
                        }
                    }
                    Stmt::Expr { .. } => {}
                }
            }
            let pat = match vars.as_slice() {
                [one] => Pat::Var(one.clone(), loc),
                _ => Pat::Tuple(vars.iter().map(|v| Pat::Var(v.clone(), loc)).collect(), loc),
            };
            let tuple = match vars.as_slice() {
                [one] => var(one, loc),
                _ => Expr::Tuple(vars.iter().map(|v| Some(var(v, loc))).collect(), loc),
            };
            let list = self.desugar_comp(&tuple, b, loc);
            lists.push((pat, list));
        }
        let Some((mut pat, mut list)) = lists.pop() else {
            return Expr::List(vec![], loc);
        };
        while let Some((p, l)) = lists.pop() {
            list = app(app(prelude("zip", loc), l), list);
            pat = Pat::Tuple(vec![p, pat], loc);
        }
        self.desugar_comp(body, &[Stmt::Bind { pat, expr: list, loc }], loc)
    }

    fn desugar_comp(&mut self, body: &Expr, quals: &[Stmt], loc: Loc) -> Expr {
        let Some((first, rest)) = quals.split_first() else {
            return Expr::List(vec![body.clone()], loc);
        };
        let tail = self.desugar_comp(body, rest, loc);
        match first {
            Stmt::Expr { expr, loc: l } => Expr::If(
                Box::new(expr.clone()),
                Box::new(tail),
                Box::new(Expr::List(vec![], *l)),
                *l,
            ),
            Stmt::Let { decls, loc: l } => Expr::Let(decls.clone(), Box::new(tail), *l),
            Stmt::Bind { pat, expr, loc: l } => {
                let x = self.fresh_name("lcx");
                let case = Expr::Case(
                    Box::new(var(&x, *l)),
                    vec![
                        AstAlt {
                            pat: pat.clone(),
                            rhs: plain_rhs(tail),
                            loc: *l,
                        },
                        AstAlt {
                            pat: Pat::Wild(*l),
                            rhs: plain_rhs(Expr::List(vec![], *l)),
                            loc: *l,
                        },
                    ],
                    *l,
                );
                // foldr (\x acc -> case .. ++ acc) [] xs: built from foldr and (++), which every
                // base module defines (concatMap lives in a later module).
                // A fresh name for each (a comprehension in another's qualifiers has its own).
                let acc = self.fresh_name("lcacc");
                let step = Expr::Lam(
                    vec![Pat::Var(x, *l), Pat::Var(acc.clone(), *l)],
                    Box::new(app(
                        app(prelude("++", *l), case),
                        Expr::Var(QName::unqual(acc), *l),
                    )),
                    *l,
                );
                app(app(app(prelude("foldr", *l), step), Expr::List(vec![], *l)), expr.clone())
            }
        }
    }

    fn desugar_rec_upd(&mut self, e: &Expr, fields: &[(QName, Option<Expr>)], loc: Loc) -> TR<Expr> {
        // Constructors that have every updated field.
        let mut cons: Option<Vec<String>> = None;
        for (f, _) in fields {
            let key = qkey(f);
            let owners: Vec<String> = self
                .env
                .fields
                .get(&key)
                .map(|v| v.iter().map(|(c, _)| c.clone()).collect())
                .unwrap_or_default();
            cons = Some(match cons {
                None => owners,
                Some(prev) => prev.into_iter().filter(|c| owners.contains(c)).collect(),
            });
        }
        let cons = cons.unwrap_or_default();
        if cons.is_empty() {
            return self.err("No constructor has all these fields");
        }
        // The new values go straight into the constructor applications (only one alternative
        // runs), so that a field of a rank-N type is checked against its polymorphic type.
        let new_values: Vec<(String, Expr)> = fields
            .iter()
            .map(|(f, v)| (f.name.clone(), v.clone().unwrap_or_else(|| var(&f.name, loc))))
            .collect();
        let mut alts = Vec::new();
        for con_key in &cons {
            let con = self.env.datacons.get(con_key).cloned().ok_or_else(|| super::TcError {
                loc,
                msg: "internal: missing constructor".into(),
            })?;
            let vars: Vec<String> = (0..con.arity).map(|_| self.fresh_name("uf")).collect();
            let mut args: Vec<Expr> = vars.iter().map(|v| var(v, loc)).collect();
            for (occ, value) in &new_values {
                if let Some(pos) = con.fields.iter().position(|o| o == occ) {
                    args[pos] = value.clone();
                }
            }
            let con_expr = Expr::Con(split_qualified(con_key), loc);
            let body = args.into_iter().fold(con_expr, app);
            let pat = Pat::Con(
                split_qualified(con_key),
                vars.iter().map(|v| Pat::Var(v.clone(), loc)).collect(),
                loc,
            );
            alts.push(AstAlt {
                pat,
                rhs: plain_rhs(body),
                loc,
            });
        }
        Ok(Expr::Case(Box::new(e.clone()), alts, loc))
    }

    fn infer_rec_con(&mut self, con: &QName, fields: &[(QName, Option<Expr>)], loc: Loc) -> TR<(Core, Ty)> {
        self.infer_rec_con_at(con, fields, loc, None)
    }

    fn infer_rec_con_at(&mut self, con: &QName, fields: &[(QName, Option<Expr>)], loc: Loc, expected: Option<&Ty>) -> TR<(Core, Ty)> {
        let key = qkey(con);
        let info = self.env.datacons.get(&key).cloned().ok_or_else(|| super::TcError {
            loc,
            msg: format!("Data constructor not in scope: {key}"),
        })?;
        // The constructor is instantiated like a plain constructor reference, so its context
        // dictionaries are passed (as wanteds) ahead of the fields, as the wrapper expects.
        let (con_core, cty) = self.inst_var(&key, &info.scheme, loc);
        let mut arg_tys = Vec::new();
        let mut cur = cty;
        for _ in 0..info.arity {
            let (a, r) = match cur.as_fun() {
                Some((a, r)) => (a.clone(), r.clone()),
                None => return self.err("internal: constructor type"),
            };
            arg_tys.push(a);
            cur = r;
        }
        if let Some(t) = expected {
            self.unify_at(t, &cur)?;
        }
        let mut args: Vec<Option<Core>> = vec![None; info.arity];
        for (f, value) in fields {
            let pos = info.fields.iter().position(|o| *o == f.name).ok_or_else(|| super::TcError {
                loc,
                msg: format!("Constructor ‘{}’ does not have field ‘{}’", con.name, f.name),
            })?;
            let value = match value {
                Some(v) => v.clone(),
                None => var(&f.name, loc),
            };
            // A field of a rank-N type is checked like a constructor argument of that type.
            args[pos] = Some(self.check_arg(&value, &arg_tys[pos])?);
        }
        let core = args.into_iter().enumerate().fold(con_core, |acc, (i, a)| {
            let a = a.unwrap_or_else(|| {
                Core::Fail(format!(
                    "Missing field in record construction {}",
                    info.fields.get(i).cloned().unwrap_or_default()
                ))
            });
            Core::App(Box::new(acc), Box::new(a))
        });
        Ok((core, cur))
    }

    /// Elaborates a group of local bindings (a `let` or `where`).
    pub fn infer_local(&mut self, decls: &[Decl]) -> TR<Vec<(String, Core)>> {
        self.infer_binds(decls, false)
    }

    /// Local bindings, with those whose type the body determines left for later (see
    /// `finish_deferred`): under MonoLocalBinds a binding that is not closed has a monomorphic
    /// type, and one that is a `case` (a GADT match, whose alternatives refine its type
    /// differently: ghc-lib-parser's `x = case ghcPass @p of GhcPs -> noAnn; ..`) is checked
    /// after its use fixes that type, as GHC's untouchable variables have it.
    pub fn infer_local_deferring(&mut self, decls: &[Decl]) -> TR<(Vec<(String, Core)>, Vec<Deferred>)> {
        if !self.mono_local_binds {
            return Ok((self.infer_local(decls)?, Vec::new()));
        }
        let mut sig_of: HashMap<String, Type> = HashMap::new();
        for d in decls {
            if let Decl::Sig { names, ty, .. } = d {
                for n in names {
                    sig_of.insert(n.clone(), ty.clone());
                }
            }
        }
        // Which declarations wait (by position), and as what.
        let candidate = |d: &Decl| -> Option<Deferred> {
            match d {
                Decl::Fun { name, clauses, loc } => {
                    if sig_of.contains_key(name) {
                        return None;
                    }
                    if let [c] = clauses.as_slice() {
                        if let Body::Plain(e @ Expr::Case(..)) = &c.rhs.body {
                            if c.pats.is_empty() && c.rhs.wheres.is_empty() && !is_closed_rhs(e) {
                                return Some(Deferred::Var(name.clone(), e.clone(), Ty::Con(String::new())));
                            }
                        }
                    }
                    // A function that mentions a scoped type variable (`pp_implicit ext = .. case
                    // ghcPass @p of ..`): checked as a `\case`-like lambda over its clauses.
                    let arity = clauses.first()?.pats.len();
                    if arity > 0 && clauses.iter().all(|c| c.pats.len() == arity) && clauses.iter().any(|c| mentions_type_var_rhs(&c.rhs)) {
                        return Some(Deferred::Var(name.clone(), clauses_lambda(clauses, *loc), Ty::Con(String::new())));
                    }
                    None
                }
                // A pattern binding of a `case` (`(rm_fun, check_fun) = case lookup_mode of ..`),
                // its variables simple and any signature monomorphic.
                Decl::PatBind { pat, rhs, loc } => {
                    let Body::Plain(e @ Expr::Case(..)) = &rhs.body else { return None };
                    if is_closed_rhs(e) || !simple_pattern(pat) {
                        return None;
                    }
                    let vars: Vec<String> = crate::rename::pat_vars(pat).into_iter().map(|(v, _)| v).collect();
                    if vars.iter().any(|v| matches!(sig_of.get(v), Some(Type::Forall(..)) | Some(Type::Qual(..)))) {
                        return None;
                    }
                    Some(Deferred::Pat { pat: pat.clone(), rhs: rhs.clone(), vars: vars.into_iter().map(|v| (v, Ty::Con(String::new()))).collect(), loc: *loc })
                }
                _ => None,
            }
        };
        let defined = |x: &Deferred| -> Vec<String> {
            match x {
                Deferred::Var(n, ..) => vec![n.clone()],
                Deferred::Pat { vars, .. } => vars.iter().map(|(v, _)| v.clone()).collect(),
            }
        };
        // A binding the others use is checked with them.
        let others: Vec<Decl> = decls.iter().filter(|d| candidate(d).is_none()).cloned().collect();
        let used = super::bind::decls_referenced_names(&others);
        let mut deferred: Vec<Deferred> = Vec::new();
        let mut rest: Vec<Decl> = Vec::new();
        let mut deferred_names: Vec<String> = Vec::new();
        for d in decls {
            match candidate(d) {
                Some(x) if !defined(&x).iter().any(|n| used.contains(n)) => {
                    deferred_names.extend(defined(&x));
                    deferred.push(x);
                }
                _ => rest.push(d.clone()),
            }
        }
        // The signatures of deferred pattern variables are used here, not by the others.
        rest.retain(|d| !matches!(d, Decl::Sig { names, .. } if names.iter().all(|n| deferred_names.contains(n))));
        for x in deferred.iter_mut() {
            match x {
                Deferred::Var(n, _, t) => {
                    *t = self.meta();
                    self.vars.insert(n.clone(), Scheme::mono(t.clone()));
                }
                Deferred::Pat { vars, .. } => {
                    for (v, t) in vars.iter_mut() {
                        *t = self.meta();
                        if let Some(sig) = sig_of.get(v) {
                            let sc = self.sig_scheme(sig, &[])?;
                            if sc.nvars == 0 && sc.preds.is_empty() {
                                self.unify_at(&sc.ty, t)?;
                            }
                        }
                        self.vars.insert(v.clone(), Scheme::mono(t.clone()));
                    }
                }
            }
        }
        let binds = self.infer_local(&rest)?;
        Ok((binds, deferred))
    }

    /// Checks the deferred bindings (see `infer_local_deferring`) at the types their uses gave.
    pub fn finish_deferred(&mut self, deferred: Vec<Deferred>) -> TR<Vec<(String, Core)>> {
        let mut out = Vec::new();
        for x in deferred {
            match x {
                Deferred::Var(n, e, t) => {
                    let core = self.check(&e, &t)?;
                    out.push((n, core));
                }
                Deferred::Pat { pat, rhs, vars, loc } => {
                    self.loc = loc;
                    // The pattern first, its variables at their types, then the right-hand side
                    // against the pattern's type.
                    let t = self.meta();
                    let cpat = self.check_pat(&pat, &t)?;
                    for (v, vt) in &vars {
                        let bound = self.vars[v].ty.clone();
                        self.unify_at(&bound, vt)?;
                    }
                    let body = self.rhs_body(&rhs, &t)?;
                    let s = self.fresh_name("pb");
                    let mut fresh = |b: &str| self.fresh_name(b);
                    let rhs_core = compile_body(&body, Core::Fail("Non-exhaustive guards in pattern binding".into()), &mut fresh);
                    out.push((s.clone(), rhs_core));
                    for (v, _) in &vars {
                        let extract = compile_rows(
                            std::slice::from_ref(&s),
                            &[Row { pats: vec![cpat.clone()], body: CBody::Expr(Core::Var(v.clone())) }],
                            Core::Fail("Irrefutable pattern failed".into()),
                            &mut fresh,
                        );
                        out.push((v.clone(), extract));
                    }
                }
            }
        }
        Ok(out)
    }
}

/// The boxing constructors of unboxed types (`I#` and friends), which are identities here.
/// The unboxed and boxed types of a boxing constructor such as `I#` (`Int#`, `Int`).
pub fn boxing_types(key: &str) -> Option<(&'static str, &'static str)> {
    Some(match key.rsplit_once('.').map_or(key, |(_, n)| n) {
        "I#" if key.ends_with("I#") => ("GHC.Prim.Int#", "GHC.Types.Int"),
        "W#" if key.ends_with("W#") => ("GHC.Prim.Word#", "GHC.Types.Word"),
        "D#" if key.ends_with("D#") => ("GHC.Prim.Double#", "GHC.Types.Double"),
        "F#" if key.ends_with("F#") => ("GHC.Prim.Float#", "GHC.Types.Float"),
        "C#" if key.ends_with("C#") => ("GHC.Prim.Char#", "GHC.Types.Char"),
        _ => return None,
    })
}

pub fn is_boxing_con(key: &str) -> bool {
    matches!(
        key,
        "GHC.Exts.I#" | "GHC.Exts.D#" | "GHC.Exts.C#" | "GHC.Exts.W#" | "GHC.Exts.F#"
            | "GHC.Types.I#" | "GHC.Types.D#" | "GHC.Types.C#" | "GHC.Types.W#" | "GHC.Types.F#"
    )
}

/// Splits a function type, assuming it is one.
fn split_fun_ty(t: &Ty) -> (Ty, Ty) {
    match t.as_fun() {
        Some((a, r)) => (a.clone(), r.clone()),
        None => (t.clone(), t.clone()),
    }
}

fn plain_rhs(e: Expr) -> Rhs {
    Rhs {
        body: Body::Plain(e),
        wheres: Vec::new(),
    }
}

/// Splits a qualified key `M.x` back into a renamed name.
fn split_qualified(key: &str) -> QName {
    let u = super::types::unqualify(key);
    let m = &key[..key.len() - u.len()];
    if m.is_empty() {
        QName::unqual(key)
    } else {
        QName {
            qual: Some(m.trim_end_matches('.').to_string()),
            name: u.to_string(),
        }
    }
}

/// Prefix of the alias of a banged pattern binding: the body forces these before running.
pub const STRICT_BIND: &str = "strict%";

pub fn wrap_let(binds: Vec<(String, Core)>, body: Core) -> Core {
    if binds.is_empty() {
        return body;
    }
    let forced: Vec<String> = binds
        .iter()
        .filter(|(n, _)| n.starts_with(STRICT_BIND))
        .map(|(n, _)| n.clone())
        .collect();
    let mut body = body;
    for n in forced.into_iter().rev() {
        body = Core::Case(Box::new(Core::Var(n.clone())), format!("{n}!"), vec![Alt::Default(body)]);
    }
    Core::Let(binds, Box::new(body))
}

/// A MagicHash literal as Core, with the name of its primitive type (from GHC.Prim).
pub fn prim_lit(p: &crate::syntax::token::PrimLit, negative: bool) -> (Core, &'static str) {
    use crate::syntax::token::PrimLit;
    let sign = if negative { "-" } else { "" };
    match p {
        PrimLit::Int(s) => (Core::Lit(CLit::Int(format!("{sign}{s}").parse().unwrap_or(0))), "GHC.Prim.Int#"),
        PrimLit::Word(s) => (Core::Lit(CLit::Int(s.parse::<i128>().unwrap_or(0) as u64 as i64)), "GHC.Prim.Word#"),
        PrimLit::Float(s) => (Core::Lit(CLit::Double(format!("{sign}{s}").parse().unwrap_or(0.0))), "GHC.Prim.Float#"),
        PrimLit::Double(s) => (Core::Lit(CLit::Double(format!("{sign}{s}").parse().unwrap_or(0.0))), "GHC.Prim.Double#"),
        PrimLit::Char(c) => (Core::Lit(CLit::Char(*c)), "GHC.Prim.Char#"),
        PrimLit::Str(s) => (Core::Lit(CLit::Addr(s.clone())), "GHC.Prim.Addr#"),
    }
}

/// Whether a binding's right-hand side is closed (MonoLocalBinds): it mentions no local
/// variable and no type variable (approximately: a local's renamed name has a `%`).
fn is_closed_rhs(e: &Expr) -> bool {
    let names = super::bind::referenced_names(e);
    !names.iter().any(|n| n.contains('%')) && !mentions_type_var(e)
}

/// Whether an expression has a type variable in a visible type application or an annotation,
/// anywhere inside it (a scoped type variable: the binding is not closed).
fn mentions_type_var(e: &Expr) -> bool {
    fn ty_has_var(t: &Type) -> bool {
        match t {
            Type::Var(..) => true,
            Type::App(a, b) | Type::Fun(a, b) => ty_has_var(a) || ty_has_var(b),
            Type::List(a, _) => ty_has_var(a),
            Type::Tuple(ts, _) => ts.iter().any(ty_has_var),
            Type::Forall(_, b) | Type::Qual(_, b) => ty_has_var(b),
            Type::Sig(a, _) => ty_has_var(a),
            _ => false,
        }
    }
    fn stmt(s: &Stmt) -> bool {
        match s {
            Stmt::Bind { expr, .. } | Stmt::Expr { expr, .. } => mentions_type_var(expr),
            Stmt::Let { decls, .. } => decls.iter().any(decl),
        }
    }
    fn rhs(r: &Rhs) -> bool {
        let body = match &r.body {
            Body::Plain(e) => mentions_type_var(e),
            Body::Guarded(gs) => gs.iter().any(|(conds, e)| conds.iter().any(stmt) || mentions_type_var(e)),
        };
        body || r.wheres.iter().any(decl)
    }
    fn decl(d: &Decl) -> bool {
        match d {
            Decl::Fun { clauses, .. } => clauses.iter().any(|c| rhs(&c.rhs)),
            Decl::PatBind { rhs: r, .. } => rhs(r),
            _ => false,
        }
    }
    match e {
        Expr::TyApp(f, t, _) => ty_has_var(t) || mentions_type_var(f),
        Expr::Sig(x, t, _) => ty_has_var(t) || mentions_type_var(x),
        Expr::App(f, x) => mentions_type_var(f) || mentions_type_var(x),
        Expr::OpChain(items, _) => items.iter().any(|i| matches!(i, crate::syntax::ast::OpItem::Operand(x) if mentions_type_var(x))),
        Expr::Lam(_, b, _) => mentions_type_var(b),
        Expr::Let(ds, b, _) => ds.iter().any(decl) || mentions_type_var(b),
        Expr::If(c, t, f, _) => mentions_type_var(c) || mentions_type_var(t) || mentions_type_var(f),
        Expr::MultiIf(alts, _) => alts.iter().any(|(conds, x)| conds.iter().any(stmt) || mentions_type_var(x)),
        Expr::Case(s, alts, _) => mentions_type_var(s) || alts.iter().any(|a| rhs(&a.rhs)),
        Expr::LamCase(alts, _) => alts.iter().any(|a| rhs(&a.rhs)),
        Expr::Do(ss, _) => ss.iter().any(stmt),
        Expr::Tuple(xs, _) => xs.iter().flatten().any(mentions_type_var),
        Expr::List(xs, _) => xs.iter().any(mentions_type_var),
        Expr::LeftSection(x, _, _) | Expr::RightSection(_, x, _) => mentions_type_var(x),
        Expr::RecCon { fields, .. } => fields.iter().any(|(_, v)| v.as_ref().is_some_and(mentions_type_var)),
        Expr::RecUpd(x, fields, _) => mentions_type_var(x) || fields.iter().any(|(_, v)| v.as_ref().is_some_and(mentions_type_var)),
        _ => false,
    }
}

pub(super) fn rhs_mentions_type_var(r: &Rhs) -> bool {
    mentions_type_var_rhs(r)
}

fn mentions_type_var_rhs(r: &Rhs) -> bool {
    mentions_type_var(&Expr::Case(Box::new(Expr::Wild(Loc::default())), vec![AstAlt { pat: Pat::Wild(Loc::default()), rhs: r.clone(), loc: Loc::default() }], Loc::default()))
}

/// A function's clauses as one lambda: `\x1 .. xn -> case (x1, .., xn) of` the clauses (one
/// argument: no tuple).
fn clauses_lambda(clauses: &[Clause], loc: Loc) -> Expr {
    let n = clauses[0].pats.len();
    let names: Vec<String> = (0..n).map(|i| format!("dfx%{i}")).collect();
    let scrut = if n == 1 {
        var(&names[0], loc)
    } else {
        Expr::Tuple(names.iter().map(|v| Some(var(v, loc))).collect(), loc)
    };
    let alts = clauses
        .iter()
        .map(|c| AstAlt {
            pat: if n == 1 { c.pats[0].clone() } else { Pat::Tuple(c.pats.clone(), c.loc) },
            rhs: c.rhs.clone(),
            loc: c.loc,
        })
        .collect();
    Expr::Lam(names.iter().map(|v| Pat::Var(v.clone(), loc)).collect(), Box::new(Expr::Case(Box::new(scrut), alts, loc)), loc)
}

/// A binding deferred until its use has given its type (see `infer_local_deferring`).
pub enum Deferred {
    /// A variable (or a function, as a lambda) and the type its uses gave it.
    Var(String, Expr, Ty),
    /// A pattern binding, with its variables' types.
    Pat { pat: Pat, rhs: Rhs, vars: Vec<(String, Ty)>, loc: Loc },
}

/// A pattern of variables, tuples and laziness only (a banged binding is forced before the body,
/// so it is not deferred).
fn simple_pattern(p: &Pat) -> bool {
    match p {
        Pat::Var(..) | Pat::Wild(..) => true,
        Pat::Tuple(ps, _) => ps.iter().all(simple_pattern),
        Pat::Lazy(q, ..) => simple_pattern(q),
        _ => false,
    }
}

/// ApplicativeDo for a block of independent statements ending in `pure e` or `return e` (with
/// or without `$`): `(\p1 .. pn -> e) <$> e1 <*> .. <*> en`, which needs only Applicative (one
/// statement: only Functor). Other blocks are desugared with `>>=` as usual.
fn applicative_do(stmts: &[Stmt], loc: Loc) -> Option<Expr> {
    let (last, init) = stmts.split_last()?;
    let Stmt::Expr { expr: last, .. } = last else { return None };
    let body = returned(last)?;
    if init.is_empty() {
        return None;
    }
    let mut pats = Vec::new();
    let mut actions = Vec::new();
    let mut bound: Vec<String> = Vec::new();
    for st in init {
        let (pat, action) = match st {
            Stmt::Bind { pat, expr, .. } => (pat.clone(), expr.clone()),
            Stmt::Expr { expr, loc } => (Pat::Wild(*loc), expr.clone()),
            Stmt::Let { .. } => return None,
        };
        // A refutable pattern would need MonadFail; a later action using a bound variable, Monad.
        if !irrefutable_simple(&pat) {
            return None;
        }
        let used = super::bind::referenced_names(&action);
        if bound.iter().any(|b| used.contains(b)) {
            return None;
        }
        bound.extend(crate::rename::pat_vars(&pat).into_iter().map(|(v, _)| v));
        pats.push(pat);
        actions.push(action);
    }
    let global = |n: &str| Expr::Var(QName { qual: Some("GHC.Internal.Base".into()), name: n.into() }, loc);
    let f = Expr::Lam(pats, Box::new(body), loc);
    let mut acc = app(app(global("fmap"), f), actions[0].clone());
    for a in &actions[1..] {
        acc = app(app(global("<*>"), acc), a.clone());
    }
    Some(acc)
}

/// The `e` of `pure e`, `return e`, `pure $ e` or `return $ e`.
fn returned(e: &Expr) -> Option<Expr> {
    let is_ret = |f: &Expr| matches!(f, Expr::Var(q, _) if q.name == "pure" || q.name == "return");
    match e {
        Expr::App(f, x) if is_ret(f) => Some((**x).clone()),
        // `pure $ e`, its operator resolved.
        Expr::App(f, x) if matches!(f.as_ref(), Expr::App(d, r) if matches!(d.as_ref(), Expr::Var(q, _) if q.name == "$") && is_ret(r)) => {
            Some((**x).clone())
        }
        Expr::OpChain(items, _) => match items.as_slice() {
            [crate::syntax::ast::OpItem::Operand(f), crate::syntax::ast::OpItem::Op(op, _), crate::syntax::ast::OpItem::Operand(x)]
                if is_ret(f) && op.name == "$" =>
            {
                Some(x.clone())
            }
            _ => None,
        },
        _ => None,
    }
}

/// A pattern whose match cannot fail: variables, wildcards, lazy patterns and tuples of them.
fn irrefutable_simple(p: &Pat) -> bool {
    match p {
        Pat::Var(..) | Pat::Wild(..) | Pat::Lazy(..) => true,
        Pat::Tuple(ps, _) => ps.iter().all(irrefutable_simple),
        _ => false,
    }
}
