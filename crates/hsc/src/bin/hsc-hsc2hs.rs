//! hsc-hsc2hs: the hsc2hs of hsc, as a command (`hsc-hsc2hs [-I dir] [-D x] [-o out.hs] file.hsc`), for
//! build tools that run hsc2hs themselves.

use std::process::ExitCode;

fn main() -> ExitCode {
    // `@file` arguments are response files (cabal's: arguments separated by white space, which a
    // backslash escapes).
    let mut args: Vec<String> = Vec::new();
    for a in std::env::args().skip(1) {
        match a.strip_prefix('@').and_then(|f| std::fs::read_to_string(f).ok()) {
            Some(text) => args.extend(response_args(&text)),
            None => args.push(a),
        }
    }
    let mut flags = Vec::new();
    let mut out: Option<String> = None;
    let mut input: Option<String> = None;
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        if a == "-o" {
            out = it.next();
        } else if let Some(o) = a.strip_prefix("--output=") {
            out = Some(o.to_string());
        } else if a == "-I" || a == "-D" {
            if let Some(v) = it.next() {
                flags.push(format!("{a}{v}"));
            }
        } else if a.starts_with("-I") || a.starts_with("-D") {
            flags.push(a);
        } else if let Some(f) = a.strip_prefix("--cflag=") {
            if f.starts_with("-I") || f.starts_with("-D") {
                flags.push(f.to_string());
            } else {
                // Other C flags (`-include file`) go to the C compiler as they are.
                flags.push(format!("{}{f}", hsc::hsc2hs::CFLAG_PREFIX));
            }
        } else if a == "--version" || a == "-V" {
            println!("hsc2hs version 0.68.10");
            return ExitCode::SUCCESS;
        } else if a.starts_with('-') {
        } else {
            input = Some(a);
        }
    }
    let Some(input) = input else {
        eprintln!("hsc-hsc2hs: no input file");
        return ExitCode::from(1);
    };
    match hsc::hsc2hs::translate(std::path::Path::new(&input), &flags) {
        Ok(generated) => {
            let out = out.unwrap_or_else(|| input.trim_end_matches(".hsc").to_string() + ".hs");
            match std::fs::copy(&generated, &out) {
                Ok(_) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("hsc-hsc2hs: {out}: {e}");
                    ExitCode::from(1)
                }
            }
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::from(1)
        }
    }
}

/// The arguments of a response file.
fn response_args(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut any = false;
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                if let Some(n) = chars.next() {
                    cur.push(n);
                    any = true;
                }
            }
            c if c.is_whitespace() => {
                if any {
                    out.push(std::mem::take(&mut cur));
                    any = false;
                }
            }
            c => {
                cur.push(c);
                any = true;
            }
        }
    }
    if any {
        out.push(cur);
    }
    out
}
