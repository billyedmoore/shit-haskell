//! Types, contexts and kinds.

use super::{PResult, Parser};
use crate::syntax::ast::*;
use crate::syntax::token::{Kw, ROp, Tok};

impl Parser {
    /// `[forall binders .] [context =>] type`
    pub(super) fn sigtype(&mut self) -> PResult<Type> {
        if self.at_varid("forall") {
            self.bump();
            let mut vars = Vec::new();
            while !self.at_varsym(".") {
                vars.push(self.tyvar_binder()?);
            }
            self.bump();
            let body = self.sigtype()?;
            return Ok(Type::Forall(vars, Box::new(body)));
        }
        let t = self.type_expr()?;
        if self.at_res(ROp::DArrow) {
            self.bump();
            let ctx = context_items(t);
            let body = self.sigtype()?;
            return Ok(Type::Qual(ctx, Box::new(body)));
        }
        Ok(t)
    }

    /// `btype [-> type]`
    pub(super) fn type_expr(&mut self) -> PResult<Type> {
        let t = self.btype()?;
        if self.at_res(ROp::RArrow) {
            self.bump();
            let r = if self.at_varid("forall") {
                self.sigtype()?
            } else {
                self.type_expr()?
            };
            return Ok(Type::Fun(Box::new(t), Box::new(r)));
        }
        Ok(t)
    }

    /// Type application chain with infix type operators.
    pub(super) fn btype(&mut self) -> PResult<Type> {
        let loc = self.loc();
        let mut items = Vec::new();
        let mut app = self.app_type()?;
        while let Some(op) = self.type_operator() {
            items.push(TypeOpItem::Operand(app));
            items.push(TypeOpItem::Op(op, loc));
            app = self.app_type()?;
        }
        if items.is_empty() {
            return Ok(app);
        }
        items.push(TypeOpItem::Operand(app));
        Ok(Type::OpChain(items, loc))
    }

    fn app_type(&mut self) -> PResult<Type> {
        let mut t = self.atype()?;
        while self.starts_atype() {
            let a = self.atype()?;
            t = Type::App(Box::new(t), Box::new(a));
        }
        Ok(t)
    }

    fn starts_atype(&self) -> bool {
        if !self.real() {
            return false;
        }
        match self.tok() {
            Tok::VarId(s) => s.as_str() != "forall",
            Tok::ConId(_)
            | Tok::QConId(..)
            | Tok::Special('(' | '[')
            | Tok::Kw(Kw::Underscore)
            | Tok::Int(_)
            | Tok::Str(_) => true,
            _ => false,
        }
    }

    fn type_operator(&mut self) -> Option<QName> {
        if !self.real() {
            return None;
        }
        let q = match self.tok().clone() {
            Tok::ConSym(s) => QName::unqual(s),
            Tok::QConSym(m, s) => QName {
                qual: Some(m),
                name: s,
            },
            Tok::VarSym(s) if s != "!" && s != "." && s != "*" => QName::unqual(s),
            Tok::Res(ROp::Tilde) => QName::unqual("~"),
            Tok::Special('`') => {
                let name = match self.peek_raw(1).clone() {
                    Tok::ConId(s) | Tok::VarId(s) if *self.peek_raw(2) == Tok::Special('`') => {
                        QName::unqual(s)
                    }
                    _ => return None,
                };
                self.bump();
                self.bump();
                self.bump();
                return Some(name);
            }
            _ => return None,
        };
        self.bump();
        Some(q)
    }

