//! Derived instances (`deriving` clauses). The instances are generated as renamed
//! syntax and then checked like hand-written instances. The shapes follow GHC's stock
//! deriving, including the `Show` output format.

use super::{Checker, TR};
use crate::syntax::ast::{
    Alt, Body, Clause, ConDecl, ConFields, DataDecl, Decl, Expr, InstDecl, Lit, Loc, Pat, QName, Rhs,
    Strategy, Type,
};

const BASE: &str = "Base";

fn base_var(name: &str, loc: Loc) -> Expr {
    Expr::Var(
        QName {
            qual: Some(BASE.into()),
            name: name.into(),
        },
        loc,
    )
}

fn base_con(name: &str, loc: Loc) -> Expr {
    Expr::Con(
        QName {
            qual: Some(BASE.into()),
            name: name.into(),
        },
        loc,
    )
}

fn local(name: &str, loc: Loc) -> Expr {
    Expr::Var(QName::unqual(name), loc)
}

fn app(f: Expr, args: Vec<Expr>) -> Expr {
    args.into_iter().fold(f, |acc, a| Expr::App(Box::new(acc), Box::new(a)))
}

fn int(n: i64, loc: Loc) -> Expr {
    Expr::Lit(Lit::Int(n.to_string()), loc)
}

fn text(s: &str, loc: Loc) -> Expr {
    Expr::Lit(Lit::Str(s.to_string()), loc)
}

fn ch(c: char, loc: Loc) -> Expr {
    Expr::Lit(Lit::Char(c), loc)
}

/// `f . g . h` as nested compositions of Base's `.`.
fn compose(parts: Vec<Expr>, loc: Loc) -> Expr {
    let mut it = parts.into_iter().rev();
    let last = it.next().expect("at least one part");
    it.fold(last, |acc, p| app(base_var(".", loc), vec![p, acc]))
}

fn con_pat(module: &str, con: &str, args: Vec<Pat>, loc: Loc) -> Pat {
    Pat::Con(
        QName {
            qual: Some(module.into()),
            name: con.into(),
        },
        args,
        loc,
    )
}

fn con_expr(module: &str, con: &str, loc: Loc) -> Expr {
    Expr::Con(
        QName {
            qual: Some(module.into()),
            name: con.into(),
        },
        loc,
    )
}

fn clause(pats: Vec<Pat>, body: Expr, loc: Loc) -> Clause {
    Clause {
        pats,
        rhs: Rhs {
            body: Body::Plain(body),
            wheres: Vec::new(),
        },
        loc,
    }
}

fn fun(name: &str, clauses: Vec<Clause>, loc: Loc) -> Decl {
    Decl::Fun {
        name: name.into(),
        clauses,
        loc,
    }
}

/// Field names of a constructor (empty for positional constructors).
fn field_names(c: &ConDecl) -> Vec<String> {
    match &c.fields {
        ConFields::Record(groups) => groups.iter().flat_map(|(ns, _)| ns.iter().cloned()).collect(),
        ConFields::Positional(_) => Vec::new(),
    }
}

fn arity(c: &ConDecl) -> usize {
    match &c.fields {
        ConFields::Positional(fs) => fs.len(),
        ConFields::Record(groups) => groups.iter().map(|(ns, _)| ns.len()).sum(),
    }
}

fn is_operator(name: &str) -> bool {
    !name.chars().next().is_some_and(|c| c.is_alphabetic() || c == '_')
}

/// Fresh variable names for `n` pattern positions.
fn fresh_vars(checker: &mut Checker, base: &str, n: usize) -> Vec<String> {
    (0..n).map(|_| checker.fresh_name(base)).collect()
}

