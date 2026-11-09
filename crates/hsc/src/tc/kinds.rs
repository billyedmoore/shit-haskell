//! Kind checking for declarations and signatures. Kinds are `Type`, arrows, and unification
//! variables. Declared types get inferred kinds, generalised over any left-over variables
//! (GHC2021 enables PolyKinds). Signatures must have kind `Type`.

use std::collections::HashMap;

use super::convert::global_tycon_name;
use super::{Checker, TcError, TR};
use crate::syntax::ast::{ClassDecl, DataDecl, Decl, Loc, Type};

#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Star,
    Arr(Box<Kind>, Box<Kind>),
    Meta(u32),
    /// A generalised kind variable of a declaration's kind.
    Gen(u32),
}

fn arr(a: Kind, b: Kind) -> Kind {
    Kind::Arr(Box::new(a), Box::new(b))
}

fn show(k: &Kind) -> String {
    match k {
        Kind::Star => "*".into(),
        Kind::Arr(a, b) => {
            let l = if matches!(**a, Kind::Arr(..)) { format!("({})", show(a)) } else { show(a) };
            format!("{l} -> {}", show(b))
        }
        Kind::Meta(m) => format!("k{m}"),
        Kind::Gen(g) => format!("k{g}"),
    }
}

/// The kinds of type constructors, and the metas of the kind checker.
#[derive(Default)]
pub struct KindEnv {
    pub tycons: HashMap<String, Kind>,
    metas: Vec<Option<Kind>>,
}

impl KindEnv {
    fn fresh(&mut self) -> Kind {
        self.metas.push(None);
        Kind::Meta(self.metas.len() as u32 - 1)
    }

    fn prune(&self, k: &Kind) -> Kind {
        let mut cur = k.clone();
        while let Kind::Meta(m) = cur {
            match &self.metas[m as usize] {
                Some(next) => cur = next.clone(),
                None => break,
            }
        }
        cur
    }

    fn unify(&mut self, a: &Kind, b: &Kind) -> Result<(), ()> {
        let a = self.prune(a);
        let b = self.prune(b);
        match (&a, &b) {
            (Kind::Meta(x), Kind::Meta(y)) if x == y => Ok(()),
            (Kind::Meta(x), t) | (t, Kind::Meta(x)) => {
                if occurs(&self.zonk(t), *x) {
                    return Err(());
                }
                self.metas[*x as usize] = Some(t.clone());
                Ok(())
            }
            (Kind::Star, Kind::Star) => Ok(()),
            (Kind::Gen(x), Kind::Gen(y)) if x == y => Ok(()),
            (Kind::Arr(a1, b1), Kind::Arr(a2, b2)) => {
                self.unify(a1, a2)?;
                self.unify(b1, b2)
            }
            _ => Err(()),
        }
    }

    fn zonk(&self, k: &Kind) -> Kind {
        match self.prune(k) {
            Kind::Arr(a, b) => arr(self.zonk(&a), self.zonk(&b)),
            other => other,
        }
    }

    /// Replaces unsolved metas by generic kind variables, in order of appearance.
    fn generalise(&mut self, k: &Kind, map: &mut Vec<u32>) -> Kind {
        match self.prune(k) {
            Kind::Meta(m) => {
                let idx = match map.iter().position(|x| *x == m) {
                    Some(i) => i,
                    None => {
                        map.push(m);
                        map.len() - 1
                    }
                };
                Kind::Gen(idx as u32)
            }
            Kind::Arr(a, b) => {
                let a2 = self.generalise(&a, map);
                arr(a2, self.generalise(&b, map))
            }
            other => other,
        }
    }

    /// Instantiates generic kind variables with fresh metas.
    fn instantiate(&mut self, k: &Kind, args: &mut Vec<Kind>) -> Kind {
        match k {
            Kind::Gen(g) => {
                while args.len() <= *g as usize {
                    let m = self.fresh();
                    args.push(m);
                }
                args[*g as usize].clone()
            }
            Kind::Arr(a, b) => {
                let a2 = self.instantiate(a, args);
                arr(a2, self.instantiate(b, args))
            }
            other => other.clone(),
        }
    }
}

fn occurs(k: &Kind, m: u32) -> bool {
    match k {
        Kind::Meta(x) => *x == m,
        Kind::Arr(a, b) => occurs(a, m) || occurs(b, m),
        _ => false,
    }
}

