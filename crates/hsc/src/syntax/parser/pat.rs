//! Patterns are parsed as expressions and converted here.

use super::{PResult, Parser};
use crate::syntax::ast::*;

impl Parser {
    pub(super) fn expr_to_pat(&self, e: Expr) -> PResult<Pat> {
        Ok(match e {
            Expr::Var(q, loc) => {
                if q.qual.is_some() {
                    return self.err("qualified name in binding position");
                }
                Pat::Var(q.name, loc)
            }
            Expr::Wild(loc) => Pat::Wild(loc),
            Expr::Con(q, loc) => Pat::Con(q, Vec::new(), loc),
            Expr::Lit(l, loc) => Pat::Lit(l, loc),
            Expr::App(..) => {
                let mut args = Vec::new();
                let mut head = e;
                while let Expr::App(f, a) = head {
                    args.push(*a);
                    head = *f;
                }
                args.reverse();
                match head {
                    Expr::Con(q, loc) => {
                        let mut pats = Vec::with_capacity(args.len());
                        for a in args {
                            pats.push(self.expr_to_pat(a)?);
                        }
                        Pat::Con(q, pats, loc)
                    }
                    // The tuple constructor in prefix form, `(,) a b`: the tuple pattern.
                    Expr::Tuple(elems, loc) if elems.iter().all(Option::is_none) && elems.len() == args.len() => {
                        let mut pats = Vec::with_capacity(args.len());
                        for a in args {
                            pats.push(self.expr_to_pat(a)?);
                        }
                        Pat::Tuple(pats, loc)
                    }
                    _ => return self.err("parse error in pattern"),
                }
            }
            Expr::Tuple(elems, loc) => {
                let mut pats = Vec::with_capacity(elems.len());
                for el in elems {
                    match el {
                        Some(x) => pats.push(self.expr_to_pat(x)?),
                        None => return self.err("tuple section in pattern"),
                    }
                }
                Pat::Tuple(pats, loc)
            }
            Expr::List(elems, loc) => {
                let mut pats = Vec::with_capacity(elems.len());
                for x in elems {
                    pats.push(self.expr_to_pat(x)?);
                }
                Pat::List(pats, loc)
            }
            Expr::AsPat(name, inner, loc) => Pat::As(name, Box::new(self.expr_to_pat(*inner)?), loc),
            Expr::LazyPat(inner, loc) => Pat::Lazy(Box::new(self.expr_to_pat(*inner)?), loc),
            Expr::ViewPat(f, inner, loc) => Pat::View(f, Box::new(self.expr_to_pat(*inner)?), loc),
            Expr::BangPat(inner, loc) => Pat::Bang(Box::new(self.expr_to_pat(*inner)?), loc),
            Expr::Sig(inner, ty, loc) => Pat::Sig(Box::new(self.expr_to_pat(*inner)?), ty, loc),
            Expr::RecCon {
                con,
                fields,
                dotdot,
                loc,
            } => {
                let mut out = Vec::with_capacity(fields.len());
                for (f, v) in fields {
                    let p = match v {
                        Some(x) => Some(self.expr_to_pat(x)?),
                        None => None,
                    };
                    out.push((f, p));
                }
                Pat::Rec {
                    con,
                    fields: out,
                    dotdot,
                    loc,
                }
            }
            Expr::OpChain(items, loc) => {
                let mut out: Vec<PatOpItem> = Vec::with_capacity(items.len());
                let mut iter = items.into_iter();
                while let Some(item) = iter.next() {
                    match item {
                        OpItem::Operand(x) => out.push(PatOpItem::Operand(self.expr_to_pat(x)?)),
                        OpItem::Op(q, l) => out.push(PatOpItem::Op(q, l)),
                        OpItem::Neg(l) => match iter.next() {
                            Some(OpItem::Operand(Expr::Lit(lit, _))) => {
                                out.push(PatOpItem::Operand(Pat::NegLit(lit, l)));
                            }
                            _ => return self.err("parse error in pattern"),
                        },
                    }
                }
                if out.len() == 1 {
                    if let Some(PatOpItem::Operand(p)) = out.pop() {
                        return Ok(p);
                    }
                }
                Pat::OpChain(out, loc)
            }
            _ => return self.err("parse error in pattern"),
        })
    }
}