/// Instances for the `deriving` clauses of a data declaration.
pub fn derive_instances(checker: &mut Checker, dd: &DataDecl) -> TR<Vec<Decl>> {
    let mut out = Vec::new();
    for d in &dd.deriving {
        for cls in &d.classes {
            let class_qname = match cls {
                Type::Con(q, _) => q.clone(),
                Type::App(f, _) => match f.as_ref() {
                    Type::Con(q, _) => q.clone(),
                    _ => continue,
                },
                _ => continue,
            };
            let name = class_qname.name.clone();
            // Newtype deriving is handled by the dictionary alias in the checker.
            if d.strategy == Some(Strategy::Newtype) || (dd.newtype && !is_stock(&name) && d.strategy.is_none()) {
                continue;
            }
            let derived = match name.as_str() {
                "Eq" => derive_eq(checker, dd),
                "Ord" => derive_ord(checker, dd),
                "Show" => derive_show(checker, dd),
                "Enum" => derive_enum(checker, dd),
                "Bounded" => derive_bounded(checker, dd),
                "Read" => derive_read(checker, dd),
                "Functor" => derive_functor(checker, dd),
                "Foldable" => derive_foldable(checker, dd),
                "Traversable" => derive_traversable(checker, dd),
                // DeriveAnyClass: an instance with the class's default methods. Derived
                // instances of unknown classes are skipped when the instance is registered.
                _ => Vec::new(),
            };
            // Constructor classes are derived for the type constructor without its last parameter.
            let drop_last = matches!(name.as_str(), "Functor" | "Foldable" | "Traversable");
            let mut inst = instance(&checker.module, dd, &class_qname, derived, drop_last);
            if let Decl::Instance(id) = &mut inst {
                id.overlap = Some(DERIVED_MARK.into());
            }
            out.push(inst);
        }
    }
    Ok(out)
}

/// Marks generated instances, so that a missing class is not an error for them.
pub const DERIVED_MARK: &str = "derived";

/// Classes with a stock derivation.
pub fn is_stock(name: &str) -> bool {
    matches!(
        name,
        "Eq" | "Ord" | "Show" | "Enum" | "Bounded" | "Read" | "Functor" | "Foldable" | "Traversable"
    )
}

/// Builds the instance declaration: the context has the class at every type parameter.
fn instance(module: &str, dd: &DataDecl, class: &QName, decls: Vec<Decl>, drop_last: bool) -> Decl {
    let loc = dd.loc;
    let kept: Vec<&crate::syntax::ast::TyVarBind> = if drop_last {
        dd.tyvars[..dd.tyvars.len().saturating_sub(1)].iter().collect()
    } else {
        dd.tyvars.iter().collect()
    };
    let module_qname = |n: &str| QName {
        qual: Some(BASE.into()),
        name: n.into(),
    };
    let head_ty = kept.iter().fold(
        Type::Con(
            QName {
                qual: Some(module.to_string()),
                name: dd.name.clone(),
            },
            loc,
        ),
        |acc, tv| Type::App(Box::new(acc), Box::new(Type::Var(tv.name.clone(), loc))),
    );
    let head = Type::App(Box::new(Type::Con(class.clone(), loc)), Box::new(head_ty));
    let context = if drop_last {
        Vec::new()
    } else {
        dd.tyvars
            .iter()
            .map(|tv| Type::App(Box::new(Type::Con(class.clone(), loc)), Box::new(Type::Var(tv.name.clone(), loc))))
            .collect()
    };
    Decl::Instance(InstDecl {
        overlap: None,
        context,
        head,
        decls,
        loc,
    })
}

