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
        // LinearTypes: a multiplicity arrow `%1 ->` or `%m ->` is an ordinary arrow here.
        if self.real() && self.at_varsym("%") && !self.raw().space_after {
            let saved = self.pos_state();
            self.bump();
            self.atype()?;
            if !self.at_res(ROp::RArrow) {
                self.restore_pos_state(saved);
            }
        }
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
        loop {
            if self.starts_atype() {
                let a = self.atype()?;
                t = Type::App(Box::new(t), Box::new(a));
            } else if self.real() && self.at_res(ROp::At) && self.raw().space_before && !self.raw().space_after {
                // A visible kind argument `T @k`: kinds are not checked, so it is dropped.
                self.bump();
                self.atype()?;
            } else {
                break;
            }
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
            | Tok::Char(_)
            | Tok::Str(_) => true,
            // A tick before a constructor operator is an infix promoted operator, not an argument.
            Tok::Special('\'') => !matches!(self.peek_raw(1), Tok::ConSym(_)),
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
            Tok::VarSym(s)
                if s != "!" && s != "." && s != "#" && s != "%" && (s != "*" || !self.exts.is_on("StarIsType")) =>
            {
                QName::unqual(s)
            }
            Tok::Res(ROp::Tilde) => QName::unqual("~"),
            // DataKinds: a promoted infix constructor `':+:`.
            Tok::Special('\'') if matches!(self.peek_raw(1), Tok::ConSym(_)) => {
                let Tok::ConSym(s) = self.peek_raw(1).clone() else { return None };
                self.bump();
                QName::unqual(format!("'{s}"))
            }
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
            // A strictness mark on a field type (`!t`); strictness is not recorded for GADT fields.
            Tok::VarSym(s) if s == "!" && self.real() => {
                self.bump();
                self.atype()
            }
            // ImplicitParams: `?x :: t` is the constraint `?x t`.
            Tok::VarSym(s) if s == "?" && self.real() && self.exts.is_on("ImplicitParams") => {
                self.bump();
                let name = match self.tok().clone() {
                    Tok::VarId(v) if self.real() => v,
                    _ => return self.unexpected(),
                };
                self.bump();
                self.expect_res(ROp::DColon)?;
                let ty = self.sigtype()?;
                Ok(Type::App(Box::new(Type::Con(QName::unqual(format!("?{name}")), loc)), Box::new(ty)))
            }
            // DataKinds: `'Con` names a promoted data constructor; `'[]` the promoted empty list.
            Tok::Special('\'') => {
                self.bump();
                match self.tok().clone() {
                    Tok::ConId(_) | Tok::QConId(..) => {
                        let q = self.qcon()?;
                        Ok(Type::Con(QName { qual: q.qual, name: format!("'{}", q.name) }, loc))
                    }
                    Tok::Special('[') => {
                        self.bump();
                        let mut elems = Vec::new();
                        if !self.at_special(']') {
                            elems.push(self.sigtype()?);
                            while self.at_special(',') {
                                self.bump();
                                elems.push(self.sigtype()?);
                            }
                        }
                        self.expect_special(']')?;
                        Ok(promoted_list(elems, loc))
                    }
                    // `'(:)` and `'(,)`: a parenthesised constructor, promoted.
                    Tok::Special('(') => match self.atype()? {
                        Type::Con(q, l) => Ok(Type::Con(QName { qual: q.qual, name: format!("'{}", q.name) }, l)),
                        other => Ok(other),
                    },
                    _ => self.unexpected(),
                }
            }
            // StarIsType: `*` is the kind of types.
            Tok::VarSym(s) if s == "*" && self.exts.is_on("StarIsType") => {
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
            // A type-level character (DataKinds): `'x'`.
            Tok::Char(c) => {
                self.bump();
                Ok(Type::Lit(Lit::Char(c), loc))
            }
            Tok::Special('(') => {
                self.bump();
                // UnboxedTuples: `(##)` is the unboxed unit type, lexed as one symbol.
                if self.at_varsym("##") && *self.peek_raw(1) == Tok::Special(')') {
                    self.bump();
                    self.bump();
                    return Ok(Type::Con(QName::unqual("()"), loc));
                }
                // UnboxedTuples: `(# t1, t2 #)` is an ordinary tuple type here.
                if self.at_varsym("#") && (self.raw().space_after || self.exts.is_on("UnboxedTuples") || self.exts.is_on("UnboxedSums")) {
                    self.bump();
                    if self.at_varsym("#") {
                        self.bump();
                        self.expect_special(')')?;
                        return Ok(Type::Con(QName::unqual("()"), loc));
                    }
                    let mut elems = vec![self.sigtype()?];
                    // UnboxedSums: `(# a | b #)` is the sum type constructor applied to its parts.
                    if self.at_res(ROp::Bar) {
                        while self.at_res(ROp::Bar) {
                            self.bump();
                            elems.push(self.sigtype()?);
                        }
                        if !self.at_varsym("#") {
                            return self.unexpected();
                        }
                        self.bump();
                        self.expect_special(')')?;
                        let mut t = Type::Con(QName::unqual(format!("(#|{}#)", elems.len())), loc);
                        for e in elems {
                            t = Type::App(Box::new(t), Box::new(e));
                        }
                        return Ok(t);
                    }
                    while self.at_special(',') {
                        self.bump();
                        elems.push(self.sigtype()?);
                    }
                    if !self.at_varsym("#") {
                        return self.unexpected();
                    }
                    self.bump();
                    self.expect_special(')')?;
                    return Ok(if elems.len() == 1 {
                        elems.pop().expect("one element")
                    } else {
                        Type::Tuple(elems, loc)
                    });
                }
                // `(:+:)` and `(==)` name a type constructor operator.
                let op_name = match self.tok().clone() {
                    Tok::ConSym(s) | Tok::VarSym(s) if self.real() => Some(QName::unqual(s)),
                    Tok::QConSym(m, s) if self.real() => Some(QName { qual: Some(m), name: s }),
                    Tok::Res(ROp::Tilde) if self.real() => Some(QName::unqual("~")),
                    _ => None,
                };
                if let Some(q) = op_name {
                    if *self.peek_raw(1) == Tok::Special(')') {
                        self.bump();
                        self.bump();
                        return Ok(Type::Con(q, loc));
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
                if self.at_special(',') {
                    // DataKinds: a promoted list literal `[t1, t2, ...]` is `t1 ': t2 ': ... '[]`.
                    let mut elems = vec![t];
                    while self.at_special(',') {
                        self.bump();
                        elems.push(self.sigtype()?);
                    }
                    self.expect_special(']')?;
                    return Ok(promoted_list(elems, loc));
                }
                self.expect_special(']')?;
                Ok(Type::List(Box::new(t), loc))
            }
            _ => self.unexpected(),
        }
    }

    /// A type variable binder: `a` or `(a :: k)`.
    pub(super) fn tyvar_binder(&mut self) -> PResult<TyVarBind> {
        // Inferred binders `{a}` and `{a :: k}` are accepted like ordinary ones.
        if self.at_special('{') {
            self.bump();
            let name = match self.tok().clone() {
                Tok::VarId(s) if self.real() => s,
                _ => return self.unexpected(),
            };
            self.bump();
            let kind = if self.at_res(ROp::DColon) {
                self.bump();
                Some(self.sigtype()?)
            } else {
                None
            };
            self.expect_special('}')?;
            return Ok(TyVarBind { name, kind });
        }
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

/// A promoted list `t1 ': t2 ': ... '[]` built from its elements.
fn promoted_list(elems: Vec<Type>, loc: Loc) -> Type {
    let mut acc = Type::Con(QName::unqual("'[]"), loc);
    for e in elems.into_iter().rev() {
        acc = Type::App(
            Box::new(Type::App(Box::new(Type::Con(QName::unqual("':"), loc)), Box::new(e))),
            Box::new(acc),
        );
    }
    acc
}
