//! Constraint solving: givens (with superclasses), instances, and defaulting.

use super::env::InstInfo;
use super::types::{subst_gen, Pred, Ty};
use super::{select_field, Checker, Given, Wanted, TR};
use crate::core::Core;

/// Classes whose presence makes a type variable defaultable.
const NUMERIC_CLASSES: &[&str] = &[
    "Base.Num",
    "Base.Real",
    "Base.Integral",
    "Base.Fractional",
    "Base.Floating",
    "Base.RealFrac",
    "Base.RealFloat",
];

/// Candidate types for defaulting, in order.
const DEFAULT_CANDIDATES: &[&str] = &["GHC.Num.Integer.Integer", "GHC.Types.Double"];

/// Whether a class is one of the standard numeric classes, under the mini Prelude's names or the
/// real base's (`GHC.Internal.Num.Num` and the like).
fn is_numeric_class(class: &str) -> bool {
    if NUMERIC_CLASSES.contains(&class) {
        return true;
    }
    let unqualified = class.rsplit('.').next().unwrap_or(class);
    class.starts_with("GHC.Internal.")
        && matches!(
            unqualified,
            "Num" | "Real" | "Integral" | "Fractional" | "Floating" | "RealFrac" | "RealFloat"
        )
}

/// The class of equality constraints `a ~ b` (the builtin type operator `~`). Their evidence is
/// a unit value; a wanted one is solved by unification and a given one refines rigid variables.
pub const EQ_CLASS: &str = "Builtin.~";

/// Whether a predicate class is an equality constraint (`~` or the heterogeneous `~~`).
pub fn is_eq_class(class: &str) -> bool {
    class == EQ_CLASS || class == "Builtin.~~"
}

/// The class of `dataToTag#` (GHC.Magic), whose instances the compiler builds.
pub const DATA_TO_TAG_CLASS: &str = "GHC.Magic.DataToTag";

/// The class of `withDict`: `WithDict cls meth`, solved by the compiler (GHC.Magic.Dict).
pub const WITH_DICT_CLASS: &str = "GHC.Magic.Dict.WithDict";

/// The class `Typeable`, whose instances the compiler builds.
pub const TYPEABLE_CLASS: &str = "GHC.Internal.Data.Typeable.Internal.Typeable";

impl Checker {
    /// The evidence for `DataToTag t`: a function from a value to the position of its
    /// constructor (a newtype is erased, so its only constructor is position 0).
    fn data_to_tag_evidence(&mut self, pred: &Pred) -> Option<Core> {
        let tycon = match pred.args[0].split_apps().0 {
            Ty::Con(c) => c.clone(),
            _ => return None,
        };
        let mut cons: Vec<(usize, String, usize, bool)> = self
            .env
            .datacons
            .values()
            .filter(|d| d.tycon == tycon)
            .map(|d| (d.tag, d.name.clone(), d.arity, d.newtype))
            .collect();
        if cons.is_empty() {
            return None;
        }
        cons.sort();
        let dict_con = self.class(DATA_TO_TAG_CLASS)?.dict_con.clone();
        let x = self.fresh_name("dtt");
        let body = if cons.iter().any(|c| c.3) {
            Core::Lit(crate::core::Lit::Int(0))
        } else {
            let scrut = self.fresh_name("dtt");
            let mut alts: Vec<crate::core::Alt> = Vec::new();
            for (tag, name, arity, _) in &cons {
                let vars: Vec<String> = (0..*arity).map(|_| self.fresh_name("dttf")).collect();
                alts.push(crate::core::Alt::Con(name.clone(), vars, Core::Lit(crate::core::Lit::Int(*tag as i64))));
            }
            Core::Case(Box::new(Core::Var(x.clone())), scrut, alts)
        };
        Some(Core::Con(dict_con, vec![Core::Lam(x, Box::new(body))]))
    }