/// `(==)`: constructor by constructor, fields compared with `==`.
fn derive_eq(checker: &mut Checker, dd: &DataDecl) -> Vec<Decl> {
    let loc = dd.loc;
    let module = checker.module.clone();
    let mut clauses = Vec::new();
    for c in &dd.cons {
        let n = arity(c);
        let a = fresh_vars(checker, "a", n);
        let b = fresh_vars(checker, "b", n);
        let pa = con_pat(&module, &c.name, a.iter().map(|v| Pat::Var(v.clone(), loc)).collect(), loc);
        let pb = con_pat(&module, &c.name, b.iter().map(|v| Pat::Var(v.clone(), loc)).collect(), loc);
        let mut body: Option<Expr> = None;
        for (x, y) in a.iter().zip(&b).rev() {
            let eq = app(base_var("==", loc), vec![local(x, loc), local(y, loc)]);
            body = Some(match body {
                None => eq,
                Some(rest) => app(base_var("&&", loc), vec![eq, rest]),
            });
        }
        let body = body.unwrap_or_else(|| base_con("True", loc));
        clauses.push(clause(vec![pa, pb], body, loc));
    }
    if dd.cons.len() != 1 {
        clauses.push(clause(
            vec![Pat::Wild(loc), Pat::Wild(loc)],
            if dd.cons.is_empty() { base_con("True", loc) } else { base_con("False", loc) },
            loc,
        ));
    }
    vec![fun("==", clauses, loc)]
}

/// The constructor index of an expression, as an `Int` expression.
fn tag_of(dd: &DataDecl, module: &str, e: Expr, loc: Loc) -> Expr {
    let alts: Vec<Alt> = dd
        .cons
        .iter()
        .enumerate()
        .map(|(i, c)| Alt {
            pat: con_pat(module, &c.name, (0..arity(c)).map(|_| Pat::Wild(loc)).collect(), loc),
            rhs: Rhs {
                body: Body::Plain(int(i as i64, loc)),
                wheres: Vec::new(),
            },
            loc,
        })
        .collect();
    Expr::Case(Box::new(e), alts, loc)
}

/// `compare`: by constructor index, then field by field.
fn derive_ord(checker: &mut Checker, dd: &DataDecl) -> Vec<Decl> {
    let loc = dd.loc;
    let module = checker.module.clone();
    let x = checker.fresh_name("x");
    let y = checker.fresh_name("y");
    let mut alts: Vec<Alt> = Vec::new();
    for c in &dd.cons {
        let n = arity(c);
        let a = fresh_vars(checker, "a", n);
        let b = fresh_vars(checker, "b", n);
        let inner = if n == 0 {
            base_con("EQ", loc)
        } else {
            let mut cmps: Vec<Expr> = a
                .iter()
                .zip(&b)
                .map(|(p, q)| app(base_var("compare", loc), vec![local(p, loc), local(q, loc)]))
                .collect();
            let mut acc = cmps.pop().expect("at least one field");
            while let Some(prev) = cmps.pop() {
                // case prev of EQ -> acc; r -> r
                let r = checker.fresh_name("r");
                acc = Expr::Case(
                    Box::new(prev),
                    vec![
                        Alt {
                            pat: Pat::Con(QName { qual: Some(BASE.into()), name: "EQ".into() }, vec![], loc),
                            rhs: Rhs { body: Body::Plain(acc), wheres: Vec::new() },
                            loc,
                        },
                        Alt {
                            pat: Pat::Var(r.clone(), loc),
                            rhs: Rhs { body: Body::Plain(local(&r, loc)), wheres: Vec::new() },
                            loc,
                        },
                    ],
                    loc,
                );
            }
            acc
        };
        let pat_y = con_pat(&module, &c.name, b.iter().map(|v| Pat::Var(v.clone(), loc)).collect(), loc);
        let pat_x = con_pat(&module, &c.name, a.iter().map(|v| Pat::Var(v.clone(), loc)).collect(), loc);
        // Inner case on y: same constructor compares fields; any other constructor compares tags.
        let same_tag_cmp = Expr::Case(
            Box::new(local(&y, loc)),
            vec![
                Alt {
                    pat: pat_y,
                    rhs: Rhs { body: Body::Plain(inner), wheres: Vec::new() },
                    loc,
                },
                Alt {
                    pat: Pat::Wild(loc),
                    rhs: Rhs {
                        body: Body::Plain(app(
                            base_var("compareInt", loc),
                            vec![
                                tag_of(dd, &module, local(&x, loc), loc),
                                tag_of(dd, &module, local(&y, loc), loc),
                            ],
                        )),
                        wheres: Vec::new(),
                    },
                    loc,
                },
            ],
            loc,
        );
        alts.push(Alt {
            pat: pat_x,
            rhs: Rhs { body: Body::Plain(same_tag_cmp), wheres: Vec::new() },
            loc,
        });
    }
    let body = if dd.cons.is_empty() {
        base_con("EQ", loc)
    } else {
        Expr::Case(Box::new(local(&x, loc)), alts, loc)
    };
    vec![fun("compare", vec![clause(vec![Pat::Var(x, loc), Pat::Var(y, loc)], body, loc)], loc)]
}

