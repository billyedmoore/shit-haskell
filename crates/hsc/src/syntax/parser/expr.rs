//! Expressions, statements and case alternatives.

use super::{group_binds, PResult, Parser};
use crate::syntax::ast::*;
use crate::syntax::token::{Kw, ROp, Tok};

impl Parser {
    /// exp → infixexp [:: sigtype]
    pub(super) fn expr(&mut self) -> PResult<Expr> {
        let loc = self.loc();
        let e = self.infix_expr()?;
        if self.at_res(ROp::DColon) {
            self.bump();
            let t = self.sigtype()?;
            return Ok(Expr::Sig(Box::new(e), t, loc));
        }
        Ok(e)
    }

    /// infixexp without a trailing operator; the operator chain stays flat.
    pub(super) fn infix_expr(&mut self) -> PResult<Expr> {
        let (e, sect) = self.infix_expr_sect()?;
        match sect {
            None => Ok(e),
            Some(_) => self.unexpected(),
        }
    }

    /// Like `infix_expr`, but a trailing operator directly before `)` is returned
    /// as a left section instead of an error.
    fn infix_expr_sect(&mut self) -> PResult<(Expr, Option<(QName, Loc)>)> {
        let loc = self.loc();
        let mut items = Vec::new();
        loop {
            if self.at_varsym("-") {
                items.push(OpItem::Neg(self.loc()));
                self.bump();
                continue;
            }
            items.push(OpItem::Operand(self.lexp()?));
            match self.operator() {
                Some((op, oloc)) => {
                    if self.at_special(')') {
                        return Ok((collapse(items, loc), Some((op, oloc))));
                    }
                    items.push(OpItem::Op(op, oloc));
                }
                None => break,
            }
        }
        Ok((collapse(items, loc), None))
    }

    /// Consumes a binary operator if one is next. Prefix `!` and `~` are patterns.
    fn operator(&mut self) -> Option<(QName, Loc)> {
        if !self.real() {
            return None;
        }
        let loc = self.loc();
        let q = match self.tok().clone() {
            Tok::VarSym(s) => {
                if s == "!" && self.is_prefix_occurrence() {
                    return None;
                }
                // `#)` closes an unboxed tuple.
                if s == "#" && *self.peek_raw(1) == Tok::Special(')') {
                    return None;
                }
                QName::unqual(s)
            }
            Tok::ConSym(s) => QName::unqual(s),
            Tok::QVarSym(m, s) | Tok::QConSym(m, s) => QName {
                qual: Some(m),
                name: s,
            },
            Tok::Special('`') => {
                let name = match self.peek_raw(1).clone() {
                    Tok::VarId(s) | Tok::ConId(s) if *self.peek_raw(2) == Tok::Special('`') => {
                        QName::unqual(s)
                    }
                    Tok::QVarId(m, s) | Tok::QConId(m, s) if *self.peek_raw(2) == Tok::Special('`') => {
                        QName {
                            qual: Some(m),
                            name: s,
                        }
                    }
                    _ => return None,
                };
                self.bump();
                self.bump();
                self.bump();
                return Some((name, loc));
            }
            _ => return None,
        };
        self.bump();
        Some((q, loc))
    }

    /// GHC's rule: a prefix occurrence has whitespace (or an opening bracket) before
    /// the token and none after it.
    fn is_prefix_occurrence(&self) -> bool {
        let t = self.raw();
        t.space_before && !t.space_after
    }

