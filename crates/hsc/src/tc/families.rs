//! Type families: reduction of family applications, and deferral of equalities that are
//! stuck on unsolved variables.

use super::env::{FamEqn, FamilyInfo};
use super::types::{subst_gen, Metas, Ty};
use super::{Checker, TR};
use crate::syntax::ast::{Loc, Type};

#[derive(Debug, PartialEq)]
enum Match {
    Yes,
    No,
    /// Not decidable yet: an unsolved variable or a rigid variable is in the way.
    Stuck,
}

/// One-way matching of an equation pattern against a zonked type.
pub(super) fn match_pat(pat: &Ty, t: &Ty, sub: &mut Vec<Option<Ty>>) -> Match {
    match (pat, t) {
        (Ty::Gen(i), _) => {
            let i = *i as usize;
            match &sub[i] {
                Some(prev) => {
                    if prev == t {
                        Match::Yes
                    } else if is_rigid_ground(prev) && is_rigid_ground(t) {
                        // A repeated variable bound to two different types without variables
                        // (`a == a` at 'User and 'System): the equation does not match.
                        Match::No
                    } else {
                        Match::Stuck
                    }
                }
                None => {
                    sub[i] = Some(t.clone());
                    Match::Yes
                }
            }
        }
        (_, Ty::Meta(_)) | (_, Ty::Skol(..)) => Match::Stuck,
        (Ty::Con(a), Ty::Con(b)) => {
            if a == b {
                Match::Yes
            } else {
                Match::No
            }
        }
        (Ty::App(f1, a1), Ty::App(f2, a2)) => {
            let r1 = match_pat(f1, f2, sub);
            if r1 == Match::No {
                return Match::No;
            }
            let r2 = match_pat(a1, a2, sub);
            match (r1, r2) {
                (_, Match::No) => Match::No,
                (Match::Yes, Match::Yes) => Match::Yes,
                _ => Match::Stuck,
            }
        }
        _ => Match::No,
    }
}

/// A family equation's pattern with each wildcard `_` replaced by a variable of its own.
fn name_wildcards(t: &Type, n: &mut usize) -> Type {
    match t {
        Type::Wild(l) => {
            *n += 1;
            Type::Var(format!("_wild{n}"), *l)
        }
        Type::App(f, a) => Type::App(Box::new(name_wildcards(f, n)), Box::new(name_wildcards(a, n))),
        Type::Fun(f, a) => Type::Fun(Box::new(name_wildcards(f, n)), Box::new(name_wildcards(a, n))),
        Type::List(e, l) => Type::List(Box::new(name_wildcards(e, n)), *l),
        Type::Tuple(ts, l) => Type::Tuple(ts.iter().map(|x| name_wildcards(x, n)).collect(), *l),
        Type::Sig(x, k) => Type::Sig(Box::new(name_wildcards(x, n)), k.clone()),
        other => other.clone(),
    }
}

/// A type with no unification or rigid variables: two different ones never become equal.
fn is_rigid_ground(t: &Ty) -> bool {
    match t {
        Ty::Con(_) => true,
        Ty::App(f, a) => is_rigid_ground(f) && is_rigid_ground(a),
        _ => false,
    }
}

impl Checker {
    /// The representation type of a data family applied to fewer arguments than its arity: the
    /// right-hand side of the first equation whose leading arguments match. (A data instance's
    /// representation is a data type, so the partial application names it.)
    pub(super) fn partial_family_rep(&self, t: &Ty) -> Option<String> {
        let (head, args) = t.split_apps();
        let Ty::Con(name) = head else {
            return None;
        };
        let info = self.env.families.get(name)?;
        if args.len() >= info.arity {
            return None;
        }
        let args: Vec<Ty> = args.into_iter().cloned().collect();
        for eqn in &info.equations {
            let mut sub: Vec<Option<Ty>> = vec![None; eqn.nvars as usize];
            let matched = eqn.args.iter().zip(&args).all(|(p, a)| matches!(match_pat(p, a, &mut sub), Match::Yes));
            if matched {
                if let Ty::Con(rep) = eqn.rhs.split_apps().0 {
                    return Some(rep.clone());
                }
            }
        }
        None
    }

