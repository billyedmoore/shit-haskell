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
    // The compiler recurses over deep syntax and Core (long lists, operator chains, large
    // generated modules): it runs on a thread with a large stack (reserved, committed on use).
    let worker = std::thread::Builder::new().name("hsc".into()).stack_size(1 << 30).spawn(real_main);
    match worker {
        Ok(h) => h.join().unwrap_or(ExitCode::from(101)),
        Err(_) => real_main(),
    }
}

fn real_main() -> ExitCode {
    tune_allocator();
    if let Some(payload) = embedded_payload() {
        return ExitCode::from(driver::run_payload_bytes(payload) as u8);
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    // `runghc` / `runhaskell` (as the program's name, or `hsc --runghc`).
    let invoked = std::env::args().next().map(|a| Path::new(&a).file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default()).unwrap_or_default();
    if invoked == "runghc" || invoked == "runhaskell" {
        return runghc(args);
    }
    if args.first().is_some_and(|a| a == "--runghc") {
        return runghc(args[1..].to_vec());
    }
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
    // `--abi-hash` of no modules (cabal asks it of a package with only C sources): GHC's hash
    // of the empty set of interfaces.
    if opts.abi_hash && opts.inputs.is_empty() {
        println!("f1c6d8ec5f245fb25a4c98a26aaeb8c2");
        return ExitCode::SUCCESS;
    }
    if opts.inputs.is_empty() {
        eprintln!("hsc: no input files\nUsage: For basic information, try the `--help' option.");
        return ExitCode::from(1);
    }
    let t = std::time::Instant::now();
    let result = cli::compile(&opts);
    driver::timing::add("total", t);
    driver::timing::report();
    match result {
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
        for (name, target) in [("ghc", &me), ("ghc-pkg", &pkg), ("hsc2hs", &hsc2hs), ("runghc", &me), ("runhaskell", &me)] {
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

/// `runghc [flags] file [args]`: the flags are the compiler's (`--ghc-arg=x` too), the arguments
/// after the file the program's. `-e expr` (instead of a file) runs an expression: an IO action,
/// or a value to print.
fn runghc(args: Vec<String>) -> ExitCode {
    let mut flags: Vec<String> = Vec::new();
    let mut exprs: Vec<String> = Vec::new();
    let mut file: Option<String> = None;
    let mut rest: Vec<String> = Vec::new();
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        if file.is_some() {
            rest.push(a);
        } else if a == "-e" {
            match it.next() {
                Some(e) => exprs.push(e),
                None => {
                    eprintln!("runghc: -e needs an expression");
                    return ExitCode::from(1);
                }
            }
        } else if let Some(f) = a.strip_prefix("--ghc-arg=") {
            flags.push(f.to_string());
        } else if a == "--" {
            file = it.next();
        } else if a.starts_with('-') {
            flags.push(a);
        } else {
            file = Some(a);
        }
    }
    let opts = match cli::parse(flags.clone()) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("runghc: {e}");
            return ExitCode::from(1);
        }
    };
    let run = |path: &Path, args: Vec<String>| -> Result<i32, String> { cli::run_program(&opts, path, args) };
    let result = if !exprs.is_empty() {
        // Each expression in turn: as an IO action if it is one, else printed.
        let dir = driver::cache_dir("eval");
        if let Err(e) = std::fs::create_dir_all(&dir) {
            eprintln!("runghc: {}: {e}", dir.display());
            return ExitCode::from(1);
        }
        let mut code = 0;
        for e in &exprs {
            let path = dir.join(format!("Eval{}.hs", std::process::id()));
            let io = format!("main :: IO ()\nmain = ({e}) >>= \\_ -> return ()\n");
            let shown = format!("main :: IO ()\nmain = print ({e})\n");
            let as_io = std::fs::write(&path, &io).map_err(|x| x.to_string()).and_then(|()| {
                // Checked first, quietly: an expression that is not an IO action is printed.
                let mut d = driver::Driver::new(vec![dir.clone()], driver::default_package_dirs(), Vec::new());
                driver::compile_with(&mut d, &path, &[]).map(|_| ())
            });
            if as_io.is_err() {
                let _ = std::fs::write(&path, &shown);
            }
            let r = run(&path, rest.clone());
            let _ = std::fs::remove_file(&path);
            match r {
                Ok(c) => code = c,
                Err(msg) => {
                    eprintln!("{msg}");
                    return ExitCode::from(1);
                }
            }
        }
        Ok(code)
    } else if let Some(f) = file {
        run(Path::new(&f), rest)
    } else {
        Err("runghc: no file given".into())
    };
    match result {
        Ok(code) => ExitCode::from(code as u8),
        Err(msg) => {
            eprintln!("{msg}");
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
