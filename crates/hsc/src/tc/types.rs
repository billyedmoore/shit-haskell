//! Types, predicates, schemes, and unification with levels for generalisation.

use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;

pub const ARROW: &str = "Builtin.->";
pub const LIST: &str = "Builtin.[]";

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Ty {
    /// A type constructor by global name.
    Con(String),
    /// An application. The parts are shared: a type is copied often (a unification variable's
    /// solution, each pruning), and a large one (a derived `Rep`) must not be copied whole.
    App(Rc<Ty>, Rc<Ty>),
    /// A unification variable.
    Meta(u32),
    /// A rigid type variable from a signature; the id makes it unique.
    Skol(u32, String),
    /// A quantified variable of a scheme, by index.
    Gen(u32),
    /// A constrained type in argument position, `C a => t` (rank-N): a value of it is a
    /// function of the dictionaries of its predicates.
    Qual(Vec<Pred>, Rc<Ty>),
    /// A variable bound by a nested `forall` (rank-N), by a unique id.
    Bound(u32),
    /// A nested `forall ids. preds => t`: a polymorphic type, instantiated where it is used
    /// and skolemised where it is checked against.
    Forall(Vec<u32>, Vec<Pred>, Rc<Ty>),
}

impl Ty {
    pub fn con(name: &str) -> Ty {
        Ty::Con(name.to_string())
    }

    pub fn app(f: Ty, a: Ty) -> Ty {
        Ty::App(Rc::new(f), Rc::new(a))
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

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
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
        Ty::App(f, a) => Ty::App(subst_gen_rc(f, args), subst_gen_rc(a, args)),
        Ty::Qual(ps, b) => Ty::Qual(ps.iter().map(|p| subst_gen_pred(p, args)).collect(), subst_gen_rc(b, args)),
        Ty::Forall(ids, ps, b) => Ty::Forall(
            ids.clone(),
            ps.iter().map(|p| subst_gen_pred(p, args)).collect(),
            subst_gen_rc(b, args),
        ),
        other => other.clone(),
    }
}

