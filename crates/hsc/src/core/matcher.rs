//! Pattern-match compilation: rows of patterns with guarded bodies become nested
//! `Case` and `Let`. Rows are tried in order; each row's failure continuation is a
//! let-bound thunk, which avoids duplicating code.

use super::{Alt, Core, Lit};

/// The Bool constructors, used for guards and predicate tests.
pub const TRUE: &str = "GHC.Types.True";

/// A typed, elaborated pattern.
#[derive(Clone, Debug)]
pub enum CPat {
    Wild,
    Var(String),
    As(String, Box<CPat>),
    Con(String, Vec<CPat>),
    /// A constructor whose fields are matched in the given order (record patterns match
    /// the fields in the order they are written).
    Ordered(String, Vec<CPat>, Vec<usize>),
    /// A predicate function applied to the scrutinee; the match succeeds when it is True.
    /// Used for literal patterns, which need `==` from an `Eq` dictionary.
    Test(Core),
    /// A primitive literal, compared directly (see `Alt::Lit`).
    Lit(Lit),
    Bang(Box<CPat>),
    Lazy(Box<CPat>),
    View(Core, Box<CPat>),
}

#[derive(Clone, Debug)]
pub enum CGuard {
    Bool(Core),
    Pat(CPat, Core),
    Let(Vec<(String, Core)>),
}

/// The right-hand side of one alternative.
#[derive(Clone, Debug)]
pub enum CBody {
    Expr(Core),
    /// Guarded alternatives, tried in order.
    Guards(Vec<(Vec<CGuard>, Core)>),
    Where(Vec<(String, Core)>, Box<CBody>),
}

#[derive(Clone, Debug)]
pub struct Row {
    pub pats: Vec<CPat>,
    pub body: CBody,
}

pub type Fresh<'a> = &'a mut dyn FnMut(&str) -> String;

/// Compiles rows matching the scrutinee variables `scruts`. `fail` is the expression
/// evaluated when no row matches.
pub fn compile_rows(scruts: &[String], rows: &[Row], fail: Core, fresh: Fresh) -> Core {
    if let [s] = scruts {
        let n = rows
            .iter()
            .take_while(|r| {
                matches!(&r.pats[..], [CPat::Lit(Lit::Int(_) | Lit::Char(_))]) && !body_has_guards(&r.body)
            })
            .count();
        if n >= 2 {
            #[derive(Eq, Hash, PartialEq)]
            enum LitKey {
                Int(i64),
                Char(char),
            }
            let rest_code = compile_rows(scruts, &rows[n..], fail, fresh);
            let mut alts = Vec::with_capacity(n + 1);
            let mut seen = std::collections::HashSet::with_capacity(n);
            for r in &rows[..n] {
                let [CPat::Lit(l)] = &r.pats[..] else { unreachable!() };
                let key = match l {
                    Lit::Int(i) => LitKey::Int(*i),
                    Lit::Char(c) => LitKey::Char(*c),
                    _ => unreachable!(),
                };
                if !seen.insert(key) {
                    continue;
                }
                let body = compile_body(&r.body, Core::Fail(String::new()), fresh);
                alts.push(Alt::Lit(l.clone(), body));
            }
            alts.push(Alt::Default(rest_code));
            let b = fresh("lb");
            return Core::Case(Box::new(Core::Var(s.clone())), b, alts);
        }
    }
    match rows.split_first() {
        None => fail,
        Some((row, [])) => row_code(scruts, row, fail, fresh),
        Some((row, rest)) => {
            // The remaining rows are a function of a unit argument, so falling through is a
            // tail call; a thunk here would be forced recursively for every failed row.
            let k = fresh("k");
            let u = fresh("u");
            let rest_code = compile_rows(scruts, rest, fail, fresh);
            let fallthrough = Core::App(
                Box::new(Core::Var(k.clone())),
                Box::new(Core::Con("Builtin.()".into(), vec![])),
            );
            let code = row_code(scruts, row, fallthrough, fresh);
            Core::Let(vec![(k, Core::Lam(u, Box::new(rest_code)))], Box::new(code))
        }
    }
}

