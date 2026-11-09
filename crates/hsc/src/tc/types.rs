//! Types, predicates, schemes, and unification with levels for generalisation.

use std::collections::HashMap;
use std::fmt;

pub const ARROW: &str = "Builtin.->";
pub const LIST: &str = "Builtin.[]";

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Ty {
    /// A type constructor by global name.
    Con(String),
    App(Box<Ty>, Box<Ty>),
    /// A unification variable.
    Meta(u32),
    /// A rigid type variable from a signature; the id makes it unique.
    Skol(u32, String),
    /// A quantified variable of a scheme, by index.
    Gen(u32),
    /// A constrained type in argument position, `C a => t` (rank-N): a value of it is a
    /// function of the dictionaries of its predicates.
    Qual(Vec<Pred>, Box<Ty>),
    /// A variable bound by a nested `forall` (rank-N), by a unique id.
    Bound(u32),
    /// A nested `forall ids. preds => t`: a polymorphic type, instantiated where it is used
    /// and skolemised where it is checked against.
    Forall(Vec<u32>, Vec<Pred>, Box<Ty>),
}

impl Ty {
    pub fn con(name: &str) -> Ty {
        Ty::Con(name.to_string())
    }

    pub fn app(f: Ty, a: Ty) -> Ty {
        Ty::App(Box::new(f), Box::new(a))
    }

    pub fn apps(head: Ty, args: Vec<Ty>) -> Ty {
        args.into_iter().fold(head, Ty::app)
    }

    pub fn fun(a: Ty, b: Ty) -> Ty {
        Ty::app(Ty::app(Ty::con(ARROW), a), b)
    }

    pub fn funs(args: Vec<Ty>, res: Ty) -> Ty {
        args.into_iter().rev().fold(res, |acc, a| Ty::fun(a, acc))
    }

    pub fn list(a: Ty) -> Ty {
        Ty::app(Ty::con(LIST), a)
    }

    /// The constructor of an `n`-tuple: unit for 0, `(,)` for 2, and so on.
    pub fn tuple_con(n: usize) -> String {
        if n == 0 {
            "Builtin.()".to_string()
        } else if n == 1 {
            "Builtin.(1)".to_string()
        } else {
            format!("Builtin.({})", ",".repeat(n - 1))
        }
    }

    pub fn tuple(elems: Vec<Ty>) -> Ty {
        let n = elems.len();
        Ty::apps(Ty::Con(Ty::tuple_con(n)), elems)
    }

    /// Splits `a -> b`.
    pub fn as_fun(&self) -> Option<(&Ty, &Ty)> {
        if let Ty::App(f, b) = self {
            if let Ty::App(arrow, a) = f.as_ref() {
                if **arrow == Ty::con(ARROW) {
                    return Some((a, b));
                }
            }
        }
        None
    }

    /// Head and arguments of a type application chain.
    pub fn split_apps(&self) -> (&Ty, Vec<&Ty>) {
        let mut args = Vec::new();
        let mut t = self;
        while let Ty::App(f, a) = t {
            args.push(a.as_ref());
            t = f;
        }
        args.reverse();
        (t, args)
    }
}