    /// The evidence for `Typeable t` from the structure of `t`: a constructor's representation
    /// comes from its name, and an application's from its parts (whose evidence is wanted).
    /// Returns `None` when `t` is not yet known (a unification variable) or a given applies.
    fn typeable_evidence(&mut self, pred: &Pred, work: &mut Vec<Wanted>, loc: crate::syntax::ast::Loc) -> TR<Option<Core>> {
        let class = match self.class(TYPEABLE_CLASS) {
            Some(c) => c.dict_con.clone(),
            None => return Ok(None),
        };
        let t = pred.args[0].clone();
        let rep = match self.metas.prune(&t) {
            Ty::App(f, x) => {
                let (ef, ex) = (self.fresh_name("tyf"), self.fresh_name("tyx"));
                work.push(Wanted { pred: Pred { class: TYPEABLE_CLASS.into(), args: vec![*f] }, ev: ef.clone(), loc });
                work.push(Wanted { pred: Pred { class: TYPEABLE_CLASS.into(), args: vec![*x] }, ev: ex.clone(), loc });
                let mk = Core::Var("GHC.Internal.Data.Typeable.Internal.mkTrApp".into());
                let rf = self.typeable_rep_of(&ef, &class);
                let rx = self.typeable_rep_of(&ex, &class);
                Core::App(Box::new(Core::App(Box::new(mk), Box::new(rf))), Box::new(rx))
            }
            Ty::Con(c) => {
                let (modl, name) = match c.rfind('.') {
                    Some(i) if i > 0 && i + 1 < c.len() => (c[..i].to_string(), c[i + 1..].to_string()),
                    _ => ("Builtin".to_string(), c.clone()),
                };
                let (m, _) = self.infer(&crate::syntax::ast::Expr::Lit(crate::syntax::ast::Lit::Str(modl), loc))?;
                let (n, _) = self.infer(&crate::syntax::ast::Expr::Lit(crate::syntax::ast::Lit::Str(name), loc))?;
                let mk = Core::Var("GHC.Internal.Data.Typeable.Internal.mkTypeableCon#".into());
                Core::App(Box::new(Core::App(Box::new(mk), Box::new(m))), Box::new(n))
            }
            _ => return Ok(None),
        };
        Ok(Some(Core::Con(class, vec![rep])))
    }

    /// The call stack of a call to `fname` at `loc`: its frame pushed onto `base`.
    fn push_call_frame(&self, fname: &str, loc: crate::syntax::ast::Loc, base: Core) -> Core {
        let pkg = if self.module.starts_with("GHC.Internal.") { "ghc-internal" } else { "main" };
        let lit = |s: &str| Core::Lit(crate::core::Lit::Str(s.to_string()));
        let int = |n: u32| Core::Lit(crate::core::Lit::Int(i64::from(n)));
        let srcloc = Core::Con(
            "GHC.Internal.Stack.Types.SrcLoc".into(),
            vec![
                lit(pkg),
                lit(&self.module),
                lit(&self.source_name),
                int(loc.line),
                int(loc.col),
                int(loc.line),
                int(loc.col + fname.chars().count() as u32),
            ],
        );
        let frame = Core::Con("Builtin.(,)".into(), vec![lit(fname), srcloc]);
        Core::App(
            Box::new(Core::App(
                Box::new(Core::Var("GHC.Internal.Stack.Types.pushCallStack".into())),
                Box::new(frame),
            )),
            Box::new(base),
        )
    }

    /// The `typeRep#` field of a Typeable dictionary variable.
    fn typeable_rep_of(&mut self, dict: &str, class: &str) -> Core {
        super::select_field(Core::Var(dict.to_string()), class, 1, 0, &mut || self.fresh_name("trv"))
    }

