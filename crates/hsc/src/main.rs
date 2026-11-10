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

use hsc::{core, driver, interp, rename, syntax, tc};

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::process::ExitCode;


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

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut flags: Vec<String> = Vec::new();
    let mut files: Vec<String> = Vec::new();
    let mut output: Option<String> = None;
    let mut run = false;
    let mut dump_ast = false;
    let mut preprocess_only = false;
    let mut dump_core = false;
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        match a {
            "--run" => run = true,
            "--dump-ast" => dump_ast = true,
            "-E" => preprocess_only = true,
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

    if preprocess_only {
        let path = std::path::Path::new(&file);
        return match driver::read_source(path).and_then(|s| {
            driver::preprocess_if_needed(s, path, &flags).map_err(std::io::Error::other)
        }) {
            Ok(s) => {
                print!("{s}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("hsc: {file}: {e}");
                ExitCode::from(1)
            }
        };
    }

    if dump_ast {
        let path = std::path::Path::new(&file);
        let src = match driver::read_source(path).and_then(|s| {
            driver::preprocess_if_needed(s, path, &flags).map_err(std::io::Error::other)
        }) {
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
        return match driver::write_native(&out, &binds) {
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
    ExitCode::from(driver::run_binds_code(binds, prog_name) as u8)
}

