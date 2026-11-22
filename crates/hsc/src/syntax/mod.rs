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

impl SyntaxError {
    pub fn render(&self, path: &std::path::Path, source: &str) -> String {
        crate::diagnostic::error(path, Some(source), ast::Loc { line: self.line, col: self.col }, &self.msg)
    }
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
    exts.tabs = lexed.tabs;
    for flag in flags {
        exts.warnings.apply(flag);
        if let Some(x) = flag.strip_prefix("-X") {
            exts.apply(x).map_err(|msg| SyntaxError {
                line: 0,
                col: 0,
                msg,
            })?;
        }
    }
    // Only the file header's pragmas (before the first token) name extensions, as in GHC: one later
    // in the file is ignored (base16's stray `{-# LANGUAGE lenientLoop #-}`).
    let header_end = lexed.tokens.first().map(|t| (t.line, t.col));
    for p in lexed.pragmas.iter().filter(|p| header_end.is_none_or(|end| (p.line, p.col) < end)) {
        let at = |loc: ast::Loc, msg| SyntaxError { line: loc.line, col: loc.col, msg };
        match p.name.as_str() {
            "LANGUAGE" => {
                let mut loc = p.body_loc;
                for part in p.body.split(',') {
                    let item = part.trim();
                    let start = loc.after(&part[..part.len() - part.trim_start().len()]);
                    if item.is_empty() {
                        return Err(at(start, "Expected an extension name in the LANGUAGE pragma".into()));
                    }
                    exts.apply(item).map_err(|msg| at(start, msg))?;
                    loc = loc.after(part).after(",");
                }
            }
            "OPTIONS_GHC" => {
                for flag in p.body.split_whitespace() {
                    exts.warnings.apply(flag);
                    if let Some(ext) = flag.strip_prefix("-X") {
                        exts.apply(ext).map_err(|msg| at(p.body_loc, msg))?;
                    }
                }
            }
            _ => {}
        }
    }
    // Names end in `#` only with MagicHash; Unicode symbols are reserved ones only with
    // UnicodeSyntax (the first lexing assumed MagicHash on and UnicodeSyntax off).
    let magic = exts.is_on("MagicHash");
    let unicode = exts.is_on("UnicodeSyntax");
    let binary = exts.is_on("BinaryLiterals");
    let foreign_interface = exts.is_on("ForeignFunctionInterface");
    let lexed = if (!magic && src.contains('#')) || (unicode && !src.is_ascii()) || (!binary && lexed.saw_binary)
        || (!foreign_interface && src.contains("foreign"))
    {
        lexer::lex_for_module(src, &exts).map_err(|e| SyntaxError { line: e.line, col: e.col, msg: e.msg })?
    } else {
        lexed
    };
    let th_quotes = exts.is_on("TemplateHaskell") || exts.is_on("TemplateHaskellQuotes");
    // QuasiQuotes: `[q|text|]` is rewritten to an application the splice pass recognises, and
    // the source lexed again.
    let lexed = if exts.is_on("QuasiQuotes") {
        match rewrite_quasiquotes(src, th_quotes) {
            Some(rewritten) => lexer::lex_for_module(&rewritten, &exts).map_err(|e| SyntaxError { line: e.line, col: e.col, msg: e.msg })?,
            None => lexed,
        }
    } else {
        lexed
    };
    if !exts.is_on("NumericUnderscores") {
        if let Some(loc) = lexed.numeric_separator {
            return Err(SyntaxError {
                line: loc.line,
                col: loc.col,
                msg: "Underscore separators in numeric literals require NumericUnderscores".into(),
            });
        }
    }
    let rule_pragmas: Vec<lexer::Pragma> = lexed.pragmas.iter().filter(|p| p.name == "RULES").cloned().collect();
    // Top-level INLINE and NOINLINE pragmas (those in class, instance and local bodies are
    // indented, and ignored).
    let inline_pragmas: Vec<lexer::Pragma> =
        lexed.pragmas.iter().filter(|p| (p.name == "INLINE" || p.name == "NOINLINE") && p.col == 1).cloned().collect();
    exts.unknown_pragmas = lexed.unknown_pragmas;
    let mut tokens = lexed.tokens;
    if th_quotes {
        tokens = quote_brackets(tokens);
    }
    let mut module = parser::parse_module(tokens, exts.clone()).map_err(|e| SyntaxError {
        line: e.line,
        col: e.col,
        msg: e.msg,
    })?;
    // Rewrite rules: each pragma's body is lexed on its own, at the pragma's lines.
    for p in rule_pragmas {
        let Ok(lexed) = lexer::lex_for_module(&p.body, &exts) else { continue };
        let tokens: Vec<token::Token> = lexed
            .tokens
            .into_iter()
            .map(|mut t| {
                t.line += p.line - 1;
                t.end_line += p.line - 1;
                t
            })
            .collect();
        module.rules.extend(parser::parse_rules(tokens, &exts));
    }
    for p in inline_pragmas {
        let Ok(lexed) = lexer::lex_for_module(&p.body, &exts) else { continue };
        let tokens: Vec<token::Token> = lexed.tokens.into_iter().map(|mut t| {
            if t.line == 1 {
                t.col += p.body_loc.col - 1;
            }
            if t.end_line == 1 {
                t.end_col += p.body_loc.col - 1;
            }
            t.line += p.body_loc.line - 1;
            t.end_line += p.body_loc.line - 1;
            t
        }).collect();
        module.inlines.extend(parser::parse_inline(&tokens, p.name == "INLINE"));
    }
    Ok((module, exts))
}

