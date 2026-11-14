//! Derived instances (`deriving` clauses). The instances are generated as renamed
//! syntax and then checked like hand-written instances. The shapes follow GHC's stock
//! deriving, including the `Show` output format.

use super::{Checker, TR};
use super::types::Ty;
use crate::syntax::ast::{
    Alt, Body, Clause, ConDecl, ConFields, DataDecl, Decl, Expr, InstDecl, Lit, Loc, Pat, QName, Rhs,
    Strategy, Type,
};

const BASE: &str = "Base";

/// The module that defines a Prelude name: the wired-in types of GHC.Types, or Base.
fn defining_module(name: &str) -> &'static str {
    match name {
        "True" | "False" | "Bool" | "Char" | "Int" | "Word" | "Float" | "Double" | "Ordering" | "LT" | "EQ" | "GT"
        | "I#" | "W#" | "C#" | "D#" | "F#" => {
            "GHC.Types"
        }
        "Eq" | "Ord" | "==" | "/=" | "<" | "<=" | ">" | ">=" | "compare" | "max" | "min" | "&&" | "||" | "not" | "compareInt#" => {
            "GHC.Classes"
        }
        _ => BASE,
    }
}

fn base_var(name: &str, loc: Loc) -> Expr {
    Expr::Var(
        QName {
            qual: Some(defining_module(name).into()),
            name: name.into(),
        },
        loc,
    )
}

