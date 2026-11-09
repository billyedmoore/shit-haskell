//! Types, predicates, schemes, and unification with levels for generalisation.

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
        other => other.clone(),
    }
}

pub fn subst_gen_pred(p: &Pred, args: &[Ty]) -> Pred {
    Pred {
        class: p.class.clone(),
        args: p.args.iter().map(|t| subst_gen(t, args)).collect(),
    }
}

/// The unification variables of the checker, with their levels.
#[derive(Default)]
pub struct Metas {
    pub subst: Vec<Option<Ty>>,
    pub levels: Vec<u32>,
    pub skolems: u32,
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

    /// Resolves the outermost meta chain of a type.
    pub fn prune(&self, t: &Ty) -> Ty {
        let mut cur = t.clone();
        while let Ty::Meta(i) = cur {
            match &self.subst[i as usize] {
                Some(next) => cur = next.clone(),
                None => break,
            }
        }
        cur
    }

    /// Fully substitutes all solved metas.
    pub fn zonk(&self, t: &Ty) -> Ty {
        match t {
            Ty::Meta(i) => match &self.subst[*i as usize] {
                Some(s) => self.zonk(s),
                None => t.clone(),
            },
            Ty::App(f, a) => Ty::app(self.zonk(f), self.zonk(a)),
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
            _ => Err(UnifyError::Mismatch),
        }
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
