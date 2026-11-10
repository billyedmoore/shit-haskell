//! The C entry points that base imports with `foreign import ccall`. Each is a call into the C
//! library that std already links (no crates). Pointer arguments arrive as `Ptr` constructors around
//! an address (a shared byte buffer and an offset); a buffer's bytes are passed to the C call by
//! address, which is valid while the buffer is borrowed for that call.

use std::cell::RefCell;
use std::rc::Rc;

use super::prims::io_result_of;
use super::{unit_value, Cell, RR, Runtime, RtError, Value};

extern "C" {
    fn isatty(fd: i32) -> i32;
    fn write(fd: i32, buf: *const u8, n: usize) -> isize;
    fn read(fd: i32, buf: *mut u8, n: usize) -> isize;
    fn close(fd: i32) -> i32;
    fn lseek(fd: i32, off: i64, whence: i32) -> i64;
    fn getpid() -> i32;
    fn getppid() -> i32;
    fn __errno_location() -> *mut i32;
    fn poll(fds: *mut PollFd, nfds: u64, timeout: i32) -> i32;
}

/// The descriptor record `poll` takes.
#[repr(C)]
struct PollFd {
    fd: i32,
    events: i16,
    revents: i16,
}

const POLLIN: i16 = 1;

type Buf = Rc<RefCell<Vec<u8>>>;

/// The address behind a pointer argument: its buffer and offset.
fn addr_of(rt: &Runtime, c: &Cell) -> Result<(Buf, usize), RtError> {
    let v = rt.force(c)?;
    let inner = match v {
        Value::Con(con) if con.name == "GHC.Internal.Ptr.Ptr" || con.name == "GHC.Internal.Ptr.FunPtr" => {
            let field = con
                .args
                .first()
                .cloned()
                .ok_or_else(|| RtError::Fail("internal: a pointer without an address".into()))?;
            rt.force(&field)?
        }
        other => other,
    };
    match inner {
        Value::Addr(b, o) => Ok((b, o)),
        _ => Err(RtError::Fail("internal: expected a pointer argument".into())),
    }
}

/// An integer argument: a plain Int, or a sized box (`I32#` and the like) around one.
fn int_of(rt: &Runtime, c: &Cell) -> Result<i64, RtError> {
    match rt.force(c)? {
        Value::Con(con) if con.args.len() == 1 => int_of(rt, &con.args[0].clone()),
        _ => super::prims::int_of(rt, c),
    }
}

fn ret_int(n: i64) -> RR {
    Ok(io_result_of(Value::Int(n)))
}

fn errno() -> i64 {
    // SAFETY: __errno_location returns the calling thread's errno, valid for reading.
    unsafe { *__errno_location() as i64 }
}