    /// The evidence for `WithDict cls meth`: a dictionary whose method takes a value of the
    /// class's single method and a continuation expecting the class dictionary.
    fn with_dict_evidence(&mut self, pred: &Pred) -> TR<Core> {
        let (head, cargs) = pred.args[0].split_apps();
        let cname = match head {
            Ty::Con(c) => c.clone(),
            _ => return self.err("withDict: the constraint must be a class"),
        };
        let cargs: Vec<Ty> = cargs.into_iter().cloned().collect();
        let info = match self.class(&cname).cloned() {
            Some(info) => info,
            None => return self.err(format!("withDict: ‘{cname}’ is not a class")),
        };
        if !info.supers.is_empty() || info.methods.len() != 1 || info.methods[0].1.nvars as usize != info.nparams as usize {
            return self.err(format!("withDict: ‘{cname}’ must have a single method and no superclasses"));
        }
        let meth_ty = super::types::subst_gen(&info.methods[0].1.ty, &cargs);
        self.unify_at(&pred.args[1], &meth_ty)?;
        let m = self.fresh_name("wdm");
        let k = self.fresh_name("wdk");
        let dict = Core::Con(info.dict_con.clone(), vec![Core::Var(m.clone())]);
        let body = Core::App(Box::new(Core::Var(k.clone())), Box::new(dict));
        let fun = Core::Lam(m, Box::new(Core::Lam(k, Box::new(body))));
        let wd = match self.class(WITH_DICT_CLASS) {
            Some(c) => c.dict_con.clone(),
            None => return self.err("withDict: GHC.Magic.Dict is not loaded"),
        };
        Ok(Core::Con(wd, vec![fun]))
    }

    /// Adds a given constraint. An equality given `a ~ b` refines the rigid variables it
    /// mentions, for the rest of the enclosing scope.
    pub fn add_given(&mut self, g: Given) {
        if is_eq_class(&g.pred.class) && g.pred.args.len() == 2 {
            let (a, b) = (g.pred.args[0].clone(), g.pred.args[1].clone());
            let _ = self.metas.unify_refine(&a, &b);
        }
        self.givens.push(g);
    }

