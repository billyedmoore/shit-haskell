//! Native code for output programs: lowering to STG, code generation to x86-64 assembly, and the
//! assembly that starts a program and carries its payload.
//!
//! An output program is an object file generated here, linked with the hsc runtime (the static
//! library of this crate). The object's `main` hands the embedded program payload (the serialised
//! Core) to `hsc_native_main`.

pub mod codegen;
pub mod stg;

use std::collections::{HashMap, HashSet};

use crate::core::{Alt, Core, TopBind};

/// The library bindings the runtime itself uses, by name: they are kept in every program that has
/// them, and the program's table lists where they are.
pub const RTS_GLOBALS: &[&str] = &[
    "GHC.Internal.Control.Exception.Base.nonTerminationException",
    "GHC.Internal.Exception.errorCallException",
    "GHC.Internal.IO.Exception.blockedIndefinitelyOnMVar",
    "GHC.Internal.IO.Exception.blockedIndefinitelyOnSTM",
    "GHC.Internal.TopHandler.flushStdHandles",
    "GHC.Internal.Conc.Signal.hscRunPendingSignals",
];

/// The constructors the runtime builds: each has an id in every program.
pub const RTS_CONS: &[&str] = &[
    "Builtin.()",
    "Builtin.(,)",
    "Builtin.(,,)",
    "Builtin.:",
    "Builtin.[]",
    "GHC.Internal.Exception.ErrorCall",
    "GHC.Internal.Maybe.Just",
    "GHC.Internal.Maybe.Nothing",
    "GHC.Internal.Real.:%",
    "GHC.Num.Integer.IN",
    "GHC.Num.Integer.IP",
    "GHC.Num.Integer.IS",
    "GHC.Types.False",
    "GHC.Types.True",
];

/// The global names an expression mentions.
pub(crate) fn mentions<'a>(e: &'a Core, out: &mut Vec<&'a str>) {
    match e {
        Core::Var(v) => out.push(v),
        Core::Lit(_) | Core::Fail(_) => {}
        Core::App(f, a) => {
            mentions(f, out);
            mentions(a, out);
        }
        Core::Lam(_, b) => mentions(b, out),
        Core::Let(binds, body) => {
            for (_, b) in binds {
                mentions(b, out);
            }
            mentions(body, out);
        }
        Core::Case(s, _, alts) => {
            mentions(s, out);
            for a in alts {
                match a {
                    Alt::Con(_, _, b) | Alt::Lit(_, b) | Alt::Default(b) => mentions(b, out),
                }
            }
        }
        Core::Con(_, args) | Core::Prim(_, args) => args.iter().for_each(|a| mentions(a, out)),
    }
}

/// The bindings a program needs: those reachable from its roots. The order is kept.
pub fn reachable(binds: Vec<TopBind>, roots: &[&str]) -> Vec<TopBind> {
    let index: HashMap<&str, usize> = binds.iter().enumerate().map(|(i, b)| (b.name.as_str(), i)).collect();
    let mut keep: HashSet<usize> = HashSet::new();
    let mut work: Vec<usize> = roots.iter().filter_map(|r| index.get(r).copied()).collect();
    while let Some(i) = work.pop() {
        if !keep.insert(i) {
            continue;
        }
        let mut names = Vec::new();
        mentions(&binds[i].body, &mut names);
        for n in names {
            // A local of the same name as a global does not occur: locals carry a `%N` suffix.
            if let Some(&j) = index.get(n) {
                if !keep.contains(&j) {
                    work.push(j);
                }
            }
        }
    }
    drop(index);
    binds.into_iter().enumerate().filter(|(i, _)| keep.contains(i)).map(|(_, b)| b).collect()
}

/// The assembly of a program's `main`, which passes the program's table to the runtime.
pub fn program_main() -> String {
    let mut s = String::new();
    s.push_str("    .text\n    .globl main\nmain:\n");
    s.push_str("    sub $8, %rsp\n");
    s.push_str("    lea hsc_table(%rip), %rdi\n");
    s.push_str("    call hsc_native_main\n");
    s.push_str("    add $8, %rsp\n    ret\n");
    s
}

