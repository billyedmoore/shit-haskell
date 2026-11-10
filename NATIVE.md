# Native back end and runtime (milestone 5): design

Status: the output path links generated objects with the runtime library (`libhsc.a`); the program is
still run by the interpreter inside the runtime. This file is the design of the native machine that
replaces the interpreter for output programs.

## Goals and the rule for each choice

- Output programs are machine code (x86-64, AT&T assembly text assembled by `gcc -c`, linked by gcc).
  Milestone 13 replaces gcc with our own encoder and linker.
- The machine follows the plan: STG-style, eval/apply, a Haskell stack separate from the C stack,
  tail calls as real jumps, a precise copying garbage collector, green threads, exceptions by
  stack unwinding.
- Core is untyped, so every runtime value is a uniformly boxed heap object (a pointer). Raw
  `Int#`/`Double#`/`Char#`/`Addr#` values are heap objects with a raw payload. This keeps the garbage
  collector precise without type information. Typed representations (unboxed values in registers and
  slots) are a later optimisation (milestone 8).

## Heap objects

All objects are 8-byte aligned. Word 0 is the header: `kind:8 | size:24 | aux:32` (size in words after
the header; aux is a kind-specific id).

| kind | words after header | layout |
|---|---|---|
| CON | n | aux = constructor id; n pointer fields |
| INT | 1 | raw i64 (also Char, with kind CHAR) |
| DOUBLE | 1 | raw f64 |
| INTEGER | k | sign + raw 64-bit limbs (bignum payload) |
| FUN | 1 + nfree | word 1 = code entry; aux = arity; pointer fields = free variables |
| PAP | 2 + n | word 1 = function object; aux = number of arguments held; pointer fields |
| THUNK | 1 + nfree | word 1 = code entry; pointer fields = free variables; min size 2 |
| IND | 1 | pointer to the value (a thunk after update) |
| BLACKHOLE | 1 | owner thread id (under evaluation) |
| REF (MutVar#) | 1 | pointer field |
| ARRAY | n | pointer fields (Array#, MutableArray#) |
| BYTES | k | raw bytes (ByteArray#, MutableByteArray#), length in aux |
| ADDR | 2 | pointer to a BYTES object, raw offset |
| MVAR | 2 | value pointer (or null when empty), queue pointer |
| TVAR | 2 | value pointer, version |
| WEAK / STABLE / THREAD | fixed | runtime-defined |

## Machine registers (generated code)

- `rbp`: Haskell stack pointer (the stack grows down; a chunk per thread, grown on demand).
- `r12`: heap allocation pointer; `r13`: heap limit.
- `rbx`: the machine record (`*mut Machine`: thread state, globals, stable pointer table).
- `r14`: R1, the value being returned or the closure being entered.
- The C stack is used only for calls into the runtime (Rust).

## Calling convention

- Arguments of a call are pushed on the Haskell stack by the caller, the first argument highest.
  The callee pops them. A tail call reuses the caller's frame.
- Return: a value in R1 and a jump to the continuation at the top of the stack. A continuation
  frame is `[return code address][saved live pointers...]`. Each return address has an info word
  before it, `.quad frame_size; .quad pointer_bitmap`, read by the collector.
- Update frame: `[update code][thunk pointer]`. Catch frame: `[catch code][handler][mask state]`.
- Eval of a pointer: if it is a constructor or function (WHNF), continue; if it is a thunk, push an
  update frame and enter the thunk's code; if it is an IND, follow it; if it is a BLACKHOLE of the
  current thread, raise `<<loop>>`.
- Apply of an unknown function to n arguments: the generic `stg_ap_n` routine checks the function's
  arity: exact entry, PAP building for fewer arguments, or applying the result to the rest (over
  saturation pushes an apply frame).

## Garbage collection

Copying (Cheney) collector over the heap; roots: the Haskell stacks, the machine record (globals,
stable pointers), and thread queues. Frames give their pointer slots through the info word. Objects
give their pointer fields through the header. Generational collection (a nursery and an old
generation with a write barrier on REF, ARRAY, MVAR, TVAR and thunk updates) comes after the first
working collector.

## Runtime services called from generated code (C ABI)

Allocation slow path, GC entry, blackhole/`<<loop>>` raising, primitive operations that do not
allocate in the generated code (called with the machine record), thread switching, `raise#`
unwinding, the FFI call stubs, and `hsc_native_main`. Primitive operations are implemented in Rust
over the raw object model (ported from the interpreter's `prims*.rs`, `ffi.rs`, `conc.rs`).

## Stages

1. Heap, objects, copying collector, machine record, with unit tests (crate `rts`, or the `interp::heap`
   module of the library).
2. STG lowering from Core (free variables, arities, let/letrec closures, case continuations).
3. Code generation to assembly for STG, with the generic apply and update routines in the emitted
   prelude. Hello world runs natively.
4. Port the primitive operations to the object model, family by family (the corpus decides order).
5. Exceptions (catch/raise/async), threads (green, scheduler on the machine record), MVar, STM
   (TVar with retry), IO through the FFI stubs, bignum (INTEGER objects), the stable pointer table.
6. Switch difftest to native output only (the interpreter stays for `--run` and, later, Template
   Haskell and GHCi); performance tracking against `ghc -O0`.