    /// Solves what can be solved. Returns evidence bindings and the residual wanteds
    /// that still mention unsolved unification variables.
    pub fn solve(&mut self, wanteds: Vec<Wanted>) -> TR<(Vec<(String, Core)>, Vec<Wanted>)> {
        self.retry_deferred()?;
        let mut work = wanteds;
        let mut binds = Vec::new();
        let mut residual = Vec::new();
        while let Some(w) = work.pop() {
            self.improve(&w.pred)?;
            let pred = self.metas_zonk_pred(&w.pred);
            if is_eq_class(&pred.class) && pred.args.len() == 2 {
                self.unify_at(&pred.args[0], &pred.args[1])?;
                binds.push((w.ev.clone(), Core::Con("Builtin.()".into(), vec![])));
                continue;
            }
            // A constraint family (`type family C a :: Constraint`) that reduces to no class
            // constraints holds; its evidence is unused.
            if self.env.families.contains_key(&pred.class) {
                let applied = Ty::apps(Ty::con(&pred.class), pred.args.clone());
                let reduced = self.normalize(&applied);
                if reduced != applied {
                    let preds = super::convert::ty_to_preds(&reduced);
                    for p in preds {
                        let ev = self.fresh_name("cf");
                        work.push(Wanted { pred: p, ev, loc: w.loc });
                    }
                    binds.push((w.ev.clone(), Core::Con("Builtin.()".into(), vec![])));
                    continue;
                }
                // A stuck family constraint is taken to hold: its evidence is never inspected,
                // and GHC's own proof (from the pattern that provides it) is not reproduced here.
                if !self.pred_has_metas(&pred) {
                    binds.push((w.ev.clone(), Core::Con("Builtin.()".into(), vec![])));
                    continue;
                }
            }
            // DataToTag is solved by the compiler for every data type: the tag of a value's constructor.
            if pred.class == DATA_TO_TAG_CLASS && pred.args.len() == 1 {
                if let Some(ev) = self.data_to_tag_evidence(&pred) {
                    binds.push((w.ev.clone(), ev));
                    continue;
                }
            }
            // Typeable is solved by the compiler: a representation built from the type's structure.
            if pred.class == TYPEABLE_CLASS && pred.args.len() == 1 {
                if let Some(ev) = self.typeable_evidence(&pred, &mut work, w.loc)? {
                    binds.push((w.ev.clone(), ev));
                    continue;
                }
            }
            // `withDict @C m k`: the compiler solves `WithDict C meth` for a single-method class.
            if pred.class == WITH_DICT_CLASS && pred.args.len() == 2 {
                let ev = self.with_dict_evidence(&pred)?;
                binds.push((w.ev.clone(), ev));
                continue;
            }
            // Representational equality is not checked (coercions are erased), so it always holds.
            if pred.class == "GHC.Types.Coercible" {
                binds.push((w.ev.clone(), Core::Con("Builtin.()".into(), vec![])));
                continue;
            }
            if pred.class.starts_with('?') {
                // An implicit parameter is solved by the innermost binding of that name. A call
                // of a HasCallStack function pushes its frame onto the enclosing stack, which is
                // the empty stack when there is none.
                let site = if pred.class == "?callStack" { self.callsites.get(&w.ev).cloned() } else { None };
                let given = self.givens.iter().rev().find(|g| g.pred.class == pred.class).cloned();
                match given {
                    Some(g) => {
                        self.unify_at(&g.pred.args[0], &pred.args[0])?;
                        let ev = match site {
                            Some((f, l)) => self.push_call_frame(&f, l, g.ev.clone()),
                            None => g.ev.clone(),
                        };
                        binds.push((w.ev.clone(), ev));
                    }
                    None if pred.class == "?callStack" && self.env.values.contains_key("GHC.Internal.Stack.Types.emptyCallStack") => {
                        self.unify_at(&Ty::con("GHC.Internal.Stack.Types.CallStack"), &pred.args[0])?;
                        let empty = Core::Var("GHC.Internal.Stack.Types.emptyCallStack".into());
                        let ev = match site {
                            Some((f, l)) => self.push_call_frame(&f, l, empty),
                            None => empty,
                        };
                        binds.push((w.ev.clone(), ev));
                    }
                    None => residual.push(Wanted { pred, ..w }),
                }
                continue;
            }
            if let Some(ev) = self.find_given(&pred) {
                binds.push((w.ev.clone(), ev));
                continue;
            }
            match self.find_instance(&pred) {
                Some((inst_dict, ctx)) => {
                    let mut args = Vec::with_capacity(ctx.len());
                    for cp in ctx {
                        let ev = self.fresh_name("d");
                        args.push(Core::Var(ev.clone()));
                        work.push(Wanted {
                            pred: cp,
                            ev,
                            loc: w.loc,
                        });
                    }
                    let core = args
                        .into_iter()
                        .fold(Core::Var(inst_dict), |f, a| Core::App(Box::new(f), Box::new(a)));
                    binds.push((w.ev.clone(), core));
                }
                None => {
                    if self.pred_has_metas(&pred) {
                        residual.push(Wanted { pred, ..w });
                    } else {
                        let msg = format!("No instance for ({}) arising from a use of a class method", self.show_pred(&pred));
                        return Err(super::TcError { loc: w.loc, msg });
                    }
                }
            }
        }
        Ok((binds, residual))
    }

