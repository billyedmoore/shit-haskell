//! Haskell lexer. Produces tokens with positions; layout is applied by the parser.

use super::token::{Kw, PrimLit, ROp, Tok, Token};

#[derive(Debug, Clone)]
pub struct LexError {
    pub line: u32,
    pub col: u32,
    pub msg: String,
}

/// A `{-# NAME body #-}` pragma. Only LANGUAGE, OPTIONS_GHC, RULES, INLINE and NOINLINE are
/// kept.
#[derive(Debug, Clone)]
pub struct Pragma {
    pub name: String,
    pub body: String,
    pub line: u32,
    /// The column of its `{-#` (1 for one at a module's top level).
    pub col: u32,
    pub(crate) body_loc: super::ast::Loc,
}

pub struct Lexed {
    pub tokens: Vec<Token>,
    pub pragmas: Vec<Pragma>,
    pub(crate) tabs: Option<(super::ast::Loc, usize)>,
    pub(crate) saw_binary: bool,
    pub(crate) numeric_separator: Option<super::ast::Loc>,
    pub(crate) unknown_pragmas: Vec<(super::ast::Loc, String)>,
}

/// Known pragma names, including GHC's spelling variants and legacy pragmas it ignores.
pub(crate) const KNOWN_PRAGMAS: &[&str] = &[
    "ANN", "CFILES", "COLUMN", "COMPLETE", "CONTRACT", "CTYPE", "DEPRECATED",
    "INCLUDE", "INCOHERENT", "INLINABLE", "INLINE", "INLINEABLE", "LANGUAGE", "LINE",
    "MINIMAL", "NOINLINE", "NOTINLINE", "NOUNPACK", "OPAQUE", "OPTIONS", "OPTIONS_CATCH",
    "OPTIONS_DERIVE", "OPTIONS_GHC", "OPTIONS_HADDOCK", "OPTIONS_HUGS", "OPTIONS_JHC",
    "OPTIONS_NHC98", "OPTIONS_YHC", "OVERLAPPABLE", "OVERLAPPING", "OVERLAPS", "RULES",
    "SCC", "SOURCE", "SPECIALISE", "SPECIALIZE", "UNPACK", "WARNING",
];

const SYMBOL_CHARS: &str = "!#$%&*+./<=>?@\\^|-~:";

pub fn is_symbol(c: char) -> bool {
    if c.is_ascii() {
        SYMBOL_CHARS.contains(c)
    } else {
        !c.is_alphanumeric() && !c.is_whitespace()
    }
}

fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '\''
}

fn keyword(word: &str, foreign_interface: bool) -> Option<Kw> {
    Some(match word {
        "case" => Kw::Case,
        "class" => Kw::Class,
        "data" => Kw::Data,
        "default" => Kw::Default,
        "deriving" => Kw::Deriving,
        "do" => Kw::Do,
        "else" => Kw::Else,
        "foreign" if foreign_interface => Kw::Foreign,
        "if" => Kw::If,
        "import" => Kw::Import,
        "in" => Kw::In,
        "infix" => Kw::Infix,
        "infixl" => Kw::Infixl,
        "infixr" => Kw::Infixr,
        "instance" => Kw::Instance,
        "let" => Kw::Let,
        "module" => Kw::Module,
        "newtype" => Kw::Newtype,
        "of" => Kw::Of,
        "then" => Kw::Then,
        "type" => Kw::Type,
        "where" => Kw::Where,
        "_" => Kw::Underscore,
        _ => return None,
    })
}

/// ASCII control-character names usable in escapes such as `\SOH`.
const ASCII_ESCAPES: &[(&str, u32)] = &[
    ("NUL", 0),
    ("SOH", 1),
    ("STX", 2),
    ("ETX", 3),
    ("EOT", 4),
    ("ENQ", 5),
    ("ACK", 6),
    ("BEL", 7),
    ("BS", 8),
    ("HT", 9),
    ("LF", 10),
    ("VT", 11),
    ("FF", 12),
    ("CR", 13),
    ("SO", 14),
    ("SI", 15),
    ("DLE", 16),
    ("DC1", 17),
    ("DC2", 18),
    ("DC3", 19),
    ("DC4", 20),
    ("NAK", 21),
    ("SYN", 22),
    ("ETB", 23),
    ("CAN", 24),
    ("EM", 25),
    ("SUB", 26),
    ("ESC", 27),
    ("FS", 28),
    ("GS", 29),
    ("RS", 30),
    ("US", 31),
    ("SP", 32),
    ("DEL", 127),
];

