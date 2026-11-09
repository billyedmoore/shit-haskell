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
    pub env: Env,
    pub metas: Metas,
    pub level: u32,
    /// Schemes of local and top-level variables, by unique or qualified name.
    pub vars: HashMap<String, Scheme>,
    pub wanted: Vec<Wanted>,
    pub givens: Vec<Given>,
    pub counter: u64,
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
            env: Env::default(),
            metas: Metas::default(),
            level: 0,
            vars: HashMap::new(),
            wanted: Vec::new(),
            givens: Vec::new(),
            counter: 0,
            module: BUILTIN.to_string(),
            loc: Loc::default(),
            pat_bound: Vec::new(),
            out: Vec::new(),
            user_module: false,
            boot: false,
            bound_scope: Vec::new(),
            bound_counter: 0,
            source_name: String::new(),
            kinds: kinds::KindEnv::default(),
            scoped: Vec::new(),
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
        if self.vars.contains_key(key) || self.env.values.contains_key(key) {
            return key.to_string();
        }
        let Some(occ) = key.strip_prefix("Base.") else {
            return key.to_string();
        };
        let own = format!("{}.{occ}", self.module);
        if self.vars.contains_key(&own) {
            return own;
        }
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

    /// Structural unification with families: applications are decomposed, so only the
    /// equalities whose head is a stuck family application are deferred.
    fn unify_fam(&mut self, actual: &Ty, expected: &Ty) -> TR<()> {
        let a = self.normalize(actual);
        let e = self.normalize(expected);
        let a_meta = matches!(self.metas.prune(&a), Ty::Meta(_));
        let e_meta = matches!(self.metas.prune(&e), Ty::Meta(_));
        if a_meta || e_meta || !(self.has_family(&a) || self.has_family(&e)) {
            return self.unify_at_raw(&a, &e);
        }
        if self.family_app(&a).is_some() || self.family_app(&e).is_some() {
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
}

impl Checker {
    pub fn alt_begin(&mut self) -> AltMark {
        AltMark {
            givens: self.givens.len(),
            skol: self.metas.skol_binds.clone(),
            outer: std::mem::take(&mut self.wanted),
        }
    }

    /// Ends an alternative. Constraints raised in it are solved while its givens are still
    /// in scope; the rest float outwards.
    pub fn alt_end(&mut self, mark: AltMark, body: crate::core::matcher::CBody) -> TR<crate::core::matcher::CBody> {
        let inner = std::mem::replace(&mut self.wanted, mark.outer);
        let introduced = self.givens.len() > mark.givens || self.metas.skol_binds.len() != mark.skol.len();
        let body = if introduced {
            let (binds, residual) = self.solve(inner)?;
            self.wanted.extend(residual);
            if binds.is_empty() {
                body
            } else {
                crate::core::matcher::CBody::Where(binds, Box::new(body))
            }
        } else {
            self.wanted.extend(inner);
            body
        };
        self.givens.truncate(mark.givens);
        self.metas.skol_binds = mark.skol;
        Ok(body)
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
