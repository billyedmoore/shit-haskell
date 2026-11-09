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
fn match_pat(pat: &Ty, t: &Ty, sub: &mut Vec<Option<Ty>>) -> Match {
    match (pat, t) {
        (Ty::Gen(i), _) => {
            let i = *i as usize;
            match &sub[i] {
                Some(prev) => {
                    if prev == t {
                        Match::Yes
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

impl Checker {
    /// The family at the head of a type application, if it is saturated.
    fn family_app(&self, t: &Ty) -> Option<(String, Vec<Ty>)> {
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

    /// One reduction step at the head of `t`, if it is a family application that can reduce.
    fn reduce_head(&self, t: &Ty) -> Option<Ty> {
        let (name, args) = self.family_app(t)?;
        let info: &FamilyInfo = self.env.families.get(&name)?;
        let arity = info.arity;
        let args: Vec<Ty> = args.iter().map(|a| self.metas.zonk(a)).collect();
        for eqn in &info.equations {
            let FamEqn { nvars, args: pats, rhs } = eqn;
            let mut sub: Vec<Option<Ty>> = vec![None; *nvars as usize];
            let mut result = Match::Yes;
            for (p, a) in pats.iter().zip(&args[..arity]) {
                match match_pat(p, a, &mut sub) {
                    Match::Yes => {}
                    Match::No => {
                        result = Match::No;
                        break;
                    }
                    Match::Stuck => result = Match::Stuck,
                }
            }
            match result {
                Match::Yes => {
                    let vals: Vec<Ty> = sub.into_iter().map(|x| x.unwrap_or(Ty::con("Builtin.?"))).collect();
                    let body = subst_gen(rhs, &vals);
                    let extra: Vec<Ty> = args[arity..].to_vec();
                    return Some(Ty::apps(body, extra));
                }
                // An equation that might still match blocks the later ones.
                Match::Stuck => return None,
                Match::No => continue,
            }
        }
        None
    }

    /// Reduces every reducible family application in a type, to a fixed point.
    pub fn normalize(&self, t: &Ty) -> Ty {
        let t = self.metas.zonk(t);
        self.normalize_zonked(&t, 0)
    }

    fn normalize_zonked(&self, t: &Ty, depth: usize) -> Ty {
        if depth > 200 {
            return t.clone();
        }
        if let Some(r) = self.reduce_head(t) {
            return self.normalize_zonked(&self.metas.zonk(&r), depth + 1);
        }
        match t {
            Ty::App(f, a) => Ty::app(
                self.normalize_zonked(f, depth + 1),
                self.normalize_zonked(a, depth + 1),
            ),
            other => other.clone(),
        }
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
            if self.has_family(&a2) || self.has_family(&b2) {
                self.deferred_eqs.push((a2, b2, loc));
            } else {
                self.loc = loc;
                self.unify_at_raw(&a2, &b2)?;
            }
        }
        Ok(())
    }

    /// Registers a family from its declaration.
    pub fn register_family(&mut self, name: &str, arity: usize, closed: bool) {
        self.env.families.insert(
            name.to_string(),
            FamilyInfo {
                arity,
                closed,
                equations: Vec::new(),
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
        for a in args {
            pats.push(self.convert_type(a, &mut tvs)?);
        }
        let rhs_ty = self.convert_type(rhs, &mut tvs)?;
        let eqn = FamEqn {
            nvars: tvs.len() as u32,
            args: pats,
            rhs: rhs_ty,
        };
        if let Some(info) = self.env.families.get_mut(name) {
            info.equations.push(eqn);
        }
        Ok(())
    }
}

/// Unused helper kept for symmetry with `Metas` users.
#[allow(dead_code)]
fn zonk_with(m: &Metas, t: &Ty) -> Ty {
    m.zonk(t)
}
