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
//! arguments go on the stack, after the registers' (none of the floating-point ones may then
//! spill).

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
pub fn parse(name: &str) -> Option<(&str, char, &str)> {
    let rest = name.strip_prefix("ccall:")?;
    let (sig, sym) = rest.split_once(':')?;
    let (args, ret) = sig.split_once('>')?;
    Some((args, ret.chars().next().unwrap_or('v'), sym))
}

/// The address of a C symbol in the running program, if it has one.
pub fn lookup(sym: &str) -> Option<*mut c_void> {
    let c = CString::new(sym).ok()?;
    // SAFETY: dlsym reads the NUL-terminated name.
    let p = unsafe { dlsym(RTLD_DEFAULT, c.as_ptr()) };
    (!p.is_null()).then_some(p)
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

type IntFn = unsafe extern "C" fn(u64, u64, u64, u64, u64, u64, f64, f64, f64, f64, f64, f64, f64, f64, u64, u64, u64, u64, u64, u64) -> u64;
type DoubleFn = unsafe extern "C" fn(u64, u64, u64, u64, u64, u64, f64, f64, f64, f64, f64, f64, f64, f64, u64, u64, u64, u64, u64, u64) -> f64;
type FloatFn = unsafe extern "C" fn(u64, u64, u64, u64, u64, u64, f64, f64, f64, f64, f64, f64, f64, f64, u64, u64, u64, u64, u64, u64) -> f32;

/// Calls the C function at `f` with integer and floating-point arguments (a float is given as the
/// bits of an f32 in the low half), and answers as its result letter says.
///
/// # Safety
/// `f` must be a C function whose parameters are the given arguments, in their classes and order,
/// and whose result is of the kind `ret` names.
pub unsafe fn call(f: *mut c_void, ints: &[u64], floats: &[f64], ret: char) -> Answer {
    let mut i = [0u64; 12];
    for (k, v) in ints.iter().take(12).enumerate() {
        i[k] = *v;
    }
    let mut d = [0f64; 8];
    for (k, v) in floats.iter().take(8).enumerate() {
        d[k] = *v;
    }
    match ret {
        'd' => {
            let g: DoubleFn = std::mem::transmute(f);
            Answer::Double(g(i[0], i[1], i[2], i[3], i[4], i[5], d[0], d[1], d[2], d[3], d[4], d[5], d[6], d[7], i[6], i[7], i[8], i[9], i[10], i[11]))
        }
        'f' => {
            let g: FloatFn = std::mem::transmute(f);
            Answer::Double(g(i[0], i[1], i[2], i[3], i[4], i[5], d[0], d[1], d[2], d[3], d[4], d[5], d[6], d[7], i[6], i[7], i[8], i[9], i[10], i[11]) as f64)
        }
        _ => {
            let g: IntFn = std::mem::transmute(f);
            let r = g(i[0], i[1], i[2], i[3], i[4], i[5], d[0], d[1], d[2], d[3], d[4], d[5], d[6], d[7], i[6], i[7], i[8], i[9], i[10], i[11]);
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
