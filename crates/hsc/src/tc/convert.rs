//! Conversion of renamed syntactic types into checker types and schemes.

use super::types::{Pred, Scheme, Ty};
use super::{Checker, TR};
use crate::syntax::ast::{Lit, QName, Type};

/// The global name of a renamed type-level name, e.g. `Prelude.Int` or `Builtin.[]`.
pub fn global_tycon_name(q: &QName) -> String {
    match &q.qual {
        Some(m) => format!("{m}.{}", q.name),
        None => q.name.clone(),
    }
}

impl Checker {
    /// Converts a type. Type variables are looked up in (or appended to) `tvs`, so that
    /// the caller decides how they are quantified.
    pub fn convert_type(&mut self, t: &Type, tvs: &mut Vec<String>) -> TR<Ty> {
        Ok(match t {
            Type::Var(name, _) => {
                if let Some((_, id)) = self.bound_scope.iter().rev().find(|(n, _)| n == name) {
                    return Ok(Ty::Bound(*id));
                }
                if let Some((_, t)) = self.scoped.iter().rev().find(|(n, _)| n == name) {
                    return Ok(t.clone());
                }
                Ty::Gen(tvar_index(tvs, name))
            }
            Type::Con(q, _) => self.apply_synonym(&global_tycon_name(q), &[], tvs)?,
            Type::App(..) => {
                // Collect the application spine so that synonyms see all their arguments.
                let mut args = Vec::new();
                let mut head = t;
                while let Type::App(f, a) = head {
                    args.push(a.as_ref());
                    head = f;
                }
                args.reverse();
                let mut arg_tys = Vec::with_capacity(args.len());
                for a in args {
                    arg_tys.push(self.convert_type(a, tvs)?);
                }
                match head {
                    // GHC's arrow `FUN m a b` is the built-in arrow: its multiplicity is dropped.
                    Type::Con(q, _) if global_tycon_name(q) == "GHC.Prim.FUN" && (1..=3).contains(&arg_tys.len()) => {
                        Ty::apps(Ty::con(super::types::ARROW), arg_tys[1..].to_vec())
                    }
                    Type::Con(q, _) => self.apply_synonym(&global_tycon_name(q), &arg_tys, tvs)?,
                    other => {
                        let h = self.convert_type(other, tvs)?;
                        Ty::apps(h, arg_tys)
                    }
                }
            }
            Type::Fun(a, b) => Ty::fun(self.convert_type(a, tvs)?, self.convert_type(b, tvs)?),
            Type::List(e, _) => Ty::list(self.convert_type(e, tvs)?),
            Type::Tuple(ts, _) => {
                let mut elems = Vec::with_capacity(ts.len());
                for x in ts {
                    elems.push(self.convert_type(x, tvs)?);
                }
                Ty::tuple(elems)
            }
            Type::Sig(t, _kind) => self.convert_type(t, tvs)?,
            Type::Lit(l, _) => Ty::Con(match l {
                Lit::Int(s) => format!("#{s}"),
                Lit::Str(s) => format!("#{s:?}"),
                Lit::Char(c) => format!("#{c:?}"),
                Lit::Frac(s) => format!("#{s}"),
                Lit::Prim(p) => format!("#{p:?}"),
            }),
            Type::Wild(_) => self.meta(),
            // A nested forall is approximated by quantifying its variables at the
            // enclosing signature. This accepts rank-N programs whose uses are
            // instantiation-compatible, such as `runST`, without checking them rank-N.
            Type::Forall(binders, body) => {
                // A nested polymorphic type: its variables are bound here, not by the signature.
                let mut ids = Vec::with_capacity(binders.len());
                let mark = self.bound_scope.len();
                for b in binders {
                    self.bound_counter += 1;
                    ids.push(self.bound_counter);
                    self.bound_scope.push((b.name.clone(), self.bound_counter));
                }
                let inner = self.convert_type(body, tvs);
                self.bound_scope.truncate(mark);
                match inner? {
                    Ty::Qual(preds, t) => Ty::Forall(ids, preds, t),
                    other => Ty::Forall(ids, Vec::new(), Box::new(other)),
                }
            }
            // A constrained type nested in a signature (rank-N): see `Ty::Qual`.
            Type::Qual(ctx, body) => {
                let mut preds = Vec::with_capacity(ctx.len());
                for c in ctx {
                    preds.extend(self.convert_context_item(c, tvs)?);
                }
                let body = self.convert_type(body, tvs)?;
                if preds.is_empty() {
                    body
                } else {
                    Ty::Qual(preds, Box::new(body))
                }
            }
            Type::OpChain(..) => return self.err("internal: unresolved type operator"),
        })
    }