    /// The family at the head of a type application, if it is saturated.
    pub(super) fn family_app(&self, t: &Ty) -> Option<(String, Vec<Ty>)> {
        let (head, args) = t.split_apps();
        let Ty::Con(name) = head else {
            return None;
        };
        let info = self.env.families.get(name)?;
        if args.len() < info.arity {
            return None;
        }
        Some((name.clone(), args.into_iter().cloned().collect()))
    }

    /// A family applied to fewer arguments than its arity reduces when an equation's right-hand side
    /// ends in the missing arguments: `F p1 .. pk` with `F p1 .. pk q = R x q` is `R x`. This is how a
    /// data family's representation is named by a partial application.
    fn partial_family_reduce(&self, t: &Ty) -> Option<Ty> {
        let (head, args) = t.split_apps();
        let Ty::Con(name) = head else {
            return None;
        };
        let info = self.env.families.get(name)?;
        if args.len() >= info.arity {
            return None;
        }
        let args: Vec<Ty> = args.into_iter().cloned().collect();
        let extra = info.arity - args.len();
        for eqn in &info.equations {
            if eqn.args.len() != info.arity {
                continue;
            }
            let mut sub: Vec<Option<Ty>> = vec![None; eqn.nvars as usize];
            let leading_match = eqn.args[..args.len()]
                .iter()
                .zip(&args)
                .all(|(p, a)| matches!(match_pat(p, a, &mut sub), Match::Yes));
            if !leading_match {
                continue;
            }
            let trailing = &eqn.args[args.len()..];
            if !trailing.iter().all(|p| matches!(p, Ty::Gen(_))) {
                continue;
            }
            let (rhs_head, rhs_args) = eqn.rhs.split_apps();
            if rhs_args.len() < extra {
                continue;
            }
            let split = rhs_args.len() - extra;
            if !rhs_args[split..].iter().zip(trailing).all(|(r, p)| **r == *p) {
                continue;
            }
            // The rest of the right-hand side must not mention the missing arguments
            // (`XRec (GhcPass p) a = Located (Anno a) a` is no `Located (Anno _)` partly applied).
            let missing: Vec<u32> = trailing.iter().filter_map(|p| if let Ty::Gen(i) = p { Some(*i) } else { None }).collect();
            if rhs_args[..split].iter().any(|a| missing.iter().any(|&i| mentions_gen(a, i))) {
                continue;
            }
            let mut rebuilt = rhs_head.clone();
            for a in &rhs_args[..split] {
                rebuilt = Ty::app(rebuilt, (*a).clone());
            }
            let vals: Vec<Ty> = sub.into_iter().map(|x| x.unwrap_or_else(|| Ty::con("Builtin.()"))).collect();
            return Some(super::types::subst_gen(&rebuilt, &vals));
        }
        None
    }