/// `showsPrec`, in GHC's format: prefix constructors parenthesised at precedence 11,
/// records with `{f = v, ...}`, infix constructors at their (default) precedence.
fn derive_show(checker: &mut Checker, dd: &DataDecl) -> Vec<Decl> {
    let loc = dd.loc;
    let module = checker.module.clone();
    let mut clauses = Vec::new();
    for c in &dd.cons {
        let n = arity(c);
        let d = checker.fresh_name("d");
        let a = fresh_vars(checker, "a", n);
        let pat = con_pat(&module, &c.name, a.iter().map(|v| Pat::Var(v.clone(), loc)).collect(), loc);
        let show_arg = |v: &String, prec: i64| app(base_var("showsPrec", loc), vec![int(prec, loc), local(v, loc)]);
        let shows_str = |s: &str| app(base_var("showString", loc), vec![text(s, loc)]);
        let body = if n == 0 {
            let name = display_name(&c.name);
            shows_str(&name)
        } else if c.infix && n == 2 && field_names(c).is_empty() {
            // Infix constructor at the default precedence 9.
            let prec = 9;
            let parts = vec![
                show_arg(&a[0], prec + 1),
                app(base_var("showString", loc), vec![text(&format!(" {} ", c.name), loc)]),
                show_arg(&a[1], prec + 1),
            ];
            app(
                base_var("showParen", loc),
                vec![
                    app(base_var(">", loc), vec![local(&d, loc), int(prec, loc)]),
                    compose(parts, loc),
                ],
            )
        } else {
            let names = field_names(c);
            let mut parts: Vec<Expr> = Vec::new();
            if names.is_empty() {
                parts.push(shows_str(&format!("{} ", display_name(&c.name))));
                for (i, v) in a.iter().enumerate() {
                    if i > 0 {
                        parts.push(app(base_var("showChar", loc), vec![ch(' ', loc)]));
                    }
                    parts.push(show_arg(v, 11));
                }
            } else {
                parts.push(shows_str(&format!("{} {{", display_name(&c.name))));
                for (i, (f, v)) in names.iter().zip(&a).enumerate() {
                    let label = if i == 0 { format!("{} = ", f) } else { format!(", {} = ", f) };
                    parts.push(shows_str(&label));
                    parts.push(show_arg(v, 0));
                }
                parts.push(app(base_var("showChar", loc), vec![ch('}', loc)]));
            }
            let needs_parens = if names.is_empty() { n > 0 } else { true };
            if needs_parens {
                app(
                    base_var("showParen", loc),
                    vec![
                        app(base_var(">=", loc), vec![local(&d, loc), int(11, loc)]),
                        compose(parts, loc),
                    ],
                )
            } else {
                compose(parts, loc)
            }
        };
        clauses.push(clause(vec![Pat::Var(d, loc), pat], body, loc));
    }
    if dd.cons.is_empty() {
        clauses.push(clause(vec![Pat::Wild(loc), Pat::Wild(loc)], base_var("undefined", loc), loc));
    }
    vec![fun("showsPrec", clauses, loc)]
}

/// Operator constructors are shown in parentheses when used prefix.
fn display_name(name: &str) -> String {
    if is_operator(name) {
        format!("({name})")
    } else {
        name.to_string()
    }
}

