//! Template Haskell splices: the splices of a module are run before the module is checked,
//! and what they produce (Template Haskell syntax, as values) becomes source syntax.
//!
//! A splice may only use what the module imports (GHC's stage restriction), so the driver
//! compiles the splices of a module as a module of their own, with the same imports, runs them
//! with the interpreter, and puts the syntax they build in their place (see
//! `Driver::expand_splices`). This module finds the splices, builds that module, and converts
//! the results back.

use crate::interp::{Runtime, Value};
use crate::syntax::ast::*;
use crate::syntax::token::PrimLit;

/// A splice taken out of a module: what it runs, as an expression of type `Q Exp`.
pub struct Splice {
    pub body: Expr,
    pub loc: Loc,
}

/// The name the placeholder of the i-th splice has until the splice has run.
fn placeholder(i: usize) -> String {
    format!("hsc'splice{i}")
}

/// A qualified name in the Template Haskell library, by module (a `%G:` name, which the renamer
/// resolves directly, whatever the module imports).
fn th_var(module: &str, name: &str, loc: Loc) -> Expr {
    Expr::Var(QName { qual: Some(format!("%G:{module}")), name: name.to_string() }, loc)
}

fn app(f: Expr, a: Expr) -> Expr {
    Expr::App(Box::new(f), Box::new(a))
}

/// Takes the splices out of a module (those not inside a quotation, which belong to the
/// quotation), leaving placeholders; quasi-quotations are splices of their quoter.
pub fn take_splices(m: &mut Module) -> Vec<Splice> {
    let mut out = Vec::new();
    walk_decls(&mut m.decls, &mut |e: &mut Expr| take_one(e, &mut out));
    out
}

fn take_one(e: &mut Expr, out: &mut Vec<Splice>) -> bool {
    match e {
        // A quotation's splices are part of it.
        Expr::Quote(..) => true,
        Expr::Splice(typed, body, loc) => {
            let loc = *loc;
            let inner = std::mem::replace(body.as_mut(), Expr::Wild(loc));
            // `$$e` with `e :: Code Q t`: its expression is `unTypeCode e`.
            let inner = if *typed { app(th_var("GHC.Internal.TH.Syntax", "unTypeCode", loc), inner) } else { inner };
            out.push(Splice { body: inner, loc });
            *e = Expr::Var(QName::unqual(placeholder(out.len() - 1)), loc);
            true
        }
        // A quasi-quotation, as the parser leaves it: `hsc'quasiquote quoter "text"`.
        Expr::App(f, text) => {
            let Expr::App(g, quoter) = f.as_ref() else { return false };
            let Expr::Var(q, loc) = g.as_ref() else { return false };
            if q.name != QUASI_QUOTE || q.qual.is_some() {
                return false;
            }
            let loc = *loc;
            let quote_exp = th_var("GHC.Internal.TH.Quote", "quoteExp", loc);
            let body = app(app(quote_exp, (**quoter).clone()), (**text).clone());
            out.push(Splice { body, loc });
            *e = Expr::Var(QName::unqual(placeholder(out.len() - 1)), loc);
            true
        }
        _ => false,
    }
}

/// The function a quasi-quotation `[q|text|]` is parsed as an application of.
pub const QUASI_QUOTE: &str = "hsc'quasiquote";

/// Puts the expressions the splices produced in place of their placeholders.
pub fn fill_splices(m: &mut Module, results: &[Expr]) {
    walk_decls(&mut m.decls, &mut |e: &mut Expr| {
        if let Expr::Var(q, _) = e {
            if q.qual.is_none() {
                if let Some(i) = q.name.strip_prefix("hsc'splice").and_then(|n| n.parse::<usize>().ok()) {
                    if let Some(r) = results.get(i) {
                        *e = r.clone();
                    }
                    return true;
                }
            }
        }
        false
    });
}

/// The module the splices of `m` are compiled in: `m`'s imports, and one binding per splice
/// that runs it (`unsafePerformIO (runQ splice)`, an `Exp`).
pub fn splice_module(m: &Module, splices: &[Splice], name: &str) -> Module {
    let mut decls = Vec::new();
    for (i, s) in splices.iter().enumerate() {
        let loc = s.loc;
        let run = app(
            th_var("GHC.Internal.IO.Unsafe", "unsafePerformIO", loc),
            app(th_var("GHC.Internal.TH.Syntax", "runQ", loc), s.body.clone()),
        );
        let typed = Expr::Sig(
            Box::new(run),
            Type::Con(QName { qual: Some("%G:GHC.Internal.TH.Syntax".into()), name: "Exp".into() }, loc),
            loc,
        );
        decls.push(Decl::Fun {
            name: placeholder(i),
            clauses: vec![Clause {
                pats: Vec::new(),
                rhs: Rhs { body: Body::Plain(typed), wheres: Vec::new() },
                loc,
                end: loc,
            }],
            loc,
        });
    }
    Module { name: name.to_string(), exports: None, imports: m.imports.clone(), decls, loc: m.loc }
}

