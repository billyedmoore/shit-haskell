//! Type inference with dictionary-passing elaboration to Core.
//!
//! The checker works on the renamed AST. Inference is Hindley–Milner with levels for
//! generalisation. Class constraints become *wanted* evidence placeholders, which are
//! solved by givens (from signatures and instance contexts) or by instances, and
//! residual constraints are either quantified over, floated outwards, or defaulted at
//! the top level.

pub mod bind;
pub mod convert;
pub mod decls;
pub mod derive;
pub mod env;
pub mod families;
pub mod expr;
pub mod kinds;
pub mod solve;
pub mod types;

use std::collections::HashMap;

use crate::core::{Alt, Core, TopBind};
use crate::syntax::ast::Loc;
use env::{ClassInfo, DataCon, Env, TyConInfo};
use types::{Metas, Pred, Scheme, Ty, LIST};

#[derive(Debug, Clone)]
pub struct TcError {
    pub loc: Loc,
    pub msg: String,
}

pub type TR<T> = Result<T, TcError>;

/// A constraint whose evidence (a Core expression bound to `ev`) is still needed.
#[derive(Clone, Debug)]
pub struct Wanted {
    pub pred: Pred,
    pub ev: String,
    pub loc: Loc,
}

/// A dictionary available in scope, as a Core expression.
#[derive(Clone, Debug)]
pub struct Given {
    pub pred: Pred,
    pub ev: Core,
}

pub struct Checker {
    /// MonoLocalBinds: local bindings that are not closed are not generalised.
    pub mono_local_binds: bool,
    /// Rigid variables equal to an application that mentions them (a superclass equality
    /// `b ~ (Body b) GhcPs`): where a unification fails, the variable is taken as that
    /// application, which decomposes against an unknown applied (`body0 (GhcPass p0)`).
    pub skol_apps: HashMap<u32, (String, Ty)>,
    /// Local bindings that were generalised (closed ones, MonoLocalBinds): a binding that uses
    /// only these and top-level names is closed too.
    pub closed_locals: std::collections::HashSet<String>,
    /// ApplicativeDo: `do` blocks of independent statements need only Applicative.
    pub applicative_do: bool,
    /// The `Coercible a b` constraints solved so far that still mention unification variables:
    /// a variable left ambiguous is the type opposite it (see `coercible_partner`).
    pub coercions: Vec<(Ty, Ty)>,
    pub env: Env,
    pub metas: Metas,
    pub level: u32,
    /// Schemes of local and top-level variables, by unique or qualified name.
    pub vars: HashMap<String, Scheme>,
    pub wanted: Vec<Wanted>,
    pub givens: Vec<Given>,
    pub counter: u64,
    /// Numbers the instance dictionaries of the module being checked. It starts afresh with each
    /// module, so that a dictionary's name depends on its module only (the module cache keeps
    /// the names other modules' Core refers to).
    pub dict_counter: u64,
    pub module: String,
    pub loc: Loc,
    /// Variables bound by the pattern currently being checked, with their types.
    pub pat_bound: Vec<(String, Ty)>,
    /// Top-level Core bindings produced for the current module.
    pub out: Vec<TopBind>,
    /// Whether the module being checked is user code (not the library). Call sites of
    /// `error` and `undefined` in user code record their location.
    pub user_module: bool,
    /// The source file name, as shown in call stacks.
    pub source_name: String,
    /// The call sites of `HasCallStack` functions in user code: the evidence name of each
    /// `?callStack` wanted, with the called function's name and the call's location.
    pub callsites: HashMap<String, (String, Loc)>,
    /// Checking a `.hs-boot` file: its signatures are the module's values, without bindings.
    pub boot: bool,
    /// Variables bound by nested `forall`s in the type being converted (innermost last).
    pub bound_scope: Vec<(String, u32)>,
    /// The next id for a variable bound by a nested `forall`.
    pub bound_counter: u32,
    /// Kinds of type constructors.
    pub kinds: kinds::KindEnv,
    /// Type variables in scope from enclosing signatures with an explicit `forall`.
    pub scoped: Vec<(String, Ty)>,
    /// The variables a signature uses in kinds (`(a :: k)`, `TYPE r`), by binding key: they stay
    /// flexible when the binding is checked (kinds are not inferred).
    pub sig_kind_vars: HashMap<String, Vec<String>>,
    /// OverloadedStrings is on in the module being checked: string literals are `fromString`.
    pub overloaded_strings: bool,
    /// OverloadedLists: list literals, sequences and list patterns go through `IsList`.
    pub overloaded_lists: bool,
    /// GeneralizedNewtypeDeriving: a newtype derives most classes through its representation.
    pub newtype_deriving: bool,
    /// Quantified constraints are kept while a signature's context is converted.
    pub allow_quantified: bool,
    /// MonomorphismRestriction (on unless the module turns it off).
    pub monomorphism_restriction: bool,
    /// The monads of the typed quotations being checked, innermost last (their splices share them).
    pub quote_monads: Vec<Ty>,
    /// The flexible kind variables of the signature being checked. Each clause of its body
    /// sees them afresh, as it would the rigid kind variables of a signature.
    pub flex_kinds: Vec<u32>,
    /// Bindings whose signature has an explicit `forall` (their variables scope over the body).
    pub explicit_sigs: std::collections::HashSet<String>,
    /// Equalities involving type families that cannot be decided yet.
    pub deferred_eqs: Vec<(Ty, Ty, Loc)>,
}