/// Environment of type variables in a declaration: name to kind.
type TyVars = HashMap<String, Kind>;

impl Checker {
    fn kind_err<T>(&self, loc: Loc, msg: String) -> TR<T> {
        Err(TcError { loc, msg })
    }

    /// The kind of a type, unifying as needed.
    fn kind_of(&mut self, t: &Type, vars: &mut TyVars, loc: Loc) -> TR<Kind> {
        match t {
            Type::Var(name, _) => match vars.get(name) {
                Some(k) => Ok(k.clone()),
                None => {
                    let k = self.kinds.fresh();
                    vars.insert(name.clone(), k.clone());
                    Ok(k)
                }
            },
            Type::Con(q, l) => {
                let name = global_tycon_name(q);
                let l = if l.line == 0 { loc } else { *l };
                match self.kinds.tycons.get(&name).cloned() {
                    Some(k) => {
                        let mut args = Vec::new();
                        Ok(self.kinds.instantiate(&k, &mut args))
                    }
                    None => {
                        // Classes, promoted constructors and unknown names are not checked.
                        let _ = l;
                        Ok(self.kinds.fresh())
                    }
                }
            }
            Type::App(f, a) => {
                let kf = self.kind_of(f, vars, loc)?;
                let ka = self.kind_of(a, vars, loc)?;
                let r = self.kinds.fresh();
                let expected = arr(ka, r.clone());
                if self.kinds.unify(&kf, &expected).is_err() {
                    let shown = show(&self.kinds.zonk(&kf));
                    let head = type_head_name(t);
                    return self.kind_err(
                        loc,
                        format!("Expected kind ‘{}’, but ‘{head}’ has kind ‘{shown}’", show(&expected)),
                    );
                }
                Ok(r)
            }
            Type::Fun(a, b) => {
                self.expect_star(a, vars, loc)?;
                self.expect_star(b, vars, loc)?;
                Ok(Kind::Star)
            }
            Type::List(e, _) => {
                self.expect_star(e, vars, loc)?;
                Ok(Kind::Star)
            }
            Type::Tuple(ts, _) => {
                for x in ts {
                    self.expect_star(x, vars, loc)?;
                }
                Ok(Kind::Star)
            }
            Type::Forall(binders, body) => {
                for b in binders {
                    let k = self.kinds.fresh();
                    vars.insert(b.name.clone(), k);
                }
                self.expect_star(body, vars, loc)?;
                Ok(Kind::Star)
            }
            Type::Qual(ctx, body) => {
                for c in ctx {
                    self.kind_of(c, vars, loc)?;
                }
                self.expect_star(body, vars, loc)?;
                Ok(Kind::Star)
            }
            Type::Sig(t, k) => {
                let kt = self.kind_of(t, vars, loc)?;
                let ks = self.kind_from_type(k);
                if self.kinds.unify(&kt, &ks).is_err() {
                    return self.kind_err(loc, "Kind mismatch in a kind signature".into());
                }
                Ok(kt)
            }
            Type::Lit(..) | Type::Wild(_) => Ok(self.kinds.fresh()),
            Type::OpChain(..) => Ok(self.kinds.fresh()),
        }
    }

    fn expect_star(&mut self, t: &Type, vars: &mut TyVars, loc: Loc) -> TR<()> {
        let k = self.kind_of(t, vars, loc)?;
        if self.kinds.unify(&k, &Kind::Star).is_err() {
            let shown = show(&self.kinds.zonk(&k));
            let head = type_head_name(t);
            let msg = if shown.contains("->") {
                format!("Expecting one more argument to ‘{head}’\n    Expected a type, but ‘{head}’ has kind ‘{shown}’")
            } else {
                format!("Expected a type, but ‘{head}’ has kind ‘{shown}’")
            };
            return self.kind_err(loc, msg);
        }
        Ok(())
    }

    /// Kinds written in signatures: `Type`, `k1 -> k2`, and names of kinds.
    fn kind_from_type(&mut self, t: &Type) -> Kind {
        match t {
            Type::Fun(a, b) => arr(self.kind_from_type(a), self.kind_from_type(b)),
            _ => Kind::Star,
        }
    }