pub fn lex(src: &str) -> Result<Lexed, LexError> {
    lex_with(src, true, false)
}

/// Lexes a module; without `magic_hash` (MagicHash off) a name does not take a `#` suffix, so
/// that `(# q, r#)` closes its unboxed tuple. With `unicode` (UnicodeSyntax), `∷ ⇒ → ← ∀ ★` are
/// the reserved symbols they stand for; without it they are ordinary operators.
pub fn lex_with(src: &str, magic_hash: bool, unicode: bool) -> Result<Lexed, LexError> {
    Lexer::new(src.chars().collect(), magic_hash, unicode).finish()
}

pub(crate) fn lex_for_module(src: &str, exts: &super::extensions::Extensions) -> Result<Lexed, LexError> {
    let mut lexer = Lexer::new(src.chars().collect(), exts.is_on("MagicHash"), exts.is_on("UnicodeSyntax"));
    lexer.binary = exts.is_on("BinaryLiterals");
    lexer.foreign_interface = exts.is_on("ForeignFunctionInterface");
    lexer.finish()
}

/// Measures the token starting at a diagnostic position on one displayed line.
pub(crate) fn first_token_width(chars: impl Iterator<Item = char>) -> Option<usize> {
    let mut lexer = Lexer::new(chars.collect(), true, true);
    lexer.peek(0)?;
    lexer.lex_token().ok()?;
    Some(lexer.pos)
}

struct Lexer {
    chars: Vec<char>,
    pos: usize,
    line: u32,
    col: u32,
    pragmas: Vec<Pragma>,
    toks: Vec<Token>,
    magic_hash: bool,
    /// UnicodeSyntax: the Unicode forms of reserved symbols are those symbols.
    unicode: bool,
    tabs: Option<(super::ast::Loc, usize)>,
    binary: bool,
    foreign_interface: bool,
    saw_binary: bool,
    numeric_separator: Option<super::ast::Loc>,
    unknown_pragmas: Vec<(super::ast::Loc, String)>,
}

impl Lexer {
    fn new(chars: Vec<char>, magic_hash: bool, unicode: bool) -> Self {
        Self {
            chars,
            pos: 0,
            line: 1,
            col: 1,
            pragmas: Vec::new(),
            toks: Vec::new(),
            magic_hash,
            unicode,
            tabs: None,
            binary: true,
            foreign_interface: true,
            saw_binary: false,
            numeric_separator: None,
            unknown_pragmas: Vec::new(),
        }
    }

    fn finish(mut self) -> Result<Lexed, LexError> {
        self.run()?;
        Ok(Lexed {
            tokens: self.toks,
            pragmas: self.pragmas,
            tabs: self.tabs,
            saw_binary: self.saw_binary,
            numeric_separator: self.numeric_separator,
            unknown_pragmas: self.unknown_pragmas,
        })
    }

    fn peek(&self, k: usize) -> Option<char> {
        self.chars.get(self.pos + k).copied()
    }

    fn bump(&mut self) -> char {
        let c = self.chars[self.pos];
        self.pos += 1;
        match c {
            '\n' => {
                self.line += 1;
                self.col = 1;
            }
            '\t' => self.col = ((self.col - 1) / 8 + 1) * 8 + 1,
            _ => self.col += 1,
        }
        c
    }

    fn error(&self, msg: impl Into<String>) -> LexError {
        LexError {
            line: self.line,
            col: self.col,
            msg: msg.into(),
        }
    }

    fn run(&mut self) -> Result<(), LexError> {
        let mut bol = true;
        loop {
            let (saw_nl, saw_space, pragma) = self.skip_space()?;
            bol |= saw_nl;
            if let Some(mut pragma) = pragma {
                pragma.first_on_line = bol;
                self.toks.push(pragma);
                bol = false;
                continue;
            }
            let (line, col) = (self.line, self.col);
            if self.pos >= self.chars.len() {
                self.toks.push(Token {
                    tok: Tok::Eof,
                    line,
                    col,
                    end_line: line,
                    end_col: col,
                    first_on_line: true,
                    space_before: true,
                    space_after: true,
                });
                return Ok(());
            }
            let space_before = saw_space
                || self.pos == 0
                || matches!(self.chars[self.pos - 1], '(' | '[' | ',' | ';' | '{');
            let first_on_line = bol;
            bol = false;
            let tok = self.lex_token()?;
            let space_after = match self.peek(0) {
                None => true,
                Some(c) => c.is_whitespace() || matches!(c, ')' | ']' | ',' | ';' | '}'),
            };
            // The token's last character is the one before the lexer's position.
            let (end_line, end_col) = (self.line, self.col.saturating_sub(1).max(1));
            self.toks.push(Token {
                tok,
                line,
                col,
                end_line,
                end_col,
                first_on_line,
                space_before,
                space_after,
            });
        }
    }