#[derive(Clone, Debug, PartialEq)]
#[derive(Eq, Hash)]
pub struct Pred {
    pub class: String,
    pub args: Vec<Ty>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Scheme {
    pub nvars: u32,
    /// Names of the quantified variables, for messages.
    pub names: Vec<String>,
    pub preds: Vec<Pred>,
    pub ty: Ty,
}

impl Scheme {
    pub fn mono(ty: Ty) -> Scheme {
        Scheme {
            nvars: 0,
            names: Vec::new(),
            preds: Vec::new(),
            ty,
        }
    }
}

/// Substitutes `Gen(i)` by `args[i]` throughout a type.
pub fn subst_gen(t: &Ty, args: &[Ty]) -> Ty {
    match t {
        Ty::Gen(i) => args.get(*i as usize).cloned().unwrap_or(Ty::Gen(*i)),
        Ty::App(f, a) => Ty::app(subst_gen(f, args), subst_gen(a, args)),
        Ty::Qual(ps, b) => Ty::Qual(ps.iter().map(|p| subst_gen_pred(p, args)).collect(), Box::new(subst_gen(b, args))),
        Ty::Forall(ids, ps, b) => Ty::Forall(
            ids.clone(),
            ps.iter().map(|p| subst_gen_pred(p, args)).collect(),
            Box::new(subst_gen(b, args)),
        ),
        other => other.clone(),
    }
}

/// Replaces the bound variables `ids` of a nested `forall` by `tys`.
pub fn subst_bound(t: &Ty, ids: &[u32], tys: &[Ty]) -> Ty {
    match t {
        Ty::Bound(i) => match ids.iter().position(|x| x == i) {
            Some(k) => tys[k].clone(),
            None => t.clone(),
        },
        Ty::App(f, a) => Ty::app(subst_bound(f, ids, tys), subst_bound(a, ids, tys)),
        Ty::Qual(ps, b) => Ty::Qual(ps.iter().map(|p| subst_bound_pred(p, ids, tys)).collect(), Box::new(subst_bound(b, ids, tys))),
        Ty::Forall(is, ps, b) => Ty::Forall(
            is.clone(),
            ps.iter().map(|p| subst_bound_pred(p, ids, tys)).collect(),
            Box::new(subst_bound(b, ids, tys)),
        ),
        other => other.clone(),
    }
}

pub fn subst_bound_pred(p: &Pred, ids: &[u32], tys: &[Ty]) -> Pred {
    Pred {
        class: p.class.clone(),
        args: p.args.iter().map(|a| subst_bound(a, ids, tys)).collect(),
    }
}

pub fn subst_gen_pred(p: &Pred, args: &[Ty]) -> Pred {
    let out = Pred {
        class: p.class.clone(),
        args: p.args.iter().map(|t| subst_gen(t, args)).collect(),
    };
    // A constraint variable instantiated with a class application is that class's predicate.
    if out.class == super::convert::CONSTRAINT_VAR && out.args.len() == 1 {
        let (head, cargs) = out.args[0].split_apps();
        if let Ty::Con(c) = head {
            return Pred { class: c.clone(), args: cargs.into_iter().cloned().collect() };
        }
    }
    out
}

/// The unification variables of the checker, with their levels. Rigid variables can
/// be bound locally (`skol_binds`) while checking a GADT alternative, which refines them.
#[derive(Default)]
pub struct Metas {
    pub subst: Vec<Option<Ty>>,
    pub levels: Vec<u32>,
    pub skolems: u32,
    pub skol_binds: HashMap<u32, Ty>,
    /// The global names of the type families in scope (see `unify_refine`).
    pub families: std::collections::HashSet<String>,
}

#[derive(Debug, PartialEq)]
pub enum UnifyError {
    Mismatch,
    Occurs,
}

impl Metas {
    pub fn fresh(&mut self, level: u32) -> Ty {
        self.subst.push(None);
        self.levels.push(level);
        Ty::Meta(self.subst.len() as u32 - 1)
    }

    pub fn fresh_skolem(&mut self, name: &str) -> Ty {
        self.skolems += 1;
        Ty::Skol(self.skolems, name.to_string())
    }

    /// Resolves the outermost meta chain of a type (and locally refined rigid variables).
    pub fn prune(&self, t: &Ty) -> Ty {
        let mut cur = t.clone();
        loop {
            match &cur {
                Ty::Meta(i) => match &self.subst[*i as usize] {
                    Some(next) => cur = next.clone(),
                    None => break,
                },
                Ty::Skol(id, _) => match self.skol_binds.get(id) {
                    Some(next) => cur = next.clone(),
                    None => break,
                },
                _ => break,
            }
        }
        cur
    }

    /// Fully substitutes all solved metas and refined rigid variables.
    pub fn zonk(&self, t: &Ty) -> Ty {
        match t {
            Ty::Meta(i) => match &self.subst[*i as usize] {
                Some(s) => self.zonk(s),
                None => t.clone(),
            },
            Ty::Skol(id, _) => match self.skol_binds.get(id) {
                Some(s) => self.zonk(s),
                None => t.clone(),
            },
            Ty::App(f, a) => Ty::app(self.zonk(f), self.zonk(a)),
            Ty::Qual(ps, b) => Ty::Qual(ps.iter().map(|p| self.zonk_pred(p)).collect(), Box::new(self.zonk(b))),
            Ty::Forall(ids, ps, b) => Ty::Forall(
                ids.clone(),
                ps.iter().map(|p| self.zonk_pred(p)).collect(),
                Box::new(self.zonk(b)),
            ),
            other => other.clone(),
        }
    }

