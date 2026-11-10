//! Tokens produced by the lexer and consumed by the layout-aware parser.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kw {
    Case,
    Class,
    Data,
    Default,
    Deriving,
    Do,
    Else,
    Foreign,
    If,
    Import,
    In,
    Infix,
    Infixl,
    Infixr,
    Instance,
    Let,
    Module,
    Newtype,
    Of,
    Then,
    Type,
    Where,
    Underscore,
}

impl Kw {
    pub fn text(self) -> &'static str {
        match self {
            Kw::Case => "case",
            Kw::Class => "class",
            Kw::Data => "data",
            Kw::Default => "default",
            Kw::Deriving => "deriving",
            Kw::Do => "do",
            Kw::Else => "else",
            Kw::Foreign => "foreign",
            Kw::If => "if",
            Kw::Import => "import",
            Kw::In => "in",
            Kw::Infix => "infix",
            Kw::Infixl => "infixl",
            Kw::Infixr => "infixr",
            Kw::Instance => "instance",
            Kw::Let => "let",
            Kw::Module => "module",
            Kw::Newtype => "newtype",
            Kw::Of => "of",
            Kw::Then => "then",
            Kw::Type => "type",
            Kw::Where => "where",
            Kw::Underscore => "_",
        }
    }
}

/// Reserved operators: these are never ordinary operators.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ROp {
    DotDot,
    DColon,
    Eq,
    Backslash,
    Bar,
    LArrow,
    RArrow,
    At,
    Tilde,
    DArrow,
}

impl ROp {
    pub fn text(self) -> &'static str {
        match self {
            ROp::DotDot => "..",
            ROp::DColon => "::",
            ROp::Eq => "=",
            ROp::Backslash => "\\",
            ROp::Bar => "|",
            ROp::LArrow => "<-",
            ROp::RArrow => "->",
            ROp::At => "@",
            ROp::Tilde => "~",
            ROp::DArrow => "=>",
        }
    }
}

/// A primitive literal, with the type its MagicHash suffix gives it.
#[derive(Clone, Debug, PartialEq)]
pub enum PrimLit {
    /// `3#`: `Int#`.
    Int(String),
    /// `3##`: `Word#`.
    Word(String),
    /// `1.5#`: `Float#`.
    Float(String),
    /// `1.5##`: `Double#`.
    Double(String),
    /// `'c'#`: `Char#`.
    Char(char),
    /// `"s"#`: `Addr#`, a byte string.
    Str(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Tok {
    VarId(String),
    ConId(String),
    /// (module qualifier, name)
    QVarId(String, String),
    QConId(String, String),
    VarSym(String),
    ConSym(String),
    QVarSym(String, String),
    QConSym(String, String),
    /// Decimal digits (hex, octal and binary literals are converted).
    Int(String),
    /// Literal text with underscores removed, e.g. `1.5e-3`.
    Float(String),
    Char(char),
    Str(String),
    /// A MagicHash literal (`3#`, `3##`, `2.0#`, `'c'#`, `"s"#`), of a primitive type.
    Prim(PrimLit),
    Kw(Kw),
    Res(ROp),
    /// One of `( ) , ; [ ] ` { }`.
    Special(char),
    Eof,
}

impl Tok {
    /// Rendering used in "parse error on input" messages.
    pub fn describe(&self) -> String {
        match self {
            Tok::VarId(s) | Tok::ConId(s) | Tok::VarSym(s) | Tok::ConSym(s) => s.clone(),
            Tok::QVarId(q, s) | Tok::QConId(q, s) | Tok::QVarSym(q, s) | Tok::QConSym(q, s) => {
                format!("{q}.{s}")
            }
            Tok::Int(s) | Tok::Float(s) => s.clone(),
            Tok::Char(c) => format!("{c:?}"),
            Tok::Str(s) => format!("{s:?}"),
            Tok::Prim(p) => format!("{p:?}"),
            Tok::Kw(k) => k.text().to_string(),
            Tok::Res(r) => r.text().to_string(),
            Tok::Special(c) => c.to_string(),
            Tok::Eof => "end of input".to_string(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Token {
    pub tok: Tok,
    pub line: u32,
    pub col: u32,
    /// The position of the token's last character (its end, for source spans).
    pub end_line: u32,
    pub end_col: u32,
    /// True when this is the first token on its line (drives the layout rule).
    pub first_on_line: bool,
    /// Whitespace, or an opening bracket, precedes the token.
    pub space_before: bool,
    /// Whitespace, or a closing bracket, follows the token.
    pub space_after: bool,
}
