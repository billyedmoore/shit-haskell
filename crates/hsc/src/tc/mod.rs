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
pub mod expr;
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
}

/// The module name of built-in syntax.
pub const BUILTIN: &str = "Builtin";
/// The module whose names the desugarings refer to (`>>=`, `fromInteger`, ...).
pub const PRELUDE_MODULE: &str = "Base";

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
    pub fn lookup_var(&self, name: &str) -> Option<Scheme> {
        self.vars
            .get(name)
            .cloned()
            .or_else(|| self.env.values.get(name).cloned())
    }

    /// Unifies `actual` with `expected`, reporting GHC-style errors.
    pub fn unify_at(&mut self, actual: &Ty, expected: &Ty) -> TR<()> {
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
        Ty::con("Base.Bool")
    }

    /// Registers the built-in type constructors, syntax and their Core wrappers.
    fn install_builtins(&mut self) {
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
        );
        // Unit and tuples.
        // Arity 1 has no tuple; `tuple_con(1)` would clash with unit.
        for n in (0..=12usize).filter(|n| *n != 1) {
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
            );
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

    /// Registers a data constructor, its type and its Core wrapper.
    pub fn add_con(&mut self, name: &str, tycon: &str, tag: usize, scheme: Scheme, fields: Vec<String>) {
        let arity = count_args(&scheme.ty);
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
            },
        );
        let body = if arity == 0 {
            Core::Con(name.to_string(), vec![])
        } else {
            let params: Vec<String> = (0..arity).map(|i| format!("c{i}%")).collect();
            let args: Vec<Core> = params.iter().map(|p| Core::Var(p.clone())).collect();
            let mut core = Core::Con(name.to_string(), args);
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