    pub fn zonk_pred(&self, p: &Pred) -> Pred {
        Pred {
            class: p.class.clone(),
            args: p.args.iter().map(|t| self.zonk(t)).collect(),
        }
    }

    /// Unsolved metas occurring in a type.
    pub fn free_metas(&self, t: &Ty, out: &mut Vec<u32>) {
        match t {
            Ty::Meta(i) => match &self.subst[*i as usize] {
                Some(s) => self.free_metas(s, out),
                None => {
                    if !out.contains(i) {
                        out.push(*i);
                    }
                }
            },
            Ty::App(f, a) => {
                self.free_metas(f, out);
                self.free_metas(a, out);
            }
            Ty::Qual(ps, b) | Ty::Forall(_, ps, b) => {
                for p in ps {
                    for a in &p.args {
                        self.free_metas(a, out);
                    }
                }
                self.free_metas(b, out);
            }
            _ => {}
        }
    }

    pub fn unify(&mut self, a: &Ty, b: &Ty) -> Result<(), UnifyError> {
        let a = self.prune(a);
        let b = self.prune(b);
        match (&a, &b) {
            (Ty::Meta(i), Ty::Meta(j)) if i == j => Ok(()),
            (Ty::Meta(i), t) | (t, Ty::Meta(i)) => self.bind(*i, t),
            (Ty::Con(x), Ty::Con(y)) if x == y => Ok(()),
            (Ty::Skol(x, _), Ty::Skol(y, _)) if x == y => Ok(()),
            (Ty::App(f1, a1), Ty::App(f2, a2)) => {
                self.unify(f1, f2)?;
                self.unify(a1, a2)
            }
            (Ty::Forall(i1, p1, b1), Ty::Forall(i2, p2, b2)) if i1.len() == i2.len() && p1.len() == p2.len() => {
                // Alpha-equivalence: both bodies over the same fresh rigid variables.
                let sk: Vec<Ty> = (0..i1.len()).map(|k| self.fresh_skolem(&format!("r{k}"))).collect();
                let x = subst_bound(b1, i1, &sk);
                let y = subst_bound(b2, i2, &sk);
                self.unify(&x, &y)
            }
            (Ty::Qual(p1, b1), Ty::Qual(p2, b2)) if p1.len() == p2.len() => {
                for (x, y) in p1.iter().zip(p2) {
                    if x.class != y.class || x.args.len() != y.args.len() {
                        return Err(UnifyError::Mismatch);
                    }
                    for (u, v) in x.args.iter().zip(&y.args) {
                        self.unify(u, v)?;
                    }
                }
                self.unify(b1, b2)
            }
            _ => Err(UnifyError::Mismatch),
        }
    }

    /// Unification for matching a constructor's result type against a scrutinee: an
    /// unbound rigid variable is refined to the other side, for the rest of the alternative.
    pub fn unify_refine(&mut self, a: &Ty, b: &Ty) -> Result<(), UnifyError> {
        let a = self.prune(a);
        let b = self.prune(b);
        // An irreducible family application cannot be refined; its equality is not used.
        if self.is_family_app(&a) || self.is_family_app(&b) {
            return Ok(());
        }
        match (&a, &b) {
            (Ty::Skol(x, _), Ty::Skol(y, _)) if x == y => Ok(()),
            (Ty::Skol(x, _), t) | (t, Ty::Skol(x, _)) => {
                self.skol_binds.insert(*x, t.clone());
                Ok(())
            }
            (Ty::App(f1, a1), Ty::App(f2, a2)) => {
                self.unify_refine(f1, f2)?;
                self.unify_refine(a1, a2)
            }
            _ => self.unify(&a, &b),
        }
    }

    fn is_family_app(&self, t: &Ty) -> bool {
        matches!(t.split_apps().0, Ty::Con(c) if self.families.contains(c))
    }

    fn bind(&mut self, i: u32, t: &Ty) -> Result<(), UnifyError> {
        let mut metas = Vec::new();
        self.free_metas(t, &mut metas);
        if metas.contains(&i) {
            return Err(UnifyError::Occurs);
        }
        let level = self.levels[i as usize];
        for m in metas {
            if self.levels[m as usize] > level {
                self.levels[m as usize] = level;
            }
        }
        self.subst[i as usize] = Some(t.clone());
        Ok(())
    }