    /// Expands a type synonym when fully applied; otherwise builds the application.
    fn apply_synonym(&mut self, name: &str, args: &[Ty], _tvs: &mut Vec<String>) -> TR<Ty> {
        // GHC.Types defines `(->)` as `FUN 'Many`; here it is the built-in arrow.
        if name == "GHC.Types.->" {
            return Ok(Ty::apps(Ty::con(super::types::ARROW), args.to_vec()));
        }
        if let Some(syn) = self.env.synonyms.get(name).cloned() {
            let n = syn.nparams as usize;
            if args.len() >= n {
                let expanded = super::types::subst_gen(&syn.rhs, &args[..n]);
                return Ok(Ty::apps(expanded, args[n..].to_vec()));
            }
        }
        Ok(Ty::apps(Ty::Con(name.to_string()), args.to_vec()))
    }

    /// Converts one item of a context into predicates: a class constraint, an implicit
    /// parameter, or a constraint synonym, which may stand for several.
    pub fn convert_context_item(&mut self, t: &Type, tvs: &mut Vec<String>) -> TR<Vec<Pred>> {
        // QuantifiedConstraints are not supported: `forall x. C x` in a context is dropped.
        if let Type::Forall(_, body) = t {
            let _ = body;
            return Ok(Vec::new());
        }
        let mut head = t;
        while let Type::App(f, _) = head {
            head = f;
        }
        if let Type::Con(q, _) = head {
            let name = global_tycon_name(q);
            if !q.name.starts_with('?') && self.env.synonyms.contains_key(&name) {
                let ty = self.convert_type(t, tvs)?;
                return Ok(ty_to_preds(&ty));
            }
        }
        Ok(vec![self.convert_pred(t, tvs)?])
    }

    /// Converts a context item such as `Show a` into a predicate.
    pub fn convert_pred(&mut self, t: &Type, tvs: &mut Vec<String>) -> TR<Pred> {
        let mut args = Vec::new();
        let mut head = t;
        while let Type::App(f, a) = head {
            args.push(a.as_ref());
            head = f;
        }
        args.reverse();
        let class = match head {
            // ImplicitParams: `?x t` is the constraint named by the parameter.
            Type::Con(q, _) if q.name.starts_with('?') => q.name.clone(),
            Type::Con(q, _) => global_tycon_name(q),
            // ConstraintKinds: a constraint variable `c` (or `c a`) is a predicate on its type.
            Type::Var(..) => {
                let ty = self.convert_type(t, tvs)?;
                return Ok(Pred { class: CONSTRAINT_VAR.to_string(), args: vec![ty] });
            }
            _ => return self.err("Malformed class constraint"),
        };
        let mut tys = Vec::with_capacity(args.len());
        for a in args {
            tys.push(self.convert_type(a, tvs)?);
        }
        Ok(Pred { class, args: tys })
    }

    /// Splits a signature into its predicates and body, collecting type variables.
    pub fn sig_parts(&mut self, t: &Type, tvs: &mut Vec<String>) -> TR<(Vec<Pred>, Ty)> {
        let mut preds = Vec::new();
        let mut cur = t;
        loop {
            match cur {
                Type::Forall(binders, body) => {
                    for b in binders {
                        tvar_index(tvs, &b.name);
                    }
                    cur = body;
                }
                Type::Qual(ctx, body) => {
                    for c in ctx {
                        preds.extend(self.convert_context_item(c, tvs)?);
                    }
                    cur = body;
                }
                other => {
                    let ty = self.convert_type(other, tvs)?;
                    return Ok((preds, ty));
                }
            }
        }
    }

    /// The scheme of a signature. Free type variables are quantified in order of appearance.
    pub fn sig_scheme(&mut self, t: &Type, fixed: &[String]) -> TR<Scheme> {
        let mut tvs: Vec<String> = fixed.to_vec();
        let (preds, ty) = self.sig_parts(t, &mut tvs)?;
        Ok(Scheme {
            nvars: tvs.len() as u32,
            names: tvs,
            preds,
            ty,
        })
    }
}

/// The index of a type variable name, appending it if new.
pub fn tvar_index(tvs: &mut Vec<String>, name: &str) -> u32 {
    if let Some(i) = tvs.iter().position(|n| n == name) {
        return i as u32;
    }
    tvs.push(name.to_string());
    (tvs.len() - 1) as u32
}

/// The predicates of a converted constraint type: a class application, a tuple of
/// constraints, or nothing for `()`.
pub fn ty_to_preds(t: &Ty) -> Vec<Pred> {
    let (head, args) = t.split_apps();
    match head {
        Ty::Con(c) if c == "()" => Vec::new(),
        Ty::Con(c) if super::types::tuple_arity(c) == Some(args.len()) && args.len() >= 2 => {
            args.iter().flat_map(|a| ty_to_preds(a)).collect()
        }
        Ty::Con(c) => vec![Pred { class: c.clone(), args: args.into_iter().cloned().collect() }],
        _ => Vec::new(),
    }
}

/// The class name of a predicate that is a constraint variable (see `convert_pred`).
pub const CONSTRAINT_VAR: &str = "%constraint";