/// Runs a C entry point by its symbol, or answers None when it is not one of them.
pub fn call(rt: &Runtime, name: &str, args: &[Cell]) -> Option<RR> {
    let r: RR = match name {
        "isatty" => int_of(rt, &args[0]).and_then(|fd| {
            // SAFETY: isatty only reads the descriptor number.
            let r = unsafe { isatty(fd as i32) };
            ret_int(r as i64)
        }),
        "write" => (|| {
            let fd = int_of(rt, &args[0])? as i32;
            let (buf, off) = addr_of(rt, &args[1])?;
            let n = int_of(rt, &args[2])?.max(0) as usize;
            let data = buf.borrow();
            let end = (off + n).min(data.len());
            let start = off.min(end);
            // SAFETY: the slice is a live buffer borrowed for the call; the kernel reads at most n bytes.
            let r = unsafe { write(fd, data[start..end].as_ptr(), end - start) };
            if r < 0 {
                return ret_int(-errno());
            }
            ret_int(r as i64)
        })(),
        "read" => (|| {
            let fd = int_of(rt, &args[0])? as i32;
            let (buf, off) = addr_of(rt, &args[1])?;
            let n = int_of(rt, &args[2])?.max(0) as usize;
            let mut data = buf.borrow_mut();
            let end = (off + n).min(data.len());
            let start = off.min(end);
            // SAFETY: the slice is a live buffer borrowed mutably for the call; the kernel writes at most n bytes.
            let r = unsafe { read(fd, data[start..end].as_mut_ptr(), end - start) };
            if r < 0 {
                return ret_int(-errno());
            }
            ret_int(r as i64)
        })(),
        "close" => int_of(rt, &args[0]).and_then(|fd| {
            // SAFETY: close takes a descriptor number.
            let r = unsafe { close(fd as i32) };
            ret_int(r as i64)
        }),
        "lseek" => (|| {
            let fd = int_of(rt, &args[0])? as i32;
            let off = int_of(rt, &args[1])?;
            let whence = int_of(rt, &args[2])? as i32;
            // SAFETY: lseek takes plain integers.
            let r = unsafe { lseek(fd, off, whence) };
            ret_int(r)
        })(),
        "__hsbase_MD5Init" => (|| {
            let (ctx, off) = addr_of(rt, &args[0])?;
            super::md5::init(&mut ctx.borrow_mut()[off..off + super::md5::CONTEXT_SIZE]);
            Ok(io_result_of(unit_value()))
        })(),
        "__hsbase_MD5Update" => (|| {
            let (ctx, coff) = addr_of(rt, &args[0])?;
            let (data, doff) = addr_of(rt, &args[1])?;
            let n = int_of(rt, &args[2])?.max(0) as usize;
            let bytes: Vec<u8> = data.borrow()[doff..doff + n].to_vec();
            super::md5::update(&mut ctx.borrow_mut()[coff..coff + super::md5::CONTEXT_SIZE], &bytes);
            Ok(io_result_of(unit_value()))
        })(),
        "__hsbase_MD5Final" => (|| {
            let (digest, doff) = addr_of(rt, &args[0])?;
            let (ctx, coff) = addr_of(rt, &args[1])?;
            let d = super::md5::finish(&mut ctx.borrow_mut()[coff..coff + super::md5::CONTEXT_SIZE]);
            digest.borrow_mut()[doff..doff + 16].copy_from_slice(&d);
            Ok(io_result_of(unit_value()))
        })(),
        "rts_setMainThread" => Ok(io_result_of(unit_value())),
        n if n.starts_with("getOrSet") => (|| {
            // The RTS's named pointer slot: its value if set, otherwise the argument, which is stored.
            let (addr, off) = addr_of(rt, &args[0]).map(|(b, o)| (Some(b), o)).unwrap_or((None, 0));
            let given = match addr {
                Some(b) => Value::Addr(b, off),
                None => int_of(rt, &args[0]).map(Value::Int)?,
            };
            let mut slots = rt.ffi_slots.borrow_mut();
            let stored = slots.get(n).cloned().unwrap_or(Value::Int(0));
            let is_null = matches!(stored, Value::Int(0));
            let result = if is_null {
                slots.insert(n.to_string(), given.clone());
                given
            } else {
                stored
            };
            Ok(io_result_of(result))
        })(),
        "hs_free_stable_ptr" => {
            let i = int_of(rt, &args[0]).unwrap_or(0);
            if let Some(slot) = rt.stable.borrow_mut().get_mut(i as usize) {
                *slot = None;
            }
            Ok(io_result_of(unit_value()))
        }
        "stg_sig_install" => ret_int(0),
        // The RTS's exit: standard output is flushed, then the process ends with the code. A
        // signal exit ends the process with the conventional 128 + signal status.
        "shutdownHaskellAndExit" => int_of(rt, &args[0]).map(|code| {
            super::prims::flush_stdout();
            std::process::exit(code as i32)
        }),
        "shutdownHaskellAndSignal" => int_of(rt, &args[0]).map(|sig| {
            super::prims::flush_stdout();
            std::process::exit(128 + sig as i32)
        }),
        "errorBelch2" => (|| {
            // The RTS's errorBelch: the format is printed with its %s replaced by the message, after
            // the program's name.
            let cstr = |c: &Cell| -> Result<String, RtError> {
                let (buf, off) = addr_of(rt, c)?;
                let bytes: Vec<u8> = buf.borrow()[off..].iter().take_while(|&&b| b != 0).copied().collect();
                Ok(String::from_utf8_lossy(&bytes).into_owned())
            };
            let fmt = cstr(&args[0])?;
            let msg = cstr(&args[1])?;
            super::prims::flush_stdout();
            eprintln!("{}: {}", rt.prog_name, fmt.replacen("%s", &msg, 1));
            Ok(io_result_of(unit_value()))
        })(),
        "rtsSupportsBoundThreads" => ret_int(0),
        "getNumberOfProcessors" => ret_int(
            std::thread::available_parallelism().map(|n| n.get() as i64).unwrap_or(1),
        ),
        "isDoubleNaN" | "isFloatNaN" | "isDoubleInfinite" | "isFloatInfinite" | "isDoubleDenormalized"
        | "isFloatDenormalized" | "isDoubleNegativeZero" | "isFloatNegativeZero" | "isDoubleFinite"
        | "isFloatFinite" => (|| {
            let d = super::prims::double_of(rt, &args[0])?;
            let r = match name {
                "isDoubleNaN" | "isFloatNaN" => d.is_nan(),
                "isDoubleInfinite" | "isFloatInfinite" => d.is_infinite(),
                "isDoubleDenormalized" | "isFloatDenormalized" => d.is_subnormal(),
                "isDoubleNegativeZero" | "isFloatNegativeZero" => d == 0.0 && d.is_sign_negative(),
                _ => d.is_finite(),
            };
            ret_int(i64::from(r))
        })(),
        "fdReady" => (|| {
            // The RTS's readiness test: a write is always ready here; a read waits up to msecs.
            let fd = int_of(rt, &args[0])? as i32;
            let write = int_of(rt, &args[1])? != 0;
            let msecs = int_of(rt, &args[2])?;
            if write {
                return ret_int(1);
            }
            let mut pfd = PollFd { fd, events: POLLIN, revents: 0 };
            let timeout = msecs.clamp(-1, i32::MAX as i64) as i32;
            // SAFETY: pfd is a live, correctly laid out pollfd for one descriptor.
            let r = unsafe { poll(&mut pfd, 1, timeout) };
            ret_int(i64::from(r > 0))
        })(),
        "getpid" => {
            // SAFETY: getpid has no arguments.
            ret_int(unsafe { getpid() } as i64)
        }
        "getppid" => {
            // SAFETY: getppid has no arguments.
            ret_int(unsafe { getppid() } as i64)
        }
        _ => return None,
    };
    Some(r)
}
