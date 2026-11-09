//! Haskell lexer. Produces tokens with positions; layout is applied by the parser.

use super::token::{Kw, PrimLit, ROp, Tok, Token};

#[derive(Debug, Clone)]
pub struct LexError {
    pub line: u32,
    pub col: u32,
    pub msg: String,
}

/// A `{-# NAME body #-}` pragma. Only LANGUAGE and OPTIONS_GHC are kept.
#[derive(Debug, Clone)]
pub struct Pragma {
    pub name: String,
    pub body: String,
    pub line: u32,
}

pub struct Lexed {
    pub tokens: Vec<Token>,
    pub pragmas: Vec<Pragma>,
}

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

fn keyword(word: &str) -> Option<Kw> {
    Some(match word {
        "case" => Kw::Case,
        "class" => Kw::Class,
        "data" => Kw::Data,
        "default" => Kw::Default,
        "deriving" => Kw::Deriving,
        "do" => Kw::Do,
        "else" => Kw::Else,
        "foreign" => Kw::Foreign,
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
    let mut lx = Lexer {
        chars: src.chars().collect(),
        pos: 0,
        line: 1,
        col: 1,
        pragmas: Vec::new(),
        toks: Vec::new(),
    };
    lx.run()?;
    Ok(Lexed {
        tokens: lx.toks,
        pragmas: lx.pragmas,
    })
}

struct Lexer {
    chars: Vec<char>,
    pos: usize,
    line: u32,
    col: u32,
    pragmas: Vec<Pragma>,
    toks: Vec<Token>,
}

impl Lexer {
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
            let (saw_nl, saw_space) = self.skip_space()?;
            bol |= saw_nl;
            let (line, col) = (self.line, self.col);
            if self.pos >= self.chars.len() {
                self.toks.push(Token {
                    tok: Tok::Eof,
                    line,
                    col,
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
            self.toks.push(Token {
                tok,
                line,
                col,
                first_on_line,
                space_before,
                space_after,
            });
        }
    }

    /// Skips whitespace and comments. Returns (saw a newline, saw any space).
    fn skip_space(&mut self) -> Result<(bool, bool), LexError> {
        let (mut nl, mut sp) = (false, false);
        loop {
            match self.peek(0) {
                Some(c) if c.is_whitespace() => {
                    nl |= c == '\n';
                    sp = true;
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
                    let newline = if self.peek(2) == Some('#') {
                        self.pragma()?
                    } else {
                        self.block_comment()?
                    };
                    nl |= newline;
                    sp = true;
                }
                _ => return Ok((nl, sp)),
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

    fn pragma(&mut self) -> Result<bool, LexError> {
        let (line, start_line) = (self.line, self.line);
        let _ = start_line;
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
        if name == "LANGUAGE" || name == "OPTIONS_GHC" {
            let rest = trimmed[name.len().min(trimmed.len())..].trim().to_string();
            self.pragmas.push(Pragma {
                name,
                body: rest,
                line,
            });
        }
        Ok(nl)
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
                // A quote that cannot open a character literal is a promotion tick (DataKinds).
                let promoted = self.peek(1).is_some_and(|c| c.is_alphabetic() || c == '[' || c == '(' || c == ':')
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
            // MagicHash: names may end in one or more '#'.
            while self.peek(0) == Some('#') && !self.peek(1).is_some_and(is_symbol) {
                word.push(self.bump());
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
                        if keyword(&next_word).is_some() {
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
                if let Some(kw) = keyword(&word) {
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

    fn take_digits(&mut self) -> String {
        self.take_while(|c| c.is_ascii_digit() || c == '_')
    }

    /// Counts and consumes the MagicHash suffix of a literal (`3#` is 1, `3##` is 2).
    fn take_hashes(&mut self) -> usize {
        let mut n = 0;
        while self.peek(0) == Some('#') && !self.peek(1).is_some_and(is_symbol) {
            self.bump();
            n += 1;
        }
        n
    }

    /// Skips the MagicHash suffix of a literal (`3#`, `3##`, `'c'#`, `"s"#`).
    fn skip_hashes(&mut self) {
        while self.peek(0) == Some('#') && !self.peek(1).is_some_and(is_symbol) {
            self.bump();
        }
    }

    fn lex_number(&mut self) -> Result<Tok, LexError> {
        if self.peek(0) == Some('0') {
            let radix = match (self.peek(1), self.peek(2)) {
                (Some('x' | 'X'), Some(c)) if c.is_ascii_hexdigit() => Some(16),
                (Some('o' | 'O'), Some(c)) if c.is_digit(8) => Some(8),
                (Some('b' | 'B'), Some(c)) if c.is_digit(2) => Some(2),
                _ => None,
            };
            if let Some(r) = radix {
                self.bump();
                self.bump();
                let digits = self.take_while(|c| c == '_' || c.is_digit(r));
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
    char::from_u32(value).ok_or_else(|| lx.error("numeric escape sequence out of range"))
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