    /// The type-level arithmetic and comparisons GHC computes itself (GHC.TypeNats, GHC.TypeLits),
    /// on literal arguments.
    fn builtin_family(&self, name: &str, args: &[Ty]) -> Option<Ty> {
        let occ = name.rsplit_once('.').map(|(m, o)| (m, o))?;
        let (module, op) = occ;
        if !module.starts_with("GHC.Internal.TypeNats") && !module.starts_with("GHC.Internal.TypeLits") {
            return None;
        }
        let args: Vec<Ty> = args.iter().map(|a| self.normalize(a)).collect();
        let lit = |t: &Ty| -> Option<String> {
            match t {
                Ty::Con(c) => c.strip_prefix('#').map(str::to_string),
                _ => None,
            }
        };
        let nat = |t: &Ty| -> Option<u128> { lit(t)?.parse().ok() };
        let sym = |t: &Ty| -> Option<String> {
            let l = lit(t)?;
            let inner = l.strip_prefix('"')?.strip_suffix('"')?;
            if inner.contains('\\') {
                return None;
            }
            Some(inner.to_string())
        };
        let chr = |t: &Ty| -> Option<char> {
            let l = lit(t)?;
            let inner = l.strip_prefix('\'')?.strip_suffix('\'')?;
            let mut cs = inner.chars();
            let c = cs.next()?;
            if cs.next().is_some() {
                return None;
            }
            Some(c)
        };
        let nat_ty = |n: u128| Ty::Con(format!("#{n}"));
        let ordering = |o: std::cmp::Ordering| {
            Ty::Con(
                match o {
                    std::cmp::Ordering::Less => "GHC.Types.'LT",
                    std::cmp::Ordering::Equal => "GHC.Types.'EQ",
                    std::cmp::Ordering::Greater => "GHC.Types.'GT",
                }
                .to_string(),
            )
        };
        let two = || -> Option<(u128, u128)> { Some((nat(args.first()?)?, nat(args.get(1)?)?)) };
        Some(match op {
            "+" => nat_ty(two().and_then(|(a, b)| a.checked_add(b))?),
            "*" => nat_ty(two().and_then(|(a, b)| a.checked_mul(b))?),
            "^" => nat_ty(two().and_then(|(a, b)| a.checked_pow(u32::try_from(b).ok()?))?),
            "-" => nat_ty(two().and_then(|(a, b)| a.checked_sub(b))?),
            "Div" => nat_ty(two().and_then(|(a, b)| a.checked_div(b))?),
            "Mod" => nat_ty(two().and_then(|(a, b)| a.checked_rem(b))?),
            "Log2" => {
                let a = nat(args.first()?)?;
                if a == 0 {
                    return None;
                }
                nat_ty(u128::from(127 - a.leading_zeros()))
            }
            "CmpNat" => ordering(two().map(|(a, b)| a.cmp(&b))?),
            "CmpSymbol" => ordering(sym(args.first()?)?.cmp(&sym(args.get(1)?)?)),
            "CmpChar" => ordering(chr(args.first()?)?.cmp(&chr(args.get(1)?)?)),
            "AppendSymbol" => Ty::Con(format!("#{:?}", sym(args.first()?)? + &sym(args.get(1)?)?)),
            "ConsSymbol" => Ty::Con(format!("#{:?}", format!("{}{}", chr(args.first()?)?, sym(args.get(1)?)?))),
            "CharToNat" => nat_ty(u128::from(chr(args.first()?)? as u32)),
            "NatToChar" => Ty::Con(format!("#{:?}", char::from_u32(u32::try_from(nat(args.first()?)?).ok()?)?)),
            _ => return None,
        })
    }

    /// The kind of a type, as the name of a type, when it is plain from the type: a promoted
    /// constructor's is its data type, a literal's Natural or Symbol, and a saturated type
    /// constructor's Type ("->" when it is not saturated).
    fn kind_name_of(&self, t: &Ty) -> Option<String> {
        let t = self.metas.zonk(t);
        let (head, args) = t.split_apps();
        let Ty::Con(c) = head else { return None };
        if let Some(lit) = c.strip_prefix('#') {
            return Some(if lit.starts_with('"') { "Symbol" } else if lit.starts_with('\'') { "Char" } else { "Natural" }.into());
        }
        if let Some((m, occ)) = c.rsplit_once('.') {
            if let Some(con) = occ.strip_prefix('\'') {
                return self.env.datacons.get(&format!("{m}.{con}")).map(|d| d.tycon.clone());
            }
        }
        if self.env.families.contains_key(c) {
            return None;
        }
        let arity = if c == super::types::LIST {
            Some(1)
        } else if c.starts_with("Builtin.(") {
            Some(c.matches(',').count() + usize::from(c != "Builtin.()"))
        } else {
            self.env.tycons.get(c).map(|i| i.arity)
        };
        match arity {
            Some(n) if args.len() == n => Some("Type".into()),
            Some(_) => Some("->".into()),
            None => None,
        }
    }

    /// One reduction step at the head of `t`, if it is a family application that can reduce.
    pub(super) fn reduce_head(&self, t: &Ty) -> Option<Ty> {
        let r = self.reduce_head_once(t);
        // A rigid variable equal to an application (see `skol_apps`) may match as that.
        if r.is_none() && !self.skol_apps.is_empty() {
            if let Some((name, args)) = self.family_app(t) {
                let expanded: Vec<Ty> = args.iter().map(|a| self.expand_skol_apps(&self.metas.zonk(a))).collect();
                if expanded != args {
                    return self.reduce_head_once(&Ty::apps(Ty::con(&name), expanded));
                }
            }
        }
        r
    }

