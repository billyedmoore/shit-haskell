//! Foreign calls: the C and RTS functions base imports (by their entity names). A C result is
//! answered as `(# state, value #)`, and the typechecker takes the value out for a pure call or boxes
//! it by its declared type; the arguments arrive boxed (`CInt` is `I32#`, `Ptr` wraps an `Addr#`).

use super::heap::{self, kind, Word};
use super::prims::{con_value, force_arg, int_value};
use super::prims_mem::{state_pair, unit};
use super::Machine;

extern "C" {
    fn isatty(fd: i32) -> i32;
    fn write(fd: i32, buf: *const u8, n: usize) -> isize;
    fn read(fd: i32, buf: *mut u8, n: usize) -> isize;
    fn close(fd: i32) -> i32;
    fn lseek(fd: i32, off: i64, whence: i32) -> i64;
    fn getpid() -> i32;
    fn getppid() -> i32;
    fn open(path: *const u8, flags: i32, mode: u32) -> i32;
    fn fcntl(fd: i32, cmd: i32, arg: i64) -> i32;
    fn ftruncate(fd: i32, len: i64) -> i32;
    fn dup(fd: i32) -> i32;
    fn dup2(fd: i32, to: i32) -> i32;
    fn __errno_location() -> *mut i32;
    fn poll(fds: *mut PollFd, n: u64, timeout: i32) -> i32;
}

/// `struct pollfd` for the readiness test.
#[repr(C)]
struct PollFd {
    fd: i32,
    events: i16,
    revents: i16,
}

const POLLIN: i16 = 1;

fn errno() -> i64 {
    // SAFETY: __errno_location returns this thread's errno cell.
    unsafe { *__errno_location() as i64 }
}

/// An integer argument: a plain `Int#`, or a box around one (`I32#` and the like).
fn int_arg(m: &mut Machine, k: usize) -> Option<i64> {
    let v = force_arg(m, k)?;
    let v = unbox(m, v)?;
    Some(m.heap_get(v, 1) as i64)
}

/// The object inside one-field boxes (`I32#`, `Ptr`), forced.
fn unbox(m: &mut Machine, v: Word) -> Option<Word> {
    let mut v = v;
    loop {
        if heap::kind_of(m.heap_get(v, 0)) != kind::CON || heap::size_of(m.heap_get(v, 0)) != 1 {
            return Some(v);
        }
        let inner = m.heap_get(v, 1);
        v = m.force(inner)?;
    }
}

/// The address behind a pointer argument: its byte array and offset (`Ptr` wraps an `Addr#`).
fn addr_arg(m: &mut Machine, k: usize) -> Option<(Word, i64)> {
    let v = force_arg(m, k)?;
    let v = unbox(m, v)?;
    if m.heap_get(v, 0) as u8 != kind::ADDR {
        // A null pointer from a literal address.
        return Some((0, 0));
    }
    let bytes = m.heap_get(v, 1);
    let off = m.heap_get(v, 2) as i64;
    Some((bytes, off))
}

/// The raw address of a byte position (bytes object data plus offset).
fn raw(bytes: Word, off: i64) -> *mut u8 {
    if bytes == 0 {
        return std::ptr::null_mut();
    }
    (bytes as i64 + 8 + off) as *mut u8
}

fn ret_int(m: &mut Machine, n: i64) -> Word {
    let v = int_value(m, n);
    state_pair(m, v)
}