    pub(super) fn lexp(&mut self) -> PResult<Expr> {
        let loc = self.loc();
        match self.tok().clone() {
            Tok::Res(ROp::Backslash) if self.real() => {
                self.bump();
                if self.at_kw(Kw::Case) {
                    self.require_ext("LambdaCase")?;
                    self.bump();
                    let alts = self.block(|p| p.alt())?;
                    return Ok(Expr::LamCase(alts, loc));
                }
                let mut pats = Vec::new();
                while !self.at_res(ROp::RArrow) {
                    let a = self.aexp()?;
                    pats.push(self.expr_to_pat(a)?);
                }
                self.bump();
                let body = self.expr()?;
                Ok(Expr::Lam(pats, Box::new(body), loc))
            }
            Tok::Kw(Kw::Let) if self.real() => {
                self.bump();
                let decls = group_binds(self.block(|p| p.local_decl())?);
                self.expect_kw(Kw::In)?;
                let body = self.expr()?;
                Ok(Expr::Let(decls, Box::new(body), loc))
            }
            Tok::Kw(Kw::If) if self.real() => {
                self.bump();
                if self.at_res(ROp::Bar) {
                    self.require_ext("MultiWayIf")?;
                    // The guards form a layout block opened at the first `|`: a `|` further
                    // left than that column belongs to an enclosing construct.
                    let bar_col = self.loc().col;
                    let mut alts = Vec::new();
                    while self.at_res(ROp::Bar) && self.loc().col >= bar_col {
                        self.bump();
                        let conds = self.guard_stmts()?;
                        self.expect_res(ROp::RArrow)?;
                        let e = self.expr()?;
                        alts.push((conds, e));
                    }
                    return Ok(Expr::MultiIf(alts, loc));
                }
                let c = self.expr()?;
                self.skip_semi_before(Kw::Then);
                self.expect_kw(Kw::Then)?;
                let t = self.expr()?;
                self.skip_semi_before(Kw::Else);
                self.expect_kw(Kw::Else)?;
                let e = self.expr()?;
                Ok(Expr::If(Box::new(c), Box::new(t), Box::new(e), loc))
            }
            Tok::Kw(Kw::Case) if self.real() => {
                self.bump();
                let scrut = self.expr()?;
                self.expect_kw(Kw::Of)?;
                let alts = self.block(|p| p.alt())?;
                Ok(Expr::Case(Box::new(scrut), alts, loc))
            }
            Tok::Kw(Kw::Do) if self.real() => {
                self.bump();
                let stmts = self.block(|p| p.stmt())?;
                Ok(Expr::Do(stmts, loc))
            }
            _ => self.fexp(),
        }
    }

    /// DoAndIfThenElse: a virtual or explicit semicolon may precede `then` or `else`.
    fn skip_semi_before(&mut self, kw: Kw) {
        let next = if self.cur() == super::Cur::VSemi {
            self.peek_raw(0).clone()
        } else if self.at_special(';') {
            self.peek_raw(1).clone()
        } else {
            return;
        };
        if next == Tok::Kw(kw) {
            self.bump();
        }
    }

    fn fexp(&mut self) -> PResult<Expr> {
        let mut e = self.aexp_postfix()?;
        loop {
            if self.starts_aexp() {
                let a = self.aexp_postfix()?;
                e = Expr::App(Box::new(e), Box::new(a));
            } else if self.at_res(ROp::At) && self.raw().space_before && !self.raw().space_after {
                let loc = self.loc();
                self.bump();
                let t = self.atype()?;
                e = Expr::TyApp(Box::new(e), t, loc);
            } else {
                break;
            }
        }
        Ok(e)
    }

    fn starts_aexp(&self) -> bool {
        if !self.real() {
            return false;
        }
        match self.tok() {
            Tok::VarId(_)
            | Tok::ConId(_)
            | Tok::QVarId(..)
            | Tok::QConId(..)
            | Tok::Int(_)
            | Tok::Float(_)
            | Tok::Char(_)
            | Tok::Str(_)
            | Tok::Special('(' | '[')
            | Tok::Kw(Kw::Underscore) => true,
            Tok::VarSym(s) => s == "!" && self.is_prefix_occurrence(),
            Tok::Res(ROp::Tilde) => self.is_prefix_occurrence(),
            _ => false,
        }
    }

    fn aexp_postfix(&mut self) -> PResult<Expr> {
        let mut e = self.aexp()?;
        loop {
            if self.at_special('{') {
                e = self.record_braces(e)?;
            } else if self.exts.is_on("OverloadedRecordDot")
                && self.real()
                && self.at_varsym(".")
                && !self.raw().space_before
                && !self.raw().space_after
                && matches!(self.peek_raw(1), Tok::VarId(_))
            {
                // `e.field` with no spaces: the field selector applied to `e`.
                let loc = self.loc();
                self.bump();
                let field = match self.tok().clone() {
                    Tok::VarId(f) => f,
                    _ => return self.unexpected(),
                };
                self.bump();
                e = Expr::App(Box::new(Expr::Var(QName::unqual(field), loc)), Box::new(e));
            } else {
                break;
            }
        }
        Ok(e)
    }