/// The module name of built-in syntax.
pub const BUILTIN: &str = "Builtin";
/// The module whose names the desugarings refer to (`>>=`, `fromInteger`, ...).
pub const PRELUDE_MODULE: &str = "Base";

/// Modules of the real base layer that define names the checker refers to as `Base.x`.
pub const WIRED_MODULES: &[&str] = &[
    "GHC.Internal.Base",
    "GHC.Internal.Num",
    "GHC.Internal.Real",
    "GHC.Internal.Enum",
    "GHC.Internal.Show",
    "GHC.Internal.Err",
    "GHC.Internal.List",
    "GHC.Internal.Data.Maybe",
    "GHC.Internal.Float",
    "GHC.Internal.Control.Monad.Fail",
    "GHC.Internal.Data.Foldable",
    "GHC.Internal.Data.Traversable",
    "GHC.Internal.Read",
    "GHC.Internal.Exception",
    "GHC.Internal.Stack",
    "GHC.Internal.Unicode",
    "GHC.Internal.Data.OldList",
    "GHC.Internal.Data.Tuple",
    "GHC.Internal.Data.Typeable",
];

impl Checker {
    pub fn new() -> Checker {
        let mut c = Checker {
            mono_local_binds: false,
            skol_apps: HashMap::new(),
            closed_locals: std::collections::HashSet::new(),
            applicative_do: false,
            coercions: Vec::new(),
            env: Env::default(),
            metas: Metas::default(),
            level: 0,
            vars: HashMap::new(),
            wanted: Vec::new(),
            givens: Vec::new(),
            // (Numbered from this compilation's own base: see `core::name_base`.)
            counter: crate::core::name_base() + (1 << 23),
            dict_counter: 0,
            module: BUILTIN.to_string(),
            loc: Loc::default(),
            pat_bound: Vec::new(),
            out: Vec::new(),
            user_module: false,
            boot: false,
            bound_scope: Vec::new(),
            bound_counter: 0,
            source_name: String::new(),
            callsites: HashMap::new(),
            kinds: kinds::KindEnv::default(),
            scoped: Vec::new(),
            sig_kind_vars: HashMap::new(),
            overloaded_strings: false,
            overloaded_lists: false,
            newtype_deriving: false,
            allow_quantified: false,
            monomorphism_restriction: true,
            quote_monads: Vec::new(),
            flex_kinds: Vec::new(),
            explicit_sigs: std::collections::HashSet::new(),
            deferred_eqs: Vec::new(),
        };
        c.install_builtins();
        c
    }

    pub fn fresh_name(&mut self, base: &str) -> String {
        self.counter += 1;
        format!("{base}%{}", self.counter)
    }

    pub fn err<T>(&self, msg: impl Into<String>) -> TR<T> {
        Err(TcError {
            loc: self.loc,
            msg: msg.into(),
        })
    }

    pub fn meta(&mut self) -> Ty {
        self.metas.fresh(self.level)
    }

    pub fn show(&self, t: &Ty) -> String {
        types::show_ty(&self.metas, t)
    }

    pub fn show_pred(&self, p: &Pred) -> String {
        types::show_pred(&self.metas, p)
    }

