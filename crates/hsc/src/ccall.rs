//! Calls of C functions by name, for `foreign import ccall` declarations that the runtimes do not
//! implement themselves. Both back ends use it.
//!
//! A foreign import's primitive is named `ccall:<args>><result>:<symbol>` (see `tc::decls`): one
//! letter per argument, `i` for an integer or pointer and `d`/`f` for a double/float, and the
//! result's letter, which also gives the width of an integer result: `b`/`B` 8 bits signed/unsigned,
//! `h`/`H` 16, `w`/`W` 32, `i` 64, `p` a pointer, `d`, `f`, and `v` for none.
//!
//! The call goes through one function type with six integer and eight floating-point parameters:
//! under the System V ABI an integer argument goes in the next integer register and a
//! floating-point one in the next vector register, whatever their order, so every C function of up
//! to six integer and eight floating-point arguments is called correctly this way. More integer
//! arguments (up to twenty in all) go on the stack, after the registers' (none of the
//! floating-point ones may then spill).

use std::ffi::{c_void, CString};

extern "C" {
    fn dlsym(handle: *mut c_void, symbol: *const std::ffi::c_char) -> *mut c_void;
    fn dlopen(file: *const std::ffi::c_char, mode: i32) -> *mut c_void;
    fn dlerror() -> *const std::ffi::c_char;
}

/// `RTLD_DEFAULT`: the global symbols of the program and the libraries it has loaded.
const RTLD_DEFAULT: *mut c_void = std::ptr::null_mut();
const RTLD_NOW: i32 = 2;
const RTLD_GLOBAL: i32 = 0x100;

/// The parts of a `ccall:` primitive name: argument letters, result letter, symbol.
/// Whether a C call is safe (`!` before its name): other threads may run while it is in C.
pub fn is_safe(name: &str) -> bool {
    name.starts_with('!')
}

pub fn parse(name: &str) -> Option<(&str, char, &str)> {
    if let Some(c) = parse_capi(name) {
        return Some((c.args, c.ret, c.symbol));
    }
    let rest = name.trim_start_matches('!').strip_prefix("ccall:")?;
    let (sig, sym) = rest.split_once(':')?;
    let (args, ret) = sig.split_once('>')?;
    Some((args, ret.chars().next().unwrap_or('v'), sym))
}

/// A `capi` import: `capi:<args>><result>:<symbol>:<header>:<value?>`. It is called as the C function
/// of that name if there is one, else through the wrapper `hsc_capi_<symbol>` (see `capi_stubs`);
/// a `value` import always through the wrapper, which answers the value.
pub struct Capi<'a> {
    pub args: &'a str,
    pub ret: char,
    pub symbol: &'a str,
    pub header: &'a str,
    pub value: bool,
}

pub fn parse_capi(name: &str) -> Option<Capi<'_>> {
    let rest = name.trim_start_matches('!').strip_prefix("capi:")?;
    let mut it = rest.splitn(4, ':');
    let sig = it.next()?;
    let symbol = it.next()?;
    let header = it.next().unwrap_or("");
    let value = it.next().is_some_and(|v| v == "value");
    let (args, ret) = sig.split_once('>')?;
    Some(Capi { args, ret: ret.chars().next().unwrap_or('v'), symbol, header, value })
}

/// The C function a foreign primitive calls: a capi `value` import's wrapper, else the symbol (or,
/// for a capi import of a macro, its wrapper).
/// The address an address import (`foreign import ccall "&sym"`) names, if `prim` is one.
pub fn address_import(prim: &str) -> Option<u64> {
    let (_, _, sym) = parse(prim)?;
    let sym = sym.strip_prefix('&')?;
    lookup(sym.trim()).map(|p| p as u64)
}

pub fn resolve(prim: &str) -> Option<*mut c_void> {
    if let Some(c) = parse_capi(prim) {
        if c.value {
            let w = CString::new(capi_wrapper(c.symbol)).ok()?;
            // SAFETY: dlsym reads the NUL-terminated name.
            let p = unsafe { dlsym(RTLD_DEFAULT, w.as_ptr()) };
            return (!p.is_null()).then_some(p);
        }
        return lookup(c.symbol);
    }
    let (_, _, sym) = parse(prim)?;
    lookup(sym)
}

/// The wrapper's name of a capi import.
pub fn capi_wrapper(symbol: &str) -> String {
    format!("hsc_capi_{symbol}")
}

/// The C type of a class letter, for the wrappers.
fn c_type(class: char) -> &'static str {
    match class {
        'b' => "int8_t",
        'B' => "uint8_t",
        'h' => "int16_t",
        'H' => "uint16_t",
        'w' => "int32_t",
        'W' => "uint32_t",
        'p' => "void *",
        'd' => "double",
        'f' => "float",
        'v' => "void",
        _ => "int64_t",
    }
}

