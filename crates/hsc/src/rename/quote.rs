//! Template Haskell quotations: `[| e |]` (renamed first) is rebuilt as the code that builds
//! its syntax with the `GHC.Internal.TH.Lib` combinators, of type `Quote m => m Exp`. Global
//! names become `mkNameG_*` names of their module; variables the quotation binds become
//! `mkName` names; variables of the enclosing code are lifted (`lift x`); a splice inside the
//! quotation is its expression.

use std::collections::HashSet;

use super::{package_of_module, RnError, Renamer, RR};
use crate::syntax::ast::*;

const LIB: &str = "GHC.Internal.TH.Lib";
const SYN: &str = "GHC.Internal.TH.Syntax";

struct Q<'r, 'a> {
    rn: &'r Renamer<'a>,
    /// The variables bound inside the quotation (renamed names).
    bound: HashSet<String>,
    loc: Loc,
}

fn app(f: Expr, a: Expr) -> Expr {
    Expr::App(Box::new(f), Box::new(a))
}

impl<'a> Renamer<'a> {
    /// The code building the syntax of a renamed quoted expression.
    pub(super) fn quote_exp(&self, e: &Expr, loc: Loc) -> RR<Expr> {
        let mut bound = HashSet::new();
        bound_in_expr(e, &mut bound);
        let q = Q { rn: self, bound, loc };
        q.exp(e)
    }
}

impl Q<'_, '_> {
    fn lib(&self, n: &str) -> Expr {
        Expr::Var(QName { qual: Some(LIB.into()), name: n.into() }, self.loc)
    }

    fn syn(&self, n: &str) -> Expr {
        Expr::Var(QName { qual: Some(SYN.into()), name: n.into() }, self.loc)
    }

    fn str(&self, s: &str) -> Expr {
        Expr::Lit(Lit::Str(s.to_string()), self.loc)
    }

    fn list(&self, xs: Vec<Expr>) -> Expr {
        Expr::List(xs, self.loc)
    }

    fn just(&self, x: Expr) -> Expr {
        app(Expr::Con(QName { qual: Some("GHC.Internal.Maybe".into()), name: "Just".into() }, self.loc), x)
    }

    fn nothing(&self) -> Expr {
        Expr::Con(QName { qual: Some("GHC.Internal.Maybe".into()), name: "Nothing".into() }, self.loc)
    }

    fn pure(&self, x: Expr) -> Expr {
        app(Expr::Var(QName { qual: Some("GHC.Internal.Base".into()), name: "pure".into() }, self.loc), x)
    }

    fn err<T>(&self, what: &str) -> RR<T> {
        Err(RnError { loc: self.loc, msg: format!("{what} in a Template Haskell quotation is not supported yet") })
    }

    // ----- names -----

    /// The `Name` of a renamed reference: a local bound in the quotation is `mkName`, a global
    /// is `mkNameG_v`/`_d`/`_tc` of its package and module.
    fn name(&self, q: &QName, kind: &str) -> Expr {
        match &q.qual {
            None => app(self.syn("mkName"), self.str(&q.name)),
            Some(module) => {
                let module = if module == "Builtin" { "GHC.Types".to_string() } else { module.clone() };
                let pkg = package_of_module(&module, &self.rn.module);
                let f = app(app(self.syn(kind), self.str(&pkg)), self.str(&module));
                app(f, self.str(&q.name))
            }
        }
    }

    fn binder(&self, n: &str) -> Expr {
        app(self.syn("mkName"), self.str(n))
    }

    // ----- expressions -----