/// The C names base imports, as the machine answers them. None when `name` is not one of them.
pub fn run(m: &mut Machine, name: &str, nargs: usize) -> Option<Word> {
    let _ = nargs;
    let r = match name {
        "isatty" => {
            let fd = int_arg(m, 0)? as i32;
            // SAFETY: isatty reads the descriptor number.
            ret_int(m, unsafe { isatty(fd) } as i64)
        }
        "write" => {
            let fd = int_arg(m, 0)? as i32;
            let (b, off) = addr_arg(m, 1)?;
            let n = int_arg(m, 2)?.max(0) as usize;
            let (b2, off2) = (b, off);
            // SAFETY: the buffer is a live byte array; no collection happens during the call.
            let r = unsafe { write(fd, raw(b2, off2), n) };
            let v = if r < 0 { -1 } else { r as i64 };
            ret_int(m, v)
        }
        "read" => {
            let fd = int_arg(m, 0)? as i32;
            let (b, off) = addr_arg(m, 1)?;
            let n = int_arg(m, 2)?.max(0) as usize;
            // SAFETY: as for write.
            let r = unsafe { read(fd, raw(b, off), n) };
            let v = if r < 0 { -1 } else { r as i64 };
            ret_int(m, v)
        }
        "__hsbase_MD5Init" => {
            let _ = addr_arg(m, 0)?;
            let (ctx, co) = addr_arg(m, 0)?;
            // SAFETY: the context is a live byte array of the RTS's size, and no collection runs here.
            crate::interp::md5::init(unsafe { std::slice::from_raw_parts_mut(raw(ctx, co), crate::interp::md5::CONTEXT_SIZE) });
            let u = unit(m);
            state_pair(m, u)
        }
        "__hsbase_MD5Update" => {
            // Both arguments are forced first: forcing may collect, so the addresses are read after.
            let _ = addr_arg(m, 0)?;
            let _ = addr_arg(m, 1)?;
            let n = int_arg(m, 2)?.max(0) as usize;
            let (ctx, co) = addr_arg(m, 0)?;
            let (data, doff) = addr_arg(m, 1)?;
            // SAFETY: both are live byte arrays of the stated sizes; no collection runs during the call.
            let (c, d) = unsafe {
                (
                    std::slice::from_raw_parts_mut(raw(ctx, co), crate::interp::md5::CONTEXT_SIZE),
                    std::slice::from_raw_parts(raw(data, doff), n),
                )
            };
            crate::interp::md5::update(c, d);
            let u = unit(m);
            state_pair(m, u)
        }
        "__hsbase_MD5Final" => {
            let _ = addr_arg(m, 0)?;
            let _ = addr_arg(m, 1)?;
            let (digest, doff) = addr_arg(m, 0)?;
            let (ctx, co) = addr_arg(m, 1)?;
            // SAFETY: the digest is 16 bytes and the context is the RTS's size, both live byte arrays.
            unsafe {
                let c = std::slice::from_raw_parts_mut(raw(ctx, co), crate::interp::md5::CONTEXT_SIZE);
                let d = crate::interp::md5::finish(c);
                std::slice::from_raw_parts_mut(raw(digest, doff), 16).copy_from_slice(&d);
            }
            let u = unit(m);
            state_pair(m, u)
        }
        "fdReady" => {
            // The RTS's readiness test: a write is always ready here; a read waits up to msecs.
            let fd = int_arg(m, 0)? as i32;
            let write = int_arg(m, 1)? != 0;
            let msecs = int_arg(m, 2)?;
            if write {
                ret_int(m, 1)
            } else {
                let mut pfd = PollFd { fd, events: POLLIN, revents: 0 };
                let timeout = msecs.clamp(-1, i32::MAX as i64) as i32;
                // SAFETY: pfd is a live, correctly laid out pollfd for one descriptor.
                let r = unsafe { poll(&mut pfd, 1, timeout) };
                ret_int(m, i64::from(r > 0))
            }
        }
        "close" => {
            let fd = int_arg(m, 0)? as i32;
            // SAFETY: close takes a descriptor number.
            ret_int(m, unsafe { close(fd) } as i64)
        }
        "lseek" => {
            let fd = int_arg(m, 0)? as i32;
            let off = int_arg(m, 1)?;
            let whence = int_arg(m, 2)? as i32;
            // SAFETY: lseek takes integers.
            ret_int(m, unsafe { lseek(fd, off, whence) })
        }
        "getpid" => {
            // SAFETY: no arguments.
            ret_int(m, unsafe { getpid() } as i64)
        }
        "getppid" => {
            // SAFETY: no arguments.
            ret_int(m, unsafe { getppid() } as i64)
        }
        "open" | "__hscore_open" => {
            let (b, off) = addr_arg(m, 0)?;
            let flags = int_arg(m, 1)? as i32;
            let mode = int_arg(m, 2)? as u32;
            let mut bytes: Vec<u8> = Vec::new();
            let mut p = raw(b, off);
            if !p.is_null() {
                // SAFETY: the path is NUL-terminated within its array.
                unsafe {
                    while *p != 0 {
                        bytes.push(*p);
                        p = p.add(1);
                    }
                }
            }
            bytes.push(0);
            // SAFETY: the path is NUL-terminated and outlives the call.
            let fd = unsafe { open(bytes.as_ptr(), flags, mode) };
            ret_int(m, if fd < 0 { -1 } else { fd as i64 })
        }
        "fcntl" => {
            let fd = int_arg(m, 0)? as i32;
            let cmd = int_arg(m, 1)? as i32;
            let arg = int_arg(m, 2)?;
            // SAFETY: fcntl takes integers.
            ret_int(m, unsafe { fcntl(fd, cmd, arg) } as i64)
        }
        "ftruncate" | "__hscore_ftruncate" => {
            let fd = int_arg(m, 0)? as i32;
            let len = int_arg(m, 1)?;
            // SAFETY: ftruncate takes integers.
            ret_int(m, unsafe { ftruncate(fd, len) } as i64)
        }
        "dup" => {
            let fd = int_arg(m, 0)? as i32;
            // SAFETY: dup takes a descriptor.
            ret_int(m, unsafe { dup(fd) } as i64)
        }
        "dup2" => {
            let fd = int_arg(m, 0)? as i32;
            let to = int_arg(m, 1)? as i32;
            // SAFETY: dup2 takes descriptors.
            ret_int(m, unsafe { dup2(fd, to) } as i64)
        }
        "__hscore_get_errno" => ret_int(m, errno()),
        "__hscore_set_errno" => {
            let e = int_arg(m, 0)? as i32;
            // SAFETY: __errno_location returns this thread's errno cell.
            unsafe { *__errno_location() = e };
            let u = unit(m);
            state_pair(m, u)
        }
        // Constants of the C headers (Linux values).
        "__hscore_o_rdonly" => ret_int(m, 0),
        "__hscore_o_wronly" => ret_int(m, 1),
        "__hscore_o_rdwr" => ret_int(m, 2),
        "__hscore_o_append" => ret_int(m, 0o2000),
        "__hscore_o_creat" => ret_int(m, 0o100),
        "__hscore_o_excl" => ret_int(m, 0o200),
        "__hscore_o_trunc" => ret_int(m, 0o1000),
        "__hscore_o_noctty" => ret_int(m, 0o400),
        "__hscore_o_nonblock" => ret_int(m, 0o4000),
        "__hscore_o_binary" => ret_int(m, 0),
        "__hscore_f_getfl" => ret_int(m, 3),
        "__hscore_f_setfl" => ret_int(m, 4),
        "__hscore_f_setfd" => ret_int(m, 2),
        "__hscore_fd_cloexec" => ret_int(m, 1),
        "__hscore_sizeof_stat" => ret_int(m, 144),
        "__hscore_setmode" | "lockFile" | "unlockFile" => ret_int(m, 0),
        "rtsSupportsBoundThreads" | "rts_isThreaded" => ret_int(m, 0),
        "base_strerror_r" => {
            let e = int_arg(m, 0)? as i32;
            let (b, off) = addr_arg(m, 1)?;
            let len = int_arg(m, 2)?.max(0) as usize;
            let text = strerror(e);
            let n = text.len().min(len.saturating_sub(1));
            // SAFETY: the buffer holds `len` bytes (allocaBytes in the library).
            unsafe {
                let p = raw(b, off);
                std::ptr::copy_nonoverlapping(text.as_ptr(), p, n);
                *p.add(n) = 0;
            }
            ret_int(m, 0)
        }
        // A ThreadId# is the thread's number.
        "rts_getThreadId" => {
            let t = int_arg(m, 0)?;
            ret_int(m, t)
        }
        "cmp_thread" => {
            let (a, b) = (int_arg(m, 0)?, int_arg(m, 1)?);
            ret_int(m, a.cmp(&b) as i64)
        }
        "localeEncoding" => {
            // The locale's codeset name, as a C string.
            let bytes = super::prims::bytes_object(m, b"UTF-8");
            let a = m.alloc(3);
            m.heap_set(a, heap::header(kind::ADDR, 2, 0));
            m.heap_set(a + 8, bytes);
            m.heap_set(a + 16, 0);
            state_pair(m, a)
        }
        "getNumberOfProcessors" => {
            let n = std::thread::available_parallelism().map(|n| n.get() as i64).unwrap_or(1);
            ret_int(m, n)
        }
        "stg_sig_install" => ret_int(m, 0),
        "rts_setMainThread" | "rts_enableThreadAllocationLimit" | "rts_disableThreadAllocationLimit" => {
            let u = unit(m);
            state_pair(m, u)
        }
        "strlen" => {
            let (b, off) = addr_arg(m, 0)?;
            let mut n = 0i64;
            let mut p = raw(b, off);
            if !p.is_null() {
                // SAFETY: a C string is NUL-terminated within its array.
                unsafe {
                    while *p != 0 {
                        n += 1;
                        p = p.add(1);
                    }
                }
            }
            ret_int(m, n)
        }
        "__hscore_fstat" | "__hscore_stat" | "__hscore_lstat" => {
            let meta = if name == "__hscore_fstat" {
                let fd = int_arg(m, 0)? as i32;
                // SAFETY: the file is borrowed from the descriptor, and not closed here.
                let f = std::mem::ManuallyDrop::new(unsafe { <std::fs::File as std::os::fd::FromRawFd>::from_raw_fd(fd) });
                f.metadata()
            } else {
                let (b, off) = addr_arg(m, 0)?;
                let mut bytes: Vec<u8> = Vec::new();
                let mut p = raw(b, off);
                if !p.is_null() {
                    // SAFETY: a C string is NUL-terminated.
                    unsafe {
                        while *p != 0 {
                            bytes.push(*p);
                            p = p.add(1);
                        }
                    }
                }
                let path = std::path::PathBuf::from(String::from_utf8_lossy(&bytes).into_owned());
                if name == "__hscore_stat" { std::fs::metadata(path) } else { std::fs::symlink_metadata(path) }
            };
            let Ok(meta) = meta else { return Some(ret_int(m, -2)) };
            use std::os::unix::fs::MetadataExt;
            let (b, off) = addr_arg(m, if name == "__hscore_fstat" { 1 } else { 1 })?;
            let dst = raw(b, off);
            // SAFETY: the stat buffer is a live byte array of the size base reserves (144 bytes).
            unsafe {
                std::ptr::write_unaligned(dst as *mut u64, meta.dev());
                std::ptr::write_unaligned(dst.add(8) as *mut u64, meta.ino());
                std::ptr::write_unaligned(dst.add(24) as *mut u32, meta.mode());
                std::ptr::write_unaligned(dst.add(48) as *mut u64, meta.size());
                std::ptr::write_unaligned(dst.add(88) as *mut u64, meta.mtime() as u64);
            }
            ret_int(m, 0)
        }
        "__hscore_st_dev" | "__hscore_st_ino" | "__hscore_st_mode" | "__hscore_st_size" | "__hscore_st_mtime" => {
            let (b, off) = addr_arg(m, 0)?;
            let (at, width) = match name {
                "__hscore_st_dev" => (0, 8),
                "__hscore_st_ino" => (8, 8),
                "__hscore_st_mode" => (24, 4),
                "__hscore_st_size" => (48, 8),
                _ => (88, 8),
            };
            let src = raw(b, off);
            let mut bytes = [0u8; 8];
            // SAFETY: the stat buffer holds the field.
            unsafe { std::ptr::copy_nonoverlapping(src.add(at), bytes.as_mut_ptr(), width) };
            ret_int(m, i64::from_le_bytes(bytes))
        }
        "S_ISDIR" | "S_ISREG" | "S_ISCHR" | "S_ISBLK" | "S_ISFIFO" | "S_ISSOCK" => {
            let mode = int_arg(m, 0)? & 0o170000;
            let wanted = match name {
                "S_ISDIR" => 0o040000,
                "S_ISREG" => 0o100000,
                "S_ISCHR" => 0o020000,
                "S_ISBLK" => 0o060000,
                "S_ISFIFO" => 0o010000,
                _ => 0o140000,
            };
            ret_int(m, (mode == wanted) as i64)
        }
        "isDoubleNaN" | "isFloatNaN" | "isDoubleInfinite" | "isFloatInfinite" | "isDoubleDenormalized"
        | "isFloatDenormalized" | "isDoubleNegativeZero" | "isFloatNegativeZero" | "isDoubleFinite"
        | "isFloatFinite" => {
            let v = force_arg(m, 0)?;
            let x = f64::from_bits(m.heap_get(v, 1));
            let r = match name {
                "isDoubleNaN" | "isFloatNaN" => x.is_nan(),
                "isDoubleInfinite" | "isFloatInfinite" => x.is_infinite(),
                "isDoubleDenormalized" | "isFloatDenormalized" => x.is_subnormal(),
                "isDoubleNegativeZero" | "isFloatNegativeZero" => x == 0.0 && x.is_sign_negative(),
                _ => x.is_finite(),
            };
            ret_int(m, r as i64)
        }
        "errorBelch2" => {
            let (fb, fo) = addr_arg(m, 0)?;
            let (mb, mo) = addr_arg(m, 1)?;
            let fmt = c_string(raw(fb, fo));
            let msg = c_string(raw(mb, mo));
            let prog = m.prog_name.clone();
            eprintln!("{}: {}", prog, fmt.replacen("%s", &msg, 1));
            let u = unit(m);
            state_pair(m, u)
        }
        "shutdownHaskellAndExit" | "shutdownHaskellAndSignal" => {
            let code = int_arg(m, 0)? as i32;
            let _ = std::io::Write::flush(&mut std::io::stdout());
            let status = if name == "shutdownHaskellAndSignal" { 128 + code } else { code };
            std::process::exit(status);
        }
        "hs_free_stable_ptr" => {
            let i = int_arg(m, 0)? as usize;
            if let Some(s) = m.stable.get_mut(i) {
                *s = 0;
            }
            let u = unit(m);
            state_pair(m, u)
        }
        _ if name.starts_with("getOrSet") => {
            // The argument is a `Ptr`; the slot keeps and answers the raw address object, which the
            // typechecker boxes again by the declared result type.
            let v = force_arg(m, 0)?;
            let given = unbox(m, v)?;
            let stored = m.named.get(name).copied().unwrap_or(0);
            let result = if stored == 0 {
                m.named.insert(name.to_string(), given);
                given
            } else {
                stored
            };
            state_pair(m, result)
        }
        _ => return None,
    };
    Some(r)
}

/// The text of a NUL-terminated C string.
fn c_string(p: *mut u8) -> String {
    if p.is_null() {
        return String::new();
    }
    let mut bytes = Vec::new();
    // SAFETY: a C string is NUL-terminated.
    unsafe {
        let mut q = p;
        while *q != 0 {
            bytes.push(*q);
            q = q.add(1);
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

#[allow(dead_code)]
fn unused() -> Word {
    con_value as usize as Word
}

/// The C library's message for an error number (as `strerror`).
pub fn strerror(e: i32) -> String {
    let text = std::io::Error::from_raw_os_error(e).to_string();
    match text.rfind(" (os error ") {
        Some(i) => text[..i].to_string(),
        None => text,
    }
}