/// `Enum` for enumerations: the constructor index is the enumeration index.
fn derive_enum(checker: &mut Checker, dd: &DataDecl) -> Vec<Decl> {
    let loc = dd.loc;
    let module = checker.module.clone();
    let n = dd.cons.len();
    if n == 0 || dd.cons.iter().any(|c| arity(c) > 0) {
        return Vec::new();
    }
    let first = con_expr(&module, &dd.cons[0].name, loc);
    let last = con_expr(&module, &dd.cons[n - 1].name, loc);
    let x = checker.fresh_name("x");
    let to_alts: Vec<Alt> = dd
        .cons
        .iter()
        .enumerate()
        .map(|(i, c)| Alt {
            pat: Pat::Lit(Lit::Int(i.to_string()), loc),
            rhs: Rhs { body: Body::Plain(con_expr(&module, &c.name, loc)), wheres: Vec::new() },
            loc,
        })
        .chain(std::iter::once(Alt {
            pat: Pat::Wild(loc),
            rhs: Rhs {
                body: Body::Plain(app(
                    base_var("error", loc),
                    vec![text("toEnum: bad argument", loc)],
                )),
                wheres: Vec::new(),
            },
            loc,
        }))
        .collect();
    let to_enum = fun(
        "toEnum",
        vec![clause(vec![Pat::Var(x.clone(), loc)], Expr::Case(Box::new(local(&x, loc)), to_alts, loc), loc)],
        loc,
    );
    let y = checker.fresh_name("y");
    let from_enum = fun(
        "fromEnum",
        vec![clause(vec![Pat::Var(y.clone(), loc)], tag_of(dd, &module, local(&y, loc), loc), loc)],
        loc,
    );
    let enum_from = fun(
        "enumFrom",
        vec![clause(
            vec![Pat::Var("e%".into(), loc)],
            app(base_var("enumFromTo", loc), vec![local("e%", loc), last.clone()]),
            loc,
        )],
        loc,
    );
    let enum_from_then = fun(
        "enumFromThen",
        vec![clause(
            vec![Pat::Var("a%".into(), loc), Pat::Var("b%".into(), loc)],
            app(
                base_var("enumFromThenTo", loc),
                vec![
                    local("a%", loc),
                    local("b%", loc),
                    Expr::If(
                        Box::new(app(
                            base_var(">=", loc),
                            vec![
                                tag_of(dd, &module, local("b%", loc), loc),
                                tag_of(dd, &module, local("a%", loc), loc),
                            ],
                        )),
                        Box::new(last),
                        Box::new(first),
                        loc,
                    ),
                ],
            ),
            loc,
        )],
        loc,
    );
    vec![to_enum, from_enum, enum_from, enum_from_then]
}

/// `Bounded`: first and last constructor for enumerations; the single constructor's
/// bounds for a product.
fn derive_bounded(checker: &mut Checker, dd: &DataDecl) -> Vec<Decl> {
    let loc = dd.loc;
    let module = checker.module.clone();
    if dd.cons.is_empty() {
        return Vec::new();
    }
    if dd.cons.iter().all(|c| arity(c) == 0) {
        let first = con_expr(&module, &dd.cons[0].name, loc);
        let last = con_expr(&module, &dd.cons[dd.cons.len() - 1].name, loc);
        return vec![
            fun("minBound", vec![clause(vec![], first, loc)], loc),
            fun("maxBound", vec![clause(vec![], last, loc)], loc),
        ];
    }
    if dd.cons.len() == 1 {
        let c = &dd.cons[0];
        let n = arity(c);
        let mk = |bound: &str| {
            app(
                con_expr(&module, &c.name, loc),
                (0..n).map(|_| base_var(bound, loc)).collect(),
            )
        };
        return vec![
            fun("minBound", vec![clause(vec![], mk("minBound"), loc)], loc),
            fun("maxBound", vec![clause(vec![], mk("maxBound"), loc)], loc),
        ];
    }
    Vec::new()
}