// ----- walking the expressions of a module -----------------------------------------------

/// Calls `f` on every expression of the declarations, outermost first; `f` answers whether it
/// has dealt with the expression (its parts are then not visited).
pub fn walk_decls(ds: &mut [Decl], f: &mut dyn FnMut(&mut Expr) -> bool) {
    for d in ds {
        walk_decl(d, f);
    }
}

fn walk_decl(d: &mut Decl, f: &mut dyn FnMut(&mut Expr) -> bool) {
    match d {
        Decl::Fun { clauses, .. } => {
            for c in clauses {
                walk_rhs(&mut c.rhs, f);
            }
        }
        Decl::PatBind { rhs, .. } => walk_rhs(rhs, f),
        Decl::IPBind { expr, .. } => walk_expr(expr, f),
        Decl::Instance(id) => walk_decls(&mut id.decls, f),
        Decl::Class(cd) => walk_decls(&mut cd.decls, f),
        Decl::Many(ds) => walk_decls(ds, f),
        _ => {}
    }
}

fn walk_rhs(r: &mut Rhs, f: &mut dyn FnMut(&mut Expr) -> bool) {
    match &mut r.body {
        Body::Plain(e) => walk_expr(e, f),
        Body::Guarded(alts) => {
            for (gs, e) in alts {
                walk_stmts(gs, f);
                walk_expr(e, f);
            }
        }
    }
    walk_decls(&mut r.wheres, f);
}

fn walk_stmts(ss: &mut [Stmt], f: &mut dyn FnMut(&mut Expr) -> bool) {
    for s in ss {
        match s {
            Stmt::Bind { expr, .. } | Stmt::Expr { expr, .. } => walk_expr(expr, f),
            Stmt::Let { decls, .. } => walk_decls(decls, f),
        }
    }
}

fn walk_expr(e: &mut Expr, f: &mut dyn FnMut(&mut Expr) -> bool) {
    if f(e) {
        return;
    }
    match e {
        Expr::App(a, b) | Expr::ViewPat(a, b, _) => {
            walk_expr(a, f);
            walk_expr(b, f);
        }
        Expr::TyApp(a, _, _) | Expr::Sig(a, _, _) | Expr::AsPat(_, a, _) | Expr::LazyPat(a, _) | Expr::BangPat(a, _) => {
            walk_expr(a, f)
        }
        Expr::OpChain(items, _) => {
            for it in items {
                if let OpItem::Operand(x) = it {
                    walk_expr(x, f);
                }
            }
        }
        Expr::Lam(_, body, _) => walk_expr(body, f),
        Expr::Let(ds, body, _) => {
            walk_decls(ds, f);
            walk_expr(body, f);
        }
        Expr::If(a, b, c, _) => {
            walk_expr(a, f);
            walk_expr(b, f);
            walk_expr(c, f);
        }
        Expr::MultiIf(alts, _) => {
            for (gs, x) in alts {
                walk_stmts(gs, f);
                walk_expr(x, f);
            }
        }
        Expr::Case(s, alts, _) => {
            walk_expr(s, f);
            for a in alts {
                walk_rhs(&mut a.rhs, f);
            }
        }
        Expr::LamCase(alts, _) => {
            for a in alts {
                walk_rhs(&mut a.rhs, f);
            }
        }
        Expr::Do(ss, _) => walk_stmts(ss, f),
        Expr::Tuple(xs, _) => {
            for x in xs.iter_mut().flatten() {
                walk_expr(x, f);
            }
        }
        Expr::List(xs, _) => {
            for x in xs {
                walk_expr(x, f);
            }
        }
        Expr::ArithSeq { from, then, to, .. } => {
            walk_expr(from, f);
            if let Some(t) = then {
                walk_expr(t, f);
            }
            if let Some(t) = to {
                walk_expr(t, f);
            }
        }
        Expr::ListComp(body, branches, _) => {
            walk_expr(body, f);
            for b in branches {
                walk_stmts(b, f);
            }
        }
        Expr::LeftSection(a, _, _) | Expr::RightSection(_, a, _) => walk_expr(a, f),
        Expr::RecCon { fields, .. } => {
            for (_, v) in fields {
                if let Some(v) = v {
                    walk_expr(v, f);
                }
            }
        }
        Expr::RecUpd(r, fields, _) => {
            walk_expr(r, f);
            for (_, v) in fields {
                if let Some(v) = v {
                    walk_expr(v, f);
                }
            }
        }
        Expr::Splice(_, x, _) => walk_expr(x, f),
        _ => {}
    }
}

