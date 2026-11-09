//! hsc: a Haskell compiler compatible with GHC 9.12.
//!
//! Development entry point: `hsc [-X<ext>]... [--dump-ast] <file.hs>` parses one
//! module and reports syntax errors in GHC's `file:line:col` format.

mod syntax;

use std::process::ExitCode;

fn main() -> ExitCode {
    let mut flags = Vec::new();
    let mut dump = false;
    let mut files = Vec::new();
    for arg in std::env::args().skip(1) {
        if arg == "--dump-ast" {
            dump = true;
        } else if arg.starts_with('-') {
            flags.push(arg);
        } else {
            files.push(arg);
        }
    }
    let Some(file) = files.first() else {
        eprintln!("hsc: no input files");
        return ExitCode::from(1);
    };
    let src = match std::fs::read_to_string(file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("hsc: {file}: {e}");
            return ExitCode::from(1);
        }
    };
    match syntax::parse_module_source(&src, &flags) {
        Ok(module) => {
            if dump {
                println!("{module:#?}");
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{file}:{}:{}: error:\n    {}", e.line, e.col, e.msg);
            ExitCode::from(1)
        }
    }
}