    fn exp(&self, e: &Expr) -> RR<Expr> {
        let loc = self.loc;
        Ok(match e {
            Expr::Var(q, _) => match &q.qual {
                // A variable of the enclosing code: its value, lifted.
                None if !self.bound.contains(&q.name) => {
                    app(Expr::Var(QName { qual: Some("GHC.Internal.TH.Lift".into()), name: "lift".into() }, loc), e.clone())
                }
                _ => app(self.lib("varE"), self.name(q, "mkNameG_v")),
            },
            Expr::Con(q, _) => app(self.lib("conE"), self.name(q, "mkNameG_d")),
            Expr::Lit(l, _) => app(self.lib("litE"), self.lit(l)?),
            Expr::App(f, a) => app(app(self.lib("appE"), self.exp(f)?), self.exp(a)?),
            Expr::TyApp(f, t, _) => app(app(self.lib("appTypeE"), self.exp(f)?), self.ty(t)?),
            Expr::Lam(ps, body, _) => {
                let ps = ps.iter().map(|p| self.pat(p)).collect::<RR<Vec<_>>>()?;
                app(app(self.lib("lamE"), self.list(ps)), self.exp(body)?)
            }
            Expr::Let(ds, body, _) => app(app(self.lib("letE"), self.list(self.decs(ds)?)), self.exp(body)?),
            Expr::If(c, t, f, _) => app(app(app(self.lib("condE"), self.exp(c)?), self.exp(t)?), self.exp(f)?),
            Expr::MultiIf(alts, _) => {
                let gs = alts.iter().map(|(g, e)| self.guarded(g, e)).collect::<RR<Vec<_>>>()?;
                app(self.lib("multiIfE"), self.list(gs))
            }
            Expr::Case(s, alts, _) => {
                let ms = alts.iter().map(|a| self.alt(a)).collect::<RR<Vec<_>>>()?;
                app(app(self.lib("caseE"), self.exp(s)?), self.list(ms))
            }
            Expr::LamCase(alts, _) => {
                let ms = alts.iter().map(|a| self.alt(a)).collect::<RR<Vec<_>>>()?;
                app(self.lib("lamCaseE"), self.list(ms))
            }
            Expr::Do(ss, _) => app(app(self.lib("doE"), self.nothing()), self.list(self.stmts(ss)?)),
            Expr::Tuple(xs, _) => {
                let items = xs
                    .iter()
                    .map(|x| match x {
                        Some(x) => Ok(self.just(self.exp(x)?)),
                        None => Ok(self.nothing()),
                    })
                    .collect::<RR<Vec<_>>>()?;
                app(self.lib("tupE"), self.list(items))
            }
            Expr::List(xs, _) => app(self.lib("listE"), self.list(xs.iter().map(|x| self.exp(x)).collect::<RR<_>>()?)),
            Expr::ArithSeq { from, then, to, .. } => {
                let range = match (then, to) {
                    (None, None) => app(self.lib("fromR"), self.exp(from)?),
                    (Some(t), None) => app(app(self.lib("fromThenR"), self.exp(from)?), self.exp(t)?),
                    (None, Some(u)) => app(app(self.lib("fromToR"), self.exp(from)?), self.exp(u)?),
                    (Some(t), Some(u)) => app(app(app(self.lib("fromThenToR"), self.exp(from)?), self.exp(t)?), self.exp(u)?),
                };
                app(self.lib("arithSeqE"), range)
            }
            Expr::ListComp(body, branches, _) if branches.len() == 1 => {
                let mut ss = self.stmts(&branches[0])?;
                ss.push(app(self.lib("noBindS"), self.exp(body)?));
                app(self.lib("compE"), self.list(ss))
            }
            Expr::LeftSection(x, op, _) => {
                app(app(app(self.lib("infixE"), self.just(self.exp(x)?)), self.op(op)), self.nothing())
            }
            Expr::RightSection(op, x, _) => {
                app(app(app(self.lib("infixE"), self.nothing()), self.op(op)), self.just(self.exp(x)?))
            }
            Expr::RecCon { con, fields, .. } => {
                let fs = fields
                    .iter()
                    .map(|(f, v)| {
                        let v = match v {
                            Some(v) => self.exp(v)?,
                            None => self.exp(&Expr::Var(QName::unqual(f.name.clone()), loc))?,
                        };
                        Ok(app(app(self.lib("fieldExp"), self.name(f, "mkNameG_v")), v))
                    })
                    .collect::<RR<Vec<_>>>()?;
                app(app(self.lib("recConE"), self.name(con, "mkNameG_d")), self.list(fs))
            }
            Expr::RecUpd(r, fields, _) => {
                let fs = fields
                    .iter()
                    .map(|(f, v)| {
                        let v = match v {
                            Some(v) => self.exp(v)?,
                            None => self.exp(&Expr::Var(QName::unqual(f.name.clone()), loc))?,
                        };
                        Ok(app(app(self.lib("fieldExp"), self.name(f, "mkNameG_v")), v))
                    })
                    .collect::<RR<Vec<_>>>()?;
                app(app(self.lib("recUpdE"), self.exp(r)?), self.list(fs))
            }
            Expr::Sig(x, t, _) => app(app(self.lib("sigE"), self.exp(x)?), self.ty(t)?),
            // A splice in a quotation is the code it runs.
            // (A typed splice's code is a `Code m t`: its syntax is `unTypeCode`'s.)
            Expr::Splice(true, x, _) => app(self.syn("unTypeCode"), (**x).clone()),
            Expr::Splice(false, x, _) => (**x).clone(),
            Expr::IPVar(n, _) => app(self.lib("implicitParamVarE"), self.str(n)),
            Expr::OpChain(..) => return self.err("an unresolved operator"),
            _ => return self.err("this expression"),
        })
    }