/// Whether a module has splices or quasi-quotations to run.
pub fn has_splices(m: &Module) -> bool {
    let mut m = m.clone();
    let mut found = false;
    walk_decls(&mut m.decls, &mut |e: &mut Expr| {
        let mut sink = Vec::new();
        if take_one(e, &mut sink) && !sink.is_empty() {
            found = true;
        }
        found
    });
    found
}

// ----- running the splices ---------------------------------------------------------------

/// Runs the splice bindings of a compiled program and converts what each produced (a Template
/// Haskell `Exp`) to source syntax. Runs on a thread of its own with a large stack, as the
/// interpreter needs.
pub fn run_splices(binds: Vec<crate::core::TopBind>, module: &str, count: usize, locs: Vec<Loc>) -> Result<Vec<Expr>, String> {
    let module = module.to_string();
    // Only what the splices reach is compiled for the interpreter, with the bindings the runtime
    // itself looks up.
    let names: Vec<String> = (0..count).map(|i| format!("{module}.{}", placeholder(i))).collect();
    let mut roots: Vec<&str> = names.iter().map(String::as_str).collect();
    roots.extend_from_slice(crate::native::RTS_GLOBALS);
    roots.extend_from_slice(&["GHC.Internal.Exception.Type.displayException", "GHC.Internal.TopHandler.runMainIO"]);
    let binds = crate::native::reachable(binds, &roots);
    let handle = std::thread::Builder::new()
        .stack_size(4 << 30)
        .spawn(move || -> Result<Vec<Expr>, String> {
            crate::interp::conc::set_stack_base();
            let t0 = std::time::Instant::now();
            let rt = Runtime::new(binds, "hsc-splice".into());
            if std::env::var_os("HSC_TIME_SPLICES").is_some() {
                eprintln!("splice runtime: {:?}", t0.elapsed());
            }
            crate::interp::conc::set_runtime(&rt);
            let mut out = Vec::with_capacity(count);
            for (i, loc) in locs.into_iter().enumerate().take(count) {
                let name = format!("{module}.{}", placeholder(i));
                let cell = rt.global(&name).ok_or_else(|| format!("internal: splice {name} was not compiled"))?;
                let conv = Conv { rt: &rt, loc };
                out.push(conv.exp(&cell)?);
                if std::env::var_os("HSC_TIME_SPLICES").is_some() {
                    eprintln!("splice {i}: {:?}", t0.elapsed());
                }
            }
            Ok(out)
        })
        .map_err(|e| format!("cannot start the splice thread: {e}"))?;
    handle.join().map_err(|_| "a Template Haskell splice crashed".to_string())?
}

/// The conversion of Template Haskell syntax values to source syntax.
struct Conv<'a> {
    rt: &'a Runtime,
    loc: Loc,
}

type CR<T> = Result<T, String>;