    /// Skips whitespace and comments, stopping at a pragma that participates in layout.
    fn skip_space(&mut self) -> Result<(bool, bool, Option<Token>), LexError> {
        let (mut nl, mut sp) = (false, false);
        loop {
            match self.peek(0) {
                Some(c) if c.is_whitespace() => {
                    nl |= c == '\n';
                    sp = true;
                    if c == '\t' {
                        let loc = super::ast::Loc { line: self.line, col: self.col };
                        self.tabs.get_or_insert((loc, 0)).1 += 1;
                    }
                    self.bump();
                }
                Some('-') if self.peek(1) == Some('-') && self.is_line_comment() => {
                    while let Some(c) = self.peek(0) {
                        if c == '\n' {
                            break;
                        }
                        self.bump();
                    }
                    sp = true;
                }
                Some('{') if self.peek(1) == Some('-') => {
                    if self.peek(2) == Some('#') {
                        let (line, col) = (self.line, self.col);
                        let (newline, tok) = self.pragma()?;
                        if let Some(tok) = tok {
                            let pragma = Token {
                                tok, line, col,
                                end_line: self.line,
                                end_col: self.col.saturating_sub(1),
                                first_on_line: false,
                                space_before: true,
                                space_after: true,
                            };
                            return Ok((nl, sp, Some(pragma)));
                        }
                        nl |= newline;
                    } else {
                        nl |= self.block_comment()?;
                    }
                    sp = true;
                }
                _ => return Ok((nl, sp, None)),
            }
        }
    }

    /// `--` starts a comment unless it is part of a longer operator such as `-->`.
    fn is_line_comment(&self) -> bool {
        let mut i = 0;
        while self.peek(i) == Some('-') {
            i += 1;
        }
        match self.peek(i) {
            Some(c) => !is_symbol(c),
            None => true,
        }
    }

    fn block_comment(&mut self) -> Result<bool, LexError> {
        let mut depth = 0usize;
        let mut nl = false;
        loop {
            match (self.peek(0), self.peek(1)) {
                (None, _) => return Err(self.error("unterminated `{-'")),
                (Some('{'), Some('-')) => {
                    self.bump();
                    self.bump();
                    depth += 1;
                }
                (Some('-'), Some('}')) => {
                    self.bump();
                    self.bump();
                    depth -= 1;
                    if depth == 0 {
                        return Ok(nl);
                    }
                }
                (Some(c), _) => {
                    nl |= c == '\n';
                    self.bump();
                }
            }
        }
    }

    fn pragma(&mut self) -> Result<(bool, Option<Tok>), LexError> {
        let (line, col) = (self.line, self.col);
        self.bump();
        self.bump();
        self.bump();
        let mut body = String::new();
        let mut nl = false;
        loop {
            match (self.peek(0), self.peek(1), self.peek(2)) {
                (None, _, _) => return Err(self.error("unterminated pragma")),
                (Some('#'), Some('-'), Some('}')) => {
                    self.bump();
                    self.bump();
                    self.bump();
                    break;
                }
                (Some(c), _, _) => {
                    nl |= c == '\n';
                    body.push(self.bump());
                }
            }
        }
        let trimmed = body.trim();
        let name: String = trimmed
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect::<String>()
            .to_ascii_uppercase();
        let name_len = name.len();
        if !name.is_empty() && !KNOWN_PRAGMAS.contains(&name.as_str()) {
            let padding = body.len() - body.trim_start().len();
            let at = super::ast::Loc { line, col: col as u32 + 3 }.after(&body[..padding]);
            self.unknown_pragmas.push((at, name));
            return Ok((nl, None));
        }
        let name = if name == "NOTINLINE" { "NOINLINE".to_string() } else { name };
        if ["LANGUAGE", "OPTIONS_GHC", "RULES", "INLINE", "NOINLINE", "MINIMAL"].contains(&name.as_str()) {
            let body_start = body.len() - body.trim_start().len() + name_len;
            let after_name = &body[body_start..];
            let padding = after_name.len() - after_name.trim_start().len();
            let body_loc = super::ast::Loc { line, col: col as u32 + 3 }.after(&body[..body_start + padding]);
            if name == "MINIMAL" {
                return Ok((nl, Some(Tok::Minimal(after_name.trim_start().to_string(), body_loc))));
            }
            // A RULES body keeps its line breaks (its rules' positions count lines from here).
            let rest = if name == "RULES" {
                after_name.to_string()
            } else {
                after_name.trim().to_string()
            };
            self.pragmas.push(Pragma {
                name,
                body: rest,
                line,
                col: col as u32,
                body_loc,
            });
        }
        Ok((nl, None))
    }

