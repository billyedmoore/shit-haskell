//! Native code for output programs: lowering to STG, code generation to x86-64 assembly, and the
//! assembly that starts a program and carries its payload.
//!
//! An output program is an object file generated here, linked with the hsc runtime (the static
//! library of this crate). The object's `main` hands the embedded program payload (the serialised
//! Core) to `hsc_native_main`.

pub mod codegen;
pub mod stg;

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