/// Substitute a shared part without rebuilding branches that contain no replaced
/// variables. Types are immutable; each instantiation still supplies fresh metas.
fn subst_gen_rc(t: &Rc<Ty>, args: &[Ty]) -> Rc<Ty> {
    match &**t {
        Ty::Con(_) | Ty::Meta(_) | Ty::Skol(..) | Ty::Bound(_) => t.clone(),
        Ty::Gen(i) if args.get(*i as usize).is_none() => t.clone(),
        Ty::App(f, a) => {
            let (f2, a2) = (subst_gen_rc(f, args), subst_gen_rc(a, args));
            if Rc::ptr_eq(f, &f2) && Rc::ptr_eq(a, &a2) { t.clone() }
            else { Rc::new(Ty::App(f2, a2)) }
        }
        _ => Rc::new(subst_gen(t, args)),
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
        Ty::Qual(ps, b) => Ty::Qual(ps.iter().map(|p| subst_bound_pred(p, ids, tys)).collect(), Rc::new(subst_bound(b, ids, tys))),
        Ty::Forall(is, ps, b) => {
            // An inner quantifier of the same variable shadows it (a synonym's `forall`, expanded
            // twice in one type, binds the same variable twice).
            if ids.iter().any(|i| is.contains(i)) {
                let (ids2, tys2): (Vec<u32>, Vec<Ty>) =
                    ids.iter().zip(tys).filter(|(i, _)| !is.contains(i)).map(|(i, t)| (*i, t.clone())).unzip();
                return Ty::Forall(
                    is.clone(),
                    ps.iter().map(|p| subst_bound_pred(p, &ids2, &tys2)).collect(),
                    Rc::new(subst_bound(b, &ids2, &tys2)),
                );
            }
            Ty::Forall(is.clone(), ps.iter().map(|p| subst_bound_pred(p, ids, tys)).collect(), Rc::new(subst_bound(b, ids, tys)))
        }
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
        self.zonk_inner(t)
    }

    fn zonk_inner(&self, t: &Ty) -> Ty {
        match t {
            Ty::Meta(i) => match &self.subst[*i as usize] {
                Some(s) => self.zonk(s),
                None => t.clone(),
            },
            Ty::Skol(id, _) => match self.skol_binds.get(id) {
                Some(s) => self.zonk(s),
                None => t.clone(),
            },
            Ty::App(f, a) => Ty::App(self.zonk_rc(f), self.zonk_rc(a)),
            Ty::Qual(ps, b) => Ty::Qual(ps.iter().map(|p| self.zonk_pred(p)).collect(), Rc::new(self.zonk(b))),
            Ty::Forall(ids, ps, b) => Ty::Forall(
                ids.clone(),
                ps.iter().map(|p| self.zonk_pred(p)).collect(),
                Rc::new(self.zonk(b)),
            ),
            other => other.clone(),
        }
    }

    /// A shared part zonked: the same part when nothing in it is solved, so that zonking a large
    /// type that is already zonked copies nothing.
    fn zonk_rc(&self, t: &Rc<Ty>) -> Rc<Ty> {
        match &**t {
            Ty::Con(_) | Ty::Gen(_) | Ty::Bound(_) => t.clone(),
            Ty::Meta(i) if self.subst[*i as usize].is_none() => t.clone(),
            Ty::Skol(id, _) if !self.skol_binds.contains_key(id) => t.clone(),
            Ty::App(f, a) => {
                let (f2, a2) = (self.zonk_rc(f), self.zonk_rc(a));
                if Rc::ptr_eq(&f2, f) && Rc::ptr_eq(&a2, a) {
                    t.clone()
                } else {
                    Rc::new(Ty::App(f2, a2))
                }
            }
            _ => Rc::new(self.zonk_inner(t)),
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
        self.unify_refine_collecting(a, b, &mut Vec::new())
    }

    pub fn unify_refine_collecting(&mut self, a: &Ty, b: &Ty, fam_eqs: &mut Vec<(Ty, Ty)>) -> Result<(), UnifyError> {
        let a = self.prune(a);
        let b = self.prune(b);
        // An unknown equal to a family application is that application (a pattern variable's
        // type `IdGhcP p` in a field `XRec (GhcPass p) (IdP (GhcPass p))`).
        if let (Ty::Meta(_), _) | (_, Ty::Meta(_)) = (&a, &b) {
            return self.unify(&a, &b);
        }
        // A rigid variable equal to an irreducible family application stands for it (`g ~ F a`).
        // Any remaining stuck family equality (`FoldRequired mods ~ 'False` from matching `SFalse`
        // against `SBool (FoldRequired mods)`) is returned to the caller so GADT pattern matching
        // can add it as a local given equality.
        if self.is_family_app(&a) || self.is_family_app(&b) {
            match (&a, &b) {
                (Ty::Skol(x, _), t) | (t, Ty::Skol(x, _)) if !self.is_family_app(&Ty::Skol(*x, String::new())) => {
                    // Not to a type containing it (`b ~ Body b GhcPs`): that is no refinement.
                    if !matches!(t, Ty::Skol(..)) && !self.skol_in(*x, t) {
                        self.skol_binds.insert(*x, t.clone());
                    }
                }
                _ => {}
            }
            fam_eqs.push((a, b));
            return Ok(());
        }
        match (&a, &b) {
            (Ty::Skol(x, _), Ty::Skol(y, _)) if x == y => Ok(()),
            (Ty::Skol(x, _), Ty::Skol(y, _)) => {
                // Refine the newer (inner/existential) skolem to the older (outer) one so
                // an existential skolem from a pattern branch cannot escape to an outer type.
                let (from, to) = if x > y { (*x, b.clone()) } else { (*y, a.clone()) };
                if !self.skol_in(from, &to) {
                    self.skol_binds.insert(from, to);
                }
                Ok(())
            }
            // An unknown type is determined by the other side, not refined around.
            (Ty::Meta(_), _) | (_, Ty::Meta(_)) => self.unify(&a, &b),
            (Ty::Skol(x, _), t) | (t, Ty::Skol(x, _)) => {
                if !self.skol_in(*x, t) {
                    self.skol_binds.insert(*x, t.clone());
                }
                Ok(())
            }
            (Ty::App(f1, a1), Ty::App(f2, a2)) => {
                self.unify_refine_collecting(f1, f2, fam_eqs)?;
                self.unify_refine_collecting(a1, a2, fam_eqs)
            }
            _ => self.unify(&a, &b),
        }
    }

    /// Whether rigid variable `x` occurs in `t` (through the substitutions).
    fn skol_in(&self, x: u32, t: &Ty) -> bool {
        let r = self.skol_in_(x, t);
        if r && std::env::var_os("HSC_DEBUG_SKOL").is_some() {
            eprintln!("skolem {x} not refined to {t:?}");
        }
        r
    }

    fn skol_in_(&self, x: u32, t: &Ty) -> bool {
        match self.prune(t) {
            Ty::Skol(y, _) => y == x,
            Ty::App(f, a) => self.skol_in_(x, &f) || self.skol_in_(x, &a),
            _ => false,
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
    show_ty_with_names(m, t, false)
}

pub(super) fn show_ty_qualified(m: &Metas, t: &Ty) -> String {
    show_ty_with_names(m, t, true)
}

fn show_ty_with_names(m: &Metas, t: &Ty, qualified: bool) -> String {
    fn go(m: &Metas, t: &Ty, prec: u8, qualified: bool) -> String {
        let t = m.prune(t);
        if let Some((a, b)) = t.as_fun() {
            let s = format!("{} -> {}", go(m, a, 1, qualified), go(m, b, 0, qualified));
            return if prec > 0 { format!("({s})") } else { s };
        }
        let (head, args) = t.split_apps();
        match head {
            Ty::Con(c) if c == LIST && args.len() == 1 => format!("[{}]", go(m, args[0], 0, qualified)),
            Ty::Con(c) if tuple_arity(c) == Some(args.len()) && args.len() >= 2 => {
                let parts: Vec<String> = args.iter().map(|a| go(m, a, 0, qualified)).collect();
                format!("({})", parts.join(", "))
            }
            _ => {
                let mut s = match head {
                    Ty::Con(c) if qualified && !c.starts_with("Builtin.") => c.clone(),
                    Ty::Con(c) => unqualify(c).to_string(),
                    Ty::Meta(i) => format!("t{i}"),
                    Ty::Skol(_, n) => n.clone(),
                    Ty::Gen(i) => gen_name(*i),
                    Ty::Bound(i) => format!("b{i}"),
                    Ty::Forall(..) => "(forall)".to_string(),
                    Ty::Qual(..) => "(constrained)".to_string(),
                    Ty::App(..) => unreachable!("split_apps returns a non-application head"),
                };
                for a in &args {
                    s.push(' ');
                    s.push_str(&go(m, a, 2, qualified));
                }
                if prec > 1 && !args.is_empty() {
                    format!("({s})")
                } else {
                    s
                }
            }
        }
    }
    go(m, t, 0, qualified)
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

#[cfg(test)]
mod substitution_sharing_tests {
    use super::*;

    #[test]
    fn shared_closed_parts_do_not_share_instantiation_variables() {
        let fixed = Rc::new(Ty::app(Ty::con("Closed"), Ty::con("Payload")));
        let scheme = Scheme {
            nvars: 1, names: vec!["a".into()], preds: Vec::new(),
            ty: Ty::App(fixed.clone(), Rc::new(Ty::Gen(0))),
        };
        let mut metas = Metas::default();
        let (first, _, first_args) = metas.instantiate(&scheme, 0);
        let (second, _, second_args) = metas.instantiate(&scheme, 0);
        let (Ty::App(f1, a1), Ty::App(f2, a2)) = (&first, &second) else { panic!("application") };
        assert!(Rc::ptr_eq(&fixed, f1) && Rc::ptr_eq(&fixed, f2));
        assert_ne!(first_args, second_args);
        metas.unify(a1, &Ty::con("Int")).unwrap();
        assert_eq!(metas.zonk(a1), Ty::con("Int"));
        assert_eq!(metas.zonk(a2), second_args[0]);
        assert!(matches!(scheme.ty, Ty::App(_, ref a) if **a == Ty::Gen(0)));
    }

    #[test]
    fn empty_substitution_still_canonicalizes_nested_constraint_variables() {
        let qualified = Ty::Qual(
            vec![Pred {
                class: super::super::convert::CONSTRAINT_VAR.into(),
                args: vec![Ty::app(Ty::con("C"), Ty::con("T"))],
            }],
            Rc::new(Ty::con("Result")),
        );
        let input = Ty::app(Ty::con("Wrapper"), qualified);
        let expected = Ty::app(Ty::con("Wrapper"), Ty::Qual(
            vec![Pred { class: "C".into(), args: vec![Ty::con("T")] }], Rc::new(Ty::con("Result")),
        ));
        assert_eq!(subst_gen(&input, &[]), expected);
        let quantified = Ty::app(Ty::con("Wrapper"), Ty::Forall(
            vec![9], vec![Pred {
                class: super::super::convert::CONSTRAINT_VAR.into(),
                args: vec![Ty::app(Ty::con("C"), Ty::con("T"))],
            }], Rc::new(Ty::Bound(9)),
        ));
        let expected = Ty::app(Ty::con("Wrapper"), Ty::Forall(
            vec![9], vec![Pred { class: "C".into(), args: vec![Ty::con("T")] }], Rc::new(Ty::Bound(9)),
        ));
        assert_eq!(subst_gen(&quantified, &[]), expected);
        let missing = Ty::app(Ty::con("Wrapper"), Ty::Gen(7));
        assert_eq!(subst_gen(&missing, &[Ty::con("Unused")]), missing);
    }
}