    fn lex_token(&mut self) -> Result<Tok, LexError> {
        let c = self.peek(0).expect("caller checked for end of input");
        if c.is_alphabetic() || c == '_' {
            return self.lex_name();
        }
        if c.is_ascii_digit() {
            return self.lex_number();
        }
        match c {
            '"' => return self.lex_string(),
            '\'' => {
                // `''T` is Template Haskell's quote of a type name: two ticks.
                if self.peek(1) == Some('\'') && self.peek(2).is_some_and(|c| c.is_uppercase() || c == '(' || c == '[') {
                    self.bump();
                    return Ok(Tok::Special('\''));
                }
                // A quote that cannot open a character literal is a promotion tick (DataKinds)
                // or Template Haskell value name quote (including names starting with `_`, such
                // as `'_Wrapped'`).
                let promoted = self.peek(1).is_some_and(|c| c.is_alphabetic() || c == '_' || c == '[' || c == '(' || c == ':')
                    && self.peek(2) != Some('\'');
                if promoted {
                    self.bump();
                    return Ok(Tok::Special('\''));
                }
                return self.lex_char();
            }
            '(' | ')' | ',' | ';' | '[' | ']' | '`' | '{' | '}' => {
                self.bump();
                return Ok(Tok::Special(c));
            }
            _ => {}
        }
        // A negative unboxed literal (`-1#`) is one token unless the minus follows a closing
        // token (`x-1#` is a subtraction), as GHC lexes it.
        if c == '-' && self.peek(1).is_some_and(|d| d.is_ascii_digit()) {
            let prev = if self.pos == 0 { '\n' } else { self.chars[self.pos - 1] };
            let closing = matches!(prev, ')' | ']' | '"' | '\'' | '_') || prev.is_alphanumeric()
                || (prev == '}' && !(self.pos >= 2 && self.chars[self.pos - 2] == '-'));
            if !closing {
                let saved = (self.pos, self.line, self.col);
                self.bump();
                match self.lex_number()? {
                    Tok::Prim(p @ (PrimLit::Int(_) | PrimLit::Word(_) | PrimLit::Float(_) | PrimLit::Double(_))) => {
                        let neg = |v: String| format!("-{v}");
                        return Ok(Tok::Prim(match p {
                            PrimLit::Int(v) => PrimLit::Int(neg(v)),
                            PrimLit::Word(v) => PrimLit::Word(neg(v)),
                            PrimLit::Float(v) => PrimLit::Float(neg(v)),
                            PrimLit::Double(v) => PrimLit::Double(neg(v)),
                            other => other,
                        }));
                    }
                    _ => (self.pos, self.line, self.col) = saved,
                }
            }
        }
        if is_symbol(c) {
            return self.lex_symbol(None);
        }
        Err(self.error(format!("lexical error at character {c:?}")))
    }

    fn take_while(&mut self, pred: impl Fn(char) -> bool) -> String {
        let mut s = String::new();
        while let Some(c) = self.peek(0) {
            if !pred(c) {
                break;
            }
            s.push(self.bump());
        }
        s
    }