    fn reduce_head_once(&self, t: &Ty) -> Option<Ty> {
        let Some((name, args)) = self.family_app(t) else {
            return self.partial_family_reduce(t);
        };
        if let Some(r) = self.builtin_family(&name, &args) {
            return Some(r);
        }
        let info: &FamilyInfo = self.env.families.get(&name)?;
        let arity = info.arity;
        let args: Vec<Ty> = args.iter().map(|a| self.metas.zonk(a)).collect();
        for eqn in &info.equations {
            let FamEqn { nvars, args: pats, rhs, kinds } = eqn;
            let mut sub: Vec<Option<Ty>> = vec![None; *nvars as usize];
            let mut result = Match::Yes;
            for (k, (p, a)) in pats.iter().zip(&args[..arity]).enumerate() {
                // A pattern annotated with a kind matches arguments of that kind only.
                if let Some(Some(want)) = kinds.get(k) {
                    match self.kind_name_of(a) {
                        Some(have) if !same_kind(&have, want) => {
                            result = Match::No;
                            break;
                        }
                        Some(_) => {}
                        None => result = Match::Stuck,
                    }
                }
                match match_pat(p, a, &mut sub) {
                    Match::Yes => {}
                    Match::No => {
                        result = Match::No;
                        break;
                    }
                    Match::Stuck => result = Match::Stuck,
                }
            }
            if std::env::var_os("HSC_DEBUG_FAM").is_some() {
                eprintln!("family {name}: {pats:?} against {args:?}: {result:?}");
            }
            match result {
                Match::Yes => {
                    if std::env::var_os("HSC_DEBUG_FAM").is_some() && sub.iter().any(|x| x.is_none()) {
                        eprintln!("family {name}: unbound pattern variable in {pats:?} for {args:?}");
                    }
                    let vals: Vec<Ty> = sub.into_iter().map(|x| x.unwrap_or(Ty::con("Builtin.?"))).collect();
                    let body = subst_gen(rhs, &vals);
                    let extra: Vec<Ty> = args[arity..].to_vec();
                    return Some(Ty::apps(body, extra));
                }
                // An equation that might still match blocks the later ones of a closed family; an
                // open family's instances are compatible, so another that matches gives the answer.
                Match::Stuck if info.closed => return None,
                Match::Stuck => continue,
                Match::No => continue,
            }
        }
        None
    }

    /// One reduction of a constraint family application (its arguments normalised first): the
    /// constraints it stands for, before they reduce further themselves. The wanted and the given
    /// side agree on these, so the evidence is the tuple of their dictionaries.
    pub(super) fn constraint_family_parts(&self, class: &str, args: &[Ty]) -> Option<Vec<super::types::Pred>> {
        let applied = Ty::apps(Ty::con(class), args.iter().map(|a| self.normalize(a)).collect());
        let reduced = self.reduce_head(&applied)?;
        Some(super::convert::ty_to_preds(&self.metas.zonk(&reduced)))
    }

    /// Reduces every reducible family application in a type, to a fixed point.
    pub fn normalize(&self, t: &Ty) -> Ty {
        let t = self.metas.zonk(t);
        self.normalize_zonked(&t, 0)
    }

    fn normalize_zonked(&self, t: &Ty, depth: usize) -> Ty {
        let rc = std::rc::Rc::new(t.clone());
        (*self.normalize_rc(&rc, depth)).clone()
    }

    /// Whether the head of a type's application spine is a type family (or the type is one).
    fn family_headed(&self, t: &Ty) -> bool {
        let mut h = t;
        while let Ty::App(f, _) = h {
            h = f;
        }
        matches!(h, Ty::Con(c) if self.env.families.contains_key(c))
    }

