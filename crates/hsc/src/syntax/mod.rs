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
    let module = parser::parse_module(lexed.tokens, exts.clone()).map_err(|e| SyntaxError {
        line: e.line,
        col: e.col,
        msg: e.msg,
    })?;
    Ok((module, exts))
}