    pub(super) fn atype(&mut self) -> PResult<Type> {
        let loc = self.loc();
        if !self.real() {
            return self.unexpected();
        }
        match self.tok().clone() {
            Tok::VarId(s) => {
                self.bump();
                Ok(Type::Var(s, loc))
            }
            Tok::ConId(_) | Tok::QConId(..) => {
                let q = self.qcon()?;
                Ok(Type::Con(q, loc))
            }
            Tok::Kw(Kw::Underscore) => {
                self.bump();
                Ok(Type::Wild(loc))
            }
            // StarIsType: `*` is the kind of types.
            Tok::VarSym(s) if s == "*" => {
                self.bump();
                Ok(Type::Con(QName::unqual("Type"), loc))
            }
            Tok::Int(s) => {
                self.bump();
                Ok(Type::Lit(Lit::Int(s), loc))
            }
            Tok::Str(s) => {
                self.bump();
                Ok(Type::Lit(Lit::Str(s), loc))
            }
            Tok::Special('(') => {
                self.bump();
                // `(:+:)` names a type constructor operator.
                if let Tok::ConSym(s) = self.tok().clone() {
                    if *self.peek_raw(1) == Tok::Special(')') {
                        self.bump();
                        self.bump();
                        return Ok(Type::Con(QName::unqual(s), loc));
                    }
                }
                if self.at_special(')') {
                    self.bump();
                    return Ok(Type::Con(QName::unqual("()"), loc));
                }
                if self.at_res(ROp::RArrow) {
                    self.bump();
                    self.expect_special(')')?;
                    return Ok(Type::Con(QName::unqual("->"), loc));
                }
                if self.at_special(',') {
                    let mut n = 1;
                    while self.at_special(',') {
                        self.bump();
                        n += 1;
                    }
                    self.expect_special(')')?;
                    let name = format!("({})", ",".repeat(n - 1));
                    return Ok(Type::Con(QName::unqual(name), loc));
                }
                let first = self.sigtype()?;
                if self.at_res(ROp::DColon) {
                    self.bump();
                    let kind = self.sigtype()?;
                    self.expect_special(')')?;
                    return Ok(Type::Sig(Box::new(first), Box::new(kind)));
                }
                let mut elems = vec![first];
                while self.at_special(',') {
                    self.bump();
                    elems.push(self.sigtype()?);
                }
                self.expect_special(')')?;
                if elems.len() == 1 {
                    Ok(elems.pop().expect("one element"))
                } else {
                    Ok(Type::Tuple(elems, loc))
                }
            }
            Tok::Special('[') => {
                self.bump();
                if self.at_special(']') {
                    self.bump();
                    return Ok(Type::Con(QName::unqual("[]"), loc));
                }
                let t = self.sigtype()?;
                self.expect_special(']')?;
                Ok(Type::List(Box::new(t), loc))
            }
            _ => self.unexpected(),
        }
    }

    /// A type variable binder: `a` or `(a :: k)`.
    pub(super) fn tyvar_binder(&mut self) -> PResult<TyVarBind> {
        if self.at_special('(') {
            self.bump();
            let name = match self.tok().clone() {
                Tok::VarId(s) if self.real() => s,
                _ => return self.unexpected(),
            };
            self.bump();
            self.expect_res(ROp::DColon)?;
            let kind = self.sigtype()?;
            self.expect_special(')')?;
            return Ok(TyVarBind {
                name,
                kind: Some(kind),
            });
        }
        match self.tok().clone() {
            Tok::VarId(s) if self.real() => {
                self.bump();
                Ok(TyVarBind { name: s, kind: None })
            }
            _ => self.unexpected(),
        }
    }

    /// Parses `[context =>]` if present; otherwise rewinds and returns nothing.
    pub(super) fn try_context(&mut self) -> Vec<Type> {
        let saved_pos = self.pos_state();
        match self.btype() {
            Ok(t) if self.at_res(ROp::DArrow) => {
                self.bump();
                context_items(t)
            }
            _ => {
                self.restore_pos_state(saved_pos);
                Vec::new()
            }
        }
    }
}

/// Splits a context `(C a, D b)` or `C a` into its constraints.
fn context_items(t: Type) -> Vec<Type> {
    match t {
        Type::Tuple(ts, _) => ts,
        Type::Con(q, _) if q.name == "()" => Vec::new(),
        other => vec![other],
    }
}