    fn op(&self, q: &QName) -> Expr {
        if q.name.starts_with(':') || q.name == "[]" {
            app(self.lib("conE"), self.name(q, "mkNameG_d"))
        } else if q.qual.is_none() && !self.bound.contains(&q.name) {
            app(Expr::Var(QName { qual: Some("GHC.Internal.TH.Lift".into()), name: "lift".into() }, self.loc), Expr::Var(q.clone(), self.loc))
        } else {
            app(self.lib("varE"), self.name(q, "mkNameG_v"))
        }
    }

    fn lit(&self, l: &Lit) -> RR<Expr> {
        use crate::syntax::token::PrimLit;
        let wrap = |f: &str, x: Expr| app(self.lib(f), x);
        Ok(match l {
            Lit::Int(n) => wrap("integerL", Expr::Lit(Lit::Int(n.clone()), self.loc)),
            Lit::Frac(s) => wrap("rationalL", Expr::Lit(Lit::Frac(s.clone()), self.loc)),
            Lit::Char(c) => wrap("charL", Expr::Lit(Lit::Char(*c), self.loc)),
            Lit::Str(s) => wrap("stringL", self.str(s)),
            Lit::Prim(p) => match p {
                PrimLit::Int(n) => wrap("intPrimL", Expr::Lit(Lit::Int(n.clone()), self.loc)),
                PrimLit::Word(n) => wrap("wordPrimL", Expr::Lit(Lit::Int(n.clone()), self.loc)),
                PrimLit::Float(s) => wrap("floatPrimL", Expr::Lit(Lit::Frac(s.clone()), self.loc)),
                PrimLit::Double(s) => wrap("doublePrimL", Expr::Lit(Lit::Frac(s.clone()), self.loc)),
                PrimLit::Char(c) => wrap("charPrimL", Expr::Lit(Lit::Char(*c), self.loc)),
                PrimLit::Str(s) => {
                    let bytes = s.chars().map(|c| Expr::Lit(Lit::Int((c as u32 & 0xff).to_string()), self.loc)).collect();
                    wrap("stringPrimL", self.list(bytes))
                }
            },
        })
    }

    fn alt(&self, a: &Alt) -> RR<Expr> {
        let (body, wheres) = self.rhs(&a.rhs)?;
        Ok(app(app(app(self.lib("match"), self.pat(&a.pat)?), body), self.list(wheres)))
    }

    fn rhs(&self, r: &Rhs) -> RR<(Expr, Vec<Expr>)> {
        let body = match &r.body {
            Body::Plain(e) => app(self.lib("normalB"), self.exp(e)?),
            Body::Guarded(alts) => {
                let gs = alts.iter().map(|(g, e)| self.guarded(g, e)).collect::<RR<Vec<_>>>()?;
                app(self.lib("guardedB"), self.list(gs))
            }
        };
        Ok((body, self.decs(&r.wheres)?))
    }