    /// Looks up the scheme of a variable: locals and this module's top-level bindings
    /// first, then the global environment.
    /// The real defining module of a wired-in name written `Base.x`: the module being checked
    /// (for a definition in progress), or the first library module that defines it.
    pub fn resolve_key(&self, key: &str) -> String {
        if self.vars.contains_key(key) {
            return key.to_string();
        }
        let Some(occ) = key.strip_prefix("Base.") else {
            return key.to_string();
        };
        // A definition of the module being checked (a class method included) comes first, for the
        // GHC library modules that define these names themselves; elsewhere a module's own `foldr`
        // (Data.ByteString's) is not the one desugaring means.
        let own = format!("{}.{occ}", self.module);
        if !self.user_module && self.module.starts_with("GHC.") && (self.vars.contains_key(&own) || self.env.values.contains_key(&own)) {
            return own;
        }
        // The real library's definition is preferred when it is loaded: a name written `Base.x`
        // stands for it, so that its classes are the ones the program's instances are for.
        for m in WIRED_MODULES {
            let candidate = format!("{m}.{occ}");
            if self.env.values.contains_key(&candidate) {
                return candidate;
            }
        }
        key.to_string()
    }

    /// The real defining module of a wired-in type name written `Base.T`.
    pub fn resolve_type(&self, key: &str) -> String {
        if self.env.tycons.contains_key(key) {
            return key.to_string();
        }
        let Some(occ) = key.strip_prefix("Base.") else {
            return key.to_string();
        };
        let own = format!("{}.{occ}", self.module);
        if self.env.tycons.contains_key(&own) {
            return own;
        }
        for m in WIRED_MODULES {
            let candidate = format!("{m}.{occ}");
            if self.env.tycons.contains_key(&candidate) {
                return candidate;
            }
        }
        key.to_string()
    }

    pub fn lookup_var(&self, name: &str) -> Option<Scheme> {
        self.vars
            .get(name)
            .cloned()
            .or_else(|| self.env.values.get(name).cloned())
    }

    /// Unifies `actual` with `expected`, reporting GHC-style errors. Type families are reduced
    /// first; an equality stuck on a family application is deferred.
    pub fn unify_at(&mut self, actual: &Ty, expected: &Ty) -> TR<()> {
        if self.env.families.is_empty() {
            return self.unify_at_raw(actual, expected);
        }
        self.unify_fam(actual, expected)
    }

    /// A type with each rigid variable of `skol_apps` replaced by its application (once).
    pub(super) fn expand_skol_apps(&self, t: &Ty) -> Ty {
        match self.metas.prune(t) {
            Ty::Skol(id, _) => match self.skol_apps.get(&id) {
                Some((_, app)) => app.clone(),
                None => self.metas.prune(t),
            },
            Ty::App(f, x) => Ty::app(self.expand_skol_apps(&f), self.expand_skol_apps(&x)),
            other => other,
        }
    }

    fn saturated_family(&self, t: &Ty) -> bool {
        match self.family_app(t) {
            Some((name, args)) => self.env.families.get(&name).is_some_and(|f| f.arity == args.len()),
            None => false,
        }
    }