    /// Functional-dependency improvement: when the determining arguments of a predicate match an
    /// instance, the determined arguments are unified with the instance's.
    pub fn improve(&mut self, p: &Pred) -> TR<()> {
        let fundeps = match self.class(&p.class) {
            Some(info) if !info.fundeps.is_empty() => info.fundeps.clone(),
            _ => return Ok(()),
        };
        for (from, to) in fundeps {
            let insts: Vec<InstInfo> = self.env.instances.iter().filter(|i| i.class == p.class).cloned().collect();
            for inst in insts {
                if inst.head.len() != p.args.len() {
                    continue;
                }
                let mut sub: Vec<Option<Ty>> = vec![None; inst.nvars as usize];
                let zonked: Vec<Ty> = p.args.iter().map(|t| self.metas.zonk(t)).collect();
                let ok = from.iter().all(|&i| self.match_ty(&inst.head[i], &zonked[i], &mut sub));
                if !ok {
                    continue;
                }
                let args: Vec<Ty> = sub.into_iter().map(|x| x.unwrap_or_else(|| self.meta())).collect();
                for &j in &to {
                    let target = subst_gen(&inst.head[j], &args);
                    self.unify_at(&p.args[j], &target)?;
                }
                break;
            }
        }
        Ok(())
    }

    /// A predicate with solved variables substituted and type families reduced.
    pub fn metas_zonk_pred(&self, p: &Pred) -> Pred {
        if self.env.families.is_empty() {
            return self.metas.zonk_pred(p);
        }
        Pred {
            class: p.class.clone(),
            args: p.args.iter().map(|t| self.normalize(t)).collect(),
        }
    }

    pub fn pred_has_metas(&self, p: &Pred) -> bool {
        let mut out = Vec::new();
        for a in &p.args {
            self.metas.free_metas(a, &mut out);
        }
        !out.is_empty()
    }

    /// Searches the givens and their superclass closures for an exact match.
    pub fn find_given(&self, want: &Pred) -> Option<Core> {
        for g in self.givens.iter().rev() {
            if let Some(core) = self.search_closure(&g.pred, &g.ev, want, 0) {
                return Some(core);
            }
        }
        None
    }

    fn search_closure(&self, have: &Pred, ev: &Core, want: &Pred, depth: usize) -> Option<Core> {
        let have_z = self.metas_zonk_pred(have);
        if have_z == *want {
            return Some(ev.clone());
        }
        if depth > 16 {
            return None;
        }
        let info = self.class(&have.class)?;
        let nsupers = info.nsupers();
        for (i, sup) in info.supers.iter().enumerate() {
            let sup_pred = Pred {
                class: sup.class.clone(),
                args: sup.args.iter().map(|t| subst_gen(t, &have.args)).collect(),
            };
            let dict_con = info.dict_con.clone();
            let mut counter = 0usize;
            let mut fresh = || {
                counter += 1;
                format!("s%{depth}_{counter}")
            };
            let sel = select_field(ev.clone(), &dict_con, nsupers + info.methods.len(), i, &mut fresh);
            if let Some(core) = self.search_closure(&sup_pred, &sel, want, depth + 1) {
                return Some(core);
            }
        }
        None
    }

    /// Finds an instance for a predicate. Returns the dictionary function name and the
    /// instance context instantiated for this predicate.
    pub fn find_instance(&self, want: &Pred) -> Option<(String, Vec<Pred>)> {
        for inst in &self.env.instances {
            if inst.class != want.class || inst.head.len() != want.args.len() {
                continue;
            }
            let mut sub: Vec<Option<Ty>> = vec![None; inst.nvars as usize];
            let ok = inst
                .head
                .iter()
                .zip(&want.args)
                .all(|(pat, t)| self.match_ty(pat, &self.metas.zonk(t), &mut sub));
            if ok {
                let args: Vec<Ty> = sub.into_iter().map(|s| s.unwrap_or(Ty::con("Builtin.?"))).collect();
                let ctx = inst
                    .ctx
                    .iter()
                    .map(|p| Pred {
                        class: p.class.clone(),
                        args: p.args.iter().map(|t| subst_gen(t, &args)).collect(),
                    })
                    .collect();
                return Some((inst.dict.clone(), ctx));
            }
        }
        None
    }