    /// `normalize_zonked` on a shared part: the same part when nothing in it reduces, so that a
    /// large type without families is neither copied nor compared.
    fn normalize_rc(&self, t: &std::rc::Rc<Ty>, depth: usize) -> std::rc::Rc<Ty> {
        use std::rc::Rc;
        if depth > 200 {
            return t.clone();
        }
        let headed = self.family_headed(t);
        if headed {
            if let Some(r) = self.reduce_head(t) {
                return self.normalize_rc(&Rc::new(self.metas.zonk(&r)), depth + 1);
            }
        }
        let n = match &**t {
            Ty::App(f, a) => {
                let (f2, a2) = (self.normalize_rc(f, depth + 1), self.normalize_rc(a, depth + 1));
                if Rc::ptr_eq(&f2, f) && Rc::ptr_eq(&a2, a) {
                    t.clone()
                } else {
                    // With its arguments reduced, the application itself may reduce now
                    // (`IfM (Nothing ||? Nothing) c d`).
                    let n = Rc::new(Ty::App(f2, a2));
                    if headed {
                        if let Some(r) = self.reduce_head(&n) {
                            return self.normalize_rc(&Rc::new(self.metas.zonk(&r)), depth + 1);
                        }
                    }
                    n
                }
            }
            _ => t.clone(),
        };
        // A stuck family application that a given equality names (`Weights_ (Rep a) ~ L c` in a
        // context) stands for the other side.
        if headed && self.family_app(&n).is_some() {
            if let Some(r) = self.given_family_equality(&n) {
                return self.normalize_rc(&Rc::new(r), depth + 1);
            }
        }
        n
    }

    /// The other side of a given equality one of whose sides is the family application `t`.
    fn given_family_equality(&self, t: &Ty) -> Option<Ty> {
        // Compared with the applications rigid variables equal contracted (see `skol_apps`).
        let contracted = !self.skol_apps.is_empty();
        let t = &if contracted { self.contract_skol_apps(t) } else { t.clone() };
        for g in self.givens.iter().rev() {
            if !super::solve::is_eq_class(&g.pred.class) || g.pred.args.len() != 2 {
                continue;
            }
            let (mut a, mut b) = (self.metas.zonk(&g.pred.args[0]), self.metas.zonk(&g.pred.args[1]));
            if contracted {
                a = self.contract_skol_apps(&a);
                b = self.contract_skol_apps(&b);
            }
            // Never towards a type containing the one rewritten (`NoGhcTcPass p` is not
            // `NoGhcTcPass (NoGhcTcPass p)`, IsPass's superclass read backwards).
            if a == *t && b != *t && !contains_ty(&b, t) {
                return Some(b);
            }
            if b == *t && a != *t && !contains_ty(&a, t) {
                return Some(a);
            }
        }
        None
    }

    /// Whether a (normalised) type still contains a family application.
    pub fn has_family(&self, t: &Ty) -> bool {
        if self.family_app(t).is_some() {
            return true;
        }
        match t {
            Ty::App(f, a) => self.has_family(f) || self.has_family(a),
            _ => false,
        }
    }

    /// Retries deferred equalities whose families can now reduce.
    pub fn retry_deferred(&mut self) -> TR<()> {
        if self.deferred_eqs.is_empty() {
            return Ok(());
        }
        let pending = std::mem::take(&mut self.deferred_eqs);
        for (a, b, loc) in pending {
            let a2 = self.normalize(&a);
            let b2 = self.normalize(&b);
            if std::env::var_os("HSC_DEBUG_DEFER").is_some() {
                eprintln!("deferred: {} ~ {}  now  {} ~ {}", self.show(&a), self.show(&b), self.show(&a2), self.show(&b2));
            }
            // Unchanged since it was deferred: still stuck (only new information can help).
            if a2 == a && b2 == b {
                self.deferred_eqs.push((a, b, loc));
                continue;
            }
            if self.has_family(&a2) || self.has_family(&b2) {
                // Still stuck, but an injective family can improve it (`SyntaxExprGhc t ~
                // SyntaxExprGhc p` gives `t ~ p`); what stays stuck is deferred again.
                self.loc = loc;
                self.unify_fam(&a2, &b2)?;
            } else {
                self.loc = loc;
                self.unify_at_raw(&a2, &b2)?;
            }
        }
        Ok(())
    }