    /// Structural unification with families: applications are decomposed, so only the
    /// equalities whose head is a stuck family application are deferred.
    fn unify_fam(&mut self, actual: &Ty, expected: &Ty) -> TR<()> {
        // A data family application against an application of an unknown (`MV s (a, a)` and
        // `v s (a, a)` from a class method): data families are generative, so the unknown is the
        // family partly applied, as GHC solves it, rather than the instance's representation.
        if self.env.families.values().any(|f| !f.equations.is_empty()) {
            let (a0, e0) = (self.metas.zonk(actual), self.metas.zonk(expected));
            if self.align_data_families(&a0, &e0, 0, true)? {
                return Ok(());
            }
        }
        let a = self.normalize(actual);
        let e = self.normalize(expected);
        let a_meta = matches!(self.metas.prune(&a), Ty::Meta(_));
        let e_meta = matches!(self.metas.prune(&e), Ty::Meta(_));
        // `m ~ ST (PrimState m)`: the variable occurs only under a stuck family application, which
        // may reduce once the variable is known (GHC does not report it as an occurs check), so
        // the equality waits.
        for (v, other, v_meta) in [(&a, &e, a_meta), (&e, &a, e_meta)] {
            if let (true, Ty::Meta(i)) = (v_meta, self.metas.prune(v)) {
                if !matches!(self.metas.prune(other), Ty::Meta(_)) && self.has_family(other) {
                    let mut free = Vec::new();
                    self.metas.free_metas(other, &mut free);
                    if free.contains(&i) {
                        self.deferred_eqs.push((a.clone(), e.clone(), self.loc));
                        return Ok(());
                    }
                }
            }
        }
        if a_meta || e_meta || !(self.has_family(&a) || self.has_family(&e)) {
            let r = self.unify_at_raw(&a, &e);
            if r.is_err() && !self.skol_apps.is_empty() {
                let (a2, e2) = (self.expand_skol_apps(&a), self.expand_skol_apps(&e));
                if a2 != a || e2 != e {
                    return self.unify_fam(&a2, &e2);
                }
            }
            return r;
        }
        // A data family is injective: `F t1 .. tn ~ R u1 ..` with `R` the representation of the
        // instance `F p1 .. pn` fixes the arguments to that instance's patterns.
        if let Some(r) = self.data_family_injection(&a, &e).or_else(|| self.data_family_injection(&e, &a)) {
            return r;
        }
        // An injective family equal to a constructor type: the one equation whose right-hand side
        // has that constructor fixes the injective arguments; the equation itself is checked later.
        if let Some(r) = self.injective_improvement(&a, &e).or_else(|| self.injective_improvement(&e, &a)) {
            r?;
            self.deferred_eqs.push((a, e, self.loc));
            return Ok(());
        }
        // `F as ~ F bs` for a data family: the arguments are equal.
        if let (Some((f1, a1)), Some((f2, a2))) = (self.family_app(&a), self.family_app(&e)) {
            if f1 == f2 && a1.len() == a2.len() && self.is_data_family(&f1) {
                for (x, y) in a1.iter().zip(&a2) {
                    self.unify_fam(x, y)?;
                }
                return Ok(());
            }
            // An injective family (`= r | r -> a`): equal results have equal injective arguments.
            let injective = if f1 == f2 && a1.len() == a2.len() {
                self.env.families.get(&f1).map(|f| f.injective.clone()).unwrap_or_default()
            } else {
                Vec::new()
            };
            if !injective.is_empty() {
                for &i in &injective {
                    self.unify_fam(&a1[i], &a2[i])?;
                }
                if injective.len() == a1.len() {
                    return Ok(());
                }
            }
        }
        // A family applied to exactly its arity is stuck; extra arguments decompose as applications.
        if self.saturated_family(&a) || self.saturated_family(&e) {
            self.deferred_eqs.push((a, e, self.loc));
            return Ok(());
        }
        match (self.metas.prune(&a), self.metas.prune(&e)) {
            (Ty::App(f1, x1), Ty::App(f2, x2)) => {
                self.unify_fam(&f1, &f2)?;
                self.unify_fam(&x1, &x2)
            }
            _ => {
                self.deferred_eqs.push((a, e, self.loc));
                Ok(())
            }
        }
    }