/// `readsPrec`: each constructor is read by a list comprehension over `lex` tokens and
/// `readsPrec 11` for its arguments; the alternatives are concatenated.
fn derive_read(checker: &mut Checker, dd: &DataDecl) -> Vec<Decl> {
    use crate::syntax::ast::Stmt;
    let loc = dd.loc;
    let d = checker.fresh_name("d");
    let s = checker.fresh_name("s");
    let r = checker.fresh_name("r");
    let module = checker.module.clone();
    let mut alts: Vec<Expr> = Vec::new();
    for c in &dd.cons {
        if !field_names(c).is_empty() {
            continue;
        }
        let n = arity(c);
        let args = fresh_vars(checker, "a", n);
        let mut quals: Vec<Stmt> = Vec::new();
        let mut cur = checker.fresh_name("s");
        let token = if is_operator(&c.name) { c.name.clone() } else { c.name.clone() };
        quals.push(Stmt::Bind {
            pat: Pat::Tuple(
                vec![Pat::Lit(Lit::Str(token), loc), Pat::Var(cur.clone(), loc)],
                loc,
            ),
            expr: app(base_var("lex", loc), vec![local(&r, loc)]),
            loc,
        });
        for a in &args {
            let next = checker.fresh_name("s");
            quals.push(Stmt::Bind {
                pat: Pat::Tuple(vec![Pat::Var(a.clone(), loc), Pat::Var(next.clone(), loc)], loc),
                expr: app(base_var("readsPrec", loc), vec![int(11, loc), local(&cur, loc)]),
                loc,
            });
            cur = next;
        }
        let value = args
            .iter()
            .fold(con_expr(&module, &c.name, loc), |acc, a| app(acc, vec![local(a, loc)]));
        let body = Expr::Tuple(vec![Some(value), Some(local(&cur, loc))], loc);
        alts.push(Expr::ListComp(Box::new(body), vec![quals], loc));
    }
    let all = alts
        .into_iter()
        .rev()
        .fold(Expr::List(vec![], loc), |acc, alt| app(base_var("++", loc), vec![alt, acc]));
    // readsPrec d s = readParen (d > 10) (\r -> all) s
    let paren_cond = app(base_var(">", loc), vec![local(&d, loc), int(10, loc)]);
    let lam = Expr::Lam(vec![Pat::Var(r.clone(), loc)], Box::new(all), loc);
    let body = app(
        base_var("readParen", loc),
        vec![paren_cond, lam, local(&s, loc)],
    );
    vec![fun(
        "readsPrec",
        vec![clause(vec![Pat::Var(d, loc), Pat::Var(s, loc)], body, loc)],
        loc,
    )]
}

// ----- constructor classes: Functor, Foldable, Traversable --------------------------

/// The field types of a constructor, in order.
fn field_types(c: &ConDecl) -> Vec<Type> {
    match &c.fields {
        ConFields::Positional(fs) => fs.iter().map(|f| f.ty.clone()).collect(),
        ConFields::Record(groups) => groups
            .iter()
            .flat_map(|(ns, f)| ns.iter().map(move |_| f.ty.clone()))
            .collect(),
    }
}

/// Whether a type mentions a type variable.
fn mentions(t: &Type, v: &str) -> bool {
    match t {
        Type::Var(n, _) => n == v,
        Type::App(f, a) | Type::Fun(f, a) => mentions(f, v) || mentions(a, v),
        Type::List(e, _) => mentions(e, v),
        Type::Tuple(ts, _) => ts.iter().any(|x| mentions(x, v)),
        Type::Sig(x, _) => mentions(x, v),
        _ => false,
    }
}

fn pat_con(module: &str, c: &ConDecl, vars: Vec<String>, loc: Loc) -> Pat {
    con_pat(module, &c.name, vars.into_iter().map(|v| Pat::Var(v, loc)).collect(), loc)
}