/// The C source of the wrappers of a program's capi imports (by their primitive names), or None
/// when there are none.
pub fn capi_stubs(prims: &[String]) -> Option<String> {
    let mut headers: Vec<&str> = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    let mut defs = String::new();
    for p in prims {
        let Some(c) = parse_capi(p) else { continue };
        if seen.contains(&c.symbol) {
            continue;
        }
        seen.push(c.symbol);
        if !c.header.is_empty() && !headers.contains(&c.header) {
            headers.push(c.header);
        }
        let params: Vec<String> = c.args.chars().enumerate().map(|(i, a)| format!("{} a{i}", c_type(a))).collect();
        let call_args: Vec<String> = (0..c.args.len()).map(|i| format!("a{i}")).collect();
        let params = if params.is_empty() { "void".to_string() } else { params.join(", ") };
        let expr = if c.value { c.symbol.to_string() } else { format!("{}({})", c.symbol, call_args.join(", ")) };
        let body = if c.ret == 'v' { format!("{expr};") } else { format!("return ({})({expr});", c_type(c.ret)) };
        defs.push_str(&format!("{} {}({params}) {{ {body} }}\n", c_type(c.ret), capi_wrapper(c.symbol)));
    }
    if seen.is_empty() {
        return None;
    }
    let mut out = String::from("#define _GNU_SOURCE 1\n#include <stdint.h>\n");
    for h in headers {
        out.push_str(&format!("#include <{h}>\n"));
    }
    out.push_str(&defs);
    Some(out)
}

/// The address of a C symbol in the running program, if it has one.
pub fn lookup(sym: &str) -> Option<*mut c_void> {
    // The RTS's own C functions, defined here.
    match sym {
        "getProgArgv" => return Some(get_prog_argv as *mut c_void),
        "setProgArgv" => return Some(set_prog_argv as *mut c_void),
        "getMonotonicNSec" => return Some(get_monotonic_nsec as *mut c_void),
        _ => {}
    }
    let find = |s: &str| -> Option<*mut c_void> {
        let c = CString::new(s).ok()?;
        // SAFETY: dlsym reads the NUL-terminated name.
        let p = unsafe { dlsym(RTLD_DEFAULT, c.as_ptr()) };
        (!p.is_null()).then_some(p)
    };
    find(sym).or_else(|| find(&capi_wrapper(sym)))
}

/// Loads a shared library for the symbols of a package's C code.
pub fn load(path: &str) -> Result<(), String> {
    let c = CString::new(path).map_err(|e| e.to_string())?;
    // SAFETY: dlopen reads the NUL-terminated path.
    let h = unsafe { dlopen(c.as_ptr(), RTLD_NOW | RTLD_GLOBAL) };
    if h.is_null() {
        // SAFETY: dlerror answers a NUL-terminated message after a failed dlopen.
        let msg = unsafe { std::ffi::CStr::from_ptr(dlerror()) }.to_string_lossy().into_owned();
        return Err(msg);
    }
    Ok(())
}

/// What a call answers: an integer register (already extended to the result's width), or a
/// floating-point one.
pub enum Answer {
    Int(i64),
    Double(f64),
}

type IntFn = unsafe extern "C" fn(u64, u64, u64, u64, u64, u64, f64, f64, f64, f64, f64, f64, f64, f64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64) -> u64;
type DoubleFn = unsafe extern "C" fn(u64, u64, u64, u64, u64, u64, f64, f64, f64, f64, f64, f64, f64, f64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64) -> f64;
type FloatFn = unsafe extern "C" fn(u64, u64, u64, u64, u64, u64, f64, f64, f64, f64, f64, f64, f64, f64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64) -> f32;