    /// Whether a family is a data family: its instances' right-hand sides are representation types.
    /// The data family applications (or their representations) facing an application of an
    /// unknown, anywhere in two types being unified: the unknown is the family partly applied.
    /// Answers whether it unified the two types itself (the caller unifies them otherwise). With
    /// `full` off (a pattern's type, refined rather than unified) only the families are aligned.
    fn align_data_families(&mut self, a: &Ty, b: &Ty, depth: usize, full: bool) -> TR<bool> {
        if depth > 32 {
            return Ok(false);
        }
        let (a, b) = (self.metas.prune(a), self.metas.prune(b));
        for (x, y) in [(&a, &b), (&b, &a)] {
            // A side that reduces to a data instance (`Mutable V r Any` to `MV r Any`'s
            // representation) is taken as that family application.
            let x = match self.rep_as_family(x) {
                Some(f) => f,
                None if self.has_family(x) => {
                    let xn = self.normalize(x);
                    self.rep_as_family(&xn).unwrap_or_else(|| x.clone())
                }
                None => x.clone(),
            };
            let Some((fam, fargs)) = self.family_app(&x) else { continue };
            if !self.is_data_family(&fam) {
                continue;
            }
            let (yh, yargs) = y.split_apps();
            // How many of `y`'s arguments face the family's last ones: all of an unknown's, or
            // those a type family takes beyond its arity (`Mutable v s a`, Mutable having one).
            let k = match self.metas.prune(yh) {
                Ty::Meta(_) => yargs.len(),
                Ty::Con(g) => match self.env.families.get(&g) {
                    Some(info) if info.arity < yargs.len() && !self.is_data_family(&g) => yargs.len() - info.arity,
                    _ => 0,
                },
                _ => 0,
            };
            let n = fargs.len();
            if k == 0 || k > n {
                continue;
            }
            let head = Ty::apps(Ty::Con(fam.clone()), fargs[..n - k].to_vec());
            let yhead = Ty::apps(yh.clone(), yargs[..yargs.len() - k].iter().map(|t| (*t).clone()).collect());
            let ytail: Vec<Ty> = yargs[yargs.len() - k..].iter().map(|t| (*t).clone()).collect();
            self.unify_fam(&yhead, &head)?;
            for (b, f) in ytail.iter().zip(&fargs[n - k..]) {
                self.unify_fam(b, f)?;
            }
            return Ok(true);
        }
        if let (Ty::App(f1, x1), Ty::App(f2, x2)) = (&a, &b) {
            // Where the heads are the same constructor (`ST s _` against `ST s _`), not a family,
            // the arguments are aligned (or unified) pairwise.
            let (h1, n1) = (a.split_apps().0.clone(), a.split_apps().1.len());
            let (h2, n2) = (b.split_apps().0.clone(), b.split_apps().1.len());
            let plain = matches!(&h1, Ty::Con(c) if !self.env.families.contains_key(c));
            if h1 == h2 && n1 == n2 && plain {
                let (f1, x1, f2, x2) = (f1.clone(), x1.clone(), f2.clone(), x2.clone());
                let done_f = self.align_data_families(&f1, &f2, depth + 1, full)?;
                let done_x = self.align_data_families(&x1, &x2, depth + 1, full)?;
                if !full {
                    return Ok(false);
                }
                if !done_f {
                    self.unify_fam(&f1, &f2)?;
                }
                if !done_x {
                    self.unify_fam(&x1, &x2)?;
                }
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// A type with its data instance representations, at the top or inside, as the family
    /// applications they stand for (for matching instance heads).
    pub(super) fn family_form(&self, t: &Ty) -> Ty {
        if let Some(f) = self.rep_as_family(t) {
            return f;
        }
        match t {
            Ty::App(f, a) => Ty::app(self.family_form(f), self.family_form(a)),
            other => other.clone(),
        }
    }

    /// A data instance's representation type applied (`R:MV8_1 s a a`) as the family application
    /// it stands for (`MV s (a, a)`).
    fn rep_as_family(&self, t: &Ty) -> Option<Ty> {
        let (head, args) = t.split_apps();
        let Ty::Con(rep) = head else { return None };
        if !rep.contains("R:") {
            return None;
        }
        for (fam, info) in &self.env.families {
            for q in &info.equations {
                let (rh, rargs) = q.rhs.split_apps();
                if !matches!(rh, Ty::Con(c) if c == rep) || rargs.len() != args.len() {
                    continue;
                }
                let mut sub: Vec<Ty> = (0..q.nvars).map(Ty::Gen).collect();
                for (ra, a) in rargs.iter().zip(&args) {
                    match ra {
                        Ty::Gen(i) if (*i as usize) < sub.len() => sub[*i as usize] = (*a).clone(),
                        _ => return None,
                    }
                }
                let pats: Vec<Ty> = q.args.iter().map(|p| types::subst_gen(p, &sub)).collect();
                return Some(Ty::apps(Ty::Con(fam.clone()), pats));
            }
        }
        None
    }

    fn is_data_family(&self, name: &str) -> bool {
        self.env.families.get(name).is_some_and(|f| {
            !f.equations.is_empty() && f.equations.iter().all(|q| matches!(q.rhs.split_apps().0, Ty::Con(c) if c.contains("R:")))
        })
    }

    /// `fam ~ rep` where `fam` is a saturated data family application and `rep` is headed by the
    /// representation type of one of its instances: the instance's patterns, instantiated, are
    /// unified with the family's arguments and its right-hand side with `rep`.
    /// `F as ~ T ..` for an injective family `F` (TypeFamilyDependencies) and a constructor type:
    /// when one equation's right-hand side is headed by `T`, its patterns at the injective
    /// positions are unified with the arguments.
    fn injective_improvement(&mut self, fam: &Ty, other: &Ty) -> Option<TR<()>> {
        let (name, args) = self.family_app(fam)?;
        let other = self.metas.prune(other);
        let Ty::Con(head) = other.split_apps().0.clone() else { return None };
        if self.env.families.contains_key(&head) {
            return None;
        }
        let info = self.env.families.get(&name)?;
        if info.injective.is_empty() || args.len() != info.arity {
            return None;
        }
        let candidates: Vec<env::FamEqn> =
            info.equations.iter().filter(|q| matches!(q.rhs.split_apps().0, Ty::Con(c) if *c == head)).cloned().collect();
        let [eqn] = candidates.as_slice() else { return None };
        let injective = info.injective.clone();
        let fresh: Vec<Ty> = (0..eqn.nvars).map(|_| self.meta()).collect();
        let rhs = types::subst_gen(&eqn.rhs, &fresh);
        let pats: Vec<Ty> = eqn.args.iter().map(|p| types::subst_gen(p, &fresh)).collect();
        // Already determined: nothing to improve (so that the equation is not deferred forever).
        if injective.iter().all(|&i| !self.has_metas(&args[i])) {
            return None;
        }
        Some((|| {
            // The right-hand side may itself apply families (`XRec (GhcPass p) a = GenLocated (Anno a) a`).
            self.unify_fam(&rhs, &other)?;
            for &i in &injective {
                self.unify_fam(&args[i], &pats[i])?;
            }
            Ok(())
        })())
    }

    pub(super) fn has_metas(&self, t: &Ty) -> bool {
        match self.metas.prune(t) {
            Ty::Meta(_) => true,
            Ty::App(f, x) => self.has_metas(&f) || self.has_metas(&x),
            _ => false,
        }
    }

    fn data_family_injection(&mut self, fam: &Ty, rep: &Ty) -> Option<TR<()>> {
        let (name, args) = self.family_app(fam)?;
        let Ty::Con(rep_head) = self.metas.prune(rep).split_apps().0.clone() else { return None };
        if !rep_head.contains("R:") {
            return None;
        }
        let info = self.env.families.get(&name)?;
        if args.len() != info.arity {
            return None;
        }
        let eqn = info.equations.iter().find(|q| matches!(q.rhs.split_apps().0, Ty::Con(c) if *c == rep_head))?.clone();
        let fresh: Vec<Ty> = (0..eqn.nvars).map(|_| self.meta()).collect();
        let rhs = types::subst_gen(&eqn.rhs, &fresh);
        let pats: Vec<Ty> = eqn.args.iter().map(|p| types::subst_gen(p, &fresh)).collect();
        Some((|| {
            self.unify_at_raw(&rhs, rep)?;
            for (p, a) in pats.iter().zip(&args) {
                self.unify_fam(a, p)?;
            }
            Ok(())
        })())
    }

    fn unify_at_raw(&mut self, actual: &Ty, expected: &Ty) -> TR<()> {
        match self.metas.unify(actual, expected) {
            Ok(()) => Ok(()),
            Err(types::UnifyError::Occurs) => {
                let (a, e) = (self.show(actual), self.show(expected));
                self.err(format!(
                    "Occurs check: cannot construct the infinite type: {a} ~ {e}"
                ))
            }
            Err(types::UnifyError::Mismatch) => {
                let (a, e) = (self.show(actual), self.show(expected));
                self.err(format!(
                    "Couldn't match expected type ‘{e}’ with actual type ‘{a}’"
                ))
            }
        }
    }

    /// The Core type of a list of a given element type.
    pub fn list_ty(&self, t: Ty) -> Ty {
        Ty::app(Ty::con(LIST), t)
    }

    pub fn bool_ty(&self) -> Ty {
        Ty::con("GHC.Types.Bool")
    }

    /// Registers the built-in type constructors, syntax and their Core wrappers.
    fn install_builtin_kinds(&mut self) {
        use kinds::Kind;
        let star = Kind::Star;
        let arr = |a: Kind, b: Kind| Kind::Arr(Box::new(a), Box::new(b));
        self.kinds.tycons.insert("Builtin.()".into(), star.clone());
        self.kinds.tycons.insert(LIST.into(), arr(star.clone(), star.clone()));
        self.kinds
            .tycons
            .insert(types::ARROW.into(), arr(star.clone(), arr(star.clone(), star.clone())));
        for n in (2..=12usize).filter(|n| *n != 1) {
            let mut k = star.clone();
            for _ in 0..n {
                k = arr(star.clone(), k);
            }
            self.kinds.tycons.insert(Ty::tuple_con(n), k);
        }
    }

    fn install_builtins(&mut self) {
        self.install_builtin_kinds();
        let a = Ty::Gen(0);
        // Lists.
        self.env.tycons.insert(
            LIST.into(),
            TyConInfo {
                arity: 1,
                cons: vec!["Builtin.[]".into(), "Builtin.:".into()],
            },
        );
        let list_a = Ty::list(a.clone());
        self.add_con(
            "Builtin.[]",
            LIST,
            0,
            Scheme {
                nvars: 1,
                names: vec!["a".into()],
                preds: vec![],
                ty: list_a.clone(),
            },
            vec![],
            false,
            vec![],
        );
        self.add_con(
            "Builtin.:",
            LIST,
            1,
            Scheme {
                nvars: 1,
                names: vec!["a".into()],
                preds: vec![],
                ty: Ty::funs(vec![a.clone(), list_a.clone()], list_a.clone()),
            },
            vec![],
            false,
            vec![],
        );
        // Unit and tuples.
        // Arity 1 has no tuple; `tuple_con(1)` would clash with unit.
        for n in (0..=15usize).filter(|n| *n != 1) {
            let name = Ty::tuple_con(n);
            let vars: Vec<Ty> = (0..n as u32).map(Ty::Gen).collect();
            let res = Ty::apps(Ty::Con(name.clone()), vars.clone());
            let ty = Ty::funs(vars.clone(), res);
            self.env.tycons.insert(
                name.clone(),
                TyConInfo {
                    arity: n,
                    cons: vec![name.clone()],
                },
            );
            self.add_con(
                &name,
                &name,
                0,
                Scheme {
                    nvars: n as u32,
                    names: (0..n).map(|i| format!("t{i}")).collect(),
                    preds: vec![],
                    ty,
                },
                vec![],
                false,
                vec![],
            );
        }
        // Unboxed sums `(# a | b #)`: one constructor per alternative, each holding one value.
        for n in 2..=6usize {
            let name = format!("Builtin.(#|{n}#)");
            let vars: Vec<Ty> = (0..n as u32).map(Ty::Gen).collect();
            let res = Ty::apps(Ty::Con(name.clone()), vars.clone());
            let cons: Vec<String> = (0..n).map(|i| format!("Builtin.(#_{i}|{n}#)")).collect();
            self.env.tycons.insert(name.clone(), TyConInfo { arity: n, cons: cons.clone() });
            for (i, con) in cons.iter().enumerate() {
                self.add_con(
                    con,
                    &name,
                    i,
                    Scheme {
                        nvars: n as u32,
                        names: (0..n).map(|k| format!("t{k}")).collect(),
                        preds: vec![],
                        ty: Ty::fun(vars[i].clone(), res.clone()),
                    },
                    vec![],
                    false,
                    vec![],
                );
            }
        }
        // The function arrow.
        self.env.tycons.insert(
            types::ARROW.into(),
            TyConInfo {
                arity: 2,
                cons: vec![],
            },
        );
    }

    /// Registers a data constructor, its type and its Core wrapper. The wrapper takes the
    /// constructor's context dictionaries first, then its fields.
    pub fn add_con(
        &mut self,
        name: &str,
        tycon: &str,
        tag: usize,
        scheme: Scheme,
        fields: Vec<String>,
        newtype: bool,
        strict: Vec<bool>,
    ) {
        let arity = count_args(&scheme.ty);
        let ndicts = scheme.preds.len();
        self.env.values.insert(name.to_string(), scheme.clone());
        self.env.datacons.insert(
            name.to_string(),
            DataCon {
                name: name.to_string(),
                tycon: tycon.to_string(),
                tag,
                arity,
                scheme,
                fields,
                newtype,
                strict: strict.clone(),
            },
        );
        let total = ndicts + arity;
        let body = if newtype {
            Core::Lam("nt%".into(), Box::new(Core::Var("nt%".into())))
        } else if total == 0 {
            Core::Con(name.to_string(), vec![])
        } else {
            let params: Vec<String> = (0..total).map(|i| format!("c{i}%")).collect();
            let args: Vec<Core> = params.iter().map(|p| Core::Var(p.clone())).collect();
            let mut core = Core::Con(name.to_string(), args);
            // Strict fields are evaluated before the constructor is built.
            for (i, is_strict) in strict.iter().enumerate().rev() {
                if *is_strict {
                    let b = format!("sb{i}%");
                    core = Core::Case(
                        Box::new(Core::Var(params[ndicts + i].clone())),
                        b,
                        vec![Alt::Default(core)],
                    );
                }
            }
            for p in params.iter().rev() {
                core = Core::Lam(p.clone(), Box::new(core));
            }
            core
        };
        self.out.push(TopBind {
            name: name.to_string(),
            body,
        });
    }

    /// The dictionary constructor and the selectors of a class.
    pub fn add_class_info(&mut self, info: ClassInfo) {
        self.env.classes.insert(info.name.clone(), info);
    }

    pub fn class(&self, name: &str) -> Option<&ClassInfo> {
        self.env.classes.get(name)
    }
}

impl Default for Checker {
    fn default() -> Self {
        Self::new()
    }
}

/// State saved when an alternative (case branch, clause, lambda) starts, so that the
/// givens and refinements it introduces can be scoped to it.
pub struct AltMark {
    givens: usize,
    skol: std::collections::HashMap<u32, Ty>,
    outer: Vec<Wanted>,
    scoped: usize,
}

impl Checker {
    pub fn alt_begin(&mut self) -> AltMark {
        AltMark {
            givens: self.givens.len(),
            skol: self.metas.skol_binds.clone(),
            outer: std::mem::take(&mut self.wanted),
            scoped: self.scoped.len(),
        }
    }

    /// Ends an alternative. Constraints raised in it are solved while its givens are still
    /// in scope; the rest float outwards.
    pub fn alt_end(&mut self, mark: AltMark, body: crate::core::matcher::CBody) -> TR<crate::core::matcher::CBody> {
        let binds = self.alt_end_binds(mark)?;
        Ok(if binds.is_empty() { body } else { crate::core::matcher::CBody::Where(binds, Box::new(body)) })
    }

    /// Ends a scope that introduced nothing: its constraints rejoin the enclosing ones, and type
    /// variables its pattern signatures bound stay in scope (a pattern guard's, for the body).
    pub fn alt_merge(&mut self, mark: AltMark) {
        let inner = std::mem::replace(&mut self.wanted, mark.outer);
        self.wanted.extend(inner);
    }

    /// Whether the scope begun at `mark` has introduced givens or refinements so far.
    pub fn alt_introduced(&self, mark: &AltMark) -> bool {
        self.givens.len() > mark.givens || self.metas.skol_binds.len() != mark.skol.len()
    }

    /// Ends an alternative, answering the evidence bindings of the constraints solved in it (to
    /// be placed where its givens are bound).
    pub fn alt_end_binds(&mut self, mark: AltMark) -> TR<Vec<(String, Core)>> {
        let inner = std::mem::replace(&mut self.wanted, mark.outer);
        let introduced = self.givens.len() > mark.givens || self.metas.skol_binds.len() != mark.skol.len();
        let binds = if introduced {
            let (binds, residual) = self.solve(inner)?;
            self.wanted.extend(residual);
            binds
        } else {
            self.wanted.extend(inner);
            Vec::new()
        };
        self.givens.truncate(mark.givens);
        self.metas.skol_binds = mark.skol;
        // Type variables that a pattern signature bound are in scope in its alternative only.
        self.scoped.truncate(mark.scoped);
        Ok(binds)
    }
}

/// Counts the arrows of a function type (not counting the result).
pub fn count_args(t: &Ty) -> usize {
    let mut n = 0;
    let mut cur = t;
    while let Some((_, r)) = cur.as_fun() {
        n += 1;
        cur = r;
    }
    n
}

/// Helper to build an alternative selecting one field of a dictionary.
pub fn select_field(dict: Core, dict_con: &str, nfields: usize, index: usize, fresh: &mut dyn FnMut() -> String) -> Core {
    let vars: Vec<String> = (0..nfields).map(|_| fresh()).collect();
    let chosen = Core::Var(vars[index].clone());
    Core::Case(
        Box::new(dict),
        fresh(),
        vec![Alt::Con(dict_con.to_string(), vars, chosen)],
    )
}
