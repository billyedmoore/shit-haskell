//! hsc: a Haskell compiler compatible with GHC 9.12.
//!
//! Command line (a subset of GHC's, plus development modes):
//!   hsc [flags] file.hs            compile; `-o FILE` writes an executable
//!   hsc --run file.hs              compile and run
//!   hsc --dump-ast file.hs         print the parsed syntax tree
//!   hsc --dump-core file.hs        print the Core program
//!
//! Output executables are copies of this binary with the compiled program appended;
//! at startup a binary that carries a program runs it instead of compiling.

mod core;
mod driver;
mod interp;
mod rename;
mod syntax;
mod tc;

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::process::ExitCode;

const MAGIC: &[u8; 8] = b"HSCPROG1";

/// Options that take the following argument as their value.
const VALUE_FLAGS: &[&str] = &[
    "-o",
    "-outputdir",
    "-odir",
    "-hidir",
    "-stubdir",
    "-dumpdir",
    "-tmpdir",
    "-package",
    "-package-id",
    "-package-db",
    "-x",
];

extern "C" {
    fn mallopt(param: i32, value: i32) -> i32;
}

/// Keeps freed heap memory in the process: the interpreter allocates and frees many small
/// objects, and returning them to the kernel after each collection costs more than reuse.
fn tune_allocator() {
    const M_TRIM_THRESHOLD: i32 = -1;
    const M_TOP_PAD: i32 = -2;
    const M_MMAP_THRESHOLD: i32 = -3;
    // SAFETY: mallopt only adjusts glibc's allocator parameters.
    unsafe {
        mallopt(M_TRIM_THRESHOLD, 1 << 30);
        mallopt(M_TOP_PAD, 64 << 20);
        mallopt(M_MMAP_THRESHOLD, 256 << 20);
    }
}

fn main() -> ExitCode {
    tune_allocator();
    if let Some(payload) = embedded_payload() {
        return run_payload(payload);
    }

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut flags: Vec<String> = Vec::new();
    let mut files: Vec<String> = Vec::new();
    let mut output: Option<String> = None;
    let mut run = false;
    let mut dump_ast = false;
    let mut dump_core = false;
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        match a {
            "--run" => run = true,
            "--dump-ast" => dump_ast = true,
            "--dump-core" => dump_core = true,
            "-o" => {
                output = args.get(i + 1).cloned();
                i += 1;
            }
            _ if VALUE_FLAGS.contains(&a) => i += 1,
            _ if a.starts_with("-X") => flags.push(a.to_string()),
            _ if a.starts_with('-') => {}
            _ => files.push(a.to_string()),
        }
        i += 1;
    }
    let Some(file) = files.first().cloned() else {
        eprintln!("hsc: no input files");
        return ExitCode::from(1);
    };

    if dump_ast {
        let src = match std::fs::read_to_string(&file) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("hsc: {file}: {e}");
                return ExitCode::from(1);
            }
        };
        return match syntax::parse_module_source(&src, &flags) {
            Ok((module, _)) => {
                println!("{module:#?}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("{file}:{}:{}: error:\n    {}", e.line, e.col, e.msg);
                ExitCode::from(1)
            }
        };
    }

    let binds = match driver::compile_program(Path::new(&file), flags) {
        Ok(b) => b,
        Err(msg) => {
            eprintln!("{msg}");
            return ExitCode::from(1);
        }
    };
    if dump_core {
        for b in &binds {
            println!("{} = {}", b.name, crate::core::pretty::show(&b.body));
        }
        return ExitCode::SUCCESS;
    }
    let prog_name = Path::new(&file)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "main".into());
    if run {
        return run_binds(binds, prog_name);
    }
    if let Some(out) = output {
        return match write_executable(&out, &binds) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("hsc: cannot write {out}: {e}");
                ExitCode::from(1)
            }
        };
    }
    ExitCode::SUCCESS
}

/// Runs a program on a thread with a large stack: evaluation is recursive.
fn run_binds(binds: Vec<core::TopBind>, prog_name: String) -> ExitCode {
    let handle = std::thread::Builder::new()
        .stack_size(4 << 30)
        .spawn(move || {
            let rt = interp::Runtime::new(binds, prog_name);
            interp::run_main(&rt)
        })
        .expect("spawn interpreter thread");
    let code = handle.join().unwrap_or(1);
    ExitCode::from(code as u8)
}

fn run_payload(payload: Vec<u8>) -> ExitCode {
    let binds = match core::serial::decode(&payload) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("corrupt program payload: {e}");
            return ExitCode::from(1);
        }
    };
    let prog_name = std::env::args()
        .next()
        .and_then(|a| Path::new(&a).file_name().map(|s| s.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "main".into());
    run_binds(binds, prog_name)
}

/// Writes an executable: this binary, then the serialised program, its length, and a
/// magic trailer.
fn write_executable(path: &str, binds: &[core::TopBind]) -> std::io::Result<()> {
    let me = std::env::current_exe()?;
    let mut bytes = std::fs::read(me)?;
    if let Some(start) = payload_offset(&bytes) {
        bytes.truncate(start);
    }
    let payload = core::serial::encode(binds);
    bytes.extend_from_slice(&payload);
    bytes.extend_from_slice(&(payload.len() as u64).to_le_bytes());
    bytes.extend_from_slice(MAGIC);
    std::fs::write(path, &bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))?;
    }
    Ok(())
}

/// Where an embedded payload starts, if this file has one.
fn payload_offset(bytes: &[u8]) -> Option<usize> {
    if bytes.len() < 16 || &bytes[bytes.len() - 8..] != MAGIC {
        return None;
    }
    let len_bytes: [u8; 8] = bytes[bytes.len() - 16..bytes.len() - 8].try_into().ok()?;
    let len = u64::from_le_bytes(len_bytes) as usize;
    (bytes.len() - 16).checked_sub(len)
}

/// The payload of this executable, if it carries one. Reads only the trailer first.
fn embedded_payload() -> Option<Vec<u8>> {
    let me = std::env::current_exe().ok()?;
    let mut f = std::fs::File::open(me).ok()?;
    let size = f.seek(SeekFrom::End(0)).ok()?;
    if size < 16 {
        return None;
    }
    f.seek(SeekFrom::End(-16)).ok()?;
    let mut trailer = [0u8; 16];
    f.read_exact(&mut trailer).ok()?;
    if &trailer[8..] != MAGIC {
        return None;
    }
    let len = u64::from_le_bytes(trailer[..8].try_into().ok()?);
    f.seek(SeekFrom::Start(size - 16 - len)).ok()?;
    let mut payload = vec![0u8; len as usize];
    f.read_exact(&mut payload).ok()?;
    Some(payload)
}