fn base_con(name: &str, loc: Loc) -> Expr {
    Expr::Con(
        QName {
            qual: Some(defining_module(name).into()),
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
        end: loc,
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
/// The field labels of a record constructor, as the source names them.
fn field_names(c: &ConDecl) -> Vec<String> {
    match &c.fields {
        ConFields::Record(groups) => groups
            .iter()
            .flat_map(|(ns, _)| ns.iter().map(|n| crate::rename::iface::field_label(n).to_string()))
            .collect(),
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
            // Generic, Data and Lift are derived the stock way for newtypes too.
            let stock_for_newtype = stock_for_newtype(checker.newtype_deriving, &name, dd);
            // Newtype and via deriving: the instance is another type's dictionary (registered by
            // the checker), with no code.
            if d.via.is_some() || d.strategy == Some(Strategy::Newtype) || (dd.newtype && !stock_for_newtype && d.strategy.is_none()) {
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
                "Lift" => derive_lift(checker, dd),
                "Data" => derive_data(checker, dd),
                "Generic" => derive_generic(checker, dd, false),
                "Generic1" => derive_generic(checker, dd, true),
                // DeriveAnyClass: an instance with the class's default methods. Derived
                // instances of unknown classes are skipped when the instance is registered.
                _ => Vec::new(),
            };
            // Constructor classes are derived for the type constructor without its last parameter.
            let drop_last = matches!(name.as_str(), "Functor" | "Foldable" | "Traversable" | "Generic1");
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

/// Whether a newtype derives a class the stock way when no strategy is given. With
/// GeneralizedNewtypeDeriving, GHC derives every class through the representation except those
/// whose stock instance differs (Read, Show) or cannot be coerced (Traversable), and the
/// generic-programming classes.
pub fn stock_for_newtype(newtype_deriving: bool, name: &str, dd: &DataDecl) -> bool {
    if matches!(name, "Generic" | "Generic1" | "Data" | "Lift") {
        return true;
    }
    if newtype_deriving {
        // A constructor class needs the representation to end in the last parameter (`g p`); a
        // newtype of the parameter itself (`Par1 p = Par1 p`) is derived the stock way.
        matches!(name, "Read" | "Show" | "Traversable") || (is_stock(name) && !eta_reducible(dd, name))
    } else {
        is_stock(name)
    }
}

/// Whether a newtype's representation can stand for it at a constructor class: its field is
/// `g p` with `p` the last parameter, which `g` does not mention. Other classes always can.
fn eta_reducible(dd: &DataDecl, name: &str) -> bool {
    if !matches!(name, "Functor" | "Foldable" | "Traversable") {
        return true;
    }
    let Some(last) = dd.tyvars.last() else { return false };
    let field = dd.cons.first().and_then(|c| field_types(c).into_iter().next());
    match field {
        Some(Type::App(g, a)) => matches!(a.as_ref(), Type::Var(v, _) if *v == last.name) && !mentions(&g, &last.name),
        _ => false,
    }
}

/// Classes with a stock derivation.
pub fn is_stock(name: &str) -> bool {
    matches!(
        name,
        "Eq" | "Ord" | "Show" | "Enum" | "Bounded" | "Read" | "Functor" | "Foldable" | "Traversable"
    )
}

/// The context of a derived instance, as GHC infers it from the fields: a field whose type is
/// headed by a type variable (`f p`) needs the class at that type; any other field needs the class
/// at the variables it mentions (`Tree a` needs `C a`). For constructor classes the variable
/// applied to the last parameter is the one needed.
fn field_context(class: &QName, dd: &DataDecl, loc: Loc, last: Option<&str>) -> Vec<Type> {
    let mut out: Vec<Type> = Vec::new();
    for c in &dd.cons {
        let fields: Vec<&Type> = match &c.fields {
            ConFields::Positional(fs) => fs.iter().map(|f| &f.ty).collect(),
            ConFields::Record(groups) => groups.iter().map(|(_, f)| &f.ty).collect(),
        };
        for t in fields {
            collect_needs(t, last, &mut |need: &Type| {
                let item = Type::App(Box::new(Type::Con(class.clone(), loc)), Box::new(need.clone()));
                if !out.contains(&item) {
                    out.push(item);
                }
            });
        }
    }
    out
}

/// A derived `Generic1` instance needs `Functor f` for each composition `f :.: ..` whose outer
/// functor is headed by a type variable (the conversions map over it).
fn generic1_context(dd: &DataDecl, loc: Loc, last: Option<&str>) -> Vec<Type> {
    fn walk(r: &FieldRep, loc: Loc, out: &mut Vec<Type>) {
        if let FieldRep::Comp(f, inner) = r {
            if matches!(spine(f).0, Type::Var(..)) {
                let functor = QName { qual: Some("GHC.Internal.Base".into()), name: "Functor".into() };
                let item = Type::App(Box::new(Type::Con(functor, loc)), Box::new(f.clone()));
                if !out.contains(&item) {
                    out.push(item);
                }
            }
            walk(inner, loc, out);
        }
    }
    let mut out = Vec::new();
    for c in &dd.cons {
        for t in field_types(c) {
            walk(&field_rep(&t, last), loc, &mut out);
        }
    }
    out
}

/// The types a derived instance needs the class at, for one field type.
fn collect_needs(t: &Type, last: Option<&str>, add: &mut dyn FnMut(&Type)) {
    let mut vars = Vec::new();
    type_vars(t, &mut vars);
    if vars.is_empty() {
        return;
    }
    // Lists and tuples are instances of the class at their elements: a list needs what its
    // element needs (`[a]` needs `Show a`), a tuple what each component needs.
    match t {
        Type::List(e, _) => return collect_needs(e, last, add),
        // A function field (`s -> m (a, s)`): what its argument and result need.
        Type::Fun(a, b) => {
            collect_needs(a, last, add);
            return collect_needs(b, last, add);
        }
        Type::Tuple(ts, _) => {
            for x in ts {
                collect_needs(x, last, add);
            }
            return;
        }
        _ => {}
    }
    let (head, args) = spine(t);
    match (last, head) {
        // Constructor classes: the head variable applied to the last parameter.
        (Some(l), Type::Var(..)) => {
            if let Some(last_arg) = args.last().filter(|a| mentions(a, l)) {
                add(&strip_last(t));
                // The argument is itself a field of the class's kind (`f (g p)` needs `g`).
                collect_needs(last_arg, last, add);
            }
        }
        (Some(l), _) => {
            for a in args {
                if mentions(a, l) {
                    collect_needs(a, last, add);
                }
            }
        }
        // Ordinary classes: a variable-headed field needs the class at the field itself.
        (None, Type::Var(v, _)) if v.is_empty() => {}
        (None, Type::Var(..)) => add(t),
        (None, _) => {
            if let Type::Var(..) = t {
                add(t);
            } else {
                for a in args {
                    collect_needs(a, last, add);
                }
            }
        }
    }
}

/// The head of a type application and its arguments.
fn spine(t: &Type) -> (&Type, Vec<&Type>) {
    let mut args = Vec::new();
    let mut h = t;
    while let Type::App(f, a) = h {
        args.push(a.as_ref());
        h = f;
    }
    args.reverse();
    (h, args)
}

/// The type applied to everything but its last argument.
fn strip_last(t: &Type) -> Type {
    match t {
        Type::App(f, _) => (**f).clone(),
        other => other.clone(),
    }
}

/// The type variables mentioned by a type, in order of first occurrence.
pub fn type_vars(t: &Type, out: &mut Vec<String>) {
    match t {
        Type::Var(v, _) => {
            if !out.contains(v) {
                out.push(v.clone());
            }
        }
        Type::Sig(t, _) => type_vars(t, out),
        Type::App(f, a) | Type::Fun(f, a) => {
            type_vars(f, out);
            type_vars(a, out);
        }
        Type::List(e, _) => type_vars(e, out),
        Type::Tuple(ts, _) => ts.iter().for_each(|x| type_vars(x, out)),
        Type::Qual(cs, b) => {
            cs.iter().for_each(|x| type_vars(x, out));
            type_vars(b, out);
        }
        _ => {}
    }
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
    let last = if drop_last { dd.tyvars.last().map(|t| t.name.clone()) } else { None };
    // A Generic instance needs nothing of its fields' types.
    let mut context = match class.name.as_str() {
        "Generic" => Vec::new(),
        "Generic1" => generic1_context(dd, loc, last.as_deref()),
        _ => field_context(class, dd, loc, last.as_deref()),
    };
    // `Data` has `Typeable` as its superclass: each type parameter must be Typeable, as GHC infers.
    if class.name == "Data" {
        for tv in &kept {
            let typeable = QName { qual: Some("GHC.Internal.Data.Typeable.Internal".into()), name: "Typeable".into() };
            let item = Type::App(Box::new(Type::Con(typeable, loc)), Box::new(Type::Var(tv.name.clone(), loc)));
            if !context.contains(&item) {
                context.push(item);
            }
        }
    }
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
        let tys = field_types(c);
        for (k, (x, y)) in a.iter().zip(&b).enumerate().rev() {
            // Unboxed fields compare with the primitive operations, as GHC derives them.
            let prim = tys.get(k).and_then(|t| match t {
                Type::Con(q, _) => match q.name.as_str() {
                    "Int#" => Some("==#"),
                    "Word#" => Some("eqWord#"),
                    "Char#" => Some("eqChar#"),
                    "Addr#" => Some("eqAddr#"),
                    "Double#" => Some("==##"),
                    "Float#" => Some("eqFloat#"),
                    _ => None,
                },
                _ => None,
            });
            let eq = match prim {
                Some(p) => app(
                    Expr::Var(QName { qual: Some("GHC.Types".into()), name: "isTrue#".into() }, loc),
                    vec![app(
                        Expr::Var(QName { qual: Some("GHC.Prim".into()), name: p.into() }, loc),
                        vec![local(x, loc), local(y, loc)],
                    )],
                ),
                None => app(base_var("==", loc), vec![local(x, loc), local(y, loc)]),
            };
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
/// The constructor index of a value, as a boxed `Int`.
fn tag_of(dd: &DataDecl, module: &str, e: Expr, loc: Loc) -> Expr {
    let boxed = Expr::Con(QName { qual: Some("GHC.Types".into()), name: "I#".into() }, loc);
    app(boxed, vec![tag_of_prim(dd, module, e, loc)])
}

/// The constructor index of a value, as an `Int#`.
fn tag_of_prim(dd: &DataDecl, module: &str, e: Expr, loc: Loc) -> Expr {
    let alts: Vec<Alt> = dd
        .cons
        .iter()
        .enumerate()
        .map(|(i, c)| Alt {
            pat: con_pat(module, &c.name, (0..arity(c)).map(|_| Pat::Wild(loc)).collect(), loc),
            rhs: Rhs {
                // A primitive literal: the tag is compared without needing `Num`.
                body: Body::Plain(Expr::Lit(
                    Lit::Prim(crate::syntax::token::PrimLit::Int(i.to_string())),
                    loc,
                )),
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
            let ftys = field_types(c);
            let mut cmps: Vec<Expr> = a
                .iter()
                .zip(&b)
                .enumerate()
                .map(|(k, (p, q))| match ftys.get(k).and_then(prim_ops) {
                    Some((eq, lt)) => {
                        // Unboxed fields: equal, less or greater by the primitive operations.
                        let prim = |name: &str| Expr::Var(QName { qual: Some("GHC.Prim".into()), name: name.into() }, loc);
                        let test = |op: &str| {
                            app(
                                Expr::Var(QName { qual: Some("GHC.Types".into()), name: "isTrue#".into() }, loc),
                                vec![app(prim(op), vec![local(p, loc), local(q, loc)])],
                            )
                        };
                        Expr::If(
                            Box::new(test(eq)),
                            Box::new(con_expr("GHC.Types", "EQ", loc)),
                            Box::new(Expr::If(
                                Box::new(test(lt)),
                                Box::new(con_expr("GHC.Types", "LT", loc)),
                                Box::new(con_expr("GHC.Types", "GT", loc)),
                                loc,
                            )),
                            loc,
                        )
                    }
                    None => app(base_var("compare", loc), vec![local(p, loc), local(q, loc)]),
                })
                .collect();
            let mut acc = cmps.pop().expect("at least one field");
            while let Some(prev) = cmps.pop() {
                // case prev of EQ -> acc; r -> r
                let r = checker.fresh_name("r");
                acc = Expr::Case(
                    Box::new(prev),
                    vec![
                        Alt {
                            pat: Pat::Con(QName { qual: Some(defining_module("EQ").into()), name: "EQ".into() }, vec![], loc),
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
                            base_var("compareInt#", loc),
                            vec![
                                tag_of_prim(dd, &module, local(&x, loc), loc),
                                tag_of_prim(dd, &module, local(&y, loc), loc),
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
        // An unboxed field is shown as its boxed value and a `#`, as GHC does.
        // (Through synonyms: happy's `type Happy_Int = Int#`.) Word# and Double# take `##`.
        let ftys: Vec<Option<(&str, &str)>> = field_types(c)
            .iter()
            .map(|t| {
                let mut tvs: Vec<String> = dd.tyvars.iter().map(|tv| tv.name.clone()).collect();
                match checker.convert_type(t, &mut tvs).ok()? {
                    super::types::Ty::Con(name) => match name.as_str() {
                        "GHC.Prim.Int#" => Some(("I#", "#")),
                        "GHC.Prim.Word#" => Some(("W#", "##")),
                        "GHC.Prim.Char#" => Some(("C#", "#")),
                        "GHC.Prim.Double#" => Some(("D#", "##")),
                        "GHC.Prim.Float#" => Some(("F#", "#")),
                        _ => None,
                    },
                    _ => None,
                }
            })
            .collect();
        let shown_field = |i: usize, prec: i64| -> Expr {
            let v = &a[i];
            match ftys.get(i).copied().flatten() {
                Some((con, hashes)) => compose(
                    vec![
                        // At precedence 0, as GHC shows them (`Reduce -2#`).
                        app(base_var("showsPrec", loc), vec![int(0, loc), app(base_con(con, loc), vec![local(v, loc)])]),
                        app(base_var("showString", loc), vec![text(hashes, loc)]),
                    ],
                    loc,
                ),
                None => show_arg(v, prec),
            }
        };
        let shows_str = |s: &str| app(base_var("showString", loc), vec![text(s, loc)]);
        let body = if n == 0 {
            let name = display_name(&c.name);
            shows_str(&name)
        } else if c.infix && n == 2 && field_names(c).is_empty() {
            // Infix constructor at the default precedence 9.
            let prec = 9;
            let parts = vec![
                shown_field(0, prec + 1),
                app(base_var("showString", loc), vec![text(&format!(" {} ", infix_name(&c.name)), loc)]),
                shown_field(1, prec + 1),
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
                for i in 0..a.len() {
                    if i > 0 {
                        parts.push(app(base_var("showChar", loc), vec![ch(' ', loc)]));
                    }
                    parts.push(shown_field(i, 11));
                }
            } else {
                parts.push(shows_str(&format!("{} {{", display_name(&c.name))));
                for (i, f) in names.iter().enumerate() {
                    let label = if i == 0 {
                        format!("{} = ", display_name(f))
                    } else {
                        format!(", {} = ", display_name(f))
                    };
                    parts.push(shows_str(&label));
                    parts.push(shown_field(i, 0));
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
    // The unit and tuple constructors are written as themselves: `()`, `(,)`.
    if name.starts_with("(") {
        return name.to_string();
    }
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

/// A balanced binary tree over items, as GHC builds the sums and products of a representation: the
/// first half on the left.
fn balanced<T: Clone>(items: &[T], leaf: &dyn Fn(&T) -> Type, node: &dyn Fn(Type, Type) -> Type, empty: Type) -> Type {
    match items.len() {
        0 => empty,
        1 => leaf(&items[0]),
        n => {
            let (l, r) = items.split_at(n / 2);
            node(balanced(l, leaf, node, empty.clone()), balanced(r, leaf, node, empty))
        }
    }
}

/// The injections (L1/R1, false for left) of constructor `i` of `n` in a balanced sum.
fn sum_path(n: usize, i: usize) -> Vec<bool> {
    if n <= 1 {
        return Vec::new();
    }
    let l = n / 2;
    let mut out = Vec::new();
    if i < l {
        out.push(false);
        out.extend(sum_path(l, i));
    } else {
        out.push(true);
        out.extend(sum_path(n - l, i - l));
    }
    out
}

/// How a field is represented by Generic1, relative to the last type parameter `v`: `v` itself
/// (`Par1`), a type not mentioning it (`Rec0`), `f v` (`Rec1 f`), or `f t` where `t` mentions it
/// (`f :.: rep t`, built with `fmap`).
#[derive(Clone)]
enum FieldRep {
    Par,
    /// A field of an unlifted type (`Int#`): `UInt` and friends, with their constructor and field.
    Unlifted(&'static str, &'static str),
    Rec0(Type),
    Rec1(Type),
    Comp(Type, Box<FieldRep>),
}

fn field_rep(t: &Type, last: Option<&str>) -> FieldRep {
    if let Type::Con(q, _) = t {
        let unlifted = match q.name.as_str() {
            "Addr#" => Some(("UAddr", "uAddr#")),
            "Char#" => Some(("UChar", "uChar#")),
            "Double#" => Some(("UDouble", "uDouble#")),
            "Float#" => Some(("UFloat", "uFloat#")),
            "Int#" => Some(("UInt", "uInt#")),
            "Word#" => Some(("UWord", "uWord#")),
            _ => None,
        };
        if let Some((con, sel)) = unlifted {
            return FieldRep::Unlifted(con, sel);
        }
    }
    let Some(v) = last else { return FieldRep::Rec0(t.clone()) };
    if matches!(t, Type::Var(x, _) if x == v) {
        return FieldRep::Par;
    }
    if !mentions(t, v) {
        return FieldRep::Rec0(t.clone());
    }
    // The built-in syntax: `[t]` is `[] t`, `(x, .., t)` is `(,..) x .. t`.
    let desugared;
    let t = match t {
        Type::List(e, l) => {
            desugared = Type::App(Box::new(Type::Con(QName { qual: Some("Builtin".into()), name: "[]".into() }, *l)), e.clone());
            &desugared
        }
        Type::Fun(a, r) => {
            let arrow = Type::Con(QName { qual: Some("Builtin".into()), name: "->".into() }, Loc::default());
            desugared = Type::App(Box::new(Type::App(Box::new(arrow), a.clone())), r.clone());
            &desugared
        }
        Type::Tuple(ts, l) if ts.len() >= 2 => {
            let con = Type::Con(QName { qual: Some("Builtin".into()), name: format!("({})", ",".repeat(ts.len() - 1)) }, *l);
            desugared = ts.iter().fold(con, |acc, x| Type::App(Box::new(acc), Box::new(x.clone())));
            &desugared
        }
        _ => t,
    };
    if let Type::App(f, a) = t {
        if !mentions(f, v) {
            if matches!(a.as_ref(), Type::Var(x, _) if x == v) {
                return FieldRep::Rec1((**f).clone());
            }
            return FieldRep::Comp((**f).clone(), Box::new(field_rep(a, last)));
        }
    }
    // A shape Generic1 cannot represent (the parameter under a function arrow, say): as a constant.
    FieldRep::Rec0(t.clone())
}

/// `Generic` and `Generic1` (DeriveGeneric): `Rep`/`Rep1` with GHC's metadata (`MetaData`,
/// `MetaCons`, `MetaSel`), balanced sums of constructors and products of fields, and the conversions
/// `from`/`to` (`from1`/`to1`).
fn derive_generic(checker: &mut Checker, dd: &DataDecl, one: bool) -> Vec<Decl> {
    let loc = dd.loc;
    let module = checker.module.clone();
    const G: &str = "GHC.Internal.Generics";
    let gty = |n: &str| Type::Con(QName { qual: Some(G.into()), name: n.into() }, loc);
    let prom = |m: &str, n: &str| Type::Con(QName { qual: Some(m.into()), name: format!("'{n}") }, loc);
    let sym = |s: &str| Type::Lit(Lit::Str(s.to_string()), loc);
    let tapp = |f: Type, args: Vec<Type>| args.into_iter().fold(f, |acc, a| Type::App(Box::new(acc), Box::new(a)));
    let pkg = if module.starts_with("GHC.Internal.") { "ghc-internal" } else { "main" };
    let bool_ty = |b: bool| prom("GHC.Types", if b { "True" } else { "False" });
    let last: Option<String> = if one { dd.tyvars.last().map(|t| t.name.clone()) } else { None };
    fn rep_ty(r: &FieldRep, gty: &dyn Fn(&str) -> Type, tapp: &dyn Fn(Type, Vec<Type>) -> Type) -> Type {
        match r {
            FieldRep::Par => gty("Par1"),
            FieldRep::Unlifted(con, _) => gty(con),
            FieldRep::Rec0(t) => tapp(gty("Rec0"), vec![t.clone()]),
            FieldRep::Rec1(f) => tapp(gty("Rec1"), vec![f.clone()]),
            FieldRep::Comp(f, inner) => tapp(gty(":.:"), vec![f.clone(), rep_ty(inner, gty, tapp)]),
        }
    }
    // Rep
    let con_rep = |c: &ConDecl| -> Type {
        let names = field_names(c);
        let fields = field_types(c);
        let stricts: Vec<bool> = match &c.fields {
            ConFields::Positional(fs) => fs.iter().map(|f| f.strict).collect(),
            ConFields::Record(groups) => groups.iter().flat_map(|(ns, f)| ns.iter().map(move |_| f.strict)).collect(),
        };
        let sels: Vec<(Option<String>, FieldRep, bool)> = fields
            .iter()
            .enumerate()
            .map(|(i, t)| (names.get(i).cloned(), field_rep(t, last.as_deref()), stricts.get(i).copied().unwrap_or(false)))
            .collect();
        let leaf = |(name, r, strict): &(Option<String>, FieldRep, bool)| -> Type {
            let sel_name = match name {
                Some(n) => tapp(prom("GHC.Internal.Maybe", "Just"), vec![sym(n)]),
                None => prom("GHC.Internal.Maybe", "Nothing"),
            };
            let meta = tapp(
                prom(G, "MetaSel"),
                vec![
                    sel_name,
                    prom(G, "NoSourceUnpackedness"),
                    prom(G, if *strict { "SourceStrict" } else { "NoSourceStrictness" }),
                    prom(G, if *strict { "DecidedStrict" } else { "DecidedLazy" }),
                ],
            );
            tapp(gty("S1"), vec![meta, rep_ty(r, &gty, &tapp)])
        };
        let node = |a: Type, b: Type| tapp(gty(":*:"), vec![a, b]);
        let prod = balanced(&sels, &leaf, &node, gty("U1"));
        let symbolic = is_operator(&c.name) && !c.name.starts_with('[') && !c.name.starts_with('(');
        let fixity = if c.infix || symbolic {
            // `:` is infixr 5; other operator constructors are taken as infixl 9.
            let (assoc, prec) = if c.name == ":" { ("RightAssociative", "5") } else { ("LeftAssociative", "9") };
            tapp(prom(G, "InfixI"), vec![prom(G, assoc), Type::Lit(Lit::Int(prec.into()), loc)])
        } else {
            prom(G, "PrefixI")
        };
        let record = matches!(c.fields, ConFields::Record(_));
        let meta = tapp(prom(G, "MetaCons"), vec![sym(&c.name), fixity, bool_ty(record)]);
        tapp(gty("C1"), vec![meta, prod])
    };
    let sum = balanced(&dd.cons, &con_rep, &|a, b| tapp(gty(":+:"), vec![a, b]), gty("V1"));
    // The built-in types are named as GHC names them (GHC.Types.List, GHC.Tuple.Tuple2).
    let (dname, dmodule, dpkg) = if module == "Builtin" {
        let n = if dd.name == "[]" {
            "List".to_string()
        } else if dd.name == "()" {
            "Unit".to_string()
        } else {
            format!("Tuple{}", dd.name.matches(',').count() + 1)
        };
        let m = if dd.name == "[]" { "GHC.Types" } else { "GHC.Tuple" };
        (n, m.to_string(), "ghc-prim")
    } else {
        (dd.name.clone(), module.clone(), pkg)
    };
    let meta = tapp(prom(G, "MetaData"), vec![sym(&dname), sym(&dmodule), sym(dpkg), bool_ty(dd.newtype)]);
    let rep = tapp(gty("D1"), vec![meta, sum]);
    let kept = if one { &dd.tyvars[..dd.tyvars.len().saturating_sub(1)] } else { &dd.tyvars[..] };
    let head = kept.iter().fold(
        Type::Con(QName { qual: Some(module.clone()), name: dd.name.clone() }, loc),
        |acc, tv| Type::App(Box::new(acc), Box::new(Type::Var(tv.name.clone(), loc))),
    );
    let rep_inst = Decl::TypeInstance {
        name: QName { qual: Some(G.into()), name: if one { "Rep1" } else { "Rep" }.into() },
        equation: crate::syntax::ast::TypeEquation { args: vec![head], rhs: rep, loc },
        loc,
    };
    // The conversions.
    let gcon = |n: &str| QName { qual: Some(G.into()), name: n.into() };
    let gvar = |n: &str| Expr::Var(QName { qual: Some(G.into()), name: n.into() }, loc);
    let fmap = || base_var("fmap", loc);
    // The representation of a field value, and back.
    fn wrap(r: &FieldRep, e: Expr, checker: &mut Checker, loc: Loc, gcon: &dyn Fn(&str) -> QName, fmap: &dyn Fn() -> Expr) -> Expr {
        match r {
            FieldRep::Par => app(Expr::Con(gcon("Par1"), loc), vec![e]),
            FieldRep::Unlifted(con, _) => app(Expr::Con(gcon(con), loc), vec![e]),
            FieldRep::Rec0(_) => app(Expr::Con(gcon("K1"), loc), vec![e]),
            FieldRep::Rec1(_) => app(Expr::Con(gcon("Rec1"), loc), vec![e]),
            FieldRep::Comp(_, inner) => {
                let z = checker.fresh_name("gz");
                let body = wrap(inner, local(&z, loc), checker, loc, gcon, fmap);
                let f = Expr::Lam(vec![Pat::Var(z, loc)], Box::new(body), loc);
                app(Expr::Con(gcon("Comp1"), loc), vec![app(fmap(), vec![f, e])])
            }
        }
    }
    fn unwrap(r: &FieldRep, e: Expr, checker: &mut Checker, loc: Loc, gvar: &dyn Fn(&str) -> Expr, fmap: &dyn Fn() -> Expr) -> Expr {
        match r {
            FieldRep::Par => app(gvar("unPar1"), vec![e]),
            FieldRep::Unlifted(_, sel) => app(gvar(sel), vec![e]),
            FieldRep::Rec0(_) => app(gvar("unK1"), vec![e]),
            FieldRep::Rec1(_) => app(gvar("unRec1"), vec![e]),
            FieldRep::Comp(_, inner) => {
                let z = checker.fresh_name("gz");
                let body = unwrap(inner, local(&z, loc), checker, loc, gvar, fmap);
                let f = Expr::Lam(vec![Pat::Var(z, loc)], Box::new(body), loc);
                app(fmap(), vec![f, app(gvar("unComp1"), vec![e])])
            }
        }
    }
    // The product of the fields' representations: M1 r :*: ..; and its pattern, binding each r.
    fn prod_expr(items: &[Expr], loc: Loc, gcon: &dyn Fn(&str) -> QName) -> Expr {
        match items.len() {
            0 => Expr::Con(gcon("U1"), loc),
            1 => app(Expr::Con(gcon("M1"), loc), vec![items[0].clone()]),
            k => {
                let (l, r) = items.split_at(k / 2);
                app(Expr::Con(gcon(":*:"), loc), vec![prod_expr(l, loc, gcon), prod_expr(r, loc, gcon)])
            }
        }
    }
    fn prod_pat(vars: &[String], loc: Loc, gcon: &dyn Fn(&str) -> QName) -> Pat {
        match vars.len() {
            0 => Pat::Con(gcon("U1"), vec![], loc),
            1 => Pat::Con(gcon("M1"), vec![Pat::Var(vars[0].clone(), loc)], loc),
            k => {
                let (l, r) = vars.split_at(k / 2);
                Pat::Con(gcon(":*:"), vec![prod_pat(l, loc, gcon), prod_pat(r, loc, gcon)], loc)
            }
        }
    }
    let n = dd.cons.len();
    let mut from_alts = Vec::new();
    let mut to_alts = Vec::new();
    for (i, c) in dd.cons.iter().enumerate() {
        let reps: Vec<FieldRep> = field_types(c).iter().map(|t| field_rep(t, last.as_deref())).collect();
        let vars = fresh_vars(checker, "g", reps.len());
        let reprs = fresh_vars(checker, "gr", reps.len());
        let wrapped: Vec<Expr> = reps.iter().zip(&vars).map(|(r, v)| wrap(r, local(v, loc), checker, loc, &gcon, &fmap)).collect();
        let path = sum_path(n, i);
        let mut e = app(Expr::Con(gcon("M1"), loc), vec![prod_expr(&wrapped, loc, &gcon)]);
        let mut p = Pat::Con(gcon("M1"), vec![prod_pat(&reprs, loc, &gcon)], loc);
        for right in path.iter().rev() {
            let inj = if *right { "R1" } else { "L1" };
            e = app(Expr::Con(gcon(inj), loc), vec![e]);
            p = Pat::Con(gcon(inj), vec![p], loc);
        }
        let con_p = con_pat(&module, &c.name, vars.iter().map(|v| Pat::Var(v.clone(), loc)).collect(), loc);
        let args: Vec<Expr> = reps.iter().zip(&reprs).map(|(r, v)| unwrap(r, local(v, loc), checker, loc, &gvar, &fmap)).collect();
        let con_e = app(con_expr(&module, &c.name, loc), args);
        from_alts.push(crate::syntax::ast::Alt { pat: con_p, rhs: Rhs { body: Body::Plain(e), wheres: Vec::new() }, loc });
        to_alts.push(crate::syntax::ast::Alt { pat: p, rhs: Rhs { body: Body::Plain(con_e), wheres: Vec::new() }, loc });
    }
    let x = checker.fresh_name("gx");
    let r = checker.fresh_name("gr");
    let (from_name, to_name) = if one { ("from1", "to1") } else { ("from", "to") };
    let mut out = vec![rep_inst];
    if n == 0 {
        // No constructors: both directions are the empty case.
        let empty = |v: &str| Expr::Case(Box::new(local(v, loc)), Vec::new(), loc);
        out.push(fun(from_name, vec![clause(vec![Pat::Var(x.clone(), loc)], app(Expr::Con(gcon("M1"), loc), vec![empty(&x)]), loc)], loc));
        out.push(fun(to_name, vec![clause(vec![Pat::Con(gcon("M1"), vec![Pat::Var(r.clone(), loc)], loc)], empty(&r), loc)], loc));
        return out;
    }
    let from_body = app(Expr::Con(gcon("M1"), loc), vec![Expr::Case(Box::new(local(&x, loc)), from_alts, loc)]);
    let to_body = Expr::Case(Box::new(local(&r, loc)), to_alts, loc);
    out.push(fun(from_name, vec![clause(vec![Pat::Var(x, loc)], from_body, loc)], loc));
    out.push(fun(to_name, vec![clause(vec![Pat::Con(gcon("M1"), vec![Pat::Var(r, loc)], loc)], to_body, loc)], loc));
    out
}

/// `Data` (DeriveDataTypeable): `gfoldl` and `gunfold` over the fields, `toConstr` and `dataTypeOf`
/// from a `DataType` with one `Constr` per constructor, as GHC derives them.
fn derive_data(checker: &mut Checker, dd: &DataDecl) -> Vec<Decl> {
    let loc = dd.loc;
    let module = checker.module.clone();
    let dd_var = |n: &str| Expr::Var(QName { qual: Some("GHC.Internal.Data.Data".into()), name: n.into() }, loc);
    let dd_con = |n: &str| Expr::Con(QName { qual: Some("GHC.Internal.Data.Data".into()), name: n.into() }, loc);
    let list = |items: Vec<Expr>| Expr::List(items, loc);
    // let ty = mkDataType "M.T" [c1, ..]; ci = mkConstr ty "Ci" [fields] Prefix in body
    let ty_name = checker.fresh_name("ty");
    let con_names: Vec<String> = dd.cons.iter().map(|_| checker.fresh_name("con")).collect();
    let with_tables = |body: Expr| -> Expr {
        let mut decls = vec![fun(
            &ty_name,
            vec![clause(
                vec![],
                app(
                    dd_var("mkDataType"),
                    vec![text(&format!("{module}.{}", dd.name), loc), list(con_names.iter().map(|c| local(c, loc)).collect())],
                ),
                loc,
            )],
            loc,
        )];
        for (c, cn) in dd.cons.iter().zip(&con_names) {
            let fields = list(field_names(c).iter().map(|f| text(f, loc)).collect());
            let fixity = if c.infix || is_operator(&c.name) { dd_con("Infix") } else { dd_con("Prefix") };
            decls.push(fun(
                cn,
                vec![clause(vec![], app(dd_var("mkConstr"), vec![local(&ty_name, loc), text(&c.name, loc), fields, fixity]), loc)],
                loc,
            ));
        }
        Expr::Let(decls, Box::new(body), loc)
    };
    let k = checker.fresh_name("k");
    let z = checker.fresh_name("z");
    // gfoldl k z (C x1 .. xn) = (z C `k` x1) .. `k` xn
    let mut gfoldl = Vec::new();
    for c in &dd.cons {
        let xs = fresh_vars(checker, "x", arity(c));
        let pat = con_pat(&module, &c.name, xs.iter().map(|v| Pat::Var(v.clone(), loc)).collect(), loc);
        let body = xs.iter().fold(app(local(&z, loc), vec![con_expr(&module, &c.name, loc)]), |acc, x| {
            app(local(&k, loc), vec![acc, local(x, loc)])
        });
        gfoldl.push(clause(vec![Pat::Var(k.clone(), loc), Pat::Var(z.clone(), loc), pat], body, loc));
    }
    // gunfold k z c = case constrIndex c of 1 -> k (.. (z C1)); ..
    let cvar = checker.fresh_name("c");
    let alts: Vec<crate::syntax::ast::Alt> = dd
        .cons
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let body = (0..arity(c)).fold(app(local(&z, loc), vec![con_expr(&module, &c.name, loc)]), |acc, _| app(local(&k, loc), vec![acc]));
            let pat = if i + 1 == dd.cons.len() { Pat::Wild(loc) } else { Pat::Lit(Lit::Int((i + 1).to_string()), loc) };
            crate::syntax::ast::Alt { pat, rhs: Rhs { body: Body::Plain(body), wheres: Vec::new() }, loc }
        })
        .collect();
    let gunfold_body = Expr::Case(Box::new(app(dd_var("constrIndex"), vec![local(&cvar, loc)])), alts, loc);
    let gunfold = clause(vec![Pat::Var(k.clone(), loc), Pat::Var(z.clone(), loc), Pat::Var(cvar.clone(), loc)], gunfold_body, loc);
    // toConstr x = case x of C1 {} -> con1; ..
    let xv = checker.fresh_name("x");
    let to_alts: Vec<crate::syntax::ast::Alt> = dd
        .cons
        .iter()
        .zip(&con_names)
        .map(|(c, cn)| crate::syntax::ast::Alt {
            pat: con_pat(&module, &c.name, (0..arity(c)).map(|_| Pat::Wild(loc)).collect(), loc),
            rhs: Rhs { body: Body::Plain(local(cn, loc)), wheres: Vec::new() },
            loc,
        })
        .collect();
    let to_constr = with_tables(Expr::Case(Box::new(local(&xv, loc)), to_alts, loc));
    let data_type_of = with_tables(local(&ty_name, loc));
    let mut out = Vec::new();
    if !dd.cons.is_empty() {
        out.push(fun("gfoldl", gfoldl, loc));
        out.push(fun("gunfold", vec![gunfold], loc));
        out.push(fun("toConstr", vec![clause(vec![Pat::Var(xv, loc)], to_constr, loc)], loc));
    }
    out.push(fun("dataTypeOf", vec![clause(vec![Pat::Wild(loc)], data_type_of, loc)], loc));
    out
}

/// `Lift` (DeriveLift): Template Haskell cannot run yet, so the methods are errors.
/// `lift (C x1 .. xn) = conE 'C `appE` lift x1 .. `appE` lift xn`, and `liftTyped` its
/// coercion, as GHC derives them.
fn derive_lift(checker: &mut Checker, dd: &DataDecl) -> Vec<Decl> {
    let loc = dd.loc;
    let module = checker.module.clone();
    let q = |m: &str, n: &str| Expr::Var(QName { qual: Some(m.into()), name: n.into() }, loc);
    let mut clauses = Vec::new();
    for c in &dd.cons {
        let xs = fresh_vars(checker, "x", arity(c));
        let pat = con_pat(&module, &c.name, xs.iter().map(|v| Pat::Var(v.clone(), loc)).collect(), loc);
        let name = app(
            q("GHC.Internal.TH.Syntax", "mkNameG_d"),
            vec![text("main", loc), text(&module, loc), text(&c.name, loc)],
        );
        let mut body = app(q("GHC.Internal.TH.Lib", "conE"), vec![name]);
        for x in &xs {
            let lifted = app(q("GHC.Internal.TH.Lift", "lift"), vec![local(x, loc)]);
            body = app(q("GHC.Internal.TH.Lib", "appE"), vec![body, lifted]);
        }
        clauses.push(clause(vec![pat], body, loc));
    }
    // A type without constructors has no values to lift.
    if clauses.is_empty() {
        let body = app(q("GHC.Internal.Err", "errorWithoutStackTrace"), vec![text("Void lift", loc)]);
        clauses.push(clause(vec![Pat::Wild(loc)], body, loc));
    }
    let y = checker.fresh_name("y");
    let typed = app(
        q("GHC.Internal.TH.Syntax", "unsafeCodeCoerce"),
        vec![app(q("GHC.Internal.TH.Lift", "lift"), vec![local(&y, loc)])],
    );
    vec![
        fun("lift", clauses, loc),
        fun("liftTyped", vec![clause(vec![Pat::Var(y, loc)], typed, loc)], loc),
    ]
}

/// `readsPrec`: each constructor is read by a list comprehension over `lex` tokens and
/// `readsPrec 11` for its arguments; the alternatives are concatenated.
fn derive_read(checker: &mut Checker, dd: &DataDecl) -> Vec<Decl> {
    use crate::syntax::ast::Stmt;
    let loc = dd.loc;
    let d = checker.fresh_name("d");
    let s = checker.fresh_name("s");
    let module = checker.module.clone();
    let mut alts: Vec<Expr> = Vec::new();
    for c in &dd.cons {
        let n = arity(c);
        let names = field_names(c);
        let is_record = !names.is_empty();
        let args = fresh_vars(checker, "a", n);
        // Each alternative is `readParen cond (\r -> [ (value, rest) | quals ]) s`, so that
        // records (precedence 11) and nullary constructors get their own parenthesis rule.
        let r = checker.fresh_name("r");
        let mut quals: Vec<Stmt> = Vec::new();
        let mut cur = r.clone();
        let infix = c.infix && n == 2 && !is_record;
        if infix {
            // `a op b` at precedence 9: operands are read at precedence 10.
            read_value(checker, &mut quals, &mut cur, &args[0], 10, loc);
            if is_operator(&c.name) {
                read_token(checker, &mut quals, &mut cur, &c.name, loc);
            } else {
                read_token(checker, &mut quals, &mut cur, "`", loc);
                read_token(checker, &mut quals, &mut cur, &c.name, loc);
                read_token(checker, &mut quals, &mut cur, "`", loc);
            }
            read_value(checker, &mut quals, &mut cur, &args[1], 10, loc);
        } else if is_record {
            read_con_name(checker, &mut quals, &mut cur, &c.name, loc);
            read_token(checker, &mut quals, &mut cur, "{", loc);
            for (i, (fname, a)) in names.iter().zip(&args).enumerate() {
                if i > 0 {
                    read_token(checker, &mut quals, &mut cur, ",", loc);
                }
                read_con_name(checker, &mut quals, &mut cur, fname, loc);
                read_token(checker, &mut quals, &mut cur, "=", loc);
                read_value(checker, &mut quals, &mut cur, a, 0, loc);
            }
            read_token(checker, &mut quals, &mut cur, "}", loc);
        } else {
            read_con_name(checker, &mut quals, &mut cur, &c.name, loc);
            for a in &args {
                read_value(checker, &mut quals, &mut cur, a, 11, loc);
            }
        }
        let value = args
            .iter()
            .fold(con_expr(&module, &c.name, loc), |acc, a| app(acc, vec![local(a, loc)]));
        let body = Expr::Tuple(vec![Some(value), Some(local(&cur, loc))], loc);
        let lc = Expr::ListComp(Box::new(body), vec![quals], loc);
        let cond = if infix {
            app(base_var(">", loc), vec![local(&d, loc), int(9, loc)])
        } else if is_record {
            app(base_var(">", loc), vec![local(&d, loc), int(11, loc)])
        } else if n > 0 {
            app(base_var(">", loc), vec![local(&d, loc), int(10, loc)])
        } else {
            app(base_var("not", loc), vec![base_var("otherwise", loc)])
        };
        let lam = Expr::Lam(vec![Pat::Var(r, loc)], Box::new(lc), loc);
        alts.push(app(
            base_var("readParen", loc),
            vec![cond, lam, local(&s, loc)],
        ));
    }
    let all = alts
        .into_iter()
        .rev()
        .fold(Expr::List(vec![], loc), |acc, alt| app(base_var("++", loc), vec![alt, acc]));
    vec![fun(
        "readsPrec",
        vec![clause(vec![Pat::Var(d, loc), Pat::Var(s, loc)], all, loc)],
        loc,
    )]
}

/// Adds a generator that consumes the lexeme `tok` from the input `cur`.
fn read_token(checker: &mut Checker, quals: &mut Vec<crate::syntax::ast::Stmt>, cur: &mut String, tok: &str, loc: Loc) {
    use crate::syntax::ast::Stmt;
    let next = checker.fresh_name("s");
    quals.push(Stmt::Bind {
        pat: Pat::Tuple(
            vec![Pat::Lit(Lit::Str(tok.to_string()), loc), Pat::Var(next.clone(), loc)],
            loc,
        ),
        expr: app(base_var("lex", loc), vec![local(cur, loc)]),
        loc,
    });
    *cur = next;
}

/// Consumes a constructor or field name as written by `show`: operators are parenthesised.
fn read_con_name(
    checker: &mut Checker,
    quals: &mut Vec<crate::syntax::ast::Stmt>,
    cur: &mut String,
    name: &str,
    loc: Loc,
) {
    if is_operator(name) {
        read_token(checker, quals, cur, "(", loc);
        read_token(checker, quals, cur, name, loc);
        read_token(checker, quals, cur, ")", loc);
    } else {
        read_token(checker, quals, cur, name, loc);
    }
}

/// An infix constructor as `show` writes it: operators bare, identifiers in backticks.
fn infix_name(name: &str) -> String {
    if is_operator(name) {
        name.to_string()
    } else {
        format!("`{name}`")
    }
}

/// Adds a generator that reads one value of type `var`'s type at precedence `prec`.
fn read_value(
    checker: &mut Checker,
    quals: &mut Vec<crate::syntax::ast::Stmt>,
    cur: &mut String,
    var: &str,
    prec: i64,
    loc: Loc,
) {
    use crate::syntax::ast::Stmt;
    let next = checker.fresh_name("s");
    quals.push(Stmt::Bind {
        pat: Pat::Tuple(vec![Pat::Var(var.to_string(), loc), Pat::Var(next.clone(), loc)], loc),
        expr: app(base_var("readsPrec", loc), vec![int(prec, loc), local(cur, loc)]),
        loc,
    });
    *cur = next;
}

// ----- constructor classes: Functor, Foldable, Traversable --------------------------

/// The equality and less-than primitives of an unboxed field type.
fn prim_ops(t: &Type) -> Option<(&'static str, &'static str)> {
    match t {
        Type::Con(q, _) => match q.name.as_str() {
            "Int#" => Some(("==#", "<#")),
            "Word#" => Some(("eqWord#", "ltWord#")),
            "Char#" => Some(("eqChar#", "ltChar#")),
            "Addr#" => Some(("eqAddr#", "ltAddr#")),
            "Double#" => Some(("==##", "<##")),
            "Float#" => Some(("eqFloat#", "ltFloat#")),
            _ => None,
        },
        _ => None,
    }
}

/// The field types of a constructor, in order.
pub(super) fn field_types(c: &ConDecl) -> Vec<Type> {
    match &c.fields {
        ConFields::Positional(fs) => fs.iter().map(|f| f.ty.clone()).collect(),
        ConFields::Record(groups) => groups
            .iter()
            .flat_map(|(ns, f)| ns.iter().map(move |_| f.ty.clone()))
            .collect(),
    }
}

/// A constructor's field types with type synonyms expanded, for the derivations that follow a
/// field's structure (Functor, Foldable, Traversable): `[LFormula a]` with
/// `type LFormula a = Located String (Formula a)` maps inside the `Located`.
fn expanded_field_types(checker: &Checker, c: &ConDecl) -> Vec<Type> {
    field_types(c).iter().map(|t| expand_synonyms(checker, t, 0)).collect()
}

fn expand_synonyms(checker: &Checker, t: &Type, depth: usize) -> Type {
    if depth > 50 {
        return t.clone();
    }
    let mut head = t;
    let mut args: Vec<&Type> = Vec::new();
    while let Type::App(f, x) = head {
        args.push(x);
        head = f;
    }
    args.reverse();
    if let Type::Con(q, _) = head {
        let key = checker.resolve_type(&super::expr::qkey(q));
        if let Some(syn) = checker.env.synonyms.get(&key) {
            let n = syn.nparams as usize;
            if args.len() >= n {
                let params: Vec<Type> = args[..n].iter().map(|a| (*a).clone()).collect();
                if let Some(rhs) = ty_to_ast(&syn.rhs, &params) {
                    let whole = args[n..].iter().fold(rhs, |acc, a| Type::App(Box::new(acc), Box::new((*a).clone())));
                    return expand_synonyms(checker, &whole, depth + 1);
                }
            }
        }
    }
    let go = |x: &Type| Box::new(expand_synonyms(checker, x, depth + 1));
    match t {
        Type::App(f, x) => Type::App(go(f), go(x)),
        Type::Fun(a, b) => Type::Fun(go(a), go(b)),
        Type::List(e, l) => Type::List(go(e), *l),
        Type::Tuple(ts, l) => Type::Tuple(ts.iter().map(|x| expand_synonyms(checker, x, depth + 1)).collect(), *l),
        _ => t.clone(),
    }
}

/// A checked type (a synonym's right-hand side) as syntax, its parameters `Gen(i)` being
/// `params[i]`. None for what syntax cannot say here (unification variables).
fn ty_to_ast(t: &Ty, params: &[Type]) -> Option<Type> {
    let loc = Loc::default();
    let (head, args) = t.split_apps();
    let args: Vec<Type> = args.into_iter().map(|a| ty_to_ast(a, params)).collect::<Option<_>>()?;
    let con = match head {
        Ty::Gen(i) => params.get(*i as usize)?.clone(),
        Ty::Con(c) if c == super::types::ARROW && args.len() == 2 => {
            return Some(Type::Fun(Box::new(args[0].clone()), Box::new(args[1].clone())));
        }
        Ty::Con(c) if c == super::types::LIST && args.len() == 1 => return Some(Type::List(Box::new(args[0].clone()), loc)),
        Ty::Con(c) if c.starts_with("Builtin.(,") && c.len() - "Builtin.()".len() + 1 == args.len() => {
            return Some(Type::Tuple(args, loc));
        }
        Ty::Con(c) => {
            let q = match global_split(c) {
                Some((m, n)) => QName { qual: Some(m.to_string()), name: n.to_string() },
                None => QName::unqual(c.clone()),
            };
            Type::Con(q, loc)
        }
        _ => return None,
    };
    Some(args.into_iter().fold(con, |acc, a| Type::App(Box::new(acc), Box::new(a))))
}

/// A global name's module and occurrence (`GHC.Types.Int`, `GHC.Base.:|`).
fn global_split(c: &str) -> Option<(&str, &str)> {
    let mut start = 0;
    while let Some(dot) = c[start..].find('.').map(|i| i + start) {
        let rest = &c[dot + 1..];
        // The occurrence: what follows the last dot that a module component's capital follows.
        if !rest.starts_with(|ch: char| ch.is_ascii_uppercase()) || !rest.contains('.') {
            return if dot > 0 && !rest.is_empty() { Some((&c[..dot], rest)) } else { None };
        }
        start = dot + 1;
    }
    None
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
        let ftys = expanded_field_types(checker, c);
        let xs = fresh_vars(checker, "x", ftys.len());
        let mut args = Vec::new();
        for (t, x) in ftys.iter().zip(&xs) {
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
        // A tuple: its components folded right to left (`(\(t1, t2) -> f1 t1 (f2 t2 acc)) x`).
        Type::Tuple(ts, _) => {
            let names = fresh_vars(checker, "t", ts.len());
            let mut body = acc;
            for (ty, n) in ts.iter().zip(&names).rev() {
                body = fold_expr(checker, ty, last, f, local(n, loc), body, loc);
            }
            let pat = Pat::Tuple(names.iter().map(|n| Pat::Var(n.clone(), loc)).collect(), loc);
            app(Expr::Lam(vec![pat], Box::new(body), loc), vec![x])
        }
        _ => acc,
    }
}

fn fold_inner(checker: &mut Checker, arg: &Type, last: &str, f: &str, x: Expr, acc: Expr, loc: Loc) -> Expr {
    let y = checker.fresh_name("y");
    let a2 = checker.fresh_name("acc");
    let step = fold_expr(checker, arg, last, f, local(&y, loc), local(&a2, loc), loc);
    let lam = Expr::Lam(vec![Pat::Var(y, loc), Pat::Var(a2, loc)], Box::new(step), loc);
    // The class method (`Base.foldr` is the list function of GHC.Base).
    let foldr = Expr::Var(QName { qual: Some("GHC.Internal.Data.Foldable".into()), name: "foldr".into() }, loc);
    app(foldr, vec![lam, acc, x])
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
        let ftys = expanded_field_types(checker, c);
        let xs = fresh_vars(checker, "x", ftys.len());
        let mut acc = local(&z, loc);
        for (t, x) in ftys.iter().zip(&xs).rev() {
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
        // A tuple: its components traversed in order, rebuilt as a tuple
        // (`(\(t1, t2) -> pure (\a1 a2 -> (a1, a2)) <*> .. <*> ..) x`).
        Type::Tuple(ts, _) => {
            let names = fresh_vars(checker, "t", ts.len());
            let outs = fresh_vars(checker, "a", ts.len());
            let rebuild = Expr::Lam(
                outs.iter().map(|o| Pat::Var(o.clone(), loc)).collect(),
                Box::new(Expr::Tuple(outs.iter().map(|o| Some(local(o, loc))).collect(), loc)),
                loc,
            );
            let mut action = app(base_var("pure", loc), vec![rebuild]);
            for (ty, n) in ts.iter().zip(&names) {
                let step = trav_expr(checker, ty, last, f, local(n, loc), loc);
                action = app(base_var("<*>", loc), vec![action, step]);
            }
            let pat = Pat::Tuple(names.iter().map(|n| Pat::Var(n.clone(), loc)).collect(), loc);
            app(Expr::Lam(vec![pat], Box::new(action), loc), vec![x])
        }
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
        let ftys = expanded_field_types(checker, c);
        let xs = fresh_vars(checker, "x", ftys.len());
        let mut action = app(base_var("pure", loc), vec![con_expr(&module, &c.name, loc)]);
        for (t, x) in ftys.iter().zip(&xs) {
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