    /// `C { f = e, g, .. }` construction, or `e { f = e }` update.
    fn record_braces(&mut self, e: Expr) -> PResult<Expr> {
        let loc = e.loc();
        self.bump();
        // Record braces open an explicit layout context, as in GHC.
        self.ctx.push(super::Ctx::Explicit);
        let mut fields = Vec::new();
        let mut dotdot = false;
        loop {
            if self.at_special('}') {
                break;
            }
            if self.at_res(ROp::DotDot) {
                self.require_ext("RecordWildCards")?;
                self.bump();
                dotdot = true;
            } else {
                let f = self.qvar()?;
                let val = if self.at_res(ROp::Eq) {
                    self.bump();
                    Some(self.expr()?)
                } else {
                    None
                };
                fields.push((f, val));
            }
            if self.at_special(',') {
                self.bump();
            } else {
                break;
            }
        }
        self.expect_special('}')?;
        self.ctx.pop();
        match e {
            Expr::Con(con, _) => Ok(Expr::RecCon {
                con,
                fields,
                dotdot,
                loc,
            }),
            other => {
                if dotdot {
                    return self.err("record wildcard in update");
                }
                Ok(Expr::RecUpd(Box::new(other), fields, loc))
            }
        }
    }

    pub(super) fn aexp(&mut self) -> PResult<Expr> {
        let loc = self.loc();
        if !self.real() {
            return self.unexpected();
        }
        match self.tok().clone() {
            Tok::VarId(s) => {
                self.bump();
                if self.at_res(ROp::At) && !self.raw().space_before {
                    self.bump();
                    let inner = self.aexp_postfix()?;
                    return Ok(Expr::AsPat(s, Box::new(inner), loc));
                }
                Ok(Expr::Var(QName::unqual(s), loc))
            }
            Tok::QVarId(m, s) => {
                self.bump();
                Ok(Expr::Var(
                    QName {
                        qual: Some(m),
                        name: s,
                    },
                    loc,
                ))
            }
            Tok::ConId(s) => {
                self.bump();
                Ok(Expr::Con(QName::unqual(s), loc))
            }
            Tok::QConId(m, s) => {
                self.bump();
                Ok(Expr::Con(
                    QName {
                        qual: Some(m),
                        name: s,
                    },
                    loc,
                ))
            }
            Tok::Int(s) => {
                self.bump();
                Ok(Expr::Lit(Lit::Int(s), loc))
            }
            Tok::Float(s) => {
                self.bump();
                Ok(Expr::Lit(Lit::Frac(s), loc))
            }
            Tok::Char(c) => {
                self.bump();
                Ok(Expr::Lit(Lit::Char(c), loc))
            }
            Tok::Str(s) => {
                self.bump();
                Ok(Expr::Lit(Lit::Str(s), loc))
            }
            Tok::Kw(Kw::Underscore) => {
                self.bump();
                Ok(Expr::Wild(loc))
            }
            Tok::Res(ROp::Tilde) => {
                self.bump();
                let inner = self.aexp()?;
                Ok(Expr::LazyPat(Box::new(inner), loc))
            }
            Tok::VarSym(s) if s == "!" => {
                self.bump();
                let inner = self.aexp()?;
                Ok(Expr::BangPat(Box::new(inner), loc))
            }
            Tok::Special('(') => self.paren_expr(loc),
            Tok::Special('[') => self.bracket_expr(loc),
            _ => self.unexpected(),
        }
    }