/// The argument of `m` for a field of type `t`: a function mapping `t[a]` to `t[b]` through
/// `f`, or `None` when the field does not mention the last parameter.
fn map_fn(checker: &mut Checker, t: &Type, last: &str, f: &str, loc: Loc) -> Option<Expr> {
    if !mentions(t, last) {
        return None;
    }
    match t {
        Type::Var(n, _) if n == last => Some(local(f, loc)),
        Type::App(g, arg) if !mentions(g, last) => {
            let inner = map_fn(checker, arg, last, f, loc)?;
            Some(app(base_var("fmap", loc), vec![inner]))
        }
        Type::List(e, _) => {
            let inner = map_fn(checker, e, last, f, loc)?;
            Some(app(base_var("fmap", loc), vec![inner]))
        }
        Type::Tuple(ts, _) => {
            let names = fresh_vars(checker, "t", ts.len());
            let mut comps = Vec::new();
            for (ty, n) in ts.iter().zip(&names) {
                comps.push(match map_fn(checker, ty, last, f, loc) {
                    Some(m) => app(m, vec![local(n, loc)]),
                    None => local(n, loc),
                });
            }
            let pat = Pat::Tuple(names.iter().map(|n| Pat::Var(n.clone(), loc)).collect(), loc);
            Some(Expr::Lam(vec![pat], Box::new(Expr::Tuple(comps.into_iter().map(Some).collect(), loc)), loc))
        }
        Type::Fun(a, b) if !mentions(a, last) => {
            let g = checker.fresh_name("g");
            let x = checker.fresh_name("x");
            let mb = map_fn(checker, b, last, f, loc);
            let body = match mb {
                Some(m) => app(m, vec![app(local(&g, loc), vec![local(&x, loc)])]),
                None => app(local(&g, loc), vec![local(&x, loc)]),
            };
            Some(Expr::Lam(vec![Pat::Var(g, loc)], Box::new(Expr::Lam(vec![Pat::Var(x, loc)], Box::new(body), loc)), loc))
        }
        _ => None,
    }
}

fn derive_functor(checker: &mut Checker, dd: &DataDecl) -> Vec<Decl> {
    let loc = dd.loc;
    let module = checker.module.clone();
    let Some(last) = dd.tyvars.last().map(|t| t.name.clone()) else {
        return Vec::new();
    };
    let mut clauses = Vec::new();
    for c in &dd.cons {
        let f = checker.fresh_name("f");
        let n = field_types(c).len();
        let xs = fresh_vars(checker, "x", n);
        let mut args = Vec::new();
        for (t, x) in field_types(c).iter().zip(&xs) {
            args.push(match map_fn(checker, t, &last, &f, loc) {
                Some(m) => app(m, vec![local(x, loc)]),
                None => local(x, loc),
            });
        }
        let body = args.into_iter().fold(con_expr(&module, &c.name, loc), |acc, a| app(acc, vec![a]));
        clauses.push(clause(vec![Pat::Var(f, loc), pat_con(&module, c, xs, loc)], body, loc));
    }
    if dd.cons.is_empty() {
        clauses.push(clause(vec![Pat::Wild(loc), Pat::Var("z%".into(), loc)], base_var("undefined", loc), loc));
    }
    vec![fun("fmap", clauses, loc)]
}

/// `foldr`-style folding of a field of type `t` into an accumulator.
fn fold_expr(checker: &mut Checker, t: &Type, last: &str, f: &str, x: Expr, acc: Expr, loc: Loc) -> Expr {
    if !mentions(t, last) {
        return acc;
    }
    match t {
        Type::Var(n, _) if n == last => app(local(f, loc), vec![x, acc]),
        Type::App(g, arg) if !mentions(g, last) => fold_inner(checker, arg, last, f, x, acc, loc),
        Type::List(e, _) => fold_inner(checker, e, last, f, x, acc, loc),
        _ => acc,
    }
}

