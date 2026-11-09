//! Human-readable rendering of Core, for debugging and `--dump-core`.

use super::{Alt, Core, Lit};
use std::fmt::Write;

pub fn show_lit(l: &Lit) -> String {
    match l {
        Lit::Integer(s) | Lit::Frac(s) => s.clone(),
        Lit::Int(i) => format!("{i}#"),
        Lit::Double(d) => format!("{d}##"),
        Lit::Char(c) => format!("{c:?}"),
        Lit::Str(s) => format!("{s:?}"),
    }
}

pub fn show(c: &Core) -> String {
    let mut out = String::new();
    go(c, 0, &mut out);
    out
}

fn indent(n: usize, out: &mut String) {
    for _ in 0..n {
        out.push_str("  ");
    }
}

fn go(c: &Core, depth: usize, out: &mut String) {
    match c {
        Core::Var(v) => out.push_str(v),
        Core::Lit(l) => out.push_str(&show_lit(l)),
        Core::App(f, a) => {
            out.push('(');
            go(f, depth, out);
            out.push(' ');
            go(a, depth, out);
            out.push(')');
        }
        Core::Lam(v, body) => {
            let _ = write!(out, "(\\{v} -> ");
            go(body, depth, out);
            out.push(')');
        }
        Core::Let(binds, body) => {
            out.push_str("let\n");
            for (n, b) in binds {
                indent(depth + 1, out);
                let _ = write!(out, "{n} = ");
                go(b, depth + 2, out);
                out.push('\n');
            }
            indent(depth, out);
            out.push_str("in ");
            go(body, depth, out);
        }
        Core::Case(scrut, b, alts) => {
            out.push_str("case ");
            go(scrut, depth, out);
            let _ = write!(out, " as {b} of");
            for a in alts {
                out.push('\n');
                indent(depth + 1, out);
                match a {
                    Alt::Con(c, vs, body) => {
                        let _ = write!(out, "{c}");
                        for v in vs {
                            let _ = write!(out, " {v}");
                        }
                        out.push_str(" -> ");
                        go(body, depth + 2, out);
                    }
                    Alt::Lit(l, body) => {
                        let _ = write!(out, "{} -> ", show_lit(l));
                        go(body, depth + 2, out);
                    }
                    Alt::Default(body) => {
                        out.push_str("_ -> ");
                        go(body, depth + 2, out);
                    }
                }
            }
        }
        Core::Con(c, args) => {
            let _ = write!(out, "[{c}");
            for a in args {
                out.push(' ');
                go(a, depth, out);
            }
            out.push(']');
        }
        Core::Prim(p, args) => {
            let _ = write!(out, "<{p}");
            for a in args {
                out.push(' ');
                go(a, depth, out);
            }
            out.push('>');
        }
        Core::Fail(msg) => {
            let _ = write!(out, "FAIL({msg:?})");
        }
    }
}
