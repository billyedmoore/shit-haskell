//! Derived instances (`deriving` clauses). The instances are generated as renamed
//! syntax and then checked like hand-written instances. The shapes follow GHC's stock
//! deriving, including the `Show` output format.

use super::{Checker, TR};
use crate::syntax::ast::{
    Alt, Body, Clause, ConDecl, ConFields, DataDecl, Decl, Expr, InstDecl, Lit, Loc, Pat, QName, Rhs, Type,
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
            let name = match cls {
                Type::Con(q, _) => q.name.clone(),
                Type::App(f, _) => match f.as_ref() {
                    Type::Con(q, _) => q.name.clone(),
                    _ => continue,
                },
                _ => continue,
            };
            let derived = match name.as_str() {
                "Eq" => derive_eq(checker, dd),
                "Ord" => derive_ord(checker, dd),
                "Show" => derive_show(checker, dd),
                "Enum" => derive_enum(checker, dd),
                "Bounded" => derive_bounded(checker, dd),
                // Other classes are accepted and ignored until they are supported.
                _ => continue,
            };
            out.push(instance(&checker.module, dd, &name, derived));
        }
    }
    Ok(out)
}

/// Builds the instance declaration: the context has the class at every type parameter.
fn instance(module: &str, dd: &DataDecl, class: &str, decls: Vec<Decl>) -> Decl {
    let loc = dd.loc;
    let module_qname = |n: &str| QName {
        qual: Some(BASE.into()),
        name: n.into(),
    };
    let head_ty = dd.tyvars.iter().fold(
        Type::Con(
            QName {
                qual: Some(module.to_string()),
                name: dd.name.clone(),
            },
            loc,
        ),
        |acc, tv| Type::App(Box::new(acc), Box::new(Type::Var(tv.name.clone(), loc))),
    );
    let head = Type::App(Box::new(Type::Con(module_qname(class), loc)), Box::new(head_ty));
    let context = dd
        .tyvars
        .iter()
        .map(|tv| Type::App(Box::new(Type::Con(module_qname(class), loc)), Box::new(Type::Var(tv.name.clone(), loc))))
        .collect();
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
        } else if is_operator(&c.name) && n == 2 && field_names(c).is_empty() {
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
