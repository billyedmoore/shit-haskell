//! hsc: a Haskell compiler compatible with GHC 9.12.
//!
//! The command line is GHC's (see `cli`), plus development modes:
//!   hsc --run file.hs              compile and run
//!   hsc --dump-ast file.hs         print the parsed syntax tree
//!   hsc --dump-core file.hs        print the Core program
//!   hsc --native ...               native code (x86-64) instead of the interpreter
//!
//! Output executables are copies of this binary with the compiled program appended; at startup a
//! binary that carries a program runs it instead of compiling (or native code, with `--native`).

use hsc::{cli, driver, syntax};

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::process::ExitCode;

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
        return ExitCode::from(driver::run_payload_bytes(payload) as u8);
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|a| a == "--install-ghc-shims") {
        return install_shims(args.get(1).map(String::as_str).unwrap_or("."));
    }
    let opts = match cli::parse(args) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("hsc: {e}");
            return ExitCode::from(1);
        }
    };
    if let Some(what) = &opts.info {
        print!("{}", cli::info(what));
        return ExitCode::SUCCESS;
    }
    if opts.preprocess_only || opts.dump_ast || opts.run {
        return dev_mode(&opts);
    }
    if opts.inputs.is_empty() {
        eprintln!("hsc: no input files\nUsage: For basic information, try the `--help' option.");
        return ExitCode::from(1);
    }
    match cli::compile(&opts) {
        Ok(code) => ExitCode::from(code as u8),
        Err(msg) => {
            eprintln!("{msg}");
            ExitCode::from(1)
        }
    }
}

/// Makes `ghc`, `ghc-pkg` and `hsc2hs` links to hsc, hsc-pkg and hsc-hsc2hs in a directory: build
/// tools find the other tools from the compiler's name (`cabal build -w DIR/ghc`).
fn install_shims(dir: &str) -> ExitCode {
    let result = (|| -> std::io::Result<()> {
        let me = std::fs::canonicalize(std::env::current_exe()?)?;
        let pkg = me.with_file_name("hsc-pkg");
        let hsc2hs = me.with_file_name("hsc-hsc2hs");
        std::fs::create_dir_all(dir)?;
        for (name, target) in [("ghc", &me), ("ghc-pkg", &pkg), ("hsc2hs", &hsc2hs)] {
            let link = Path::new(dir).join(name);
            let _ = std::fs::remove_file(&link);
            std::os::unix::fs::symlink(target, &link)?;
        }
        Ok(())
    })();
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("hsc: {dir}: {e}");
            ExitCode::from(1)
        }
    }
}

/// The development modes on a single file.
fn dev_mode(opts: &cli::Opts) -> ExitCode {
    let Some(file) = opts.inputs.first().cloned() else {
        eprintln!("hsc: no input files");
        return ExitCode::from(1);
    };
    let path = Path::new(&file);
    let flags = &opts.flags;
    if opts.preprocess_only || opts.dump_ast {
        let src = match driver::read_source(path).and_then(|s| driver::preprocess_if_needed(s, path, flags).map_err(std::io::Error::other)) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("hsc: {file}: {e}");
                return ExitCode::from(1);
            }
        };
        if opts.preprocess_only {
            print!("{src}");
            return ExitCode::SUCCESS;
        }
        return match syntax::parse_module_source(&src, flags) {
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
    let binds = match driver::compile_program(path, flags.clone()) {
        Ok(b) => b,
        Err(msg) => {
            eprintln!("{msg}");
            return ExitCode::from(1);
        }
    };
    let prog_name = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "main".into());
    // The program's arguments: the inputs after the source file.
    hsc::ccall::set_program_args(std::iter::once(prog_name.clone()).chain(opts.inputs[1..].iter().cloned()).collect());
    ExitCode::from(driver::run_binds_code(binds, prog_name) as u8)
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
    if &trailer[8..] != cli::MAGIC {
        return None;
    }
    let len = u64::from_le_bytes(trailer[..8].try_into().ok()?);
    f.seek(SeekFrom::Start(size - 16 - len)).ok()?;
    let mut payload = vec![0u8; len as usize];
    f.read_exact(&mut payload).ok()?;
    Some(payload)
}