    fn lex_name(&mut self) -> Result<Tok, LexError> {
        let mut quals: Vec<String> = Vec::new();
        loop {
            let mut word = self.take_while(is_ident_char);
            // MagicHash: names may end in one or more '#', unless a symbol follows the run.
            let mut run = 0;
            while self.peek(run) == Some('#') {
                run += 1;
            }
            if run > 0 && self.magic_hash && !self.peek(run).is_some_and(|c| is_symbol(c) && c != '#') {
                for _ in 0..run {
                    word.push(self.bump());
                }
            }
            let upper = word.chars().next().is_some_and(|c| c.is_uppercase());
            if upper && self.peek(0) == Some('.') {
                match self.peek(1) {
                    // `Just.let` is `Just . let`: a keyword cannot be qualified.
                    Some(c) if c.is_alphabetic() || c == '_' => {
                        let next_word: String = self.chars[self.pos + 1..]
                            .iter()
                            .take_while(|c| is_ident_char(**c))
                            .collect();
                        if keyword(&next_word, self.foreign_interface).is_some() {
                            if quals.is_empty() {
                                return Ok(Tok::ConId(word));
                            }
                            return Ok(Tok::QConId(quals.join("."), word));
                        }
                        quals.push(word);
                        self.bump();
                        continue;
                    }
                    Some(c) if is_symbol(c) => {
                        quals.push(word);
                        self.bump();
                        let qual = quals.join(".");
                        return self.lex_symbol(Some(qual));
                    }
                    _ => {}
                }
            }
            if upper {
                return Ok(if quals.is_empty() {
                    Tok::ConId(word)
                } else {
                    Tok::QConId(quals.join("."), word)
                });
            }
            if quals.is_empty() {
                if let Some(kw) = keyword(&word, self.foreign_interface) {
                    return Ok(Tok::Kw(kw));
                }
                return Ok(Tok::VarId(word));
            }
            return Ok(Tok::QVarId(quals.join("."), word));
        }
    }

    fn lex_symbol(&mut self, qual: Option<String>) -> Result<Tok, LexError> {
        let sym = self.take_while(is_symbol);
        if let Some(q) = qual {
            return Ok(if sym.starts_with(':') {
                Tok::QConSym(q, sym)
            } else {
                Tok::QVarSym(q, sym)
            });
        }
        // UnicodeSyntax: `∀` is `forall`, `★` is `*` (a kind), and the arrows and colons the
        // reserved operators.
        let sym = if self.unicode {
            match sym.as_str() {
                "∀" => return Ok(Tok::VarId("forall".into())),
                "★" => "*".to_string(),
                "∷" => "::".to_string(),
                "←" => "<-".to_string(),
                "→" => "->".to_string(),
                "⇒" => "=>".to_string(),
                _ => sym,
            }
        } else {
            sym
        };
        let rop = match sym.as_str() {
            ".." => Some(ROp::DotDot),
            "::" => Some(ROp::DColon),
            "=" => Some(ROp::Eq),
            "\\" => Some(ROp::Backslash),
            "|" => Some(ROp::Bar),
            "<-" => Some(ROp::LArrow),
            "->" => Some(ROp::RArrow),
            "@" => Some(ROp::At),
            "~" => Some(ROp::Tilde),
            "=>" => Some(ROp::DArrow),
            _ => None,
        };
        if let Some(r) = rop {
            return Ok(Tok::Res(r));
        }
        Ok(if sym.starts_with(':') {
            Tok::ConSym(sym)
        } else {
            Tok::VarSym(sym)
        })
    }

    fn digit_sequence_len(&self, offset: usize, radix: u32) -> usize {
        let mut end = 0;
        for (i, &c) in self.chars[self.pos + offset..].iter().enumerate() {
            if c.is_digit(radix) {
                end = i + 1;
            } else if c != '_' {
                break;
            }
        }
        end
    }

    fn take_digits(&mut self) -> String {
        let len = self.digit_sequence_len(0, 10);
        (0..len).map(|_| self.bump()).collect()
    }

    /// Counts and consumes the MagicHash suffix of a literal (`3#` is 1, `3##` is 2).
    fn take_hashes(&mut self) -> usize {
        // At most two `#`, by maximal munch as GHC lexes them (`1#::Int#` is `1#` and `::`).
        let mut run = 0;
        while run < 2 && self.peek(run) == Some('#') {
            run += 1;
        }
        for _ in 0..run {
            self.bump();
        }
        run
    }