/// Calls the C function at `f` with integer and floating-point arguments (a float is given as the
/// bits of an f32 in the low half), and answers as its result letter says.
///
/// # Safety
/// `f` must be a C function whose parameters are the given arguments, in their classes and order,
/// and whose result is of the kind `ret` names.
pub unsafe fn call(f: *mut c_void, ints: &[u64], floats: &[f64], ret: char) -> Answer {
    let mut i = [0u64; 20];
    for (k, v) in ints.iter().take(20).enumerate() {
        i[k] = *v;
    }
    let mut d = [0f64; 8];
    for (k, v) in floats.iter().take(8).enumerate() {
        d[k] = *v;
    }
    match ret {
        'd' => {
            let g: DoubleFn = std::mem::transmute(f);
            Answer::Double(g(i[0], i[1], i[2], i[3], i[4], i[5], d[0], d[1], d[2], d[3], d[4], d[5], d[6], d[7], i[6], i[7], i[8], i[9], i[10], i[11], i[12], i[13], i[14], i[15], i[16], i[17], i[18], i[19]))
        }
        'f' => {
            let g: FloatFn = std::mem::transmute(f);
            Answer::Double(g(i[0], i[1], i[2], i[3], i[4], i[5], d[0], d[1], d[2], d[3], d[4], d[5], d[6], d[7], i[6], i[7], i[8], i[9], i[10], i[11], i[12], i[13], i[14], i[15], i[16], i[17], i[18], i[19]) as f64)
        }
        _ => {
            let g: IntFn = std::mem::transmute(f);
            let r = g(i[0], i[1], i[2], i[3], i[4], i[5], d[0], d[1], d[2], d[3], d[4], d[5], d[6], d[7], i[6], i[7], i[8], i[9], i[10], i[11], i[12], i[13], i[14], i[15], i[16], i[17], i[18], i[19]);
            Answer::Int(match ret {
                'b' => r as i8 as i64,
                'B' => r as u8 as i64,
                'h' => r as i16 as i64,
                'H' => r as u16 as i64,
                'w' => r as i32 as i64,
                'W' => r as u32 as i64,
                'v' => 0,
                _ => r as i64,
            })
        }
    }
}

/// A float argument as the bits of an f32 in the low half of a vector register.
pub fn float_arg(x: f64) -> f64 {
    f64::from_bits((x as f32).to_bits() as u64)
}

/// The program's `argv` as C strings (with a terminating null pointer), as the RTS keeps it.
struct ProgArgv {
    _strings: Vec<CString>,
    ptrs: Vec<*mut std::ffi::c_char>,
}

// SAFETY: the pointers point into the CStrings the same value owns.
unsafe impl Send for ProgArgv {}

static PROG_ARGV: std::sync::Mutex<Option<ProgArgv>> = std::sync::Mutex::new(None);

fn make_argv(args: Vec<Vec<u8>>) -> ProgArgv {
    let strings: Vec<CString> = args
        .into_iter()
        .map(|a| CString::new(a.into_iter().filter(|&b| b != 0).collect::<Vec<u8>>()).unwrap_or_default())
        .collect();
    let mut ptrs: Vec<*mut std::ffi::c_char> = strings.iter().map(|c| c.as_ptr() as *mut _).collect();
    ptrs.push(std::ptr::null_mut());
    ProgArgv { _strings: strings, ptrs }
}

/// Sets the program's arguments (its name first), for a program the interpreter runs.
pub fn set_program_args(args: Vec<String>) {
    *PROG_ARGV.lock().unwrap() = Some(make_argv(args.into_iter().map(String::into_bytes).collect()));
}

/// `getProgArgv(int *argc, char ***argv)`: the process's arguments unless they were set.
extern "C" fn get_prog_argv(argc: *mut i32, argv: *mut *mut *mut std::ffi::c_char) {
    use std::os::unix::ffi::OsStringExt;
    let mut g = PROG_ARGV.lock().unwrap();
    let a = g.get_or_insert_with(|| make_argv(std::env::args_os().map(|a| a.into_vec()).collect()));
    // SAFETY: the caller passes writable locations (or null).
    unsafe {
        if !argc.is_null() {
            *argc = (a.ptrs.len() - 1) as i32;
        }
        if !argv.is_null() {
            *argv = a.ptrs.as_mut_ptr();
        }
    }
}

/// `setProgArgv(int argc, char *argv[])`: copies the arguments.
extern "C" fn set_prog_argv(argc: i32, argv: *const *const std::ffi::c_char) {
    let mut args = Vec::new();
    for i in 0..argc.max(0) as usize {
        // SAFETY: the caller passes `argc` NUL-terminated strings.
        let p = unsafe { *argv.add(i) };
        if p.is_null() {
            break;
        }
        args.push(unsafe { std::ffi::CStr::from_ptr(p) }.to_bytes().to_vec());
    }
    *PROG_ARGV.lock().unwrap() = Some(make_argv(args));
}

#[repr(C)]
struct Timespec {
    sec: i64,
    nsec: i64,
}

extern "C" {
    fn clock_gettime(clock: i32, ts: *mut Timespec) -> i32;
}

/// `getMonotonicNSec()`: the monotonic clock, in nanoseconds (the RTS's, for GHC.Clock).
extern "C" fn get_monotonic_nsec() -> u64 {
    const CLOCK_MONOTONIC: i32 = 1;
    let mut ts = Timespec { sec: 0, nsec: 0 };
    // SAFETY: one timespec to fill.
    unsafe { clock_gettime(CLOCK_MONOTONIC, &mut ts) };
    (ts.sec as u64) * 1_000_000_000 + ts.nsec as u64
}