    fn guarded(&self, g: &[Stmt], e: &Expr) -> RR<Expr> {
        Ok(match g {
            [Stmt::Expr { expr, .. }] => app(app(self.lib("normalGE"), self.exp(expr)?), self.exp(e)?),
            _ => app(app(self.lib("patGE"), self.list(self.stmts(g)?)), self.exp(e)?),
        })
    }

    fn stmts(&self, ss: &[Stmt]) -> RR<Vec<Expr>> {
        ss.iter()
            .map(|s| {
                Ok(match s {
                    Stmt::Bind { pat, expr, .. } => app(app(self.lib("bindS"), self.pat(pat)?), self.exp(expr)?),
                    Stmt::Let { decls, .. } => app(self.lib("letS"), self.list(self.decs(decls)?)),
                    Stmt::Expr { expr, .. } => app(self.lib("noBindS"), self.exp(expr)?),
                })
            })
            .collect()
    }

    fn decs(&self, ds: &[Decl]) -> RR<Vec<Expr>> {
        let mut out = Vec::new();
        for d in ds {
            match d {
                Decl::Fun { name, clauses, .. } => {
                    let cs = clauses
                        .iter()
                        .map(|c| {
                            let ps = c.pats.iter().map(|p| self.pat(p)).collect::<RR<Vec<_>>>()?;
                            let (body, wheres) = self.rhs(&c.rhs)?;
                            Ok(app(app(app(self.lib("clause"), self.list(ps)), body), self.list(wheres)))
                        })
                        .collect::<RR<Vec<_>>>()?;
                    out.push(app(app(self.lib("funD"), self.binder(name)), self.list(cs)));
                }
                Decl::PatBind { pat, rhs, .. } => {
                    let (body, wheres) = self.rhs(rhs)?;
                    out.push(app(app(app(self.lib("valD"), self.pat(pat)?), body), self.list(wheres)));
                }
                Decl::Sig { names, ty, .. } => {
                    for n in names {
                        out.push(app(app(self.lib("sigD"), self.binder(n)), self.ty(ty)?));
                    }
                }
                Decl::Fixity { .. } => {}
                _ => return self.err("this declaration"),
            }
        }
        Ok(out)
    }

    // ----- patterns -----

    fn pat(&self, p: &Pat) -> RR<Expr> {
        Ok(match p {
            Pat::Var(v, _) => app(self.lib("varP"), self.binder(v)),
            Pat::Wild(_) => self.lib("wildP"),
            Pat::Lit(l, _) => app(self.lib("litP"), self.lit(l)?),
            Pat::NegLit(Lit::Int(n), _) => app(self.lib("litP"), app(self.lib("integerL"), Expr::Lit(Lit::Int(format!("-{n}")), self.loc))),
            Pat::NegLit(Lit::Frac(s), _) => app(self.lib("litP"), app(self.lib("rationalL"), Expr::Lit(Lit::Frac(format!("-{s}")), self.loc))),
            Pat::Con(q, ps, _) => {
                let ps = ps.iter().map(|x| self.pat(x)).collect::<RR<Vec<_>>>()?;
                app(app(app(self.lib("conP"), self.name(q, "mkNameG_d")), self.list(Vec::new())), self.list(ps))
            }
            Pat::Rec { con, fields, .. } => {
                let fs = fields
                    .iter()
                    .map(|(f, p)| {
                        let p = match p {
                            Some(p) => self.pat(p)?,
                            None => app(self.lib("varP"), self.binder(&f.name)),
                        };
                        Ok(app(app(self.lib("fieldPat"), self.name(f, "mkNameG_v")), p))
                    })
                    .collect::<RR<Vec<_>>>()?;
                app(app(self.lib("recP"), self.name(con, "mkNameG_d")), self.list(fs))
            }
            Pat::Tuple(ps, _) => app(self.lib("tupP"), self.list(ps.iter().map(|x| self.pat(x)).collect::<RR<_>>()?)),
            Pat::List(ps, _) => app(self.lib("listP"), self.list(ps.iter().map(|x| self.pat(x)).collect::<RR<_>>()?)),
            Pat::As(v, x, _) => app(app(self.lib("asP"), self.binder(v)), self.pat(x)?),
            Pat::Lazy(x, _) => app(self.lib("tildeP"), self.pat(x)?),
            Pat::Bang(x, _) => app(self.lib("bangP"), self.pat(x)?),
            Pat::Sig(x, t, _) => app(app(self.lib("sigP"), self.pat(x)?), self.ty(t)?),
            Pat::View(e, x, _) => app(app(self.lib("viewP"), self.exp(e)?), self.pat(x)?),
            _ => return self.err("this pattern"),
        })
    }