    fn lex_number(&mut self) -> Result<Tok, LexError> {
        let loc = super::ast::Loc { line: self.line, col: self.col };
        if self.peek(0) == Some('0') {
            let radix = match self.peek(1) {
                Some('x' | 'X') => Some(16),
                Some('o' | 'O') => Some(8),
                Some('b' | 'B') if self.binary => Some(2),
                _ => None,
            };
            if let Some((r, len)) = radix.map(|r| (r, self.digit_sequence_len(2, r))).filter(|&(_, len)| len != 0) {
                self.saw_binary |= r == 2;
                self.bump();
                self.bump();
                let digits: String = (0..len).map(|_| self.bump()).collect();
                if digits.contains('_') {
                    self.numeric_separator.get_or_insert(loc);
                }
                let hashes = self.take_hashes();
                let value = to_decimal(&digits, r);
                return Ok(match hashes {
                    0 => Tok::Int(value),
                    1 => Tok::Prim(PrimLit::Int(value)),
                    _ => Tok::Prim(PrimLit::Word(value)),
                });
            }
        }
        let mut text = self.take_digits();
        let mut is_float = false;
        if self.peek(0) == Some('.') && self.peek(1).is_some_and(|c| c.is_ascii_digit()) {
            self.bump();
            text.push('.');
            text.push_str(&self.take_digits());
            is_float = true;
        }
        if matches!(self.peek(0), Some('e' | 'E')) {
            let exp_ok = match self.peek(1) {
                Some('+' | '-') => self.peek(2).is_some_and(|c| c.is_ascii_digit()),
                Some(c) => c.is_ascii_digit(),
                None => false,
            };
            if exp_ok {
                self.bump();
                text.push('e');
                if matches!(self.peek(0), Some('+' | '-')) {
                    text.push(self.bump());
                }
                text.push_str(&self.take_digits());
                is_float = true;
            }
        }
        let hashes = self.take_hashes();
        if text.contains('_') {
            self.numeric_separator.get_or_insert(loc);
        }
        let clean: String = text.chars().filter(|c| *c != '_').collect();
        Ok(match (is_float, hashes) {
            (false, 0) => Tok::Int(clean),
            (false, 1) => Tok::Prim(PrimLit::Int(clean)),
            (false, _) => Tok::Prim(PrimLit::Word(clean)),
            (true, 0) => Tok::Float(clean),
            (true, 1) => Tok::Prim(PrimLit::Float(clean)),
            (true, _) => Tok::Prim(PrimLit::Double(clean)),
        })
    }

    fn lex_string(&mut self) -> Result<Tok, LexError> {
        self.bump();
        let mut out = String::new();
        loop {
            match self.peek(0) {
                None | Some('\n') => return Err(self.error("lexical error in string/character literal")),
                Some('"') => {
                    self.bump();
                    break;
                }
                Some('\\') => {
                    if self.peek(1).is_some_and(|c| c.is_whitespace()) {
                        // string gap
                        self.bump();
                        while self.peek(0).is_some_and(|c| c.is_whitespace()) {
                            self.bump();
                        }
                        if self.peek(0) != Some('\\') {
                            return Err(self.error("lexical error in string gap"));
                        }
                        self.bump();
                        continue;
                    }
                    self.bump();
                    if let Some(c) = self.escape()? {
                        out.push(c);
                    }
                }
                Some(_) => out.push(self.bump()),
            }
        }
        if self.take_hashes() > 0 {
            return Ok(Tok::Prim(PrimLit::Str(out)));
        }
        Ok(Tok::Str(out))
    }

    fn lex_char(&mut self) -> Result<Tok, LexError> {
        self.bump();
        let c = match self.peek(0) {
            Some('\\') => {
                self.bump();
                match self.escape()? {
                    Some(c) => c,
                    None => return Err(self.error("lexical error in character literal")),
                }
            }
            Some(c) if c != '\'' && c != '\n' => self.bump(),
            _ => return Err(self.error("lexical error in character literal")),
        };
        if self.peek(0) == Some('\'') {
            self.bump();
            if self.take_hashes() > 0 {
                return Ok(Tok::Prim(PrimLit::Char(c)));
            }
            Ok(Tok::Char(c))
        } else {
            Err(self.error("lexical error in character literal"))
        }
    }