impl Conv<'_> {
    fn force(&self, c: &crate::interp::Cell) -> CR<Value> {
        self.rt.force(c).map_err(|e| format!("Exception when trying to run compile-time code:\n  {}", rt_error_text(&e)))
    }

    /// A constructor value: its constructor's name (unqualified) and its fields.
    fn con(&self, c: &crate::interp::Cell) -> CR<(String, Vec<crate::interp::Cell>)> {
        match self.force(c)? {
            Value::Con(cv) => Ok((unqualify(cv.name).to_string(), cv.args.clone())),
            _ => Err("internal: a Template Haskell value is not a constructor".into()),
        }
    }

    fn list(&self, c: &crate::interp::Cell) -> CR<Vec<crate::interp::Cell>> {
        let mut out = Vec::new();
        let mut cur = c.clone();
        loop {
            let (name, args) = self.con(&cur)?;
            match name.as_str() {
                ":" => {
                    out.push(args[0].clone());
                    cur = args[1].clone();
                }
                "[]" => return Ok(out),
                // A NonEmpty (`ProjectionE`), `a :| as`.
                ":|" => {
                    out.push(args[0].clone());
                    cur = args[1].clone();
                }
                other => return Err(format!("internal: expected a list, found {other}")),
            }
        }
    }

    fn string(&self, c: &crate::interp::Cell) -> CR<String> {
        let mut s = String::new();
        for ch in self.list(c)? {
            match self.force(&ch)? {
                Value::Char(u) => s.push(crate::syntax::lexer::char_of_code(u).unwrap_or('\u{fffd}')),
                _ => return Err("internal: expected a character".into()),
            }
        }
        Ok(s)
    }

    fn integer(&self, c: &crate::interp::Cell) -> CR<String> {
        match self.force(c)? {
            Value::Int(i) => Ok(i.to_string()),
            Value::Integer(b) => Ok(b.to_decimal()),
            // `IS i`/`IP`/`IN` boxes of the real bignum.
            Value::Con(cv) => match cv.args.first() {
                Some(a) => self.integer(a),
                None => Err("internal: expected an Integer".into()),
            },
            _ => Err("internal: expected an Integer".into()),
        }
    }

    fn maybe(&self, c: &crate::interp::Cell) -> CR<Option<crate::interp::Cell>> {
        let (name, args) = self.con(c)?;
        Ok(if name == "Just" { Some(args[0].clone()) } else { None })
    }

    // ----- names -----

    /// A Template Haskell `Name`, as a source name. A name bound by `newName` gets its unique
    /// number; a global name (`NameG`) names its module directly.
    fn name(&self, c: &crate::interp::Cell) -> CR<QName> {
        let (_, args) = self.con(c)?;
        let occ = self.string(&args[0])?;
        let (flavour, fargs) = self.con(&args[1])?;
        Ok(match flavour.as_str() {
            "NameS" => match occ.rfind('.') {
                // `mkName "M.x"`: qualified as written.
                Some(i) if i > 0 && occ[..i].chars().next().is_some_and(char::is_uppercase) && i + 1 < occ.len() => {
                    QName { qual: Some(occ[..i].to_string()), name: occ[i + 1..].to_string() }
                }
                _ => QName::unqual(occ),
            },
            "NameQ" => QName { qual: Some(self.string(&fargs[0])?), name: occ },
            "NameU" | "NameL" => QName::unqual(format!("{occ}%th{}", self.integer(&fargs[0])?)),
            "NameG" => {
                let module = self.string(&fargs[2])?;
                if builtin_syntax(&occ) {
                    QName::unqual(occ)
                } else {
                    QName { qual: Some(format!("%G:{module}")), name: occ }
                }
            }
            other => return Err(format!("internal: unknown name flavour {other}")),
        })
    }

    /// A name bound by a pattern (a local variable).
    fn binder(&self, c: &crate::interp::Cell) -> CR<String> {
        Ok(self.name(c)?.name)
    }

    // ----- expressions -----

    fn exp(&self, c: &crate::interp::Cell) -> CR<Expr> {
        let loc = self.loc;
        let (k, a) = self.con(c)?;
        Ok(match k.as_str() {
            "VarE" | "UnboundVarE" => Expr::Var(self.name(&a[0])?, loc),
            "ConE" => {
                let q = self.name(&a[0])?;
                Expr::Con(q, loc)
            }
            "LitE" => self.lit_exp(&a[0])?,
            "AppE" => app(self.exp(&a[0])?, self.exp(&a[1])?),
            "AppTypeE" => Expr::TyApp(Box::new(self.exp(&a[0])?), self.ty(&a[1])?, loc),
            "InfixE" => {
                let op = self.exp(&a[1])?;
                let l = self.maybe(&a[0])?;
                let r = self.maybe(&a[2])?;
                match (l, r) {
                    (Some(l), Some(r)) => app(app(op, self.exp(&l)?), self.exp(&r)?),
                    (Some(l), None) => match op_name(&op) {
                        Some(q) => Expr::LeftSection(Box::new(self.exp(&l)?), q, loc),
                        None => app(op, self.exp(&l)?),
                    },
                    (None, Some(r)) => match op_name(&op) {
                        Some(q) => Expr::RightSection(q, Box::new(self.exp(&r)?), loc),
                        None => {
                            let x = "th%sec".to_string();
                            let body = app(app(op, Expr::Var(QName::unqual(x.clone()), loc)), self.exp(&r)?);
                            Expr::Lam(vec![Pat::Var(x, loc)], Box::new(body), loc)
                        }
                    },
                    (None, None) => op,
                }
            }
            "UInfixE" => {
                let mut items = Vec::new();
                self.uinfix(c, &mut items)?;
                Expr::OpChain(items, loc)
            }
            "ParensE" => self.exp(&a[0])?,
            "LamE" => {
                let ps = self.list(&a[0])?.iter().map(|p| self.pat(p)).collect::<CR<Vec<_>>>()?;
                Expr::Lam(ps, Box::new(self.exp(&a[1])?), loc)
            }
            "LamCaseE" => Expr::LamCase(self.list(&a[0])?.iter().map(|m| self.alt(m)).collect::<CR<_>>()?, loc),
            "TupE" => {
                let mut xs = Vec::new();
                for x in self.list(&a[0])? {
                    xs.push(match self.maybe(&x)? {
                        Some(e) => Some(self.exp(&e)?),
                        None => None,
                    });
                }
                if xs.len() == 1 {
                    if let Some(Some(e)) = xs.pop() {
                        return Ok(e);
                    }
                    return Err("a one-element tuple section".into());
                }
                Expr::Tuple(xs, loc)
            }
            "CondE" => Expr::If(Box::new(self.exp(&a[0])?), Box::new(self.exp(&a[1])?), Box::new(self.exp(&a[2])?), loc),
            "MultiIfE" => {
                let mut alts = Vec::new();
                for g in self.list(&a[0])? {
                    let (_, ga) = self.con(&g)?;
                    alts.push((self.guard(&ga[0])?, self.exp(&ga[1])?));
                }
                Expr::MultiIf(alts, loc)
            }
            "LetE" => Expr::Let(self.decs(&a[0])?, Box::new(self.exp(&a[1])?), loc),
            "CaseE" => Expr::Case(
                Box::new(self.exp(&a[0])?),
                self.list(&a[1])?.iter().map(|m| self.alt(m)).collect::<CR<_>>()?,
                loc,
            ),
            "DoE" => Expr::Do(self.stmts(&a[1])?, loc),
            "CompE" => {
                let mut ss = self.stmts(&a[0])?;
                let body = match ss.pop() {
                    Some(Stmt::Expr { expr, .. }) => expr,
                    _ => return Err("a comprehension must end in an expression".into()),
                };
                Expr::ListComp(Box::new(body), vec![ss], loc)
            }
            "ArithSeqE" => {
                let (rk, ra) = self.con(&a[0])?;
                let e = |i: usize| -> CR<Box<Expr>> { Ok(Box::new(self.exp(&ra[i])?)) };
                match rk.as_str() {
                    "FromR" => Expr::ArithSeq { from: e(0)?, then: None, to: None, loc },
                    "FromThenR" => Expr::ArithSeq { from: e(0)?, then: Some(e(1)?), to: None, loc },
                    "FromToR" => Expr::ArithSeq { from: e(0)?, then: None, to: Some(e(1)?), loc },
                    _ => Expr::ArithSeq { from: e(0)?, then: Some(e(1)?), to: Some(e(2)?), loc },
                }
            }
            "ListE" => Expr::List(self.list(&a[0])?.iter().map(|x| self.exp(x)).collect::<CR<_>>()?, loc),
            "SigE" => Expr::Sig(Box::new(self.exp(&a[0])?), self.ty(&a[1])?, loc),
            "RecConE" => Expr::RecCon { con: self.name(&a[0])?, fields: self.field_exps(&a[1])?, dotdot: false, loc },
            "RecUpdE" => Expr::RecUpd(Box::new(self.exp(&a[0])?), self.field_exps(&a[1])?, loc),
            "ImplicitParamVarE" => Expr::IPVar(self.string(&a[0])?, loc),
            other => return Err(format!("Template Haskell expression {other} is not supported yet")),
        })
    }

    fn uinfix(&self, c: &crate::interp::Cell, out: &mut Vec<OpItem>) -> CR<()> {
        let (k, a) = self.con(c)?;
        if k != "UInfixE" {
            out.push(OpItem::Operand(self.exp(c)?));
            return Ok(());
        }
        self.uinfix(&a[0], out)?;
        let op = self.exp(&a[1])?;
        match op_name(&op) {
            Some(q) => out.push(OpItem::Op(q, self.loc)),
            None => return Err("an infix operator must be a name".into()),
        }
        self.uinfix(&a[2], out)
    }

    fn field_exps(&self, c: &crate::interp::Cell) -> CR<Vec<(QName, Option<Expr>)>> {
        let mut out = Vec::new();
        for f in self.list(c)? {
            let (_, fa) = self.con(&f)?;
            out.push((self.name(&fa[0])?, Some(self.exp(&fa[1])?)));
        }
        Ok(out)
    }

    fn lit_exp(&self, c: &crate::interp::Cell) -> CR<Expr> {
        let loc = self.loc;
        let (k, a) = self.con(c)?;
        let lit = |l: Lit| Expr::Lit(l, loc);
        Ok(match k.as_str() {
            "IntegerL" => {
                let n = self.integer(&a[0])?;
                match n.strip_prefix('-') {
                    Some(abs) => app(Expr::Var(QName { qual: Some("%Base".into()), name: "negate".into() }, loc), lit(Lit::Int(abs.into()))),
                    None => lit(Lit::Int(n)),
                }
            }
            "RationalL" => {
                let (n, d) = self.ratio(&a[0])?;
                let ratio = app(
                    app(th_var("GHC.Internal.Real", "%", loc), lit_int(&n, loc)),
                    lit_int(&d, loc),
                );
                app(Expr::Var(QName { qual: Some("%Base".into()), name: "fromRational".into() }, loc), ratio)
            }
            _ => lit(self.lit(c)?),
        })
    }

    fn ratio(&self, c: &crate::interp::Cell) -> CR<(String, String)> {
        let (_, a) = self.con(c)?;
        Ok((self.integer(&a[0])?, self.integer(&a[1])?))
    }

    /// A literal of a pattern or of a primitive type.
    fn lit(&self, c: &crate::interp::Cell) -> CR<Lit> {
        let (k, a) = self.con(c)?;
        let ratio_text = |(n, d): (String, String)| -> String {
            let v = n.parse::<f64>().unwrap_or(0.0) / d.parse::<f64>().unwrap_or(1.0);
            format!("{v:?}")
        };
        Ok(match k.as_str() {
            "CharL" => match self.force(&a[0])? {
                Value::Char(u) => Lit::Char(crate::syntax::lexer::char_of_code(u).unwrap_or('\u{fffd}')),
                _ => return Err("internal: expected a character".into()),
            },
            "StringL" => Lit::Str(self.string(&a[0])?),
            "IntegerL" => Lit::Int(self.integer(&a[0])?),
            "RationalL" => Lit::Frac(ratio_text(self.ratio(&a[0])?)),
            "IntPrimL" => Lit::Prim(PrimLit::Int(self.integer(&a[0])?)),
            "WordPrimL" => Lit::Prim(PrimLit::Word(self.integer(&a[0])?)),
            "FloatPrimL" => Lit::Prim(PrimLit::Float(ratio_text(self.ratio(&a[0])?))),
            "DoublePrimL" => Lit::Prim(PrimLit::Double(ratio_text(self.ratio(&a[0])?))),
            "CharPrimL" => match self.force(&a[0])? {
                Value::Char(u) => Lit::Prim(PrimLit::Char(crate::syntax::lexer::char_of_code(u).unwrap_or('\u{fffd}'))),
                _ => return Err("internal: expected a character".into()),
            },
            "StringPrimL" => {
                let mut s = String::new();
                for b in self.list(&a[0])? {
                    let n: u32 = self.integer(&b)?.parse().unwrap_or(0);
                    s.push(char::from_u32(n & 0xff).unwrap_or('\0'));
                }
                Lit::Prim(PrimLit::Str(s))
            }
            other => return Err(format!("Template Haskell literal {other} is not supported yet")),
        })
    }

    fn alt(&self, c: &crate::interp::Cell) -> CR<Alt> {
        let (_, a) = self.con(c)?;
        Ok(Alt { pat: self.pat(&a[0])?, rhs: self.rhs(&a[1], &a[2])?, loc: self.loc })
    }

    fn rhs(&self, body: &crate::interp::Cell, decs: &crate::interp::Cell) -> CR<Rhs> {
        let (k, a) = self.con(body)?;
        let body = match k.as_str() {
            "NormalB" => Body::Plain(self.exp(&a[0])?),
            _ => {
                let mut alts = Vec::new();
                for g in self.list(&a[0])? {
                    let (_, ga) = self.con(&g)?;
                    alts.push((self.guard(&ga[0])?, self.exp(&ga[1])?));
                }
                Body::Guarded(alts)
            }
        };
        Ok(Rhs { body, wheres: self.decs(decs)? })
    }

    fn guard(&self, c: &crate::interp::Cell) -> CR<Vec<Stmt>> {
        let (k, a) = self.con(c)?;
        Ok(match k.as_str() {
            "NormalG" => vec![Stmt::Expr { expr: self.exp(&a[0])?, loc: self.loc }],
            _ => self.stmts(&a[0])?,
        })
    }

    fn stmts(&self, c: &crate::interp::Cell) -> CR<Vec<Stmt>> {
        let loc = self.loc;
        let mut out = Vec::new();
        for s in self.list(c)? {
            let (k, a) = self.con(&s)?;
            out.push(match k.as_str() {
                "BindS" => Stmt::Bind { pat: self.pat(&a[0])?, expr: self.exp(&a[1])?, loc },
                "LetS" => Stmt::Let { decls: self.decs(&a[0])?, loc },
                "NoBindS" => Stmt::Expr { expr: self.exp(&a[0])?, loc },
                other => return Err(format!("Template Haskell statement {other} is not supported yet")),
            });
        }
        Ok(out)
    }

    // ----- declarations (of `let` and `where`) -----

    fn decs(&self, c: &crate::interp::Cell) -> CR<Vec<Decl>> {
        let loc = self.loc;
        let mut out = Vec::new();
        for d in self.list(c)? {
            let (k, a) = self.con(&d)?;
            match k.as_str() {
                "FunD" => {
                    let name = self.binder(&a[0])?;
                    let mut clauses = Vec::new();
                    for cl in self.list(&a[1])? {
                        let (_, ca) = self.con(&cl)?;
                        let pats = self.list(&ca[0])?.iter().map(|p| self.pat(p)).collect::<CR<_>>()?;
                        clauses.push(Clause { pats, rhs: self.rhs(&ca[1], &ca[2])?, loc, end: loc });
                    }
                    out.push(Decl::Fun { name, clauses, loc });
                }
                "ValD" => {
                    let pat = self.pat(&a[0])?;
                    let rhs = self.rhs(&a[1], &a[2])?;
                    match pat {
                        Pat::Var(name, _) => out.push(Decl::Fun { name, clauses: vec![Clause { pats: Vec::new(), rhs, loc, end: loc }], loc }),
                        pat => out.push(Decl::PatBind { pat, rhs, loc }),
                    }
                }
                "SigD" => out.push(Decl::Sig { names: vec![self.binder(&a[0])?], ty: self.ty(&a[1])?, loc }),
                other => return Err(format!("Template Haskell declaration {other} is not supported here yet")),
            }
        }
        Ok(out)
    }

    // ----- patterns -----

    fn pat(&self, c: &crate::interp::Cell) -> CR<Pat> {
        let loc = self.loc;
        let (k, a) = self.con(c)?;
        Ok(match k.as_str() {
            "LitP" => {
                let l = self.lit(&a[0])?;
                match &l {
                    Lit::Int(n) if n.starts_with('-') => Pat::NegLit(Lit::Int(n[1..].to_string()), loc),
                    _ => Pat::Lit(l, loc),
                }
            }
            "VarP" => Pat::Var(self.binder(&a[0])?, loc),
            "TupP" => {
                let mut ps = self.list(&a[0])?.iter().map(|p| self.pat(p)).collect::<CR<Vec<_>>>()?;
                if ps.len() == 1 {
                    ps.pop().expect("one")
                } else {
                    Pat::Tuple(ps, loc)
                }
            }
            "ConP" => Pat::Con(self.name(&a[0])?, self.list(&a[2])?.iter().map(|p| self.pat(p)).collect::<CR<_>>()?, loc),
            "InfixP" | "UInfixP" => Pat::Con(self.name(&a[1])?, vec![self.pat(&a[0])?, self.pat(&a[2])?], loc),
            "ParensP" => self.pat(&a[0])?,
            "TildeP" => Pat::Lazy(Box::new(self.pat(&a[0])?), loc),
            "BangP" => Pat::Bang(Box::new(self.pat(&a[0])?), loc),
            "AsP" => Pat::As(self.binder(&a[0])?, Box::new(self.pat(&a[1])?), loc),
            "WildP" => Pat::Wild(loc),
            "RecP" => {
                let mut fields = Vec::new();
                for f in self.list(&a[1])? {
                    let (_, fa) = self.con(&f)?;
                    fields.push((self.name(&fa[0])?, Some(self.pat(&fa[1])?)));
                }
                Pat::Rec { con: self.name(&a[0])?, fields, dotdot: false, loc }
            }
            "ListP" => Pat::List(self.list(&a[0])?.iter().map(|p| self.pat(p)).collect::<CR<_>>()?, loc),
            "SigP" => Pat::Sig(Box::new(self.pat(&a[0])?), self.ty(&a[1])?, loc),
            "ViewP" => Pat::View(Box::new(self.exp(&a[0])?), Box::new(self.pat(&a[1])?), loc),
            other => return Err(format!("Template Haskell pattern {other} is not supported yet")),
        })
    }

    // ----- types -----

    fn ty(&self, c: &crate::interp::Cell) -> CR<Type> {
        let loc = self.loc;
        // An application spine: `ArrowT a b`, `ListT a` and `TupleT n` applied are special.
        let mut args = Vec::new();
        let mut head = c.clone();
        loop {
            let (k, a) = self.con(&head)?;
            if k == "AppT" {
                args.push(a[1].clone());
                head = a[0].clone();
            } else {
                break;
            }
        }
        args.reverse();
        let (k, a) = self.con(&head)?;
        let conv_args = |this: &Self| -> CR<Vec<Type>> { args.iter().map(|x| this.ty(x)).collect() };
        let applied = |this: &Self, h: Type| -> CR<Type> {
            Ok(conv_args(this)?.into_iter().fold(h, |f, x| Type::App(Box::new(f), Box::new(x))))
        };
        match k.as_str() {
            "ArrowT" if args.len() == 2 => {
                let mut xs = conv_args(self)?;
                let r = xs.pop().expect("two");
                let d = xs.pop().expect("two");
                return Ok(Type::Fun(Box::new(d), Box::new(r)));
            }
            "ListT" if args.len() == 1 => return Ok(Type::List(Box::new(conv_args(self)?.remove(0)), loc)),
            "TupleT" => {
                let n: usize = self.integer(&a[0])?.parse().unwrap_or(0);
                if n == args.len() && n != 1 {
                    return Ok(if n == 0 { Type::Con(QName::unqual("()"), loc) } else { Type::Tuple(conv_args(self)?, loc) });
                }
            }
            _ => {}
        }
        if !args.is_empty() {
            let h = self.ty(&head)?;
            return applied(self, h);
        }
        Ok(match k.as_str() {
            "ForallT" => {
                let mut binders = Vec::new();
                for b in self.list(&a[0])? {
                    let (bk, ba) = self.con(&b)?;
                    let name = self.binder(&ba[0])?;
                    let kind = if bk == "KindedTV" { Some(self.ty(&ba[2])?) } else { None };
                    binders.push(TyVarBind { name, kind });
                }
                let cxt = self.list(&a[1])?.iter().map(|t| self.ty(t)).collect::<CR<Vec<_>>>()?;
                let body = self.ty(&a[2])?;
                let body = if cxt.is_empty() { body } else { Type::Qual(cxt, Box::new(body)) };
                if binders.is_empty() {
                    body
                } else {
                    Type::Forall(binders, Box::new(body))
                }
            }
            "SigT" => Type::Sig(Box::new(self.ty(&a[0])?), Box::new(self.ty(&a[1])?)),
            "VarT" => Type::Var(self.binder(&a[0])?, loc),
            "ConT" => Type::Con(self.name(&a[0])?, loc),
            "PromotedT" => {
                let q = self.name(&a[0])?;
                Type::Con(QName { qual: q.qual, name: format!("'{}", q.name) }, loc)
            }
            "InfixT" | "UInfixT" => {
                let op = self.name(&a[1])?;
                Type::App(Box::new(Type::App(Box::new(Type::Con(op, loc)), Box::new(self.ty(&a[0])?))), Box::new(self.ty(&a[2])?))
            }
            "ParensT" => self.ty(&a[0])?,
            "TupleT" => {
                let n: usize = self.integer(&a[0])?.parse().unwrap_or(0);
                Type::Con(QName::unqual(if n == 0 { "()".to_string() } else { format!("({})", ",".repeat(n - 1)) }), loc)
            }
            "ArrowT" => Type::Con(QName::unqual("->"), loc),
            "ListT" => Type::Con(QName::unqual("[]"), loc),
            "EqualityT" => Type::Con(QName::unqual("~"), loc),
            "StarT" => Type::Con(QName { qual: Some("%G:GHC.Types".into()), name: "Type".into() }, loc),
            "ConstraintT" => Type::Con(QName { qual: Some("%G:GHC.Types".into()), name: "Constraint".into() }, loc),
            "PromotedNilT" => Type::Con(QName::unqual("'[]"), loc),
            "PromotedConsT" => Type::Con(QName::unqual("':"), loc),
            "LitT" => {
                let (lk, la) = self.con(&a[0])?;
                match lk.as_str() {
                    "NumTyLit" => Type::Lit(Lit::Int(self.integer(&la[0])?), loc),
                    "StrTyLit" => Type::Lit(Lit::Str(self.string(&la[0])?), loc),
                    _ => match self.force(&la[0])? {
                        Value::Char(u) => Type::Lit(Lit::Char(char::from_u32(u).unwrap_or('\u{fffd}')), loc),
                        _ => return Err("internal: expected a character".into()),
                    },
                }
            }
            "WildCardT" => Type::Wild(loc),
            other => return Err(format!("Template Haskell type {other} is not supported yet")),
        })
    }
}