fn body_has_guards(body: &CBody) -> bool {
    match body {
        CBody::Expr(_) => false,
        CBody::Where(_, inner) => body_has_guards(inner),
        CBody::Guards(_) => true,
    }
}

fn row_code(scruts: &[String], row: &Row, fail: Core, fresh: Fresh) -> Core {
    let body = compile_body(&row.body, fail.clone(), fresh);
    let pairs: Vec<(String, CPat)> = scruts.iter().cloned().zip(row.pats.iter().cloned()).collect();
    match_all(&pairs, body, &fail, fresh)
}

/// Compiles a right-hand side given its failure continuation.
pub fn compile_body(body: &CBody, fail: Core, fresh: Fresh) -> Core {
    match body {
        CBody::Expr(c) => c.clone(),
        CBody::Where(binds, inner) => crate::tc::expr::wrap_let(binds.clone(), compile_body(inner, fail, fresh)),
        CBody::Guards(alts) => guard_alts(alts, fail, fresh),
    }
}

fn guard_alts(alts: &[(Vec<CGuard>, Core)], fail: Core, fresh: Fresh) -> Core {
    match alts.split_first() {
        None => fail,
        Some(((conds, e), [])) => guard_seq(conds, e.clone(), fail, fresh),
        Some(((conds, e), rest)) => {
            let k = fresh("g");
            let rest_code = guard_alts(rest, fail, fresh);
            let code = guard_seq(conds, e.clone(), Core::Var(k.clone()), fresh);
            Core::Let(vec![(k, rest_code)], Box::new(code))
        }
    }
}

fn guard_seq(conds: &[CGuard], body: Core, fail: Core, fresh: Fresh) -> Core {
    let mut code = body;
    for g in conds.iter().rev() {
        code = match g {
            CGuard::Bool(c) => {
                let b = fresh("b");
                Core::Case(
                    Box::new(c.clone()),
                    b,
                    vec![Alt::Con(TRUE.into(), vec![], code), Alt::Default(fail.clone())],
                )
            }
            CGuard::Pat(p, rhs) => {
                let s = fresh("s");
                let matched = match_one(&s, p, code, &fail, fresh);
                Core::Let(vec![(s, rhs.clone())], Box::new(matched))
            }
            CGuard::Let(binds) => Core::Let(binds.clone(), Box::new(code)),
        };
    }
    code
}

fn match_all(pairs: &[(String, CPat)], success: Core, fail: &Core, fresh: Fresh) -> Core {
    let mut code = success;
    for (s, p) in pairs.iter().rev() {
        code = match_one(s, p, code, fail, fresh);
    }
    code
}

