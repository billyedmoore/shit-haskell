//! Native code for output programs: the assembly that starts a program and carries its payload.
//!
//! An output program is an object file generated here, linked with the hsc runtime (the static
//! library of this crate). The object's `main` hands the embedded program payload (the serialised
//! Core, which the runtime compiles into its code) to `hsc_native_main`.

/// The assembly for a program whose payload is the file at `payload_path`.
pub fn program_assembly(payload_path: &str) -> String {
    let mut s = String::new();
    s.push_str("    .text\n    .globl main\nmain:\n");
    s.push_str("    sub $8, %rsp\n");
    s.push_str("    lea hsc_payload(%rip), %rdi\n");
    s.push_str("    mov hsc_payload_len(%rip), %rsi\n");
    s.push_str("    call hsc_native_main\n");
    s.push_str("    add $8, %rsp\n    ret\n");
    s.push_str("    .section .rodata\n    .p2align 4\n    .globl hsc_payload\nhsc_payload:\n");
    s.push_str(&format!("    .incbin \"{payload_path}\"\nhsc_payload_end:\n"));
    s.push_str("    .p2align 3\nhsc_payload_len:\n    .quad hsc_payload_end - hsc_payload\n");
    s.push_str("    .section .note.GNU-stack,\"\",@progbits\n");
    s
}

/// The runtime's entry for a native program: runs the program in the payload at `payload`.
#[no_mangle]
pub extern "C" fn hsc_native_main(payload: *const u8, len: usize) -> i32 {
    // SAFETY: the object's `main` passes the address and length of its embedded payload, which
    // lives in the executable for the whole run.
    let bytes = unsafe { std::slice::from_raw_parts(payload, len) }.to_vec();
    crate::driver::run_payload_bytes(bytes)
}