    /// Parenthesised expressions: unit, operators, sections, tuples, plain parens.
    fn paren_expr(&mut self, loc: Loc) -> PResult<Expr> {
        self.bump();
        // UnboxedTuples: `(# a, b #)` is an ordinary tuple here.
        if self.at_varsym("#") && self.raw().space_after {
            self.bump();
            if self.at_varsym("#") {
                self.bump();
                self.expect_special(')')?;
                return Ok(Expr::Con(QName::unqual("()"), loc));
            }
            let first = self.expr()?;
            if self.at_varsym("#") {
                self.bump();
                self.expect_special(')')?;
                return Ok(first);
            }
            return self.unboxed_tuple_rest(vec![Some(first)], loc);
        }
        // OverloadedRecordDot: `(.field)` is the field selector.
        if self.exts.is_on("OverloadedRecordDot")
            && self.at_varsym(".")
            && !self.raw().space_after
            && matches!(self.peek_raw(1), Tok::VarId(_))
            && *self.peek_raw(2) == Tok::Special(')')
        {
            self.bump();
            let field = match self.tok().clone() {
                Tok::VarId(f) => f,
                _ => return self.unexpected(),
            };
            self.bump();
            self.bump();
            return Ok(Expr::Var(QName::unqual(field), loc));
        }
        if self.at_special(')') {
            self.bump();
            return Ok(Expr::Con(QName::unqual("()"), loc));
        }
        if self.at_special(',') {
            return self.tuple_rest(vec![None], loc);
        }
        // `(op)` names an operator; `(op e)` is a right section.
        let bang_pattern = self.at_varsym("!") && self.is_prefix_occurrence();
        if let Some((op, oloc)) = self.peek_operator_token().filter(|_| !bang_pattern) {
            let is_minus = op.name == "-" && op.qual.is_none();
            if self.peek_raw(self.operator_width()) == &Tok::Special(')') {
                for _ in 0..self.operator_width() {
                    self.bump();
                }
                self.bump();
                return Ok(if op.name.starts_with(':') {
                    Expr::Con(op, oloc)
                } else {
                    Expr::Var(op, oloc)
                });
            }
            if !is_minus {
                for _ in 0..self.operator_width() {
                    self.bump();
                }
                let e = self.infix_expr()?;
                self.expect_special(')')?;
                return Ok(Expr::RightSection(op, Box::new(e), loc));
            }
        }

        let (first, sect) = self.infix_expr_sect()?;
        if let Some((op, _)) = sect {
            self.expect_special(')')?;
            return Ok(Expr::LeftSection(Box::new(first), op, loc));
        }
        let first = if self.at_res(ROp::DColon) {
            self.bump();
            let t = self.sigtype()?;
            Expr::Sig(Box::new(first), t, loc)
        } else {
            first
        };
        if self.at_special(',') {
            return self.tuple_rest(vec![Some(first)], loc);
        }
        if self.at_res(ROp::RArrow) {
            return self.err("view patterns are not supported yet");
        }
        self.expect_special(')')?;
        Ok(first)
    }

    /// The operator token at the cursor, without consuming it, if any.
    fn peek_operator_token(&self) -> Option<(QName, Loc)> {
        if !self.real() {
            return None;
        }
        let loc = self.loc();
        match self.tok().clone() {
            Tok::VarSym(s) | Tok::ConSym(s) => Some((QName::unqual(s), loc)),
            Tok::QVarSym(m, s) | Tok::QConSym(m, s) => Some((
                QName {
                    qual: Some(m),
                    name: s,
                },
                loc,
            )),
            Tok::Special('`') => match self.peek_raw(1).clone() {
                Tok::VarId(s) | Tok::ConId(s) => Some((QName::unqual(s), loc)),
                Tok::QVarId(m, s) | Tok::QConId(m, s) => Some((
                    QName {
                        qual: Some(m),
                        name: s,
                    },
                    loc,
                )),
                _ => None,
            },
            _ => None,
        }
    }

    /// Number of tokens an operator occupies at the cursor (1, or 3 with backticks).
    fn operator_width(&self) -> usize {
        match self.tok() {
            Tok::Special('`') => 3,
            _ => 1,
        }
    }

    /// Elements of an unboxed tuple after the first, up to `#)`.
    fn unboxed_tuple_rest(&mut self, mut elems: Vec<Option<Expr>>, loc: Loc) -> PResult<Expr> {
        while self.at_special(',') {
            self.bump();
            elems.push(Some(self.expr()?));
        }
        if !self.at_varsym("#") {
            return self.unexpected();
        }
        self.bump();
        self.expect_special(')')?;
        Ok(Expr::Tuple(elems, loc))
    }