/// Matches the variable `s` against `p`, continuing with `success` or evaluating `fail`.
fn match_one(s: &str, p: &CPat, success: Core, fail: &Core, fresh: Fresh) -> Core {
    match p {
        CPat::Wild => success,
        CPat::Var(v) => Core::Let(vec![(v.clone(), Core::Var(s.to_string()))], Box::new(success)),
        CPat::As(v, sub) => {
            let inner = match_one(s, sub, success, fail, fresh);
            Core::Let(vec![(v.clone(), Core::Var(s.to_string()))], Box::new(inner))
        }
        CPat::Con(c, subs) | CPat::Ordered(c, subs, _) => {
            let vars: Vec<String> = subs.iter().map(|_| fresh("p")).collect();
            let mut order: Vec<usize> = match p {
                CPat::Ordered(_, _, order) => order.clone(),
                _ => Vec::new(),
            };
            // Fields not named in the order are matched afterwards, in position order.
            for i in 0..subs.len() {
                if !order.contains(&i) {
                    order.push(i);
                }
            }
            let pairs: Vec<(String, CPat)> = order.iter().map(|&i| (vars[i].clone(), subs[i].clone())).collect();
            let inner = match_all(&pairs, success, fail, fresh);
            let b = fresh("cb");
            Core::Case(
                Box::new(Core::Var(s.to_string())),
                b,
                vec![Alt::Con(c.clone(), vars, inner), Alt::Default(fail.clone())],
            )
        }
        CPat::Test(f) => {
            let b = fresh("tb");
            Core::Case(
                Box::new(Core::App(Box::new(f.clone()), Box::new(Core::Var(s.to_string())))),
                b,
                vec![Alt::Con(TRUE.into(), vec![], success), Alt::Default(fail.clone())],
            )
        }
        CPat::Lit(l) => {
            let b = fresh("lb");
            Core::Case(
                Box::new(Core::Var(s.to_string())),
                b,
                vec![Alt::Lit(l.clone(), success), Alt::Default(fail.clone())],
            )
        }
        CPat::Bang(sub) => {
            let inner = match_one(s, sub, success, fail, fresh);
            let b = fresh("bb");
            Core::Case(Box::new(Core::Var(s.to_string())), b, vec![Alt::Default(inner)])
        }
        CPat::Lazy(sub) => {
            // Each variable of the pattern is bound to a thunk that performs the match.
            let mut vars = Vec::new();
            cpat_vars(sub, &mut vars);
            let irrefutable = Core::Fail("Irrefutable pattern failed".into());
            let binds = vars
                .into_iter()
                .map(|v| {
                    let extract = match_one(s, sub, Core::Var(v.clone()), &irrefutable, fresh);
                    (v, extract)
                })
                .collect();
            Core::Let(binds, Box::new(success))
        }
        CPat::View(f, sub) => {
            let t = fresh("view");
            let inner = match_one(&t, sub, success, fail, fresh);
            Core::Let(
                vec![(t, Core::App(Box::new(f.clone()), Box::new(Core::Var(s.to_string()))))],
                Box::new(inner),
            )
        }
    }
}

/// Variables bound by a pattern.
pub fn cpat_vars(p: &CPat, out: &mut Vec<String>) {
    match p {
        CPat::Wild | CPat::Test(_) | CPat::Lit(_) => {}
        CPat::Var(v) => out.push(v.clone()),
        CPat::As(v, sub) => {
            out.push(v.clone());
            cpat_vars(sub, out);
        }
        CPat::Con(_, subs) | CPat::Ordered(_, subs, _) => subs.iter().for_each(|s| cpat_vars(s, out)),
        CPat::Bang(sub) | CPat::Lazy(sub) | CPat::View(_, sub) => cpat_vars(sub, out),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contiguous_literal_rows_compile_to_single_multiway_case_deduped() {
        let mut n = 0usize;
        let mut fresh = |prefix: &str| {
            n += 1;
            format!("{prefix}{n}")
        };
        let rows = vec![
            Row { pats: vec![CPat::Lit(Lit::Char('a'))], body: CBody::Expr(Core::Lit(Lit::Char('A'))) },
            Row { pats: vec![CPat::Lit(Lit::Char('b'))], body: CBody::Expr(Core::Lit(Lit::Char('B'))) },
            // Duplicate 'a' must be dropped so the first row wins.
            Row { pats: vec![CPat::Lit(Lit::Char('a'))], body: CBody::Expr(Core::Lit(Lit::Char('X'))) },
            Row { pats: vec![CPat::Wild], body: CBody::Expr(Core::Var("c".into())) },
        ];
        let core = compile_rows(&["c".into()], &rows, Core::Fail("no".into()), &mut fresh);
        let Core::Case(scrut, _, alts) = core else { panic!("expected flat Case, got {core:?}") };
        assert!(matches!(scrut.as_ref(), Core::Var(v) if v == "c"));
        assert_eq!(alts.len(), 3);
        assert!(matches!(&alts[0], Alt::Lit(Lit::Char('a'), Core::Lit(Lit::Char('A')))));
        assert!(matches!(&alts[1], Alt::Lit(Lit::Char('b'), Core::Lit(Lit::Char('B')))));
        assert!(matches!(&alts[2], Alt::Default(Core::Var(v)) if v == "c"));
    }
}