    // ----- types -----

    fn ty(&self, t: &Type) -> RR<Expr> {
        Ok(match t {
            Type::Var(v, _) => app(self.lib("varT"), self.binder(v)),
            Type::Con(q, _) => match (q.qual.as_deref(), q.name.as_str()) {
                (_, "->") => self.lib("arrowT"),
                (_, "[]") => self.lib("listT"),
                (_, "()") | (Some("Builtin"), "()") => app(self.lib("tupleT"), Expr::Lit(Lit::Int("0".into()), self.loc)),
                (_, n) if n.starts_with('\'') => {
                    let bare = QName { qual: q.qual.clone(), name: n[1..].to_string() };
                    app(self.lib("promotedT"), self.name(&bare, "mkNameG_d"))
                }
                _ => app(self.lib("conT"), self.name(q, "mkNameG_tc")),
            },
            Type::App(f, a) => app(app(self.lib("appT"), self.ty(f)?), self.ty(a)?),
            Type::Fun(a, b) => app(app(self.lib("appT"), app(app(self.lib("appT"), self.lib("arrowT")), self.ty(a)?)), self.ty(b)?),
            Type::List(e, _) => app(app(self.lib("appT"), self.lib("listT")), self.ty(e)?),
            Type::Tuple(ts, _) => {
                let mut acc = app(self.lib("tupleT"), Expr::Lit(Lit::Int(ts.len().to_string()), self.loc));
                for x in ts {
                    acc = app(app(self.lib("appT"), acc), self.ty(x)?);
                }
                acc
            }
            Type::Forall(binders, body) => {
                let tvs = binders
                    .iter()
                    .map(|b| {
                        let spec = Expr::Con(QName { qual: Some(SYN.into()), name: "SpecifiedSpec".into() }, self.loc);
                        let tv = app(app(Expr::Con(QName { qual: Some(SYN.into()), name: "PlainTV".into() }, self.loc), self.binder(&b.name)), spec);
                        self.pure(tv)
                    })
                    .collect();
                let (cxt, inner) = match body.as_ref() {
                    Type::Qual(c, b) => (c.iter().map(|x| self.ty(x)).collect::<RR<Vec<_>>>()?, b.as_ref()),
                    other => (Vec::new(), other),
                };
                app(app(app(self.lib("forallT"), self.list(tvs)), app(self.lib("cxt"), self.list(cxt))), self.ty(inner)?)
            }
            Type::Qual(c, b) => {
                let cxt = c.iter().map(|x| self.ty(x)).collect::<RR<Vec<_>>>()?;
                app(app(app(self.lib("forallT"), self.list(Vec::new())), app(self.lib("cxt"), self.list(cxt))), self.ty(b)?)
            }
            Type::Sig(x, k) => app(app(self.lib("sigT"), self.ty(x)?), self.ty(k)?),
            Type::Lit(Lit::Int(n), _) => app(self.lib("litT"), app(self.lib("numTyLit"), Expr::Lit(Lit::Int(n.clone()), self.loc))),
            Type::Lit(Lit::Str(s), _) => app(self.lib("litT"), app(self.lib("strTyLit"), self.str(s))),
            _ => return self.err("this type"),
        })
    }
}

/// The variables a quoted expression binds anywhere in it (renamed, so each is unique).
fn bound_in_expr(e: &Expr, out: &mut HashSet<String>) {
    let mut found: Vec<String> = Vec::new();
    expr_binders(e, &mut found);
    out.extend(found);
}