    /// Elements of a tuple after the first; `elems` holds what has been read so far.
    fn tuple_rest(&mut self, mut elems: Vec<Option<Expr>>, loc: Loc) -> PResult<Expr> {
        while self.at_special(',') {
            self.bump();
            if self.at_special(',') || self.at_special(')') {
                elems.push(None);
            } else {
                elems.push(Some(self.expr()?));
            }
        }
        self.expect_special(')')?;
        Ok(Expr::Tuple(elems, loc))
    }

    fn bracket_expr(&mut self, loc: Loc) -> PResult<Expr> {
        self.bump();
        if self.at_special(']') {
            self.bump();
            return Ok(Expr::Con(QName::unqual("[]"), loc));
        }
        let first = self.expr()?;
        if self.at_res(ROp::DotDot) {
            self.bump();
            let to = if self.at_special(']') {
                None
            } else {
                Some(Box::new(self.expr()?))
            };
            self.expect_special(']')?;
            return Ok(Expr::ArithSeq {
                from: Box::new(first),
                then: None,
                to,
                loc,
            });
        }
        if self.at_res(ROp::Bar) {
            let mut branches = Vec::new();
            while self.at_res(ROp::Bar) {
                self.bump();
                let mut quals = vec![self.stmt()?];
                while self.at_special(',') {
                    self.bump();
                    quals.push(self.stmt()?);
                }
                branches.push(quals);
            }
            self.expect_special(']')?;
            return Ok(Expr::ListComp(Box::new(first), branches, loc));
        }
        let mut elems = vec![first];
        if self.at_special(',') {
            self.bump();
            let second = self.expr()?;
            if self.at_res(ROp::DotDot) {
                self.bump();
                let to = if self.at_special(']') {
                    None
                } else {
                    Some(Box::new(self.expr()?))
                };
                self.expect_special(']')?;
                let from = elems.pop().expect("first element present");
                return Ok(Expr::ArithSeq {
                    from: Box::new(from),
                    then: Some(Box::new(second)),
                    to,
                    loc,
                });
            }
            elems.push(second);
            while self.at_special(',') {
                self.bump();
                elems.push(self.expr()?);
            }
        }
        self.expect_special(']')?;
        Ok(Expr::List(elems, loc))
    }

    /// A statement in `do`, a guard, or a comprehension qualifier.
    pub(super) fn stmt(&mut self) -> PResult<Stmt> {
        let loc = self.loc();
        if self.at_kw(Kw::Let) {
            self.bump();
            let decls = group_binds(self.block(|p| p.local_decl())?);
            if self.at_kw(Kw::In) {
                self.bump();
                let body = self.expr()?;
                return Ok(Stmt::Expr {
                    expr: Expr::Let(decls, Box::new(body), loc),
                    loc,
                });
            }
            return Ok(Stmt::Let { decls, loc });
        }
        let e = self.expr()?;
        if self.at_res(ROp::LArrow) {
            self.bump();
            let pat = self.expr_to_pat(e)?;
            let expr = self.expr()?;
            return Ok(Stmt::Bind { pat, expr, loc });
        }
        Ok(Stmt::Expr { expr: e, loc })
    }

    /// A case alternative: `pat -> e` or `pat | guard -> e`, with optional `where`.
    pub(super) fn alt(&mut self) -> PResult<Alt> {
        let loc = self.loc();
        let e = self.infix_expr()?;
        let pat = self.expr_to_pat(e)?;
        let rhs = self.rhs(ROp::RArrow)?;
        Ok(Alt { pat, rhs, loc })
    }

    pub(super) fn guard_stmts(&mut self) -> PResult<Vec<Stmt>> {
        let mut stmts = vec![self.stmt()?];
        while self.at_special(',') {
            self.bump();
            stmts.push(self.stmt()?);
        }
        Ok(stmts)
    }

    pub(super) fn require_ext(&self, name: &str) -> PResult<()> {
        if self.exts.is_on(name) {
            Ok(())
        } else {
            self.err(format!("Illegal syntax: enable the {name} extension"))
        }
    }
}

/// Builds the flat operator chain, or the single operand when there is no operator.
fn collapse(mut items: Vec<OpItem>, loc: Loc) -> Expr {
    if items.len() == 1 {
        if let Some(OpItem::Operand(_)) = items.first() {
            if let Some(OpItem::Operand(e)) = items.pop() {
                return e;
            }
        }
    }
    Expr::OpChain(items, loc)
}
