//! Layout-aware recursive-descent parser.
//!
//! Layout follows the Haskell 2010 report (section 10.3). Instead of rewriting the
//! token stream, the cursor synthesises virtual semicolons and close braces on
//! demand from the context stack. The parse-error(t) rule is implemented by the
//! block parser: when an implicit block cannot continue, its context is popped.

mod decl;
mod expr;
mod pat;
mod types;

use super::ast::*;
use super::extensions::Extensions;
use super::token::{Kw, ROp, Tok, Token};

#[derive(Debug, Clone)]
pub struct ParseError {
    pub line: u32,
    pub col: u32,
    pub msg: String,
}

pub type PResult<T> = Result<T, ParseError>;

/// What the cursor currently sees: a real token or a virtual layout token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cur {
    Real,
    VSemi,
    VRBrace,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ctx {
    Explicit,
    Implicit(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BlockKind {
    Explicit,
    Implicit,
    Empty,
}

enum TopItem {
    Import(Import),
    Decl(Decl),
}

struct KindArgument {
    ty: Type,
    site: KindArgumentSite,
}

enum KindArgumentSite {
    TypeApplication,
    Head(Loc),
    InfixOperand(Loc),
}

struct KindSignatureRequirement {
    declaration: String,
    signature_name: String,
    at: Loc,
}

pub struct Parser {
    toks: Vec<Token>,
    pos: usize,
    ctx: Vec<Ctx>,
    /// The (token index, context depth) for which a virtual semicolon was consumed.
    semi_done: Option<(usize, usize)>,
    pub exts: Extensions,
    kind_annotations: Vec<Type>,
    captured_kind_arguments: Option<Vec<KindArgument>>,
    kind_signature_requirements: Vec<KindSignatureRequirement>,
    /// The `static` forms parsed so far (each one's number keys its pointer).
    static_count: usize,
}

pub fn parse_module(toks: Vec<Token>, exts: Extensions) -> PResult<Module> {
    parse_module_with_warnings(toks, exts).map(|(module, _)| module)
}

pub(super) fn parse_module_with_warnings(
    toks: Vec<Token>, exts: Extensions,
) -> PResult<(Module, Vec<(Loc, String, crate::diagnostic::Warning)>)> {
    let mut p = Parser {
        toks,
        pos: 0,
        ctx: Vec::new(),
        semi_done: None,
        exts,
        kind_annotations: Vec::new(),
        captured_kind_arguments: None,
        kind_signature_requirements: Vec::new(),
        static_count: 0,
    };
    let mut m = p.module()?;
    mark_gadt_infix(&mut m);
    Ok((m, p.exts.syntax_warnings))
}

/// Parses the body of a `{-# RULES #-}` pragma (lexed, with the pragma's line) into its rules.
/// A rule this parser cannot read is left out: rules only make programs faster, so a skipped one
/// changes no result (`HSC_DEBUG_RULES` reports it).
pub fn parse_rules(toks: Vec<Token>, exts: &Extensions) -> Vec<crate::syntax::ast::Rule> {
    // Each rule starts at its name, a string literal that begins a line (or the pragma), or
    // follows a `;`.
    let mut starts = Vec::new();
    for (i, t) in toks.iter().enumerate() {
        let after_semi = i > 0 && matches!(toks[i - 1].tok, Tok::Special(';'));
        if matches!(t.tok, Tok::Str(_)) && (i == 0 || t.first_on_line || after_semi) {
            starts.push(i);
        }
    }
    let mut out = Vec::new();
    for (k, &start) in starts.iter().enumerate() {
        let mut end = starts.get(k + 1).copied().unwrap_or(toks.len());
        while end > start && matches!(toks[end - 1].tok, Tok::Special(';') | Tok::Eof) {
            end -= 1;
        }
        let rule_toks = toks[start..end].to_vec();
        match parse_rule(rule_toks, exts) {
            Ok(r) => out.push(r),
            Err(e) => {
                if std::env::var_os("HSC_DEBUG_RULES").is_some() {
                    eprintln!("rule at line {} not read: {}", toks[start].line, e.msg);
                }
            }
        }
    }
    out
}

/// Parses the body of a top-level `{-# INLINE #-}` or `{-# NOINLINE #-}` pragma (lexed):
/// `[CONLIKE] [activation] name`. Anything else is ignored (pragmas are optimisations).
pub fn parse_inline(toks: &[Token], inline: bool) -> Option<crate::syntax::ast::InlinePragma> {
    use crate::syntax::ast::{Activation, InlinePragma, QName};
    let toks: Vec<&Token> = toks.iter().filter(|t| t.tok != Tok::Eof).collect();
    let mut i = 0;
    if matches!(toks.first().map(|t| &t.tok), Some(Tok::ConId(c)) if c == "CONLIKE") {
        i += 1;
    }
    let mut activation = Activation::Always;
    if matches!(toks.get(i).map(|t| &t.tok), Some(Tok::Special('['))) {
        let before = matches!(toks.get(i + 1).map(|t| &t.tok), Some(Tok::Res(ROp::Tilde)))
            || matches!(toks.get(i + 1).map(|t| &t.tok), Some(Tok::VarSym(s)) if s == "~");
        let j = if before { i + 2 } else { i + 1 };
        match (toks.get(j).map(|t| &t.tok), toks.get(j + 1).map(|t| &t.tok)) {
            (Some(Tok::Int(n)), Some(Tok::Special(']'))) => {
                let n: u32 = n.parse().ok()?;
                activation = if before { Activation::Before(n) } else { Activation::From(n) };
                i = j + 2;
            }
            (Some(Tok::Special(']')), _) if before => {
                activation = Activation::Never;
                i = j + 1;
            }
            _ => return None,
        }
    }
    let loc = Loc { line: toks.get(i)?.line, col: toks.get(i)?.col };
    let name = match toks.get(i..)? {
        [t] => match &t.tok {
            Tok::VarId(v) => QName::unqual(v.clone()),
            Tok::QVarId(q, v) => QName { qual: Some(q.clone()), name: v.clone() },
            _ => return None,
        },
        [o, t, c] if o.tok == Tok::Special('(') && c.tok == Tok::Special(')') => match &t.tok {
            Tok::VarSym(v) => QName::unqual(v.clone()),
            Tok::QVarSym(q, v) => QName { qual: Some(q.clone()), name: v.clone() },
            _ => return None,
        },
        _ => return None,
    };
    Some(InlinePragma { name, inline, activation, loc })
}

fn parse_rule(mut toks: Vec<Token>, exts: &Extensions) -> PResult<crate::syntax::ast::Rule> {
    use crate::syntax::ast::{Activation, Rule};
    let loc = Loc { line: toks[0].line, col: toks[0].col };
    let fail = |msg: &str| ParseError { line: loc.line, col: loc.col, msg: msg.to_string() };
    let Tok::Str(name) = toks[0].tok.clone() else { return Err(fail("a rule starts with its name")) };
    let mut i = 1;
    // [n], [~n] or [~]
    let mut activation = Activation::Always;
    if matches!(toks.get(i).map(|t| &t.tok), Some(Tok::Special('['))) {
        let before = matches!(toks.get(i + 1).map(|t| &t.tok), Some(Tok::Res(ROp::Tilde)))
            || matches!(toks.get(i + 1).map(|t| &t.tok), Some(Tok::VarSym(s)) if s == "~");
        let j = if before { i + 2 } else { i + 1 };
        match (toks.get(j).map(|t| &t.tok), toks.get(j + 1).map(|t| &t.tok)) {
            (Some(Tok::Int(n)), Some(Tok::Special(']'))) => {
                let n: u32 = n.parse().map_err(|_| fail("a phase is a number"))?;
                activation = if before { Activation::Before(n) } else { Activation::From(n) };
                i = j + 2;
            }
            (Some(Tok::Special(']')), _) if before => {
                activation = Activation::Never;
                i = j + 1;
            }
            _ => return Err(fail("unreadable activation")),
        }
    }
    // A `=` at bracket depth 0 separates the two sides.
    let mut depth = 0i32;
    let mut eq = None;
    for (j, t) in toks.iter().enumerate().skip(i) {
        match &t.tok {
            Tok::Special('(' | '[' | '{') => depth += 1,
            Tok::Special(')' | ']' | '}') => depth -= 1,
            Tok::Res(ROp::Eq) if depth == 0 => {
                eq = Some(j);
                break;
            }
            _ => {}
        }
    }
    let Some(eq) = eq else { return Err(fail("a rule has an `=`")) };
    let mut rhs_toks = toks.split_off(eq + 1);
    toks.pop();
    let mut lhs_toks = toks.split_off(i);
    // forall binders . (a type-level `forall a .` before them is skipped)
    let mut binders = Vec::new();
    let is_forall = |t: &Token| matches!(&t.tok, Tok::VarId(v) if v == "forall");
    while lhs_toks.first().is_some_and(is_forall) {
        // The `.` that ends the binders, outside their (possibly forall-typed) signatures.
        let mut depth = 0i32;
        let dot = lhs_toks
            .iter()
            .position(|t| {
                match &t.tok {
                    Tok::Special('(') => depth += 1,
                    Tok::Special(')') => depth -= 1,
                    _ => {}
                }
                depth == 0 && matches!(&t.tok, Tok::VarSym(s) if s == ".")
            })
            .ok_or_else(|| fail("forall without `.`"))?;
        let bs: Vec<Token> = lhs_toks.drain(..=dot).skip(1).collect();
        let bs = &bs[..bs.len() - 1];
        binders.clear();
        let mut j = 0;
        while j < bs.len() {
            match &bs[j].tok {
                Tok::VarId(v) => {
                    binders.push((v.clone(), None));
                    j += 1;
                }
                Tok::Special('(') => {
                    let close = (j + 1..bs.len()).find(|&k| {
                        let d: i32 = bs[j..=k].iter().map(|t| match t.tok { Tok::Special('(') => 1, Tok::Special(')') => -1, _ => 0 }).sum();
                        d == 0
                    }).ok_or_else(|| fail("unclosed binder"))?;
                    let Some(Tok::VarId(v)) = bs.get(j + 1).map(|t| &t.tok) else { return Err(fail("a binder is a variable")) };
                    let ty = if matches!(bs.get(j + 2).map(|t| &t.tok), Some(Tok::Res(ROp::DColon))) {
                        let mut ty_toks = bs[j + 3..close].to_vec();
                        ty_toks.push(eof_after(&bs[close]));
                        let mut tp = Parser {
                            toks: ty_toks, pos: 0, ctx: Vec::new(), semi_done: None, exts: exts.clone(),
                            kind_annotations: Vec::new(), captured_kind_arguments: None, kind_signature_requirements: Vec::new(), static_count: 0,
                        };
                        let t = tp.sigtype()?;
                        if !tp.at_eof() {
                            return Err(fail("unreadable binder type"));
                        }
                        Some(t)
                    } else {
                        None
                    };
                    binders.push((v.clone(), ty));
                    j = close + 1;
                }
                _ => return Err(fail("unreadable binder")),
            }
        }
    }
    let side = |mut ts: Vec<Token>, exts: &Extensions| -> PResult<Expr> {
        if ts.is_empty() {
            return Err(fail("an empty side"));
        }
        let last = ts[ts.len() - 1].clone();
        ts.push(eof_after(&last));
        let mut ep = Parser {
            toks: ts, pos: 0, ctx: Vec::new(), semi_done: None, exts: exts.clone(),
            kind_annotations: Vec::new(), captured_kind_arguments: None, kind_signature_requirements: Vec::new(), static_count: 0,
        };
        let e = ep.expr()?;
        if !ep.at_eof() {
            return Err(fail("unread tokens after a side"));
        }
        Ok(e)
    };
    let lhs = side(lhs_toks, exts)?;
    rhs_toks.retain(|t| !matches!(t.tok, Tok::Eof));
    let rhs = side(rhs_toks, exts)?;
    Ok(Rule { name, activation, binders, lhs, rhs, loc })
}

/// An end-of-input token just after `t`.
fn eof_after(t: &Token) -> Token {
    Token { tok: Tok::Eof, line: t.end_line, col: t.end_col + 1, end_line: t.end_line, end_col: t.end_col + 1, first_on_line: false, space_before: true, space_after: true }
}

/// A GADT-syntax operator constructor with two arguments is infix for `deriving Show`
/// when the module gives it a fixity declaration (GHC's rule).
fn mark_gadt_infix(m: &mut Module) {
    use crate::syntax::ast::{ConFields, Decl};
    let fixed: std::collections::HashSet<String> = m
        .decls
        .iter()
        .filter_map(|d| match d {
            Decl::Fixity { ops, .. } => Some(ops.clone()),
            _ => None,
        })
        .flatten()
        .collect();
    for d in &mut m.decls {
        let Decl::Data(dd) = d else { continue };
        for c in &mut dd.cons {
            let is_op = c.name.starts_with(|ch: char| !ch.is_alphanumeric() && ch != '_');
            if c.gadt_result.is_some()
                && is_op
                && matches!(&c.fields, ConFields::Positional(fs) if fs.len() == 2)
                && fixed.contains(&c.name)
            {
                c.infix = true;
            }
        }
    }
}

impl Parser {
    // ----- cursor -----------------------------------------------------------

    fn raw(&self) -> &Token {
        &self.toks[self.pos]
    }

    fn tok(&self) -> &Tok {
        &self.toks[self.pos].tok
    }

    fn loc(&self) -> Loc {
        Loc {
            line: self.raw().line,
            col: self.raw().col,
        }
    }

    pub fn cur(&self) -> Cur {
        let t = self.raw();
        if let Some(Ctx::Implicit(n)) = self.ctx.last().copied() {
            if t.tok == Tok::Eof {
                return Cur::VRBrace;
            }
            if t.first_on_line {
                if t.col < n {
                    return Cur::VRBrace;
                }
                if t.col == n && self.semi_done != Some((self.pos, self.ctx.len())) {
                    return Cur::VSemi;
                }
            }
        }
        Cur::Real
    }

    fn real(&self) -> bool {
        self.cur() == Cur::Real
    }

    fn at_special(&self, c: char) -> bool {
        self.real() && *self.tok() == Tok::Special(c)
    }

    fn at_kw(&self, k: Kw) -> bool {
        self.real() && *self.tok() == Tok::Kw(k)
    }

    fn at_res(&self, r: ROp) -> bool {
        self.real() && *self.tok() == Tok::Res(r)
    }

    fn at_varid(&self, s: &str) -> bool {
        self.real() && matches!(self.tok(), Tok::VarId(x) if x == s)
    }

    fn at_varsym(&self, s: &str) -> bool {
        self.real() && matches!(self.tok(), Tok::VarSym(x) if x == s)
    }

    fn at_semi(&self) -> bool {
        self.cur() == Cur::VSemi || self.at_special(';')
    }

    fn at_vrbrace(&self) -> bool {
        self.cur() == Cur::VRBrace
    }

    fn at_eof(&self) -> bool {
        self.real() && *self.tok() == Tok::Eof
    }

    /// Snapshot of the cursor, layout, and collected kind arguments for bounded backtracking.
    fn pos_state(&self) -> (usize, Vec<Ctx>, Option<(usize, usize)>, Option<usize>) {
        (self.pos, self.ctx.clone(), self.semi_done, self.captured_kind_arguments.as_ref().map(Vec::len))
    }

    fn restore_pos_state(&mut self, s: (usize, Vec<Ctx>, Option<(usize, usize)>, Option<usize>)) {
        self.pos = s.0;
        self.ctx = s.1;
        self.semi_done = s.2;
        if let (Some(arguments), Some(len)) = (&mut self.captured_kind_arguments, s.3) {
            arguments.truncate(len);
        }
    }

    /// The token immediately after the current raw token, ignoring layout.
    fn peek_raw(&self, k: usize) -> &Tok {
        let i = (self.pos + k).min(self.toks.len() - 1);
        &self.toks[i].tok
    }

    /// The position of the last character of the last real token consumed.
    pub(crate) fn prev_end(&self) -> Loc {
        match self.pos.checked_sub(1).and_then(|i| self.toks.get(i)) {
            Some(t) => Loc { line: t.end_line, col: t.end_col },
            None => Loc { line: 1, col: 1 },
        }
    }

    fn bump(&mut self) {
        match self.cur() {
            Cur::VSemi => self.semi_done = Some((self.pos, self.ctx.len())),
            Cur::VRBrace => {
                self.ctx.pop();
            }
            Cur::Real => {
                if self.pos + 1 < self.toks.len() {
                    self.pos += 1;
                }
            }
        }
    }

    fn err<T>(&self, msg: impl Into<String>) -> PResult<T> {
        Err(ParseError {
            line: self.raw().line,
            col: self.raw().col,
            msg: msg.into(),
        })
    }

    fn unexpected<T>(&self) -> PResult<T> {
        match self.cur() {
            Cur::Real => {
                let desc = self.tok().describe();
                self.err(format!("parse error on input ‘{desc}’"))
            }
            _ => self.err("parse error (possibly incorrect indentation or mismatched brackets)"),
        }
    }

    fn expect_special(&mut self, c: char) -> PResult<()> {
        if self.at_special(c) {
            self.bump();
            Ok(())
        } else {
            self.unexpected()
        }
    }

    fn expect_kw(&mut self, k: Kw) -> PResult<()> {
        if self.at_kw(k) {
            self.bump();
            Ok(())
        } else {
            self.unexpected()
        }
    }

    fn expect_res(&mut self, r: ROp) -> PResult<()> {
        if self.at_res(r) {
            self.bump();
            Ok(())
        } else {
            self.unexpected()
        }
    }

    // ----- layout blocks ----------------------------------------------------

    /// Opens a block after `where`, `let`, `do` or `of`.
    fn open_block(&mut self) -> PResult<BlockKind> {
        if self.at_special('{') {
            self.bump();
            self.ctx.push(Ctx::Explicit);
            return Ok(BlockKind::Explicit);
        }
        let t = self.raw();
        let n = if t.tok == Tok::Eof { 0 } else { t.col };
        let m = match self.ctx.last() {
            Some(Ctx::Implicit(m)) => *m,
            _ => 0,
        };
        // NondecreasingIndentation lets a nested block start at the column of its enclosing one.
        if n > m || (n == m && self.exts.is_on("NondecreasingIndentation")) {
            self.ctx.push(Ctx::Implicit(n));
            self.semi_done = Some((self.pos, self.ctx.len()));
            Ok(BlockKind::Implicit)
        } else {
            Ok(BlockKind::Empty)
        }
    }

    /// Parses `{ item ; item ; ... }` with either explicit or layout braces.
    fn block<T>(&mut self, mut item: impl FnMut(&mut Parser) -> PResult<T>) -> PResult<Vec<T>> {
        let kind = self.open_block()?;
        if kind == BlockKind::Empty {
            return Ok(Vec::new());
        }
        let explicit = kind == BlockKind::Explicit;
        let mut items = Vec::new();
        loop {
            while self.at_semi() {
                self.bump();
            }
            if explicit {
                if self.at_special('}') {
                    self.bump();
                    self.ctx.pop();
                    break;
                }
            } else if self.at_vrbrace() {
                self.bump();
                break;
            } else if self.token_closes_block() {
                // parse-error(t): a token that cannot begin an item closes the block.
                self.ctx.pop();
                break;
            }
            items.push(item(self)?);
            if self.at_semi() {
                continue;
            }
            if explicit {
                if self.at_special('}') {
                    self.bump();
                    self.ctx.pop();
                    break;
                }
                return self.unexpected();
            }
            if self.at_vrbrace() {
                self.bump();
                break;
            }
            // parse-error(t): the implicit block ends before this token.
            self.ctx.pop();
            break;
        }
        Ok(items)
    }

    /// Tokens that can never begin a block item. Seeing one of them in an implicit
    /// block means the block ends here (for example `where` aligned with `do` items).
    fn token_closes_block(&self) -> bool {
        if !self.real() {
            return false;
        }
        if *self.tok() == Tok::Kw(Kw::Deriving) {
            // A deriving clause ends a GADT body; a standalone `deriving instance` does not.
            let standalone = match self.peek_raw(1) {
                Tok::Kw(Kw::Instance) => true,
                Tok::VarId(s) if s == "stock" || s == "anyclass" => *self.peek_raw(2) == Tok::Kw(Kw::Instance),
                // `deriving via (V) instance ..` (a deriving clause names its classes first).
                Tok::VarId(s) if s == "via" => true,
                Tok::Kw(Kw::Newtype) => *self.peek_raw(2) == Tok::Kw(Kw::Instance),
                _ => false,
            };
            return !standalone;
        }
        // An operator cannot begin an item (`(do ..\n  <|> e)` with `<|>` at the items' column),
        // except a negation, a bang or lazy pattern, a splice, an implicit parameter and a label.
        match self.tok() {
            Tok::VarSym(s) if !matches!(s.as_str(), "-" | "!" | "~" | "$" | "$$" | "?" | "#") && !s.starts_with('?') => return true,
            Tok::ConSym(_) | Tok::QVarSym(..) | Tok::QConSym(..) | Tok::Special('`') => return true,
            _ => {}
        }
        matches!(
            self.tok(),
            Tok::Kw(Kw::Where | Kw::In | Kw::Then | Kw::Else | Kw::Of)
                | Tok::Special(')' | ']' | '}' | ',')
                | Tok::QuoteClose(_)
                | Tok::Res(ROp::RArrow | ROp::DArrow | ROp::Eq | ROp::Bar)
                | Tok::Eof
        )
    }

    // ----- names ------------------------------------------------------------

    fn modid(&mut self) -> PResult<String> {
        if !self.real() {
            return self.unexpected();
        }
        let name = match self.tok().clone() {
            Tok::ConId(s) => s,
            Tok::QConId(q, s) => format!("{q}.{s}"),
            _ => return self.unexpected(),
        };
        self.bump();
        Ok(name)
    }

    /// A variable name, possibly qualified (not an operator).
    fn qvar(&mut self) -> PResult<QName> {
        if !self.real() {
            return self.unexpected();
        }
        let q = match self.tok().clone() {
            Tok::VarId(s) => QName::unqual(s),
            Tok::QVarId(m, s) => QName {
                qual: Some(m),
                name: s,
            },
            _ => return self.unexpected(),
        };
        self.bump();
        Ok(q)
    }

    /// A constructor name, possibly qualified.
    fn qcon(&mut self) -> PResult<QName> {
        if !self.real() {
            return self.unexpected();
        }
        let q = match self.tok().clone() {
            Tok::ConId(s) => QName::unqual(s),
            Tok::QConId(m, s) => QName {
                qual: Some(m),
                name: s,
            },
            _ => return self.unexpected(),
        };
        self.bump();
        Ok(q)
    }

    /// A parenthesised operator in a name position, e.g. `(+)` or `(:|)`.
    fn paren_op(&mut self) -> PResult<QName> {
        self.expect_special('(')?;
        let q = match self.tok().clone() {
            Tok::VarSym(s) | Tok::ConSym(s) if self.real() => QName::unqual(s),
            Tok::Res(ROp::Tilde) if self.real() => QName::unqual("~"),
            Tok::Res(ROp::At) if self.real() => QName::unqual("@"),
            Tok::QVarSym(m, s) | Tok::QConSym(m, s) if self.real() => QName {
                qual: Some(m),
                name: s,
            },
            _ => return self.unexpected(),
        };
        self.bump();
        self.expect_special(')')?;
        Ok(q)
    }

    /// An unqualified variable or parenthesised operator: a binder name.
    fn binder_name(&mut self) -> PResult<String> {
        if self.at_special('(') {
            return Ok(self.paren_op()?.name);
        }
        if self.real() {
            if let Tok::VarId(s) = self.tok().clone() {
                self.bump();
                return Ok(s);
            }
        }
        self.unexpected()
    }

    // ----- module structure -------------------------------------------------

    fn module(&mut self) -> PResult<Module> {
        let loc = self.loc();
        let mut export_locs = Vec::new();
        let (name, exports) = if self.at_kw(Kw::Module) {
            self.bump();
            let name = self.modid()?;
            let exports = if self.at_special('(') {
                Some(self.ie_list(Some(&mut export_locs))?)
            } else {
                None
            };
            self.expect_kw(Kw::Where)?;
            (name, exports)
        } else {
            (
                "Main".to_string(),
                Some(vec![IEItem::Var(QName::unqual("main"))]),
            )
        };

        let items = self.block(|p| p.top_item(&name))?;
        if !self.at_eof() {
            return self.unexpected();
        }

        let mut imports = Vec::new();
        let mut decls = Vec::new();
        for item in items {
            match item {
                TopItem::Import(i) => {
                    if !decls.is_empty() {
                        return Err(ParseError {
                            line: i.loc.line,
                            col: i.loc.col,
                            msg: "parse error on input ‘import’".into(),
                        });
                    }
                    imports.push(i);
                }
                TopItem::Decl(Decl::Many(ds)) => decls.extend(ds),
                TopItem::Decl(d) => decls.push(d),
            }
        }
        self.check_kind_signature_requirements(&decls)?;
        Ok(Module {
            name,
            exports,
            export_locs,
            imports,
            decls: group_binds(decls),
            kind_annotations: std::mem::take(&mut self.kind_annotations),
            loc,
            rules: Vec::new(),
            inlines: Vec::new(),
        })
    }

    fn check_kind_signature_requirements(&self, declarations: &[Decl]) -> PResult<()> {
        if self.kind_signature_requirements.is_empty() {
            return Ok(());
        }
        let signatures: std::collections::HashSet<_> = declarations.iter().filter_map(|declaration| {
            if let Decl::KindSig { name, .. } = declaration { Some(name.as_str()) } else { None }
        }).collect();
        for requirement in &self.kind_signature_requirements {
            if !signatures.contains(requirement.signature_name.as_str()) {
                let name = &requirement.signature_name;
                return Err(ParseError {
                    line: requirement.at.line,
                    col: requirement.at.col,
                    msg: format!(
                        "An invisible binder in ‘{}’ needs an explicit kind signature\nAdd a standalone kind signature for ‘{name}’, or supply a complete kind with CUSKs enabled.",
                        requirement.declaration,
                    ),
                });
            }
        }
        Ok(())
    }

    fn top_item(&mut self, module: &str) -> PResult<TopItem> {
        if self.at_kw(Kw::Import) {
            return Ok(TopItem::Import(self.import_decl()?));
        }
        Ok(TopItem::Decl(self.topdecl(module)?))
    }

    fn import_decl(&mut self) -> PResult<Import> {
        let loc = self.loc();
        self.expect_kw(Kw::Import)?;
        let mut qualified = false;
        if self.at_varid("safe") {
            self.bump();
        }
        if self.at_varid("qualified") {
            self.bump();
            qualified = true;
        }
        let mut package = None;
        if self.real() && matches!(self.tok(), Tok::Str(_)) {
            if let Tok::Str(s) = self.tok() {
                package = Some(s.to_string());
            }
            self.bump();
        }
        let module = self.modid()?;
        if self.at_varid("qualified") {
            if qualified {
                return self.unexpected();
            }
            self.bump();
            qualified = true;
        }
        let alias = if self.at_varid("as") {
            self.bump();
            Some(self.modid()?)
        } else {
            None
        };
        let hiding = if self.at_varid("hiding") {
            self.bump();
            true
        } else {
            false
        };
        let items = if self.at_special('(') {
            Some(self.ie_list(None)?)
        } else {
            None
        };
        Ok(Import {
            module,
            package,
            qualified,
            alias,
            hiding,
            items,
            loc,
        })
    }

    fn ie_list(&mut self, mut locations: Option<&mut Vec<Loc>>) -> PResult<Vec<IEItem>> {
        self.expect_special('(')?;
        let exporting = locations.is_some();
        let mut items = Vec::new();
        loop {
            if self.at_special(')') {
                self.bump();
                break;
            }
            let loc = self.loc();
            let item = self.ie_item()?;
            if matches!(item, IEItem::Thing { subs: Subs::AllPlus(_), .. }) {
                let problem = if !exporting {
                    Some("An import list cannot mix ‘..’ with named members\nUse this form in a module export to bundle pattern synonyms.")
                } else if !self.exts.is_on("PatternSynonyms") {
                    Some("Mixing ‘..’ with named export members requires PatternSynonyms")
                } else {
                    None
                };
                if let Some(msg) = problem {
                    return Err(ParseError { line: loc.line, col: loc.col, msg: msg.into() });
                }
            }
            if let Some(locations) = &mut locations {
                locations.push(loc);
            }
            items.push(item);
            if self.at_special(',') {
                self.bump();
                continue;
            }
            self.expect_special(')')?;
            break;
        }
        Ok(items)
    }

    fn ie_item(&mut self) -> PResult<IEItem> {
        if self.at_kw(Kw::Module) {
            self.bump();
            return Ok(IEItem::Module(self.modid()?));
        }
        if self.at_kw(Kw::Type) {
            // ExplicitNamespaces: `type (op)` or `type T`. A parenthesised operator after `type`
            // names a type-level operator, not a value.
            if !self.exts.is_on("ExplicitNamespaces") {
                return self.err("The ‘type’ namespace requires ExplicitNamespaces");
            }
            self.bump();
            if self.at_special('(') {
                let name = self.paren_op()?;
                // `type (-.->)(..)`: a type operator with its constructors (sop-core).
                let subs = self.subs()?;
                return Ok(IEItem::Thing { name, subs });
            }
        }
        if self.at_varid("pattern") {
            let constructor = matches!(self.peek_raw(1), Tok::ConId(_) | Tok::QConId(..));
            let operator = *self.peek_raw(1) == Tok::Special('(')
                && matches!(self.peek_raw(2), Tok::ConSym(_) | Tok::QConSym(..));
            if constructor || operator {
                if !self.exts.is_on("PatternSynonyms") {
                    return self.err("The ‘pattern’ namespace requires PatternSynonyms");
                }
                self.bump();
                let name = if operator { self.paren_op()? } else { self.qcon()? };
                return Ok(IEItem::Pattern(name));
            }
        }
        if self.at_special('(') {
            let name = self.paren_op()?;
            let is_con = name.name.starts_with(':');
            if is_con {
                let subs = self.subs()?;
                return Ok(IEItem::Thing { name, subs });
            }
            return Ok(IEItem::Var(name));
        }
        if self.real() && matches!(self.tok(), Tok::ConId(_) | Tok::QConId(..)) {
            let name = self.qcon()?;
            let subs = self.subs()?;
            return Ok(IEItem::Thing { name, subs });
        }
        Ok(IEItem::Var(self.qvar()?))
    }

    fn subs(&mut self) -> PResult<Subs> {
        if !self.at_special('(') {
            return Ok(Subs::None);
        }
        self.bump();
        if self.at_res(ROp::DotDot) {
            self.bump();
            if self.at_special(',') {
                // Bundled names after `..`: pattern synonyms exported with the type.
                let mut extra = Vec::new();
                while self.at_special(',') {
                    self.bump();
                    if self.at_special('(') {
                        extra.push(self.paren_op()?.name);
                        continue;
                    }
                    match self.tok().clone() {
                        Tok::ConId(s) | Tok::VarId(s) => {
                            self.bump();
                            extra.push(s);
                        }
                        _ => return self.unexpected(),
                    }
                }
                self.expect_special(')')?;
                return Ok(Subs::AllPlus(extra));
            }
            self.expect_special(')')?;
            return Ok(Subs::All);
        }
        let mut names = Vec::new();
        // `T (C, f, .., P)`: a `..` among the names (GHC allows it anywhere in the list).
        let mut all = false;
        loop {
            if self.at_special(')') {
                break;
            }
            if self.at_res(ROp::DotDot) {
                self.bump();
                all = true;
            } else if self.at_special('(') {
                names.push(self.paren_op()?.name);
            } else if self.real() {
                match self.tok().clone() {
                    Tok::VarId(s) | Tok::ConId(s) => {
                        self.bump();
                        names.push(s);
                    }
                    _ => return self.unexpected(),
                }
            } else {
                return self.unexpected();
            }
            if self.at_special(',') {
                self.bump();
            } else {
                break;
            }
        }
        self.expect_special(')')?;
        if all {
            return Ok(Subs::AllPlus(names));
        }
        Ok(Subs::Some(names))
    }
}

/// Merges adjacent clauses of the same function into one binding.
pub fn group_binds(decls: Vec<Decl>) -> Vec<Decl> {
    let mut out: Vec<Decl> = Vec::new();
    for d in decls {
        if let Decl::Fun { name, clauses, loc } = d {
            if let Some(Decl::Fun {
                name: prev_name,
                clauses: prev_clauses,
                ..
            }) = out.last_mut()
            {
                if *prev_name == name {
                    prev_clauses.extend(clauses);
                    continue;
                }
            }
            out.push(Decl::Fun { name, clauses, loc });
        } else {
            out.push(d);
        }
    }
    out
}