    /// Infers kinds for the type constructors declared in a module, then checks signatures.
    pub fn check_kinds(&mut self, decls: &[Decl]) -> TR<()> {
        // Data declarations first, then classes; these may refer to each other.
        let mut pending: Vec<(&Decl, Kind)> = Vec::new();
        for d in decls {
            match d {
                Decl::Data(dd) => {
                    let name = format!("{}.{}", self.module, dd.name);
                    let mut vars: TyVars = HashMap::new();
                    let mut params = Vec::new();
                    for tv in &dd.tyvars {
                        let k = match &tv.kind {
                            Some(k) => self.kind_from_type(k),
                            None => self.kinds.fresh(),
                        };
                        vars.insert(tv.name.clone(), k.clone());
                        params.push(k);
                    }
                    let kind = params.into_iter().rev().fold(Kind::Star, |acc, p| arr(p, acc));
                    self.kinds.tycons.insert(name.clone(), kind.clone());
                    pending.push((d, kind));
                    self.check_data_fields(dd, &mut vars)?;
                }
                Decl::TypeSyn { name, tyvars, rhs, loc } => {
                    let full = format!("{}.{name}", self.module);
                    let mut vars: TyVars = HashMap::new();
                    let mut params = Vec::new();
                    for tv in tyvars {
                        let k = self.kinds.fresh();
                        vars.insert(tv.name.clone(), k.clone());
                        params.push(k);
                    }
                    let kr = self.kind_of(rhs, &mut vars, *loc)?;
                    let kind = params.into_iter().rev().fold(kr, |acc, p| arr(p, acc));
                    self.kinds.tycons.insert(full, kind);
                }
                _ => {}
            }
        }
        for d in decls {
            if let Decl::Class(cd) = d {
                self.check_class_kinds(cd)?;
            }
        }
        // Generalise the inferred kinds of this module's type constructors.
        let prefix = format!("{}.", self.module);
        let names: Vec<String> = self.kinds.tycons.keys().filter(|k| k.starts_with(&prefix)).cloned().collect();
        for n in names {
            let k = self.kinds.tycons[&n].clone();
            let mut map = Vec::new();
            let g = self.kinds.generalise(&k, &mut map);
            self.kinds.tycons.insert(n, g);
        }
        // Signatures must be of kind Type.
        for d in decls {
            if let Decl::Sig { ty, loc, .. } = d {
                let mut vars: TyVars = HashMap::new();
                self.expect_star(ty, &mut vars, *loc)?;
            }
        }
        Ok(())
    }

    fn check_data_fields(&mut self, dd: &DataDecl, vars: &mut TyVars) -> TR<()> {
        for c in &dd.cons {
            let mut cvars = vars.clone();
            for e in &c.exist {
                let k = self.kinds.fresh();
                cvars.insert(e.name.clone(), k);
            }
            for ctx in &c.context {
                self.kind_of(ctx, &mut cvars, c.loc)?;
            }
            match &c.fields {
                crate::syntax::ast::ConFields::Positional(fs) => {
                    for f in fs {
                        self.expect_star(&f.ty, &mut cvars, c.loc)?;
                    }
                }
                crate::syntax::ast::ConFields::Record(groups) => {
                    for (_, f) in groups {
                        self.expect_star(&f.ty, &mut cvars, c.loc)?;
                    }
                }
            }
            if let Some(r) = &c.gadt_result {
                self.expect_star(r, &mut cvars, c.loc)?;
            }
        }
        Ok(())
    }

    fn check_class_kinds(&mut self, cd: &ClassDecl) -> TR<()> {
        let mut vars: TyVars = HashMap::new();
        for tv in &cd.tyvars {
            let k = self.kinds.fresh();
            vars.insert(tv.name.clone(), k);
        }
        for c in &cd.context {
            self.kind_of(c, &mut vars, cd.loc)?;
        }
        for d in &cd.decls {
            if let Decl::Sig { ty, loc, .. } = d {
                let mut mvars = vars.clone();
                self.expect_star(ty, &mut mvars, *loc)?;
            }
        }
        Ok(())
    }
}

fn type_head_name(t: &Type) -> String {
    match t {
        Type::Con(q, _) => q.name.clone(),
        Type::App(f, _) => type_head_name(f),
        Type::Var(v, _) => v.clone(),
        _ => "type".into(),
    }
}
