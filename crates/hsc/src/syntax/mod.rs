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
    let th_quotes = exts.is_on("TemplateHaskell") || exts.is_on("TemplateHaskellQuotes");
    // QuasiQuotes: `[q|text|]` is rewritten to an application the splice pass recognises, and
    // the source lexed again.
    let lexed = if exts.is_on("QuasiQuotes") {
        match rewrite_quasiquotes(src, th_quotes) {
            Some(rewritten) => lexer::lex(&rewritten).map_err(|e| SyntaxError { line: e.line, col: e.col, msg: e.msg })?,
            None => lexed,
        }
    } else {
        lexed
    };
    let mut tokens = lexed.tokens;
    if th_quotes {
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

/// Rewrites each quasi-quotation `[quoter|text|]` of a source to `(hsc'quasiquote quoter
/// "text")`, keeping the lines after it where they were. None when there is none. Strings,
/// character literals and comments are skipped; `[e|`, `[d|`, `[t|` and `[p|` are Template Haskell
/// quotes when those are on.
fn rewrite_quasiquotes(src: &str, th_quotes: bool) -> Option<String> {
    let chars: Vec<char> = src.chars().collect();
    let mut out = String::with_capacity(src.len());
    let mut changed = false;
    let mut i = 0;
    let ident = |c: char| c.is_alphanumeric() || c == '_' || c == '\'';
    let symbol = |c: char| "!#$%&*+./<=>?@\\^|-~:".contains(c);
    while i < chars.len() {
        let c = chars[i];
        // String literals.
        if c == '"' {
            out.push(c);
            i += 1;
            while i < chars.len() && chars[i] != '"' {
                if chars[i] == '\\' && i + 1 < chars.len() {
                    out.push(chars[i]);
                    i += 1;
                }
                out.push(chars[i]);
                i += 1;
            }
            if i < chars.len() {
                out.push(chars[i]);
                i += 1;
            }
            continue;
        }
        // Character literals (a quote after an identifier character is a prime).
        if c == '\'' && !(i > 0 && ident(chars[i - 1])) {
            let end = if chars.get(i + 1) == Some(&'\\') { chars[i + 2..].iter().position(|&x| x == '\'').map(|p| i + 2 + p) } else if chars.get(i + 2) == Some(&'\'') { Some(i + 2) } else { None };
            if let Some(e) = end.filter(|&e| e - i < 12) {
                out.extend(&chars[i..=e]);
                i = e + 1;
                continue;
            }
        }
        // Line comments.
        if c == '-' && chars.get(i + 1) == Some(&'-') && !(i > 0 && symbol(chars[i - 1])) {
            let mut k = i;
            while k < chars.len() && chars[k] == '-' {
                k += 1;
            }
            if k >= chars.len() || !symbol(chars[k]) {
                while i < chars.len() && chars[i] != '\n' {
                    out.push(chars[i]);
                    i += 1;
                }
                continue;
            }
        }
        // Block comments and pragmas.
        if c == '{' && chars.get(i + 1) == Some(&'-') {
            let mut depth = 0;
            while i < chars.len() {
                if chars[i] == '{' && chars.get(i + 1) == Some(&'-') {
                    depth += 1;
                    out.push_str("{-");
                    i += 2;
                } else if chars[i] == '-' && chars.get(i + 1) == Some(&'}') {
                    depth -= 1;
                    out.push_str("-}");
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    out.push(chars[i]);
                    i += 1;
                }
            }
            continue;
        }
        // A quasi-quotation: `[`, a (qualified) variable name, `|`, all without spaces.
        if c == '[' && !(i > 0 && ident(chars[i - 1])) {
            let mut k = i + 1;
            loop {
                let start = k;
                while k < chars.len() && ident(chars[k]) {
                    k += 1;
                }
                if k > start && chars.get(k) == Some(&'.') && chars[start].is_uppercase() {
                    k += 1;
                    continue;
                }
                break;
            }
            let quoter: String = chars[i + 1..k].iter().collect();
            let last = quoter.rsplit('.').next().unwrap_or("");
            let is_var = last.chars().next().is_some_and(|ch| ch.is_lowercase() || ch == '_');
            let th_quote = th_quotes && matches!(quoter.as_str(), "e" | "d" | "t" | "p");
            if is_var && !th_quote && chars.get(k) == Some(&'|') && chars.get(k + 1) != Some(&']') && chars.get(k + 1) != Some(&'|') {
                let body_start = k + 1;
                let mut e = body_start;
                while e + 1 < chars.len() && !(chars[e] == '|' && chars[e + 1] == ']') {
                    e += 1;
                }
                if e + 1 < chars.len() {
                    let body: String = chars[body_start..e].iter().collect();
                    let mut lit = String::from("\"");
                    for ch in body.chars() {
                        match ch {
                            '\\' => lit.push_str("\\\\"),
                            '"' => lit.push_str("\\\""),
                            '\n' => lit.push_str("\\n"),
                            '\t' => lit.push_str("\\t"),
                            '\r' => lit.push_str("\\r"),
                            ch => lit.push(ch),
                        }
                    }
                    lit.push('"');
                    out.push_str(&format!("(hsc'quasiquote {quoter} {lit})"));
                    // The lines the quotation spanned, so that what follows keeps its line.
                    for _ in body.chars().filter(|&ch| ch == '\n') {
                        out.push('\n');
                    }
                    i = e + 2;
                    changed = true;
                    continue;
                }
            }
        }
        out.push(c);
        i += 1;
    }
    changed.then_some(out)
}
