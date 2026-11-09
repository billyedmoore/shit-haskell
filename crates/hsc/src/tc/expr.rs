//! Expression and pattern checking, with elaboration to Core.

use super::types::{Scheme, Ty};
use super::{Checker, TR, Wanted};
use crate::core::matcher::{compile_body, compile_rows, CBody, CGuard, CPat, Row};
use crate::core::{Alt, Core, Lit as CLit};
use crate::syntax::ast::{Alt as AstAlt, Body, Clause, Decl, Expr, Lit, Loc, Pat, QName, Rhs, Stmt};

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
        let (c, t) = self.infer(e)?;
        self.unify_at(&t, ty)?;
        Ok(c)
    }

    /// Infers the type of an expression, producing Core.
    pub fn infer(&mut self, e: &Expr) -> TR<(Core, Ty)> {
        self.loc = e.loc();
        match e {
            Expr::Var(q, loc) => {
                self.loc = *loc;
                let key = qkey(q);
                let scheme = self
                    .lookup_var(&key)
                    .ok_or_else(|| super::TcError {
                        loc: *loc,
                        msg: format!("Variable not in scope: {key}"),
                    })?;
                let (core, ty) = self.inst_var(&key, &scheme, *loc);
                // Call sites of `error` and `undefined` in user code carry their location,
                // which GHC 9.12 prints in the uncaught-exception backtrace.
                if self.user_module && (key == "Base.error" || key == "Base.undefined") {
                    let (fun, frame) = if key == "Base.error" {
                        ("error", "errorAt")
                    } else {
                        ("undefined", "undefinedAt")
                    };
                    let site = format!(
                        "{fun}, called at {}:{}:{} in main:{}",
                        self.source_name, loc.line, loc.col, self.module
                    );
                    let call = Core::App(
                        Box::new(Core::Var(format!("Base.{frame}"))),
                        Box::new(Core::Lit(CLit::Str(site))),
                    );
                    return Ok((call, ty));
                }
                Ok((core, ty))
            }
            Expr::Con(q, loc) => {
                self.loc = *loc;
                let key = qkey(q);
                if is_boxing_con(&key) {
                    // I#, D#, C#: boxing is the identity on the runtime representation.
                    let a = self.meta();
                    return Ok((Core::Lam("box%".into(), Box::new(Core::Var("box%".into()))), Ty::fun(a.clone(), a)));
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
            Expr::App(f, a) => {
                let (cf, tf) = self.infer(f)?;
                let (ca, ta) = self.infer(a)?;
                let (arg, res) = match self.metas.prune(&tf).as_fun() {
                    Some((x, y)) => (x.clone(), y.clone()),
                    None => {
                        let x = self.meta();
                        let y = self.meta();
                        self.unify_at(&tf, &Ty::fun(x.clone(), y.clone()))?;
                        (x, y)
                    }
                };
                self.unify_at(&ta, &arg)?;
                Ok((Core::App(Box::new(cf), Box::new(ca)), res))
            }
            Expr::TyApp(_, _, loc) => {
                self.loc = *loc;
                self.err("Visible type application is not supported yet")
            }
            Expr::OpChain(_, loc) => {
                self.loc = *loc;
                self.err("internal: unresolved operator chain")
            }
            Expr::Lam(pats, body, loc) => {
                self.loc = *loc;
                let mark = self.alt_begin();
                let arg_tys: Vec<Ty> = pats.iter().map(|_| self.meta()).collect();
                let cpats = self.check_pats(pats, &arg_tys)?;
                let (cbody, tbody) = self.infer(body)?;
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
                for s in scruts.iter().rev() {
                    core = Core::Lam(s.clone(), Box::new(core));
                }
                Ok((core, Ty::funs(arg_tys, tbody)))
            }
            Expr::Let(decls, body, loc) => {
                self.loc = *loc;
                let binds = self.infer_local(decls)?;
                let (cb, tb) = self.infer(body)?;
                Ok((wrap_let(binds, cb), tb))
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
                Ok((Core::Let(vec![(s, cs)], Box::new(matched)), r))
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
                    let name = Ty::tuple_con(elems.len());
                    Ok((Core::Con(name, cores), Ty::tuple(tys)))
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
                let mut core = Core::Con("Builtin.[]".into(), vec![]);
                for c in cores.into_iter().rev() {
                    core = Core::Con("Builtin.:".into(), vec![c, core]);
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
                self.infer(&call)
            }
            Expr::ListComp(body, branches, loc) => {
                self.loc = *loc;
                if branches.len() != 1 {
                    return self.err("Parallel list comprehensions are not supported yet");
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
                    return self.err("Polymorphic type annotations on expressions are not supported yet");
                }
                let c = self.check(e, &scheme.ty)?;
                Ok((c, scheme.ty))
            }
            Expr::Wild(loc) => {
                self.loc = *loc;
                self.err("Found hole: _")
            }
            Expr::AsPat(_, _, loc) | Expr::LazyPat(_, loc) | Expr::BangPat(_, loc) => {
                self.loc = *loc;
                self.err("Pattern syntax in expression context")
            }
        }
    }

    /// Instantiates a variable's scheme, creating wanted evidence for its predicates.
    pub fn inst_var(&mut self, name: &str, scheme: &Scheme, loc: Loc) -> (Core, Ty) {
        let (ty, preds, _) = self.metas.instantiate(scheme, self.level);
        let mut core = Core::Var(name.to_string());
        for p in preds {
            let ev = self.fresh_name("d");
            self.wanted.push(Wanted {
                pred: p,
                ev: ev.clone(),
                loc,
            });
            core = Core::App(Box::new(core), Box::new(Core::Var(ev)));
        }
        (core, ty)
    }

    fn infer_op(&mut self, op: &QName, loc: Loc) -> TR<(Core, Ty)> {
        let key = qkey(op);
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
                self.unify_at(&arg, &Ty::con("Base.Integer"))?;
                Ok((Core::App(Box::new(fi), Box::new(Core::Lit(CLit::Integer(s.clone())))), res))
            }
            Lit::Frac(s) => {
                let (fr, fty) = self.lookup_inst("Base.fromRational")?;
                let (arg, res) = split_fun_ty(&fty);
                self.unify_at(&arg, &Ty::apps(Ty::con("Base.Ratio"), vec![Ty::con("Base.Integer")]))?;
                let lit = Core::Prim("ratioLit".into(), vec![Core::Lit(CLit::Frac(s.clone()))]);
                Ok((Core::App(Box::new(fr), Box::new(lit)), res))
            }
            Lit::Char(c) => Ok((Core::Lit(CLit::Char(*c)), Ty::con("Base.Char"))),
            Lit::Str(s) => Ok((Core::Lit(CLit::Str(s.clone())), Ty::list(Ty::con("Base.Char")))),
        }
    }

    /// Looks up a Prelude function and instantiates it (the caller adds the wanteds).
    fn lookup_inst(&mut self, name: &str) -> TR<(Core, Ty)> {
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
                let key = qkey(q);
                if is_boxing_con(&key) && args.len() == 1 {
                    return self.check_pat(&args[0], ty);
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
                let n = occs.len();
                let mut args: Vec<Pat> = (0..n).map(|_| Pat::Wild(*loc)).collect();
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
                    order.push(pos);
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
        // Refinement: the result type may be more specific than the scrutinee (GADTs).
        match self.metas.unify_refine(&cur, ty) {
            Ok(()) => {}
            Err(_) => {
                let (r, t) = (self.show(&cur), self.show(ty));
                return self.err(format!("Couldn't match expected type ‘{t}’ with actual type ‘{r}’"));
            }
        }
        // Variables that do not occur in the scrutinee type are existential: rigid.
        let mut scrut_metas = Vec::new();
        self.metas.free_metas(&self.metas.zonk(ty), &mut scrut_metas);
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
            self.givens.push(super::Given {
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
        let x = self.fresh_name("lx");
        let (lit_core, eq_ty, num_needed) = match l {
            Lit::Int(s) => {
                let s = if negative { format!("-{s}") } else { s.clone() };
                let (fi, fty) = self.lookup_inst("Base.fromInteger")?;
                let (arg, res) = split_fun_ty(&fty);
                self.unify_at(&arg, &Ty::con("Base.Integer"))?;
                self.unify_at(&res, ty)?;
                (Core::App(Box::new(fi), Box::new(Core::Lit(CLit::Integer(s)))), ty.clone(), true)
            }
            Lit::Frac(s) => {
                let s = if negative { format!("-{s}") } else { s.clone() };
                let (fr, fty) = self.lookup_inst("Base.fromRational")?;
                let (arg, res) = split_fun_ty(&fty);
                self.unify_at(&arg, &Ty::apps(Ty::con("Base.Ratio"), vec![Ty::con("Base.Integer")]))?;
                self.unify_at(&res, ty)?;
                let lit = Core::Prim("ratioLit".into(), vec![Core::Lit(CLit::Frac(s))]);
                (Core::App(Box::new(fr), Box::new(lit)), ty.clone(), true)
            }
            Lit::Char(c) => {
                self.unify_at(&Ty::con("Base.Char"), ty)?;
                (Core::Lit(CLit::Char(*c)), ty.clone(), false)
            }
            Lit::Str(s) => {
                let str_ty = Ty::list(Ty::con("Base.Char"));
                self.unify_at(&str_ty, ty)?;
                (Core::Lit(CLit::Str(s.clone())), str_ty, false)
            }
        };
        // Equality: `==` from an Eq dictionary for the scrutinee type.
        let (eq, eq_fty) = self.lookup_inst("Base.==")?;
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
        let wheres = if rhs.wheres.is_empty() {
            Vec::new()
        } else {
            self.infer_local(&rhs.wheres)?
        };
        let body = match &rhs.body {
            Body::Plain(e) => CBody::Expr(self.check(e, ty)?),
            Body::Guarded(alts) => CBody::Guards(self.guard_alts(alts, ty)?),
        };
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
            for s in conds {
                gs.push(self.guard(s)?);
            }
            let c = self.check(e, ty)?;
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
            Pat::Bang(sub, _) | Pat::Sig(sub, _, _) => self.irrefutable(sub),
            Pat::Tuple(ps, _) => ps.iter().all(|p| self.irrefutable(p)),
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
                app(app(prelude("concatMap", *l), lam1(Pat::Var(x, *l), case, *l)), expr.clone())
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
        // Bind the new values once, outside the case.
        let mut decls = Vec::new();
        let mut new_names: Vec<(String, String)> = Vec::new();
        for (f, v) in fields {
            let value = match v {
                Some(x) => x.clone(),
                None => var(&f.name, loc),
            };
            let name = self.fresh_name("upd");
            decls.push(Decl::Fun {
                name: name.clone(),
                clauses: vec![Clause {
                    pats: vec![],
                    rhs: plain_rhs(value),
                    loc,
                }],
                loc,
            });
            new_names.push((f.name.clone(), name));
        }
        let mut alts = Vec::new();
        for con_key in &cons {
            let con = self.env.datacons.get(con_key).cloned().ok_or_else(|| super::TcError {
                loc,
                msg: "internal: missing constructor".into(),
            })?;
            let vars: Vec<String> = (0..con.arity).map(|_| self.fresh_name("uf")).collect();
            let mut args: Vec<Expr> = vars.iter().map(|v| var(v, loc)).collect();
            for (occ, upd) in &new_names {
                if let Some(pos) = con.fields.iter().position(|o| o == occ) {
                    args[pos] = var(upd, loc);
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
        let case = Expr::Case(Box::new(e.clone()), alts, loc);
        Ok(if decls.is_empty() {
            case
        } else {
            Expr::Let(decls, Box::new(case), loc)
        })
    }

    fn infer_rec_con(&mut self, con: &QName, fields: &[(QName, Option<Expr>)], loc: Loc) -> TR<(Core, Ty)> {
        let key = qkey(con);
        let info = self.env.datacons.get(&key).cloned().ok_or_else(|| super::TcError {
            loc,
            msg: format!("Data constructor not in scope: {key}"),
        })?;
        let (cty, _, _) = self.metas.instantiate(&info.scheme, self.level);
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
            args[pos] = Some(self.check(&value, &arg_tys[pos])?);
        }
        if info.newtype {
            let inner = args.into_iter().next().flatten().unwrap_or_else(|| {
                Core::Fail("Missing field in record construction".into())
            });
            return Ok((inner, cur));
        }
        let cores: Vec<Core> = args
            .into_iter()
            .enumerate()
            .map(|(i, a)| {
                a.unwrap_or_else(|| {
                    Core::Fail(format!(
                        "Missing field in record construction {}",
                        info.fields.get(i).cloned().unwrap_or_default()
                    ))
                })
            })
            .collect();
        Ok((Core::Con(key, cores), cur))
    }

    /// Elaborates a group of local bindings (a `let` or `where`).
    pub fn infer_local(&mut self, decls: &[Decl]) -> TR<Vec<(String, Core)>> {
        self.infer_binds(decls, false)
    }

}

/// The boxing constructors of unboxed types (`I#` and friends), which are identities here.
pub fn is_boxing_con(key: &str) -> bool {
    matches!(key, "GHC.Exts.I#" | "GHC.Exts.D#" | "GHC.Exts.C#" | "GHC.Exts.W#")
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

pub fn wrap_let(binds: Vec<(String, Core)>, body: Core) -> Core {
    if binds.is_empty() {
        body
    } else {
        Core::Let(binds, Box::new(body))
    }
}