    /// Instantiates a scheme's quantified variables with fresh metas at `level`.
    pub fn instantiate(&mut self, s: &Scheme, level: u32) -> (Ty, Vec<Pred>, Vec<Ty>) {
        let args: Vec<Ty> = (0..s.nvars).map(|_| self.fresh(level)).collect();
        let ty = subst_gen(&s.ty, &args);
        let preds = s.preds.iter().map(|p| subst_gen_pred(p, &args)).collect();
        (ty, preds, args)
    }
}

/// Renders a type the way GHC does in messages: unqualified names, `[a]`, `(a, b)`.
pub fn show_ty(m: &Metas, t: &Ty) -> String {
    fn go(m: &Metas, t: &Ty, prec: u8) -> String {
        let t = m.prune(t);
        if let Some((a, b)) = t.as_fun() {
            let s = format!("{} -> {}", go(m, a, 1), go(m, b, 0));
            return if prec > 0 { format!("({s})") } else { s };
        }
        let (head, args) = t.split_apps();
        match head {
            Ty::Con(c) if c == LIST && args.len() == 1 => format!("[{}]", go(m, args[0], 0)),
            Ty::Con(c) if tuple_arity(c) == Some(args.len()) && args.len() >= 2 => {
                let parts: Vec<String> = args.iter().map(|a| go(m, a, 0)).collect();
                format!("({})", parts.join(", "))
            }
            _ => {
                let mut s = match head {
                    Ty::Con(c) => unqualify(c).to_string(),
                    Ty::Meta(i) => format!("t{i}"),
                    Ty::Skol(_, n) => n.clone(),
                    Ty::Gen(i) => gen_name(*i),
                    Ty::Bound(i) => format!("b{i}"),
                    Ty::Forall(..) => "(forall)".to_string(),
                    Ty::Qual(..) => "(constrained)".to_string(),
                    Ty::Qual(..) => "(qualified)".to_string(),
                    Ty::App(..) => unreachable!("split_apps returns a non-application head"),
                };
                for a in &args {
                    s.push(' ');
                    s.push_str(&go(m, a, 2));
                }
                if prec > 1 && !args.is_empty() {
                    format!("({s})")
                } else {
                    s
                }
            }
        }
    }
    go(m, t, 0)
}

pub fn show_pred(m: &Metas, p: &Pred) -> String {
    let mut s = unqualify(&p.class).to_string();
    for a in &p.args {
        s.push(' ');
        s.push_str(&show_ty_atomic(m, a));
    }
    s
}

fn show_ty_atomic(m: &Metas, t: &Ty) -> String {
    let s = show_ty(m, t);
    if s.contains(' ') && !s.starts_with('[') && !s.starts_with('(') {
        format!("({s})")
    } else {
        s
    }
}

/// Arity of a built-in tuple constructor name such as `Builtin.(,,)`.
pub fn tuple_arity(name: &str) -> Option<usize> {
    let inner = name.strip_prefix("Builtin.(")?.strip_suffix(')')?;
    if !inner.is_empty() && inner.chars().all(|c| c == ',') {
        Some(inner.len() + 1)
    } else {
        None
    }
}

pub fn unqualify(name: &str) -> &str {
    // Qualified names are `Module.name`; operators may contain dots, so split at the
    // last dot that is followed by a non-empty name and preceded by a module segment.
    let bytes = name.as_bytes();
    let mut split = None;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_uppercase() {
            let mut j = i;
            while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_' || bytes[j] == b'\'') {
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b'.' && j + 1 < bytes.len() {
                split = Some(j);
                i = j + 1;
                continue;
            }
        }
        break;
    }
    match split {
        Some(j) => &name[j + 1..],
        None => name,
    }
}

fn gen_name(i: u32) -> String {
    let letters = b"abcdefghijklmnopqrstuvwxyz";
    if (i as usize) < letters.len() {
        (letters[i as usize] as char).to_string()
    } else {
        format!("t{i}")
    }
}

impl fmt::Display for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", show_ty(&Metas::default(), self))
    }
}