fn collect_decls(ds: &mut [Decl], out: &mut Vec<String>) {
    for d in ds.iter() {
        match d {
            Decl::Fun { name, clauses, .. } => {
                out.push(name.clone());
                for c in clauses {
                    c.pats.iter().for_each(|p| pat_binders(p, out));
                    rhs_binders(&c.rhs, out);
                }
            }
            Decl::PatBind { pat, rhs, .. } => {
                pat_binders(pat, out);
                rhs_binders(rhs, out);
            }
            _ => {}
        }
    }
}

fn rhs_binders(r: &Rhs, out: &mut Vec<String>) {
    let mut wheres = r.wheres.clone();
    collect_decls(&mut wheres, out);
    match &r.body {
        Body::Plain(e) => expr_binders(e, out),
        Body::Guarded(alts) => {
            for (g, e) in alts {
                stmts_binders(g, out);
                expr_binders(e, out);
            }
        }
    }
}

fn stmts_binders(ss: &[Stmt], out: &mut Vec<String>) {
    for s in ss {
        match s {
            Stmt::Bind { pat, expr, .. } => {
                pat_binders(pat, out);
                expr_binders(expr, out);
            }
            Stmt::Let { decls, .. } => collect_decls(&mut decls.clone(), out),
            Stmt::Expr { expr, .. } => expr_binders(expr, out),
        }
    }
}

fn expr_binders(e: &Expr, out: &mut Vec<String>) {
    match e {
        Expr::Lam(ps, b, _) => {
            ps.iter().for_each(|p| pat_binders(p, out));
            expr_binders(b, out);
        }
        Expr::Let(ds, b, _) => {
            collect_decls(&mut ds.clone(), out);
            expr_binders(b, out);
        }
        Expr::Case(s, alts, _) => {
            expr_binders(s, out);
            for a in alts {
                pat_binders(&a.pat, out);
                rhs_binders(&a.rhs, out);
            }
        }
        Expr::LamCase(alts, _) => {
            for a in alts {
                pat_binders(&a.pat, out);
                rhs_binders(&a.rhs, out);
            }
        }
        Expr::Do(ss, _) => stmts_binders(ss, out),
        Expr::ListComp(b, branches, _) => {
            expr_binders(b, out);
            branches.iter().for_each(|ss| stmts_binders(ss, out));
        }
        Expr::MultiIf(alts, _) => {
            for (g, x) in alts {
                stmts_binders(g, out);
                expr_binders(x, out);
            }
        }
        Expr::App(a, b) => {
            expr_binders(a, out);
            expr_binders(b, out);
        }
        Expr::If(a, b, c, _) => {
            expr_binders(a, out);
            expr_binders(b, out);
            expr_binders(c, out);
        }
        Expr::Tuple(xs, _) => xs.iter().flatten().for_each(|x| expr_binders(x, out)),
        Expr::List(xs, _) => xs.iter().for_each(|x| expr_binders(x, out)),
        Expr::Sig(x, _, _) | Expr::TyApp(x, _, _) | Expr::LeftSection(x, _, _) | Expr::RightSection(_, x, _) => expr_binders(x, out),
        _ => {}
    }
}

fn pat_binders(p: &Pat, out: &mut Vec<String>) {
    match p {
        Pat::Var(v, _) => out.push(v.clone()),
        Pat::As(v, x, _) => {
            out.push(v.clone());
            pat_binders(x, out);
        }
        Pat::Con(_, ps, _) | Pat::Tuple(ps, _) | Pat::List(ps, _) => ps.iter().for_each(|x| pat_binders(x, out)),
        Pat::Rec { fields, .. } => {
            for (f, p) in fields {
                match p {
                    Some(p) => pat_binders(p, out),
                    None => out.push(f.name.clone()),
                }
            }
        }
        Pat::Lazy(x, _) | Pat::Bang(x, _) | Pat::Sig(x, _, _) | Pat::View(_, x, _) => pat_binders(x, out),
        _ => {}
    }
}