fn fold_inner(checker: &mut Checker, arg: &Type, last: &str, f: &str, x: Expr, acc: Expr, loc: Loc) -> Expr {
    let y = checker.fresh_name("y");
    let a2 = checker.fresh_name("acc");
    let step = fold_expr(checker, arg, last, f, local(&y, loc), local(&a2, loc), loc);
    let lam = Expr::Lam(vec![Pat::Var(y, loc), Pat::Var(a2, loc)], Box::new(step), loc);
    app(base_var("foldr", loc), vec![lam, acc, x])
}

fn derive_foldable(checker: &mut Checker, dd: &DataDecl) -> Vec<Decl> {
    let loc = dd.loc;
    let module = checker.module.clone();
    let Some(last) = dd.tyvars.last().map(|t| t.name.clone()) else {
        return Vec::new();
    };
    let mut clauses = Vec::new();
    for c in &dd.cons {
        let f = checker.fresh_name("f");
        let z = checker.fresh_name("z");
        let n = field_types(c).len();
        let xs = fresh_vars(checker, "x", n);
        let mut acc = local(&z, loc);
        for (t, x) in field_types(c).iter().zip(&xs).rev() {
            acc = fold_expr(checker, t, &last, &f, local(x, loc), acc, loc);
        }
        clauses.push(clause(
            vec![Pat::Var(f, loc), Pat::Var(z, loc), pat_con(&module, c, xs, loc)],
            acc,
            loc,
        ));
    }
    if dd.cons.is_empty() {
        clauses.push(clause(vec![Pat::Wild(loc), Pat::Var("z%".into(), loc), Pat::Wild(loc)], local("z%", loc), loc));
    }
    vec![fun("foldr", clauses, loc)]
}

/// `traverse` of a field of type `t`: an applicative action producing the new field.
fn trav_expr(checker: &mut Checker, t: &Type, last: &str, f: &str, x: Expr, loc: Loc) -> Expr {
    if !mentions(t, last) {
        return app(base_var("pure", loc), vec![x]);
    }
    match t {
        Type::Var(n, _) if n == last => app(local(f, loc), vec![x]),
        Type::App(g, arg) if !mentions(g, last) => trav_inner(checker, arg, last, f, x, loc),
        Type::List(e, _) => trav_inner(checker, e, last, f, x, loc),
        _ => app(base_var("pure", loc), vec![x]),
    }
}

fn trav_inner(checker: &mut Checker, arg: &Type, last: &str, f: &str, x: Expr, loc: Loc) -> Expr {
    let y = checker.fresh_name("y");
    let step = trav_expr(checker, arg, last, f, local(&y, loc), loc);
    let lam = Expr::Lam(vec![Pat::Var(y, loc)], Box::new(step), loc);
    app(base_var("traverse", loc), vec![lam, x])
}

fn derive_traversable(checker: &mut Checker, dd: &DataDecl) -> Vec<Decl> {
    let loc = dd.loc;
    let module = checker.module.clone();
    let Some(last) = dd.tyvars.last().map(|t| t.name.clone()) else {
        return Vec::new();
    };
    let mut clauses = Vec::new();
    for c in &dd.cons {
        let f = checker.fresh_name("f");
        let n = field_types(c).len();
        let xs = fresh_vars(checker, "x", n);
        let mut action = app(base_var("pure", loc), vec![con_expr(&module, &c.name, loc)]);
        for (t, x) in field_types(c).iter().zip(&xs) {
            let a = trav_expr(checker, t, &last, &f, local(x, loc), loc);
            action = app(base_var("<*>", loc), vec![action, a]);
        }
        clauses.push(clause(vec![Pat::Var(f, loc), pat_con(&module, c, xs, loc)], action, loc));
    }
    if dd.cons.is_empty() {
        clauses.push(clause(vec![Pat::Wild(loc), Pat::Wild(loc)], base_var("undefined", loc), loc));
    }
    vec![fun("traverse", clauses, loc)]
}
