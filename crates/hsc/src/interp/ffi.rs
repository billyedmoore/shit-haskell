//! The C entry points that base imports with `foreign import ccall`. Each is a call into the C
//! library that std already links (no crates). Pointer arguments arrive as `Ptr` constructors around
//! an address (a shared byte buffer and an offset); a buffer's bytes are passed to the C call by
//! address, which is valid while the buffer is borrowed for that call.

use std::cell::RefCell;
use std::rc::Rc;

use super::prims::io_result_of;
use super::{unit_value, Cell, RR, Runtime, RtError, Value};

extern "C" {
    fn fork() -> i32;
    fn isatty(fd: i32) -> i32;
    fn write(fd: i32, buf: *const u8, n: usize) -> isize;
    fn read(fd: i32, buf: *mut u8, n: usize) -> isize;
    fn close(fd: i32) -> i32;
    fn lseek(fd: i32, off: i64, whence: i32) -> i64;
    fn getpid() -> i32;
    fn getppid() -> i32;
    fn __errno_location() -> *mut i32;
    fn poll(fds: *mut PollFd, nfds: u64, timeout: i32) -> i32;
    fn open(path: *const u8, flags: i32, mode: u32) -> i32;
    fn readlink(path: *const u8, buf: *mut u8, n: usize) -> isize;
    fn fcntl(fd: i32, cmd: i32, arg: i64) -> i32;
    fn ftruncate(fd: i32, len: i64) -> i32;
    fn dup(fd: i32) -> i32;
    fn dup2(fd: i32, to: i32) -> i32;
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

/// The size of the stat buffer the RTS accessors read: x86-64's `struct stat`.
const STAT_SIZE: i64 = 144;

/// ENOENT, for a stat that failed without a usable errno.
fn libc_enoent() -> i64 {
    2
}

fn errno_or(fallback: i64) -> i64 {
    let e = errno();
    if e == 0 { fallback } else { e }
}

fn write_u64(data: &mut [u8], at: usize, v: u64) {
    data[at..at + 8].copy_from_slice(&v.to_le_bytes());
}

fn write_u32(data: &mut [u8], at: usize, v: u32) {
    data[at..at + 4].copy_from_slice(&v.to_le_bytes());
}

/// An integer field of a stat buffer, at a byte offset, `width` bytes wide.
fn stat_field(rt: &Runtime, c: &Cell, at: usize, width: usize) -> RR {
    let (buf, off) = addr_of(rt, c)?;
    let data = buf.borrow();
    let mut bytes = [0u8; 8];
    bytes[..width].copy_from_slice(&data[off + at..off + at + width]);
    ret_int(i64::from_le_bytes(bytes))
}

/// A C function called by its address with the arguments of a foreign import (see `crate::ccall`).
/// A pointer argument is the address of its buffer's bytes; a pointer the function answers is an
/// address into one of the argument buffers when it points into one, the null address when null,
/// and otherwise the raw address as a number.
pub fn generic(rt: &Runtime, f: *mut std::ffi::c_void, sig: &str, ret: char, args: &[Cell], safe: bool) -> RR {
    generic_call(rt, f, sig, ret, args, safe, false)
}

/// A C call by its signature; a safe call lets the other threads run, and an interruptible one
/// can be woken by an exception thrown to the thread.
pub fn generic_call(rt: &Runtime, f: *mut std::ffi::c_void, sig: &str, ret: char, args: &[Cell], safe: bool, interruptible: bool) -> RR {
    let mut ints: Vec<u64> = Vec::new();
    let mut floats: Vec<f64> = Vec::new();
    // The buffers passed (kept alive through the call), with their start addresses and lengths.
    let mut bufs: Vec<(Rc<RefCell<Vec<u8>>>, usize, usize)> = Vec::new();
    for (k, class) in sig.chars().enumerate() {
        let Some(c) = args.get(k) else { break };
        let mut v = rt.force(c)?;
        // One-field boxes (CInt, Ptr, FunPtr) hold the machine value.
        while let Value::Con(con) = &v {
            if con.args.len() != 1 {
                break;
            }
            let inner = con.args[0].clone();
            v = rt.force(&inner)?;
        }
        match class {
            'd' | 'f' => {
                let x = match v {
                    Value::Double(d) => d,
                    Value::Int(i) => i as f64,
                    _ => 0.0,
                };
                floats.push(if class == 'f' { crate::ccall::float_arg(x) } else { x });
            }
            _ => ints.push(match v {
                Value::Int(i) => i as u64,
                Value::Char(ch) => ch as u64,
                Value::Addr(b, off) => {
                    let (start, len) = {
                        // SAFETY: the buffer is not reallocated while the call runs.
                        let vec = unsafe { &mut *b.as_ptr() };
                        (vec.as_mut_ptr() as usize, vec.len())
                    };
                    let p = if len == 0 && off == 0 { 0 } else { start + off };
                    bufs.push((b, start, len));
                    p as u64
                }
                Value::Bytes(b) => {
                    let (start, len) = {
                        // SAFETY: as above.
                        let vec = unsafe { &mut *b.as_ptr() };
                        (vec.as_mut_ptr() as usize, vec.len())
                    };
                    bufs.push((b, start, len));
                    start as u64
                }
                Value::Con(con) => match con.name {
                    "GHC.Types.True" => 1,
                    _ => 0,
                },
                _ => 0,
            }),
        }
    }
    // SAFETY: the foreign import declares the function's type; the buffers outlive the call.
    // A safe call lets the other threads run meanwhile.
    let answer = if safe && super::conc::with(|s| s.multi()) {
        // SAFETY: no reference into the scheduler is held across the call.
        let back = unsafe { super::conc::S::enter_foreign(super::conc::sched(), interruptible) };
        let a = unsafe { crate::ccall::call(f, &ints, &floats, ret) };
        if let Some(back) = back {
            back.wait();
            // An exception thrown to the thread during the call is raised as it returns.
            if let Some(i) = super::conc::interrupt() {
                return Err(super::prims_conc::interrupt_error(rt, i));
            }
        }
        a
    } else {
        unsafe { crate::ccall::call(f, &ints, &floats, ret) }
    };
    let value = match (answer, ret) {
        (_, 'v') => unit_value(),
        (crate::ccall::Answer::Double(d), _) => Value::Double(d),
        (crate::ccall::Answer::Int(p), 'p') => {
            let p = p as usize;
            match bufs.iter().find(|(_, start, len)| p >= *start && p <= start + len) {
                Some((b, start, _)) => Value::Addr(b.clone(), p - start),
                None if p == 0 => Value::Addr(Rc::new(RefCell::new(Vec::new())), 0),
                None => Value::Int(p as i64),
            }
        }
        (crate::ccall::Answer::Int(i), _) => Value::Int(i),
    };
    Ok(io_result_of(value))
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
                return ret_int(-1);
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
                return ret_int(-1);
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
        // ----- files: the RTS's open/stat wrappers and their constants (Linux values) -----
        "__hscore_o_rdonly" => ret_int(0),
        "__hscore_o_wronly" => ret_int(1),
        "__hscore_o_rdwr" => ret_int(2),
        "__hscore_o_append" => ret_int(0o2000),
        "__hscore_o_creat" => ret_int(0o100),
        "__hscore_o_excl" => ret_int(0o200),
        "__hscore_o_trunc" => ret_int(0o1000),
        "__hscore_o_noctty" => ret_int(0o400),
        "__hscore_o_nonblock" => ret_int(0o4000),
        "__hscore_o_binary" => ret_int(0),
        "__hscore_f_getfl" => ret_int(3),
        "__hscore_f_setfl" => ret_int(4),
        "__hscore_f_setfd" => ret_int(2),
        "__hscore_fd_cloexec" => ret_int(1),
        "__hscore_sizeof_stat" => ret_int(STAT_SIZE),
        "__hscore_setmode" => ret_int(0),
        "lockFile" => (|| {
            let (fd, dev, ino, w) = (int_of(rt, &args[0])?, int_of(rt, &args[1])?, int_of(rt, &args[2])?, int_of(rt, &args[3])?);
            ret_int(crate::filelock::lock_file(fd, dev as u64, ino as u64, w != 0))
        })(),
        "unlockFile" => (|| ret_int(crate::filelock::unlock_file(int_of(rt, &args[0])?)))(),
        "readlink" => (|| {
            let (path, poff) = addr_of(rt, &args[0])?;
            let mut bytes: Vec<u8> = path.borrow()[poff..].iter().take_while(|&&b| b != 0).copied().collect();
            bytes.push(0);
            let (buf, off) = addr_of(rt, &args[1])?;
            let n = int_of(rt, &args[2])?.max(0) as usize;
            let mut data = buf.borrow_mut();
            let n = n.min(data.len().saturating_sub(off));
            // SAFETY: the path is NUL-terminated; the buffer slice holds `n` bytes.
            let r = unsafe { readlink(bytes.as_ptr(), data[off..].as_mut_ptr(), n) };
            ret_int(if r < 0 { -1 } else { r as i64 })
        })(),
        "__hscore_open" => (|| {
            let (path, poff) = addr_of(rt, &args[0])?;
            let mut bytes: Vec<u8> = path.borrow()[poff..].iter().take_while(|&&b| b != 0).copied().collect();
            bytes.push(0);
            let flags = int_of(rt, &args[1])? as i32;
            let mode = int_of(rt, &args[2])? as u32;
            // SAFETY: bytes is NUL-terminated and lives for the call.
            let fd = unsafe { open(bytes.as_ptr(), flags, mode) };
            if fd < 0 {
                return ret_int(-1);
            }
            ret_int(fd as i64)
        })(),
        // stat, fstat and lstat are the real C functions (the RTS shim's), which set errno.
        "__hscore_st_dev" => stat_field(rt, &args[0], 0, 8),
        "__hscore_st_ino" => stat_field(rt, &args[0], 8, 8),
        "__hscore_st_mode" => stat_field(rt, &args[0], 24, 4),
        "__hscore_st_size" => stat_field(rt, &args[0], 48, 8),
        "__hscore_st_mtime" => stat_field(rt, &args[0], 88, 8),
        "__hscore_ftruncate" => (|| {
            let fd = int_of(rt, &args[0])? as i32;
            let len = int_of(rt, &args[1])?;
            // SAFETY: ftruncate takes plain integers.
            ret_int(unsafe { ftruncate(fd, len) } as i64)
        })(),
        "dup" => int_of(rt, &args[0]).and_then(|fd| {
            // SAFETY: dup takes a descriptor number.
            ret_int(unsafe { dup(fd as i32) } as i64)
        }),
        "dup2" => (|| {
            let fd = int_of(rt, &args[0])? as i32;
            let to = int_of(rt, &args[1])? as i32;
            // SAFETY: dup2 takes descriptor numbers.
            ret_int(unsafe { dup2(fd, to) } as i64)
        })(),
        // The file-type macros of <sys/stat.h>: a test of the type bits of a mode.
        "S_ISDIR" | "S_ISREG" | "S_ISCHR" | "S_ISBLK" | "S_ISFIFO" | "S_ISSOCK" => (|| {
            let mode = int_of(rt, &args[0])? & 0o170000;
            let wanted = match name {
                "S_ISDIR" => 0o040000,
                "S_ISREG" => 0o100000,
                "S_ISCHR" => 0o020000,
                "S_ISBLK" => 0o060000,
                "S_ISFIFO" => 0o010000,
                _ => 0o140000,
            };
            ret_int(i64::from(mode == wanted))
        })(),
        "__hscore_get_errno" => ret_int(errno()),
        "__hscore_set_errno" => (|| {
            let e = int_of(rt, &args[0])?;
            // SAFETY: __errno_location returns this thread's errno cell.
            unsafe { *__errno_location() = e as i32 };
            Ok(io_result_of(unit_value()))
        })(),
        "strlen" => (|| {
            let (buf, off) = addr_of(rt, &args[0])?;
            let data = buf.borrow();
            let n = data[off..].iter().take_while(|&&b| b != 0).count();
            ret_int(n as i64)
        })(),
        "rtsSupportsBoundThreads" | "rts_isThreaded" => ret_int(0),
        // The C library's memory functions, on addresses into byte buffers.
        "memcpy" | "memmove" => (|| {
            let (dst, doff) = addr_of(rt, &args[0])?;
            let (src, soff) = addr_of(rt, &args[1])?;
            let n = int_of(rt, &args[2])?.max(0) as usize;
            if Rc::ptr_eq(&dst, &src) {
                let mut b = dst.borrow_mut();
                let n = n.min(b.len().saturating_sub(soff)).min(b.len().saturating_sub(doff));
                b.copy_within(soff..soff + n, doff);
            } else {
                let s = src.borrow();
                let mut d = dst.borrow_mut();
                let n = n.min(s.len().saturating_sub(soff)).min(d.len().saturating_sub(doff));
                d[doff..doff + n].copy_from_slice(&s[soff..soff + n]);
            }
            Ok(io_result_of(Value::Addr(dst, doff)))
        })(),
        "memset" => (|| {
            let (dst, doff) = addr_of(rt, &args[0])?;
            let c = int_of(rt, &args[1])? as u8;
            let n = int_of(rt, &args[2])?.max(0) as usize;
            {
                let mut d = dst.borrow_mut();
                let end = (doff + n).min(d.len());
                for b in d[doff.min(end)..end].iter_mut() {
                    *b = c;
                }
            }
            Ok(io_result_of(Value::Addr(dst, doff)))
        })(),
        "memcmp" => (|| {
            let (a, ao) = addr_of(rt, &args[0])?;
            let (b, bo) = addr_of(rt, &args[1])?;
            let n = int_of(rt, &args[2])?.max(0) as usize;
            let (x, y) = (a.borrow(), b.borrow());
            let xs = &x[ao.min(x.len())..(ao + n).min(x.len())];
            let ys = &y[bo.min(y.len())..(bo + n).min(y.len())];
            ret_int(match xs.cmp(ys) {
                std::cmp::Ordering::Less => -1,
                std::cmp::Ordering::Equal => 0,
                std::cmp::Ordering::Greater => 1,
            })
        })(),
        "memchr" => (|| {
            let (a, ao) = addr_of(rt, &args[0])?;
            let c = int_of(rt, &args[1])? as u8;
            let n = int_of(rt, &args[2])?.max(0) as usize;
            let found = {
                let x = a.borrow();
                x[ao.min(x.len())..(ao + n).min(x.len())].iter().position(|&b| b == c)
            };
            Ok(io_result_of(match found {
                Some(i) => Value::Addr(a, ao + i),
                // The null pointer.
                None => Value::Addr(Rc::new(RefCell::new(Vec::new())), 0),
            }))
        })(),
        "base_strerror_r" => (|| {
            let e = int_of(rt, &args[0])? as i32;
            let (buf, off) = addr_of(rt, &args[1])?;
            let len = int_of(rt, &args[2])?.max(0) as usize;
            let text = crate::machine::prims_ffi::strerror(e);
            let mut data = buf.borrow_mut();
            let n = text.len().min(len.saturating_sub(1)).min(data.len().saturating_sub(off + 1));
            data[off..off + n].copy_from_slice(&text.as_bytes()[..n]);
            data[off + n] = 0;
            ret_int(0)
        })(),
        // A ThreadId# is the thread's number.
        "rts_getThreadId" => int_of(rt, &args[0]).and_then(ret_int),
        "cmp_thread" => (|| {
            let (a, b) = (int_of(rt, &args[0])?, int_of(rt, &args[1])?);
            ret_int(a.cmp(&b) as i64)
        })(),
        "rts_enableThreadAllocationLimit" | "rts_disableThreadAllocationLimit" => Ok(io_result_of(unit_value())),
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
        // forkProcess(stable): the child runs the IO action, flushes the standard handles and exits.
        "forkProcess" => (|| {
            let idx = int_of(rt, &args[0])? as usize;
            let action = rt.stable.borrow().get(idx).cloned().flatten();
            // SAFETY: fork has no arguments; the child continues on this thread alone.
            let pid = unsafe { fork() };
            if pid == 0 {
                super::conc::with(|s| s.only_current());
                if let Some(a) = action {
                    if let Ok(f) = rt.force(&a) {
                        let _ = rt.apply(f, super::prims_conc::unit_cell());
                    }
                }
                if let Some(g) = rt.global("GHC.Internal.TopHandler.flushStdHandles") {
                    if let Ok(f) = rt.force(&g) {
                        let _ = rt.apply(f, super::prims_conc::unit_cell());
                    }
                }
                std::process::exit(0);
            }
            ret_int(pid as i64)
        })(),
        // System.Mem: memory is reference counted, so there is nothing to collect.
        "performGC" | "performMajorGC" | "performBlockingMajorGC" => Ok(io_result_of(unit_value())),
        "getppid" => {
            // SAFETY: getppid has no arguments.
            ret_int(unsafe { getppid() } as i64)
        }
        _ => return None,
    };
    Some(r)
}