/// With Template Haskell quotes on, `[|`, `[e|`, `[d|`, `[t|`, `[p|`, `[||` and `|]`, `||]`
/// (written without spaces) are brackets: the lexer saw `[`, a name and `|`, or `|` and `]`.
fn quote_brackets(toks: Vec<token::Token>) -> Vec<token::Token> {
    use token::{ROp, Tok};
    let adjacent = |a: &token::Token, b: &token::Token| a.end_line == b.line && b.col == a.end_col + 1;
    let mut out: Vec<token::Token> = Vec::with_capacity(toks.len());
    let mut i = 0;
    let sym_tok = |s: &str| -> Tok {
        match s {
            ".." => Tok::Res(ROp::DotDot),
            "::" => Tok::Res(ROp::DColon),
            "=" => Tok::Res(ROp::Eq),
            "\\" => Tok::Res(ROp::Backslash),
            "|" => Tok::Res(ROp::Bar),
            "<-" => Tok::Res(ROp::LArrow),
            "->" => Tok::Res(ROp::RArrow),
            "@" => Tok::Res(ROp::At),
            "~" => Tok::Res(ROp::Tilde),
            "=>" => Tok::Res(ROp::DArrow),
            _ if s.starts_with(':') => Tok::ConSym(s.to_string()),
            _ => Tok::VarSym(s.to_string()),
        }
    };
    let split_bar = |bar: &token::Token, allow_typed: bool| -> Option<(bool, u32, Option<token::Token>)> {
        match &bar.tok {
            Tok::Res(ROp::Bar) => Some((false, 1, None)),
            Tok::VarSym(s) if allow_typed && s.starts_with("||") => {
                let rest = &s[2..];
                let tail = (!rest.is_empty()).then(|| {
                    let mut r = bar.clone();
                    r.tok = sym_tok(rest);
                    r.col += 2;
                    r.space_before = false;
                    r
                });
                Some((true, 2, tail))
            }
            Tok::VarSym(s) if s.starts_with('|') => {
                let rest = &s[1..];
                let tail = (!rest.is_empty()).then(|| {
                    let mut r = bar.clone();
                    r.tok = sym_tok(rest);
                    r.col += 1;
                    r.space_before = false;
                    r
                });
                Some((false, 1, tail))
            }
            _ => None,
        }
    };
    while i < toks.len() {
        let t = &toks[i];
        let next = toks.get(i + 1);
        let next2 = toks.get(i + 2);
        if t.tok == Tok::Special('[') {
            if let Some(n) = next.filter(|n| adjacent(t, n)) {
                let open = match &n.tok {
                    Tok::VarId(k) if ["e", "d", "t", "p"].contains(&k.as_str()) => {
                        next2.filter(|m| adjacent(n, m)).and_then(|m| {
                            let (typed, bar_len, tail) = split_bar(m, k == "e")?;
                            let kind = if typed { 'T' } else { k.chars().next().unwrap_or('e') };
                            Some((kind, 3usize, m.end_line, m.col + bar_len - 1, tail))
                        })
                    }
                    _ => split_bar(n, true).map(|(typed, bar_len, tail)| {
                        let kind = if typed { 'T' } else { 'e' };
                        (kind, 2usize, n.end_line, n.col + bar_len - 1, tail)
                    }),
                };
                if let Some((kind, len, end_line, end_col, tail)) = open {
                    let mut tok = t.clone();
                    tok.tok = Tok::QuoteOpen(kind);
                    tok.end_line = end_line;
                    tok.end_col = end_col;
                    tok.space_after = if tail.is_some() { false } else { toks[i + len - 1].space_after };
                    out.push(tok);
                    if let Some(r) = tail {
                        out.push(r);
                    }
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

#[cfg(test)]
mod rule_tests {
    use super::ast::Activation;

    #[test]
    fn base_style_rules_parse_with_activations_and_binder_signatures() {
        let src = r#"module M where
{-# RULES
"fold/build"    forall k z (g::forall b. (a->b->b) -> b -> b) .
                foldr k z (build g) = g k z
"map"       [~1] forall f xs.   map f xs                = build (\c n -> foldr (mapFB c f) n xs)
"mapList"   [1]  forall f.      foldr (mapFB (:) f) []  = map f
"foldr/id"                        foldr (:) [] = \x  -> x
  #-}
{-# RULES "a" forall x. f x = x ; "b" [~] g = h #-}
f x = x
"#;
        let (m, _) = super::parse_module_source(src, &[]).expect("parses");
        let names: Vec<&str> = m.rules.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["fold/build", "map", "mapList", "foldr/id", "a", "b"]);
        let acts: Vec<Activation> = m.rules.iter().map(|r| r.activation).collect();
        assert_eq!(
            acts,
            [Activation::Always, Activation::Before(1), Activation::From(1), Activation::Always, Activation::Always, Activation::Never]
        );
        let fb = &m.rules[0];
        assert_eq!(fb.binders.iter().map(|(b, _)| b.as_str()).collect::<Vec<_>>(), ["k", "z", "g"]);
        assert!(fb.binders[2].1.is_some(), "g's rank-2 signature is kept");
        assert!(m.rules[3].binders.is_empty());
        assert_eq!(fb.loc.line, 3);
    }
}
