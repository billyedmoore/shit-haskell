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

/// The pseudo-class of a quantified constraint (`forall x. ctx => C t`): its one argument is the
/// polymorphic type `Ty::Forall(xs, ctx, C t)`. Its evidence is a function from the dictionaries of
/// `ctx` to the dictionary of `C t`.
pub const QUANTIFIED_CLASS: &str = "%quantified";

/// The name of the placeholder for an instance variable only its context mentions.
const UNBOUND_PREFIX: &str = "%unbound";

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
        let t = self.metas.zonk(&pred.args[0]);
        let (head, args) = t.split_apps();
        let head = head.clone();
        let args: Vec<Ty> = args.into_iter().cloned().collect();
        let mut wanted_rep = |this: &mut Self, ty: Ty, work: &mut Vec<Wanted>| -> Core {
            let ev = this.fresh_name("tya");
            work.push(Wanted { pred: Pred { class: TYPEABLE_CLASS.into(), args: vec![ty] }, ev: ev.clone(), loc });
            this.typeable_rep_of(&ev, &class)
        };
        let rep = match &head {
            // A function type: its own representation.
            Ty::Con(c) if c == super::types::ARROW && args.len() == 2 => {
                let mk = Core::Var("GHC.Internal.Data.Typeable.Internal.mkTrFun".into());
                let many = Core::Var("GHC.Internal.Data.Typeable.Internal.trMany".into());
                let ra = wanted_rep(self, args[0].clone(), work);
                let rb = wanted_rep(self, args[1].clone(), work);
                Core::App(Box::new(Core::App(Box::new(Core::App(Box::new(mk), Box::new(many))), Box::new(ra))), Box::new(rb))
            }
            // An arrow applied to fewer than two arguments: FUN's representation applied to them.
            Ty::Con(c) if c == super::types::ARROW => {
                let mut rep = Core::Var("GHC.Internal.Data.Typeable.Internal.trArrow#".into());
                for a in args {
                    let ra = wanted_rep(self, a, work);
                    let app = Core::Var("GHC.Internal.Data.Typeable.Internal.mkTrApp".into());
                    rep = Core::App(Box::new(Core::App(Box::new(app), Box::new(rep))), Box::new(ra));
                }
                rep
            }
            // A type-level literal (`3`, `"s"`, `'c'`).
            Ty::Con(c) if c.starts_with('#') && args.is_empty() => {
                let text = &c[1..];
                let sort = if text.starts_with('"') { 1 } else if text.starts_with('\'') { 2 } else { 0 };
                let s = Core::Lit(crate::core::Lit::Str(text.to_string()));
                let mk = Core::Var("GHC.Internal.Data.Typeable.Internal.mkTypeableLit#".into());
                Core::App(Box::new(Core::App(Box::new(mk), Box::new(Core::Lit(crate::core::Lit::Int(sort))))), Box::new(s))
            }
            // A constructor applied to arguments: its representation applied to theirs.
            Ty::Con(c) => {
                let (modl, name, arity) = self.typeable_tycon(c);
                // Plain String literals: the module may have OverloadedStrings on.
                let m = Core::Lit(crate::core::Lit::Str(modl));
                let n = Core::Lit(crate::core::Lit::Str(name));
                let k = Core::Lit(crate::core::Lit::Int(arity.max(args.len()) as i64));
                let mk = Core::Var("GHC.Internal.Data.Typeable.Internal.mkTypeableCon#".into());
                let mut rep = Core::App(Box::new(Core::App(Box::new(Core::App(Box::new(mk), Box::new(m))), Box::new(n))), Box::new(k));
                for a in args {
                    let ra = wanted_rep(self, a, work);
                    let app = Core::Var("GHC.Internal.Data.Typeable.Internal.mkTrApp".into());
                    rep = Core::App(Box::new(Core::App(Box::new(app), Box::new(rep))), Box::new(ra));
                }
                rep
            }
            // A variable applied to arguments: the variable's representation (a given) applied.
            _ if !args.is_empty() => {
                let (f, x) = match &t {
                    Ty::App(f, x) => ((**f).clone(), (**x).clone()),
                    _ => return Ok(None),
                };
                let rf = wanted_rep(self, f, work);
                let rx = wanted_rep(self, x, work);
                let app = Core::Var("GHC.Internal.Data.Typeable.Internal.mkTrApp".into());
                Core::App(Box::new(Core::App(Box::new(app), Box::new(rf))), Box::new(rx))
            }
            _ => return Ok(None),
        };
        Ok(Some(Core::Con(class, vec![rep])))
    }

    /// The module, name and number of parameters of a type constructor, as its `TyCon` has them
    /// (the built-in syntax named as GHC names it).
    fn typeable_tycon(&self, c: &str) -> (String, String, usize) {
        if c == super::types::LIST {
            return ("GHC.Types".into(), "List".into(), 1);
        }
        if let Some(inner) = c.strip_prefix("Builtin.(").and_then(|r| r.strip_suffix(')')) {
            if inner.is_empty() {
                return ("GHC.Tuple".into(), "Unit".into(), 0);
            }
            if inner.chars().all(|ch| ch == ',') {
                let n = inner.len() + 1;
                return ("GHC.Tuple".into(), format!("Tuple{n}"), n);
            }
        }
        let arity = self.env.tycons.get(c).map_or(0, |i| i.arity);
        match c.rfind('.') {
            Some(i) if i > 0 && i + 1 < c.len() => (c[..i].to_string(), c[i + 1..].to_string(), arity),
            _ => ("Builtin".to_string(), c.to_string(), arity),
        }
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
    pub fn add_given(&mut self, mut g: Given) {
        // A constraint variable known to be a class (`c b` in an instance for `c := ParsecPretty`)
        // is given as that class, so that its superclasses are given too.
        if g.pred.class == super::convert::CONSTRAINT_VAR {
            g.pred = self.metas_zonk_pred(&g.pred);
            // A tuple of constraints (`Dict (Show a, Eq a)` matched) gives each of them: its
            // evidence is the tuple of their dictionaries.
            if let Some(parts) = constraint_tuple_parts(&g.pred) {
                let n = parts.len();
                for (i, part) in parts.into_iter().enumerate() {
                    let ev = super::select_field(g.ev.clone(), &Ty::tuple_con(n), n, i, &mut || self.fresh_name("ct"));
                    self.add_given(Given { pred: part, ev });
                }
                return;
            }
        }
        if is_eq_class(&g.pred.class) && g.pred.args.len() == 2 {
            let (a, b) = (g.pred.args[0].clone(), g.pred.args[1].clone());
            let _ = self.metas.unify_refine(&a, &b);
        }
        let family = self.env.families.contains_key(&g.pred.class);
        self.givens.push(g);
        if family {
            self.refine_family_givens();
        }
    }

    /// Constraint-family givens that now reduce (`If (s == System) (a ~ B, ..) ()` once a GADT
    /// match has refined `s`): the equalities they reduce to refine the types in scope.
    pub fn refine_family_givens(&mut self) {
        let fam_givens: Vec<Pred> = self
            .givens
            .iter()
            .filter(|g| self.env.families.contains_key(&g.pred.class))
            .map(|g| g.pred.clone())
            .collect();
        for p in fam_givens {
            let applied = Ty::apps(Ty::con(&p.class), p.args.iter().map(|t| self.metas.zonk(t)).collect());
            let reduced = self.normalize(&applied);
            if reduced == applied {
                continue;
            }
            for part in super::convert::ty_to_preds(&reduced) {
                if is_eq_class(&part.class) && part.args.len() == 2 {
                    let _ = self.metas.unify_refine(&part.args[0], &part.args[1]);
                }
            }
        }
    }

    /// Solves what can be solved. Returns evidence bindings and the residual wanteds
    /// that still mention unsolved unification variables.
    pub fn solve(&mut self, wanteds: Vec<Wanted>) -> TR<(Vec<(String, Core)>, Vec<Wanted>)> {
        self.retry_deferred()?;
        let mut binds = Vec::new();
        let mut pending = wanteds;
        // Constraints set aside on unknown types are tried again when solving others (a fundep
        // improvement, an equality) has made their types known.
        for _round in 0..64 {
            let before: Vec<Pred> = pending.iter().map(|w| self.metas_zonk_pred(&w.pred)).collect();
            let (b, residual) = self.solve_round(std::mem::take(&mut pending))?;
            binds.extend(b);
            let progressed = residual.len() != before.len()
                || residual.iter().zip(&before).any(|(w, p)| self.metas_zonk_pred(&w.pred) != *p);
            pending = residual;
            if pending.is_empty() || !progressed {
                break;
            }
        }
        Ok((binds, pending))
    }

    /// One pass of the solver over a list of wanteds.
    fn solve_round(&mut self, wanteds: Vec<Wanted>) -> TR<(Vec<(String, Core)>, Vec<Wanted>)> {
        let mut work = wanteds;
        let mut binds = Vec::new();
        let mut residual = Vec::new();
        while let Some(w) = work.pop() {
            self.improve(&w.pred)?;
            let pred = self.metas_zonk_pred(&w.pred);
            // A wanted tuple of constraints: each is wanted, the evidence is their tuple.
            if let Some(parts) = constraint_tuple_parts(&pred) {
                let n = parts.len();
                let mut evs = Vec::with_capacity(n);
                for part in parts {
                    let ev = self.fresh_name("ctw");
                    evs.push(Core::Var(ev.clone()));
                    work.push(Wanted { pred: part, ev, loc: w.loc });
                }
                binds.push((w.ev.clone(), Core::Con(Ty::tuple_con(n), evs)));
                continue;
            }
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
            // A wanted quantified constraint: its head is solved under rigid variables, with its
            // context as givens.
            if pred.class == QUANTIFIED_CLASS {
                // A head applying a constraint variable not yet known (`forall a. c (T a)` with `c`
                // decided later by a functional dependency) waits.
                if let Ty::Forall(_, _, head) = self.metas.zonk(&pred.args[0]) {
                    if matches!(head.split_apps().0, Ty::Meta(_)) {
                        residual.push(Wanted { pred, ..w });
                        continue;
                    }
                }
                let ev = self.solve_quantified(&pred, w.loc)?;
                binds.push((w.ev.clone(), ev));
                continue;
            }
            if let Some(ev) = self.find_given(&pred) {
                binds.push((w.ev.clone(), ev));
                continue;
            }
            // KnownNat, KnownSymbol and KnownChar at a literal type: the compiler builds the dictionary.
            if let Some(ev) = self.known_literal_evidence(&pred) {
                binds.push((w.ev.clone(), ev));
                continue;
            }
            // A quantified given whose head (or a superclass of it) matches: a local instance.
            if let Some((ev, ctx)) = self.find_quantified_given(&pred) {
                let mut core = ev;
                let mut evs = Vec::new();
                for p in ctx {
                    let e = self.fresh_name("qd");
                    evs.push(e.clone());
                    work.push(Wanted { pred: p, ev: e, loc: w.loc });
                }
                // The context's dictionaries go to the given; the superclass selection follows.
                core = core_apply_base(core, &evs);
                binds.push((w.ev.clone(), core));
                continue;
            }
            match self.find_instance(&pred) {
                Some((inst_dict, ctx)) => {
                    let ctx = self.instantiate_unbound(ctx);
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
            // A given whose determining arguments are the wanted's fixes the others (`MonadReader r m`
            // given and `MonadReader ?r m` wanted: `?r := r`).
            // The givens' superclasses count too (`RegexOptions r c e` of a given `RegexMaker r c e s`).
            let zonked: Vec<Ty> = p.args.iter().map(|t| self.metas.zonk(t)).collect();
            let mut available: Vec<Pred> = Vec::new();
            for g in self.givens.iter().rev() {
                self.superclass_closure(&g.pred, &mut available, 0);
            }
            let given_match: Option<Vec<Ty>> = available.iter().find_map(|g| {
                if g.class != p.class || g.args.len() != zonked.len() {
                    return None;
                }
                let gz: Vec<Ty> = g.args.iter().map(|t| self.metas.zonk(t)).collect();
                from.iter().all(|&i| gz[i] == zonked[i]).then_some(gz)
            });
            if let Some(gz) = given_match {
                for &j in &to {
                    self.unify_at(&p.args[j], &gz[j])?;
                }
                continue;
            }
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

    /// A predicate and its superclasses, transitively (instantiated at its arguments).
    pub(super) fn superclass_closure(&self, p: &Pred, out: &mut Vec<Pred>, depth: usize) {
        if depth > 20 || out.contains(p) {
            return;
        }
        out.push(p.clone());
        if let Some(info) = self.class(&p.class) {
            for sup in info.supers.clone() {
                let q = super::types::subst_gen_pred(&sup, &p.args);
                self.superclass_closure(&q, out, depth + 1);
            }
        }
    }

    /// A predicate with solved variables substituted and type families reduced.
    pub fn metas_zonk_pred(&self, p: &Pred) -> Pred {
        let out = if self.env.families.is_empty() {
            self.metas.zonk_pred(p)
        } else {
            Pred { class: p.class.clone(), args: p.args.iter().map(|t| self.normalize(t)).collect() }
        };
        // A constraint variable solved to a class (`c a` with `c := Pretty`) is that class's
        // predicate.
        if out.class == super::convert::CONSTRAINT_VAR && out.args.len() == 1 {
            let (head, cargs) = out.args[0].split_apps();
            if let Ty::Con(c) = head {
                if self.env.classes.contains_key(c) {
                    return Pred { class: c.clone(), args: cargs.into_iter().cloned().collect() };
                }
            }
        }
        out
    }

    pub fn pred_has_metas(&self, p: &Pred) -> bool {
        let mut out = Vec::new();
        for a in &p.args {
            self.metas.free_metas(a, &mut out);
        }
        !out.is_empty()
    }

    /// Solves a wanted quantified constraint: the evidence is `\ctx-dicts -> evidence of the head`,
    /// the head solved with fresh rigid variables and the context as givens.
    fn solve_quantified(&mut self, pred: &Pred, loc: crate::syntax::ast::Loc) -> TR<Core> {
        let Ty::Forall(ids, ctx, head) = self.metas.zonk(&pred.args[0]) else {
            return self.err("internal: malformed quantified constraint");
        };
        let sk: Vec<Ty> = ids.iter().map(|_| self.metas.fresh_skolem("q")).collect();
        let ctx: Vec<Pred> = ctx.iter().map(|p| super::types::subst_bound_pred(p, &ids, &sk)).collect();
        let head = super::types::subst_bound(&head, &ids, &sk);
        let (h, hargs) = head.split_apps();
        let head_pred = match h {
            Ty::Con(class) => Pred { class: class.clone(), args: hargs.into_iter().cloned().collect() },
            // `forall a. c (T a)` with `c` a constraint variable.
            _ => self.metas_zonk_pred(&Pred { class: super::convert::CONSTRAINT_VAR.into(), args: vec![head.clone()] }),
        };
        let params: Vec<String> = ctx.iter().map(|_| self.fresh_name("qg")).collect();
        let mark = self.givens.len();
        for (p, d) in ctx.iter().zip(&params) {
            self.add_given(Given { pred: p.clone(), ev: Core::Var(d.clone()) });
        }
        let ev = self.fresh_name("qh");
        let r = self.solve(vec![Wanted { pred: head_pred.clone(), ev: ev.clone(), loc }]);
        self.givens.truncate(mark);
        let (binds, residual) = r?;
        if !residual.is_empty() {
            let msg = format!("Could not deduce ({}) for a quantified constraint", self.show_pred(&head_pred));
            return Err(super::TcError { loc, msg });
        }
        let body = Core::Let(binds, Box::new(Core::Var(ev)));
        Ok(params.into_iter().rev().fold(body, |acc, d| Core::Lam(d, Box::new(acc))))
    }

    /// A quantified given that provides a predicate: the given's evidence with the superclass
    /// selections to apply after its context's dictionaries (see `core_apply_base`), and that
    /// context instantiated for the match.
    fn find_quantified_given(&self, want: &Pred) -> Option<(Core, Vec<Pred>)> {
        for g in self.givens.iter().rev() {
            if g.pred.class != QUANTIFIED_CLASS {
                continue;
            }
            let Ty::Forall(ids, ctx, head) = self.metas.zonk(&g.pred.args[0]) else { continue };
            let gens: Vec<Ty> = (0..ids.len()).map(|k| Ty::Gen(k as u32)).collect();
            let head = super::types::subst_bound(&head, &ids, &gens);
            let (h, hargs) = head.split_apps();
            // A head applying a constraint variable (`forall a. c (T a)`) is a `%constraint` one.
            let have = match h {
                Ty::Con(class) => Pred { class: class.clone(), args: hargs.into_iter().cloned().collect() },
                _ => Pred { class: super::convert::CONSTRAINT_VAR.into(), args: vec![head.clone()] },
            };
            let mut path: Vec<(String, usize, usize)> = Vec::new();
            if let Some(sub) = self.quantified_match(&have, want, ids.len(), &mut path, 0) {
                let inst: Vec<Ty> = sub.into_iter().map(|s| s.unwrap_or(Ty::con("Builtin.()"))).collect();
                let ctx: Vec<Pred> = ctx
                    .iter()
                    .map(|p| super::types::subst_gen_pred(&super::types::subst_bound_pred(p, &ids, &gens), &inst))
                    .collect();
                // The evidence: the given applied to the context (done by the caller), then the
                // superclass selections of the path, outermost first.
                let mut core = Core::Var(QUANTIFIED_BASE.into());
                let mut counter = 0usize;
                for (con, n, i) in path {
                    let mut fresh = || {
                        counter += 1;
                        format!("qs%{counter}")
                    };
                    core = select_field(core, &con, n, i, &mut fresh);
                }
                return Some((core_with_base(core, g.ev.clone()), ctx));
            }
        }
        None
    }

    /// Matches a pattern predicate (its quantified variables are `Gen`s) or one of its superclasses
    /// against a wanted one; records the superclass path taken.
    fn quantified_match(&self, have: &Pred, want: &Pred, nvars: usize, path: &mut Vec<(String, usize, usize)>, depth: usize) -> Option<Vec<Option<Ty>>> {
        if have.class == want.class && have.args.len() == want.args.len() {
            let mut sub: Vec<Option<Ty>> = vec![None; nvars];
            if have.args.iter().zip(&want.args).all(|(p, t)| self.match_ty(p, &self.metas.zonk(t), &mut sub)) {
                return Some(sub);
            }
        }
        if depth > 8 {
            return None;
        }
        let info = self.class(&have.class)?;
        let n = info.nsupers() + info.methods.len();
        for (i, sup) in info.supers.iter().enumerate() {
            let sup_pred = super::types::subst_gen_pred(sup, &have.args);
            path.push((info.dict_con.clone(), n, i));
            if let Some(sub) = self.quantified_match(&sup_pred, want, nvars, path, depth + 1) {
                return Some(sub);
            }
            path.pop();
        }
        None
    }

    /// The dictionary of `KnownNat 5`, `KnownSymbol "x"` or `KnownChar 'c'`: the class's one method
    /// is the literal's value (`SNat`, `SSymbol` and `SChar` are newtypes of it).
    fn known_literal_evidence(&self, pred: &Pred) -> Option<Core> {
        if pred.args.len() != 1 {
            return None;
        }
        let Ty::Con(lit) = self.metas.zonk(&pred.args[0]) else { return None };
        let text = lit.strip_prefix('#')?;
        let value = match pred.class.as_str() {
            "GHC.Internal.TypeNats.KnownNat" if text.chars().all(|c| c.is_ascii_digit()) => Core::App(
                Box::new(Core::Var("GHC.Num.Integer.integerToNatural".into())),
                Box::new(Core::Lit(crate::core::Lit::Integer(text.to_string()))),
            ),
            "GHC.Internal.TypeLits.KnownSymbol" if text.starts_with('"') => {
                let s: String = text.trim_matches('"').replace("\\\"", "\"").replace("\\\\", "\\");
                Core::Lit(crate::core::Lit::Str(s))
            }
            "GHC.Internal.TypeLits.KnownChar" if text.starts_with('\'') => {
                Core::Lit(crate::core::Lit::Char(text.trim_matches('\'').chars().next()?))
            }
            _ => return None,
        };
        let dict_con = self.class(&pred.class)?.dict_con.clone();
        Some(Core::Con(dict_con, vec![value]))
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
            let sup_pred = super::types::subst_gen_pred(sup, &have.args);
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

    /// The placeholders `find_instance` leaves for an instance's context-only variables, replaced by
    /// fresh unification variables (one per variable).
    fn instantiate_unbound(&mut self, ctx: Vec<Pred>) -> Vec<Pred> {
        fn has(t: &Ty) -> bool {
            match t {
                Ty::Con(c) => c.starts_with(UNBOUND_PREFIX),
                Ty::App(f, x) => has(f) || has(x),
                _ => false,
            }
        }
        if !ctx.iter().any(|p| p.args.iter().any(has)) {
            return ctx;
        }
        let mut fresh: std::collections::HashMap<String, Ty> = std::collections::HashMap::new();
        fn replace(t: &Ty, fresh: &mut std::collections::HashMap<String, Ty>, meta: &mut dyn FnMut() -> Ty) -> Ty {
            match t {
                Ty::Con(c) if c.starts_with(UNBOUND_PREFIX) => fresh.entry(c.clone()).or_insert_with(meta).clone(),
                Ty::App(f, x) => Ty::app(replace(f, fresh, meta), replace(x, fresh, meta)),
                other => other.clone(),
            }
        }
        ctx.into_iter()
            .map(|p| Pred {
                class: p.class,
                args: p.args.iter().map(|a| replace(a, &mut fresh, &mut || self.meta())).collect(),
            })
            .collect()
    }

    /// Finds an instance for a predicate. Returns the dictionary function name and the
    /// instance context instantiated for this predicate.
    pub fn find_instance(&self, want: &Pred) -> Option<(String, Vec<Pred>)> {
        // Every matching instance; the most specific is chosen (an instance whose head another one's
        // head matches is more specific), as GHC chooses among overlapping instances. Kinds are not
        // tracked, so this also tells apart instances GHC separates by kind (`HasResolution E12` and
        // `KnownNat n => HasResolution n`).
        let mut matches: Vec<(&InstInfo, Vec<Option<Ty>>)> = Vec::new();
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
                matches.push((inst, sub));
            }
        }
        let rigid = |t: &Ty| super::types::subst_gen(t, &(0..64).map(|i| Ty::Con(format!("%rigid{i}"))).collect::<Vec<_>>());
        let more_general = |a: &InstInfo, b: &InstInfo| -> bool {
            // a's head matches b's (with b's variables rigid): a is at least as general as b.
            let mut sub: Vec<Option<Ty>> = vec![None; a.nvars as usize];
            a.head.iter().zip(&b.head).all(|(pa, pb)| self.match_ty(pa, &rigid(pb), &mut sub))
        };
        let best = (0..matches.len()).find(|&i| (0..matches.len()).all(|j| j == i || !more_general(matches[i].0, matches[j].0) || more_general(matches[j].0, matches[i].0)));
        if let Some(i) = best.or(if matches.is_empty() { None } else { Some(0) }) {
            let (inst, sub) = matches.swap_remove(i);
            {
                // A variable only the context mentions (`coh` of `(coh ~ F o) => C o`) is a
                // placeholder here; the solver gives it a fresh unification variable.
                let args: Vec<Ty> = sub
                    .into_iter()
                    .enumerate()
                    .map(|(i, s)| s.unwrap_or_else(|| Ty::Con(format!("{UNBOUND_PREFIX}{i}"))))
                    .collect();
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
            // A rigid variable (in a quantified given's head) matches only itself.
            (Ty::Skol(..), _) => pat == t,
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
                    // Typeable holds at every candidate (its evidence is built, not declared).
                    let ok = preds_on_m.iter().all(|p| {
                        if p.class == TYPEABLE_CLASS {
                            return true;
                        }
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

/// The placeholder for a quantified given's evidence applied to its context, inside the superclass
/// selections built by `find_quantified_given`.
const QUANTIFIED_BASE: &str = "%qbase";

/// The selections over the placeholder, with the given's evidence recorded beside them.
fn core_with_base(sel: Core, given: Core) -> Core {
    Core::Let(vec![(QUANTIFIED_BASE.into(), given)], Box::new(sel))
}

/// Applies the given's evidence (recorded by `core_with_base`) to the context dictionaries.
fn core_apply_base(core: Core, ctx_evs: &[String]) -> Core {
    match core {
        Core::Let(mut bs, body) if bs.len() == 1 && bs[0].0 == QUANTIFIED_BASE => {
            let (name, given) = bs.remove(0);
            let applied = ctx_evs.iter().fold(given, |f, e| Core::App(Box::new(f), Box::new(Core::Var(e.clone()))));
            Core::Let(vec![(name, applied)], body)
        }
        other => other,
    }
}

/// The parts of a constraint that is a tuple of constraints (`%constraint ((Show a, Eq a))`, or
/// `()`), each as a predicate.
fn constraint_tuple_parts(p: &Pred) -> Option<Vec<Pred>> {
    if p.class != super::convert::CONSTRAINT_VAR || p.args.len() != 1 {
        return None;
    }
    let (head, args) = p.args[0].split_apps();
    let Ty::Con(c) = head else { return None };
    let n = if c == "Builtin.()" { 0 } else { super::types::tuple_arity(c)? };
    if n != args.len() {
        return None;
    }
    Some(
        args.into_iter()
            .map(|a| match a.split_apps() {
                (Ty::Con(cls), cargs) if super::types::tuple_arity(cls).is_none() && cls != "Builtin.()" => {
                    Pred { class: cls.clone(), args: cargs.into_iter().cloned().collect() }
                }
                _ => Pred { class: super::convert::CONSTRAINT_VAR.into(), args: vec![a.clone()] },
            })
            .collect(),
    )
}