    /// One-way matching of an instance head pattern against a zonked type.
    fn match_ty(&self, pat: &Ty, t: &Ty, sub: &mut Vec<Option<Ty>>) -> bool {
        match (pat, t) {
            (Ty::Gen(i), _) => {
                let i = *i as usize;
                match &sub[i] {
                    Some(prev) => *prev == *t,
                    None => {
                        sub[i] = Some(t.clone());
                        true
                    }
                }
            }
            (Ty::Con(a), Ty::Con(b)) => a == b,
            (Ty::App(f1, a1), Ty::App(f2, a2)) => self.match_ty(f1, f2, sub) && self.match_ty(a1, a2, sub),
            _ => false,
        }
    }

    /// Defaulting for residual constraints on unconstrained type variables. Only applies
    /// when at least one class is numeric and a candidate type satisfies every constraint.
    /// Every defaultable variable is fixed in one pass; the rest is re-solved until stable.
    pub fn default_residual(&mut self, residual: Vec<Wanted>) -> TR<Vec<Wanted>> {
        let mut residual = residual;
        loop {
            if residual.is_empty() {
                break;
            }
            let mut candidates: Vec<u32> = Vec::new();
            for w in &residual {
                let p = self.metas_zonk_pred(&w.pred);
                for a in &p.args {
                    if let Ty::Meta(m) = self.metas.prune(a) {
                        if !candidates.contains(&m) {
                            candidates.push(m);
                        }
                    }
                }
            }
            let mut progress = false;
            for m in candidates {
                // Re-check: an earlier default may already have fixed this variable.
                if !matches!(self.metas.prune(&Ty::Meta(m)), Ty::Meta(x) if x == m) {
                    continue;
                }
                let preds_on_m: Vec<Pred> = residual
                    .iter()
                    .map(|w| self.metas_zonk_pred(&w.pred))
                    .filter(|p| p.args.iter().any(|a| matches!(self.metas.prune(a), Ty::Meta(x) if x == m)))
                    .collect();
                // A type that only `Typeable` constrains is a kind argument this checker does not
                // track (kinds are not inferred for signatures); it is taken as unit.
                if preds_on_m.iter().all(|p| p.class == TYPEABLE_CLASS) && !preds_on_m.is_empty() {
                    self.metas.subst[m as usize] = Some(Ty::con("Builtin.()"));
                    progress = true;
                    continue;
                }
                if !preds_on_m.iter().any(|p| is_numeric_class(&p.class)) {
                    continue;
                }
                for cand in DEFAULT_CANDIDATES {
                    let cand_ty = Ty::con(cand);
                    let ok = preds_on_m.iter().all(|p| {
                        let args: Vec<Ty> = p
                            .args
                            .iter()
                            .map(|a| if matches!(self.metas.prune(a), Ty::Meta(x) if x == m) { cand_ty.clone() } else { a.clone() })
                            .collect();
                        self.find_instance(&Pred {
                            class: p.class.clone(),
                            args,
                        })
                        .is_some()
                    });
                    if ok {
                        self.metas.subst[m as usize] = Some(cand_ty);
                        progress = true;
                        break;
                    }
                }
            }
            if !progress {
                break;
            }
            let (binds, rest) = self.solve(std::mem::take(&mut residual))?;
            self.top_binds_from_evidence(binds);
            residual = rest;
        }
        Ok(residual)
    }

    /// Evidence bindings produced at top level are kept as global bindings.
    pub fn top_binds_from_evidence(&mut self, binds: Vec<(String, Core)>) {
        for (name, body) in binds {
            self.out.push(crate::core::TopBind { name, body });
        }
    }

    /// Reports the first unresolved constraint as an ambiguity error.
    pub fn ambiguity_error<T>(&self, w: &Wanted) -> TR<T> {
        let p = self.metas_zonk_pred(&w.pred);
        Err(super::TcError {
            loc: w.loc,
            msg: format!(
                "Ambiguous type variable arising from a use requiring ({})",
                self.show_pred(&p)
            ),
        })
    }

    /// A given with no Core evidence available yet, used for skolemised signatures.
    pub fn given(&mut self, pred: Pred, ev: Core) {
        self.add_given(Given { pred, ev });
    }
}