    /// Reads an escape after the backslash. `None` is the empty escape `\&`.
    fn escape(&mut self) -> Result<Option<char>, LexError> {
        let Some(c) = self.peek(0) else {
            return Err(self.error("lexical error in escape"));
        };
        let simple = match c {
            'a' => Some('\x07'),
            'b' => Some('\x08'),
            'f' => Some('\x0c'),
            'n' => Some('\n'),
            'r' => Some('\r'),
            't' => Some('\t'),
            'v' => Some('\x0b'),
            '\\' | '"' | '\'' => Some(c),
            _ => None,
        };
        if let Some(ch) = simple {
            self.bump();
            return Ok(Some(ch));
        }
        if c == '&' {
            self.bump();
            return Ok(None);
        }
        if c == '^' {
            self.bump();
            let Some(ctl) = self.peek(0) else {
                return Err(self.error("lexical error in escape"));
            };
            if !(('@'..='_').contains(&ctl)) {
                return Err(self.error("lexical error in escape"));
            }
            self.bump();
            return Ok(Some(char::from_u32(ctl as u32 - 64).expect("control char")));
        }
        if c.is_ascii_digit() {
            let digits = self.take_while(|c| c.is_ascii_digit());
            return Ok(Some(code_char(&digits, 10, self)?));
        }
        if c == 'x' {
            self.bump();
            let digits = self.take_while(|c| c.is_ascii_hexdigit());
            if digits.is_empty() {
                return Err(self.error("lexical error in escape"));
            }
            return Ok(Some(code_char(&digits, 16, self)?));
        }
        if c == 'o' {
            self.bump();
            let digits = self.take_while(|c| c.is_digit(8));
            if digits.is_empty() {
                return Err(self.error("lexical error in escape"));
            }
            return Ok(Some(code_char(&digits, 8, self)?));
        }
        // ASCII control names: prefer the longest match.
        let mut best: Option<(&str, u32)> = None;
        for (name, code) in ASCII_ESCAPES {
            let matches = name
                .chars()
                .enumerate()
                .all(|(i, nc)| self.peek(i) == Some(nc));
            if matches && best.is_none_or(|(b, _)| name.len() > b.len()) {
                best = Some((name, *code));
            }
        }
        if let Some((name, code)) = best {
            for _ in 0..name.len() {
                self.bump();
            }
            return Ok(Some(char::from_u32(code).expect("ascii")));
        }
        Err(self.error("lexical error in escape"))
    }
}

fn code_char(digits: &str, radix: u32, lx: &Lexer) -> Result<char, LexError> {
    let decimal = to_decimal(digits, radix);
    let value: u32 = decimal.parse().unwrap_or(u32::MAX);
    char_of_code(value).ok_or_else(|| lx.error("numeric escape sequence out of range"))
}

/// Where a surrogate code point (U+D800..U+DFFF) is kept in a literal's text: a Rust `char`
/// cannot hold one, so the first 2048 code points of plane 15 (private use, U+F0000..U+F07FF)
/// stand for them, and the back ends map them back (`char_code`) when they build a `Char`.
const SURROGATE_HOME: u32 = 0xF0000;

/// The character standing for a code point in a literal (see `SURROGATE_HOME`).
pub fn char_of_code(v: u32) -> Option<char> {
    if (0xD800..=0xDFFF).contains(&v) {
        return char::from_u32(v - 0xD800 + SURROGATE_HOME);
    }
    char::from_u32(v)
}

/// The code point a literal's character stands for (surrogates included).
pub fn char_code(c: char) -> u32 {
    let v = c as u32;
    if (SURROGATE_HOME..SURROGATE_HOME + 0x800).contains(&v) {
        v - SURROGATE_HOME + 0xD800
    } else {
        v
    }
}

/// Converts digits in `radix` to a decimal string without any width limit.
pub fn to_decimal(digits: &str, radix: u32) -> String {
    const BASE: u64 = 1_000_000_000;
    let mut limbs: Vec<u64> = vec![0];
    for ch in digits.chars().filter(|c| *c != '_') {
        let mut carry = ch.to_digit(radix).unwrap_or(0) as u64;
        for limb in limbs.iter_mut() {
            let v = *limb * radix as u64 + carry;
            *limb = v % BASE;
            carry = v / BASE;
        }
        while carry > 0 {
            limbs.push(carry % BASE);
            carry /= BASE;
        }
    }
    let mut out = String::new();
    for (i, limb) in limbs.iter().rev().enumerate() {
        if i == 0 {
            out.push_str(&limb.to_string());
        } else {
            out.push_str(&format!("{limb:09}"));
        }
    }
    out
}