    /// Registers a family from its declaration.
    pub fn register_family(&mut self, name: &str, arity: usize, closed: bool) {
        self.metas.families.insert(name.to_string());
        self.env.families.insert(
            name.to_string(),
            FamilyInfo {
                arity,
                closed,
                equations: Vec::new(),
                injective: Vec::new(),
            },
        );
    }

    /// Adds an equation to a family, converting its patterns and right-hand side.
    pub fn add_family_equation(&mut self, name: &str, args: &[Type], rhs: &Type, loc: Loc) -> TR<()> {
        self.loc = loc;
        if !self.env.families.contains_key(name) {
            return self.err(format!("‘{}’ is not a type family", super::types::unqualify(name)));
        }
        let mut tvs = Vec::new();
        let mut pats = Vec::with_capacity(args.len());
        let mut kinds = Vec::with_capacity(args.len());
        // A wildcard in a pattern (`_ == _ = 'False`) is a variable of its own.
        let mut wild = 0usize;
        let args: Vec<Type> = args.iter().map(|a| name_wildcards(a, &mut wild)).collect();
        for a in &args {
            pats.push(self.convert_type(a, &mut tvs)?);
            kinds.push(match a {
                Type::Sig(_, k) => match k.as_ref() {
                    Type::Con(q, _) => Some(super::convert::global_tycon_name(q)),
                    _ => None,
                },
                _ => None,
            });
        }
        let mut rhs_ty = self.convert_type(rhs, &mut tvs)?;
        // An instance with more arguments than the family's parameters (`data family F a :: Type
        // -> Type` with `data instance F X b = ..`): the extra variables are eta-reduced away, as
        // GHC does, so that `F X` reduces on its own (`F X b = R b` becomes `F X = R`).
        let arity = self.env.families.get(name).map_or(pats.len(), |f| f.arity);
        while pats.len() > arity {
            let Some(Ty::Gen(i)) = pats.last().cloned() else { break };
            let Ty::App(f, a) = rhs_ty.clone() else { break };
            let mentions = |t: &Ty| mentions_gen(t, i);
            if *a != Ty::Gen(i) || mentions(&f) || pats[..pats.len() - 1].iter().any(|p| mentions(p)) {
                break;
            }
            pats.pop();
            kinds.pop();
            rhs_ty = (*f).clone();
        }
        let eqn = FamEqn {
            nvars: tvs.len() as u32,
            args: pats,
            rhs: rhs_ty,
            kinds,
        };
        if let Some(info) = self.env.families.get_mut(name) {
            info.equations.push(eqn);
        }
        Ok(())
    }
}

/// Whether a type mentions the quantified variable `Gen(i)`.
fn mentions_gen(t: &Ty, i: u32) -> bool {
    match t {
        Ty::Gen(j) => *j == i,
        Ty::App(f, a) => mentions_gen(f, i) || mentions_gen(a, i),
        Ty::Qual(ps, b) | Ty::Forall(_, ps, b) => {
            ps.iter().any(|p| p.args.iter().any(|a| mentions_gen(a, i))) || mentions_gen(b, i)
        }
        _ => false,
    }
}

/// Unused helper kept for symmetry with `Metas` users.
#[allow(dead_code)]
fn zonk_with(m: &Metas, t: &Ty) -> Ty {
    m.zonk(t)
}

/// Whether two kind names are the same kind (Type is named in several modules).
fn same_kind(a: &str, b: &str) -> bool {
    let is_type = |k: &str| k == "Type" || k.ends_with(".Type") || k.ends_with(".*") || k == "*";
    if is_type(a) || is_type(b) {
        return is_type(a) && is_type(b);
    }
    let base = |k: &str| k.rsplit('.').next().unwrap_or(k).to_string();
    // Natural is also written Nat; literals answer the short names.
    let norm = |k: &str| match base(k).as_str() {
        "Nat" => "Natural".to_string(),
        other => other.to_string(),
    };
    if !a.contains('.') || !b.contains('.') {
        return norm(a) == norm(b);
    }
    a == b
}

/// Whether `t` occurs in `outer`.
fn contains_ty(outer: &Ty, t: &Ty) -> bool {
    outer == t
        || match outer {
            Ty::App(f, x) => contains_ty(f, t) || contains_ty(x, t),
            _ => false,
        }
}