fn lit_int(n: &str, loc: Loc) -> Expr {
    match n.strip_prefix('-') {
        Some(abs) => app(Expr::Var(QName { qual: Some("%Base".into()), name: "negate".into() }, loc), Expr::Lit(Lit::Int(abs.into()), loc)),
        None => Expr::Lit(Lit::Int(n.into()), loc),
    }
}

/// The name of an operator expression (`VarE` or `ConE` of a name).
fn op_name(e: &Expr) -> Option<QName> {
    match e {
        Expr::Var(q, _) | Expr::Con(q, _) => Some(q.clone()),
        _ => None,
    }
}

/// Names of built-in syntax, which no module exports (the list constructors and tuples).
fn builtin_syntax(occ: &str) -> bool {
    matches!(occ, ":" | "[]" | "()") || (occ.starts_with('(') && occ.ends_with(')'))
}

/// The unqualified part of a global name (`GHC.Internal.TH.Syntax.AppE`, `Builtin.:`).
fn unqualify(name: &str) -> &str {
    // The module part ends at the last `.` that follows an identifier character and precedes the
    // name proper (an operator name may contain dots).
    let bytes = name.as_bytes();
    let mut cut = 0;
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'.' && i + 1 < bytes.len() && i > 0 && (bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_' || bytes[i - 1] == b'\'') {
            // A module component starts with an upper-case letter.
            let start = name[..i].rfind('.').map_or(0, |j| j + 1);
            if name[start..i].chars().next().is_some_and(char::is_uppercase) {
                cut = i + 1;
            } else {
                break;
            }
        }
    }
    &name[cut..]
}

fn rt_error_text(e: &crate::interp::RtError) -> String {
    use crate::interp::RtError;
    match e {
        RtError::Fail(s) | RtError::Error(s) | RtError::ErrorLoc(s, _) => s.clone(),
        RtError::Exit(code) => format!("exit {code}"),
        RtError::Thrown(_) => "an exception was thrown".into(),
        #[allow(unreachable_patterns)]
        _ => "an error".into(),
    }
}
