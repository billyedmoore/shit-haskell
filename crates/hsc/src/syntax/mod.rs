//! Front end: lexing, layout, parsing and the surface syntax tree.

pub mod ast;
pub mod cpp;
pub mod extensions;
pub mod lexer;
pub mod parser;
pub mod token;

use extensions::Extensions;

#[derive(Debug, Clone)]
pub struct SyntaxError {
    pub line: u32,
    pub col: u32,
    pub msg: String,
}

/// Parses one module. `flags` are compiler flags such as `-XLambdaCase`; pragmas
/// in the source are applied on top of them.
pub fn parse_module_source(src: &str, flags: &[String]) -> Result<(ast::Module, Extensions), SyntaxError> {
    let lexed = lexer::lex(src).map_err(|e| SyntaxError {
        line: e.line,
        col: e.col,
        msg: e.msg,
    })?;
    let mut exts = Extensions::ghc2021();
    for flag in flags {
        if let Some(x) = flag.strip_prefix("-X") {
            exts.apply(x).map_err(|msg| SyntaxError {
                line: 0,
                col: 0,
                msg,
            })?;
        }
    }
    for p in &lexed.pragmas {
        let items: Vec<String> = match p.name.as_str() {
            "LANGUAGE" => p.body.split(',').map(|s| s.trim().to_string()).collect(),
            "OPTIONS_GHC" => p
                .body
                .split_whitespace()
                .filter_map(|w| w.strip_prefix("-X").map(str::to_string))
                .collect(),
            _ => Vec::new(),
        };
        for item in items {
            exts.apply(&item).map_err(|msg| SyntaxError {
                line: p.line,
                col: 1,
                msg,
            })?;
        }
    }
    let mut tokens = lexed.tokens;
    if exts.is_on("TemplateHaskell") || exts.is_on("TemplateHaskellQuotes") {
        tokens = quote_brackets(tokens);
    }
    let module = parser::parse_module(tokens, exts.clone()).map_err(|e| SyntaxError {
        line: e.line,
        col: e.col,
        msg: e.msg,
    })?;
    Ok((module, exts))
}

/// With Template Haskell quotes on, `[|`, `[e|`, `[d|`, `[t|`, `[p|`, `[||` and `|]`, `||]`
/// (written without spaces) are brackets: the lexer saw `[`, a name and `|`, or `|` and `]`.
fn quote_brackets(toks: Vec<token::Token>) -> Vec<token::Token> {
    use token::{ROp, Tok};
    let adjacent = |a: &token::Token, b: &token::Token| a.end_line == b.line && b.col == a.end_col + 1;
    let mut out: Vec<token::Token> = Vec::with_capacity(toks.len());
    let mut i = 0;
    while i < toks.len() {
        let t = &toks[i];
        let next = toks.get(i + 1);
        let next2 = toks.get(i + 2);
        if t.tok == Tok::Special('[') {
            if let Some(n) = next.filter(|n| adjacent(t, n)) {
                let open = match &n.tok {
                    Tok::Res(ROp::Bar) => Some(('e', 2)),
                    Tok::VarSym(s) if s == "||" => Some(('T', 2)),
                    Tok::VarId(k) if ["e", "d", "t", "p"].contains(&k.as_str()) => match next2.filter(|m| adjacent(n, m)) {
                        Some(m) if m.tok == Tok::Res(ROp::Bar) => Some((k.chars().next().unwrap_or('e'), 3)),
                        Some(m) if k == "e" && matches!(&m.tok, Tok::VarSym(s) if s == "||") => Some(('T', 3)),
                        _ => None,
                    },
                    _ => None,
                };
                if let Some((kind, len)) = open {
                    let last = &toks[i + len - 1];
                    let mut tok = t.clone();
                    tok.tok = Tok::QuoteOpen(kind);
                    tok.end_line = last.end_line;
                    tok.end_col = last.end_col;
                    out.push(tok);
                    i += len;
                    continue;
                }
            }
        }
        let close = match &t.tok {
            Tok::Res(ROp::Bar) => Some(false),
            Tok::VarSym(s) if s == "||" => Some(true),
            _ => None,
        };
        if let (Some(typed), Some(n)) = (close, next) {
            if n.tok == Tok::Special(']') && adjacent(t, n) {
                let mut tok = t.clone();
                tok.tok = Tok::QuoteClose(typed);
                tok.end_col = n.end_col;
                out.push(tok);
                i += 2;
                continue;
            }
        }
        out.push(t.clone());
        i += 1;
    }
    out
}
