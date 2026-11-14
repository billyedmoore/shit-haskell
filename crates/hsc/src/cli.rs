//! The command line of hsc: GHC's flags (as cabal and users pass them), the information modes
//! (`--info`, `--numeric-version`, ...), and the compilation modes (`--make`, `-c`, `-no-link`).
//!
//! hsc compiles a program from source in one go. Building a package's modules (`-no-link`, `-c`)
//! type-checks them and writes, for each, an interface file that records its source and flags, and
//! an empty object file; a program that uses the package later compiles those sources with it. A
//! program's executable is not rebuilt when none of its sources or flags changed.

use std::path::{Path, PathBuf};

use crate::driver;
use crate::pkg;

pub const VERSION: &str = "9.12.2";

/// What the command line asks for.
#[derive(Default, Clone)]
pub struct Opts {
    /// Source files and module names, in order.
    pub inputs: Vec<String>,
    pub output: Option<String>,
    /// `-c` or `-no-link`: build the modules, do not link a program.
    pub no_link: bool,
    pub odir: Option<PathBuf>,
    pub hidir: Option<PathBuf>,
    pub hisuf: String,
    pub osuf: String,
    pub dynamic_too: bool,
    pub dynosuf: String,
    pub dynhisuf: String,
    /// `-i` directories (None: the default, the current directory).
    pub search: Option<Vec<PathBuf>>,
    /// The flags of the program's own modules: `-X`, `-cpp`, CPP's `-D`/`-U`/`-I`/`-include=`, and
    /// the development flags (`-dcore-lint`).
    pub flags: Vec<String>,
    pub package_dbs: Vec<PathBuf>,
    pub no_global_db: bool,
    pub no_user_db: bool,
    pub hide_all: bool,
    pub package_ids: Vec<String>,
    pub package_names: Vec<String>,
    pub this_unit: Option<String>,
    pub native: bool,
    pub shared: bool,
    pub verbosity: u8,
    /// Development modes.
    pub run: bool,
    pub dump_ast: bool,
    pub dump_core: bool,
    pub preprocess_only: bool,
    /// `--abi-hash`: print a hash of the named modules' interfaces.
    pub abi_hash: bool,
    /// An information mode (`--info` and the like) answers by itself.
    pub info: Option<String>,
    /// Options for C compilations and links (`-optc`, `-optl`, `-l`, `-L`).
    pub c_opts: Vec<String>,
    pub link_opts: Vec<String>,
    /// `-package-env`: a file, or `-` for none (None: look for one, as GHC does).
    pub package_env: Option<String>,
}

/// Flags whose value is the next argument.
const VALUE_FLAGS: &[&str] = &[
    "-o", "-odir", "-hidir", "-hiedir", "-stubdir", "-dumpdir", "-outputdir", "-tmpdir", "-package-db", "-package-id",
    "-package", "-hide-package", "-ignore-package", "-trust", "-distrust", "-this-unit-id", "-this-package-name",
    "-working-dir", "-dynosuf", "-dynhisuf", "-osuf", "-hisuf", "-hcsuf", "-x", "-main-is", "-dynload",
    "-framework", "-framework-path", "-unit", "-hidden-module", "-reexported-module", "-this-component-id",
    "-instantiated-with", "-component-id", "-package-name", "-ghcversion-file", "-pgmc", "-pgma", "-pgml", "-pgmP",
    "-pgmF", "-pgmlo", "-pgmlc", "-pgmi", "-pgmar", "-pgmranlib", "-pgmdll", "-pgmwindres", "-pgmlibtool", "-pgmc-supports-no-pie",
    "-dep-suffix", "-dep-makefile", "-include-pkg-deps", "-exclude-module", "-fplugin", "-fplugin-opt", "-plugin-package",
    "-plugin-package-id", "-ddump-file-prefix", "-hpcdir", "-dumpdir", "-jsem",
];

/// The arguments of a response file (`@file`): words separated by white space, with quotes and
/// backslash escapes, as GHC reads them.
fn response_args(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut has = false;
    let mut chars = text.chars();
    let mut quote: Option<char> = None;
    while let Some(c) = chars.next() {
        match (quote, c) {
            (_, '\\') => {
                if let Some(n) = chars.next() {
                    cur.push(n);
                    has = true;
                }
            }
            (Some(q), c) if c == q => quote = None,
            (Some(_), c) => cur.push(c),
            (None, '"') | (None, '\'') => {
                quote = Some(c);
                has = true;
            }
            (None, c) if c.is_whitespace() => {
                if has || !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                    has = false;
                }
            }
            (None, c) => {
                cur.push(c);
                has = true;
            }
        }
    }
    if has || !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// The arguments with response files expanded.
fn expand(args: Vec<String>) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    for a in args {
        match a.strip_prefix('@') {
            Some(f) if !f.is_empty() => {
                let text = std::fs::read_to_string(f).map_err(|e| format!("{f}: {e}"))?;
                out.extend(expand(response_args(&text))?);
            }
            _ => out.push(a),
        }
    }
    Ok(out)
}

fn absolute(p: &str) -> PathBuf {
    let path = PathBuf::from(p);
    if path.is_absolute() {
        path
    } else {
        std::env::current_dir().map(|d| d.join(&path)).unwrap_or(path)
    }
}

pub fn parse(args: Vec<String>) -> Result<Opts, String> {
    let args = expand(args)?;
    let mut o = Opts {
        hisuf: "hi".into(),
        osuf: "o".into(),
        dynosuf: "dyn_o".into(),
        dynhisuf: "dyn_hi".into(),
        verbosity: 1,
        native: std::env::var_os("HSC_NATIVE").is_some(),
        ..Opts::default()
    };
    let mut optp_include = false;
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        let value = |i: &mut usize| -> Result<String, String> {
            *i += 1;
            args.get(*i).cloned().ok_or_else(|| format!("{a} needs a value"))
        };
        match a {
            "--version" | "-V" => o.info = Some("version".into()),
            "--numeric-version" => o.info = Some("numeric-version".into()),
            "--info" => o.info = Some("info".into()),
            "--supported-languages" | "--supported-extensions" => o.info = Some("languages".into()),
            "--print-libdir" => o.info = Some("libdir".into()),
            "--print-global-package-db" => o.info = Some("global-db".into()),
            "--print-project-git-commit-id" => o.info = Some("git".into()),
            "--print-booter-version" => o.info = Some("booter".into()),
            "--abi-hash" => o.abi_hash = true,
            "--run" => o.run = true,
            "--dump-ast" => o.dump_ast = true,
            "--dump-core" => o.dump_core = true,
            "-E" => o.preprocess_only = true,
            "--native" | "-fhsc-native" => o.native = true,
            "--make" => {}
            "-c" | "-no-link" => o.no_link = true,
            "-shared" => o.shared = true,
            "-dynamic-too" => o.dynamic_too = true,
            "-o" => o.output = Some(value(&mut i)?),
            "-odir" => o.odir = Some(PathBuf::from(value(&mut i)?)),
            "-hidir" => o.hidir = Some(PathBuf::from(value(&mut i)?)),
            "-outputdir" => {
                let d = PathBuf::from(value(&mut i)?);
                o.odir = Some(d.clone());
                o.hidir = Some(d);
            }
            "-hisuf" => o.hisuf = value(&mut i)?,
            "-osuf" => o.osuf = value(&mut i)?,
            "-dynosuf" => o.dynosuf = value(&mut i)?,
            "-dynhisuf" => o.dynhisuf = value(&mut i)?,
            "-package-db" => o.package_dbs.push(absolute(&value(&mut i)?)),
            "-package-env" => o.package_env = Some(value(&mut i)?),
            "-package-id" => o.package_ids.push(value(&mut i)?),
            "-package" => o.package_names.push(value(&mut i)?),
            "-this-unit-id" => o.this_unit = Some(value(&mut i)?),
            "-hide-all-packages" => o.hide_all = true,
            "-no-user-package-db" | "-no-user-package-conf" => o.no_user_db = true,
            "-no-global-package-db" => o.no_global_db = true,
            "-global-package-db" => o.no_global_db = false,
            "-user-package-db" => o.no_user_db = false,
            "-clear-package-db" => {
                o.package_dbs.clear();
                o.no_global_db = true;
                o.no_user_db = true;
            }
            "-cpp" => o.flags.push("-XCPP".into()),
            "-i" => o.search = Some(Vec::new()),
            "-v" => o.verbosity = 3,
            _ if VALUE_FLAGS.contains(&a) => i += 1,
            _ if a.starts_with("-v") && a[2..].chars().all(|c| c.is_ascii_digit()) => {
                o.verbosity = a[2..].parse().unwrap_or(1);
            }
            _ if a.starts_with("-i") => {
                let dirs = o.search.get_or_insert_with(Vec::new);
                dirs.extend(a[2..].split(':').filter(|d| !d.is_empty()).map(PathBuf::from));
            }
            _ if a.starts_with("-X") || a == "-dcore-lint" => o.flags.push(a.to_string()),
            _ if a.starts_with("-D") || a.starts_with("-U") => o.flags.push(a.to_string()),
            _ if a.starts_with("-I") && a.len() > 2 => o.flags.push(format!("-I{}", absolute(&a[2..]).display())),
            _ if a.starts_with("-optP") => {
                let opt = &a[5..];
                if optp_include {
                    o.flags.push(format!("-include={}", absolute(opt).display()));
                    optp_include = false;
                } else if opt == "-include" {
                    optp_include = true;
                } else if let Some(file) = opt.strip_prefix("-include") {
                    o.flags.push(format!("-include={}", absolute(file).display()));
                } else if opt.starts_with("-D") || opt.starts_with("-U") {
                    o.flags.push(opt.to_string());
                } else if let Some(d) = opt.strip_prefix("-I") {
                    o.flags.push(format!("-I{}", absolute(d).display()));
                }
            }
            _ if a.starts_with("-optc") => o.c_opts.push(a[5..].to_string()),
            _ if a.starts_with("-optl") => o.link_opts.push(a[5..].to_string()),
            _ if a.starts_with("-l") || a.starts_with("-L") => o.link_opts.push(a.to_string()),
            _ if a.starts_with("-package-env=") => o.package_env = Some(a["-package-env=".len()..].to_string()),
            _ if a.starts_with("-package-db=") => o.package_dbs.push(absolute(&a["-package-db=".len()..])),
            // Warnings, optimisation, code generation and debugging flags change nothing here.
            _ if a.starts_with('-') && a.len() > 1 => {}
            _ => o.inputs.push(a.to_string()),
        }
        i += 1;
    }
    Ok(o)
}

/// The answer of an information mode.
pub fn info(what: &str) -> String {
    let libdir = driver::default_lib_dir();
    let libdir = std::fs::canonicalize(&libdir).unwrap_or(libdir);
    let global = libdir.join("package.conf.d");
    match what {
        "version" => format!("The Glorious Glasgow Haskell Compilation System, version {VERSION}\n"),
        "numeric-version" => format!("{VERSION}\n"),
        "libdir" => format!("{}\n", libdir.display()),
        "global-db" => format!("{}\n", global.display()),
        "git" => "383be28ffdddf65b57b7b111bfc89808b4229ebc\n".into(),
        "booter" => "9.10.1\n".into(),
        "languages" => {
            let mut out = String::new();
            for l in crate::syntax::extensions::supported_languages() {
                out.push_str(&l);
                out.push('\n');
            }
            out
        }
        _ => {
            let fields: Vec<(&str, String)> = vec![
                ("Project name", "The Glorious Glasgow Haskell Compilation System".into()),
                ("C compiler command", "gcc".into()),
                ("C compiler flags", String::new()),
                ("C++ compiler command", "g++".into()),
                ("C++ compiler flags", String::new()),
                ("C compiler link flags", "-Wl,--no-as-needed".into()),
                ("C compiler supports -no-pie", "YES".into()),
                ("CPP command", "gcc".into()),
                ("CPP flags", "-E".into()),
                ("Haskell CPP command", "gcc".into()),
                ("Haskell CPP flags", "-E -undef -traditional".into()),
                ("ld supports compact unwind", "NO".into()),
                ("ld supports filelist", "NO".into()),
                ("ld supports single module", "NO".into()),
                ("ld is GNU ld", "YES".into()),
                ("Merge objects command", "ld".into()),
                ("Merge objects flags", "-r".into()),
                ("Merge objects supports response files", "YES".into()),
                ("ar command", "ar".into()),
                ("ar flags", "q".into()),
                ("ar supports at file", "YES".into()),
                ("ar supports -L", "NO".into()),
                ("ranlib command", "ranlib".into()),
                ("cross compiling", "NO".into()),
                ("target platform string", "x86_64-unknown-linux".into()),
                ("target os", "OSLinux".into()),
                ("target arch", "ArchX86_64".into()),
                ("target word size", "8".into()),
                ("target word big endian", "NO".into()),
                ("target has GNU nonexec stack", "YES".into()),
                ("target has .ident directive", "YES".into()),
                ("target has subsections via symbols", "NO".into()),
                ("target has libm", "YES".into()),
                ("Unregisterised", "NO".into()),
                ("LLVM target", "x86_64-unknown-linux".into()),
                ("LLVM llc command", String::new()),
                ("LLVM opt command", String::new()),
                ("LLVM llvm-as command", String::new()),
                ("Use inplace MinGW toolchain", "NO".into()),
                ("target RTS linker only supports shared libraries", "NO".into()),
                ("Use interpreter", "NO".into()),
                ("Support SMP", "NO".into()),
                ("RTS ways", "v".into()),
                ("Tables next to code", "YES".into()),
                ("Leading underscore", "NO".into()),
                ("Use LibFFI", "NO".into()),
                ("RTS expects libdw", "NO".into()),
                ("Relative Global Package DB", "package.conf.d".into()),
                ("Project version", VERSION.into()),
                ("Project Git commit id", "383be28ffdddf65b57b7b111bfc89808b4229ebc".into()),
                ("Project Version Int", "912".into()),
                ("Project Patch Level", "2".into()),
                ("Project Patch Level1", "2".into()),
                ("Project Patch Level2", "0".into()),
                ("Project Unit Id", "ghc-9.12.2".into()),
                ("Booter version", "9.10.1".into()),
                ("Stage", "2".into()),
                ("Build platform", "x86_64-unknown-linux".into()),
                ("Host platform", "x86_64-unknown-linux".into()),
                ("Target platform", "x86_64-unknown-linux".into()),
                ("Have interpreter", "NO".into()),
                ("Object splitting supported", "NO".into()),
                ("Have native code generator", "YES".into()),
                ("target has RTS linker", "NO".into()),
                ("Target default backend", "native code generator".into()),
                ("Support dynamic-too", "NO".into()),
                ("Support parallel --make", "NO".into()),
                ("Support reexported-modules", "YES".into()),
                ("Support thinning and renaming package flags", "YES".into()),
                ("Support Backpack", "NO".into()),
                ("Requires unified installed package IDs", "YES".into()),
                ("Uses package keys", "YES".into()),
                ("Uses unit IDs", "YES".into()),
                ("GHC Dynamic", "NO".into()),
                ("GHC Profiled", "NO".into()),
                ("Debug on", "NO".into()),
                ("LibDir", libdir.display().to_string()),
                ("Global Package DB", global.display().to_string()),
            ];
            let mut out = String::new();
            for (i, (k, v)) in fields.iter().enumerate() {
                out.push_str(if i == 0 { " [" } else { " ," });
                out.push_str(&format!("({},{})\n", show_string(k), show_string(v)));
            }
            out.push_str(" ]\n");
            out
        }
    }
}

/// A string as Haskell's `show` writes it.
fn show_string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The package environment file in effect, as GHC finds one: `-package-env`, then `GHC_ENVIRONMENT`,
/// then `.ghc.environment.x86_64-linux-9.12.2` in the current directory or a parent, then hsc's
/// default environment (`~/.hsc/x86_64-linux-9.12.2/environments/default`). `-` means none.
fn environment_file(o: &Opts) -> Option<PathBuf> {
    let named = o.package_env.clone().or_else(|| std::env::var("GHC_ENVIRONMENT").ok());
    if let Some(n) = named {
        if n == "-" {
            return None;
        }
        let p = PathBuf::from(&n);
        if p.is_file() {
            return Some(p);
        }
        // A bare name is an environment in the user's directory.
        let home = std::env::var_os("HOME")?;
        return Some(PathBuf::from(home).join(".hsc/x86_64-linux-9.12.2/environments").join(n));
    }
    let local = ".ghc.environment.x86_64-linux-9.12.2";
    let mut dir = std::env::current_dir().ok();
    while let Some(d) = dir {
        if d.join(local).is_file() {
            return Some(d.join(local));
        }
        dir = d.parent().map(Path::to_path_buf);
    }
    let home = std::env::var_os("HOME")?;
    let default = PathBuf::from(home).join(".hsc/x86_64-linux-9.12.2/environments/default");
    default.is_file().then_some(default)
}

/// Applies a package environment file: its lines are package flags, and only the packages it names
/// are visible (as with `-hide-all-packages`).
fn apply_environment(o: &mut Opts) -> Result<(), String> {
    let Some(path) = environment_file(o) else { return Ok(()) };
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    if o.verbosity > 0 {
        eprintln!("Loaded package environment from {}", path.display());
    }
    o.hide_all = true;
    for line in text.lines() {
        let line = line.trim();
        let (cmd, arg) = line.split_once(char::is_whitespace).map(|(c, a)| (c, a.trim())).unwrap_or((line, ""));
        match cmd {
            "clear-package-db" => {
                o.package_dbs.clear();
                o.no_global_db = true;
                o.no_user_db = true;
            }
            "global-package-db" => o.no_global_db = false,
            "user-package-db" => o.no_user_db = false,
            "package-db" => o.package_dbs.push(PathBuf::from(arg)),
            "package-id" => o.package_ids.push(arg.to_string()),
            "package" => o.package_names.push(arg.to_string()),
            _ => {}
        }
    }
    Ok(())
}

/// The package databases a compilation reads, in order.
fn db_stack(o: &Opts) -> Vec<PathBuf> {
    let mut dbs = Vec::new();
    if !o.no_global_db {
        dbs.push(pkg::global_db_path());
    }
    if !o.no_user_db {
        if let Some(u) = pkg::user_db_path() {
            dbs.push(u);
        }
    }
    dbs.extend(o.package_dbs.iter().cloned());
    dbs
}

/// The import directories of the packages a compilation can use: the closure of the packages it
/// names, and every exposed package unless `-hide-all-packages`.
/// The import directories of the packages in scope, and their include directories (which the C
/// preprocessor searches, as GHC's does).
fn package_dirs(o: &Opts) -> Result<(Vec<PathBuf>, Vec<PathBuf>, Vec<String>), String> {
    let stack = pkg::Stack::load(&db_stack(o))?;
    let mut roots: Vec<String> = o.package_ids.clone();
    for n in &o.package_names {
        let p = stack.by_name(n).ok_or_else(|| format!("<command line>: cannot satisfy -package {n}"))?;
        roots.push(p.id());
    }
    if !o.hide_all {
        for db in &stack.dbs {
            for p in &db.packages {
                if p.get("exposed").is_none_or(|e| e.eq_ignore_ascii_case("true")) {
                    roots.push(p.id());
                }
            }
        }
    }
    // A package being built does not see an older registration of itself.
    if let Some(me) = &o.this_unit {
        roots.retain(|r| r != me);
    }
    let pkgs = stack.closure(&roots)?;
    // The packages in scope (not their dependencies) have MIN_VERSION_ macros.
    let mut visible = Vec::new();
    for r in &roots {
        if let Some(p) = stack.by_id(r) {
            let flag = format!("-hsc-pkgver={}:{}", p.name(), p.version());
            if !visible.contains(&flag) {
                visible.push(flag);
            }
        }
    }
    // Reexported modules (`Happy.Paths from unit:Paths_happy_lib`), of every package the program
    // uses (an installed module may import its package's dependency's reexport): the name, the
    // module it is, and that unit's import directory (`driver::REEXPORT_FLAG`).
    for p in &pkgs {
        for (name, unit, real) in p.reexports() {
            let Some(dir) = stack.by_id(&unit).and_then(|u| u.list("import-dirs").into_iter().next()) else { continue };
            let flag = format!("{}{name}={real}={dir}", crate::driver::REEXPORT_FLAG);
            if !visible.contains(&flag) {
                visible.push(flag);
            }
        }
    }
    let mut dirs = Vec::new();
    let mut includes = Vec::new();
    // The packages named directly come before their dependencies in the search.
    for p in pkgs.iter().rev() {
        for d in p.list("import-dirs") {
            let d = PathBuf::from(d);
            if !dirs.contains(&d) {
                dirs.push(d);
            }
        }
        for d in p.list("include-dirs") {
            let d = PathBuf::from(d);
            if !includes.contains(&d) {
                includes.push(d);
            }
        }
        // A package built by a build tool keeps its C code in its library archive.
        for lib in p.list("hs-libraries") {
            for d in p.list("library-dirs") {
                let a = PathBuf::from(&d).join(format!("lib{lib}.a"));
                let flag = format!("-hsc-archive={}", a.display());
                if a.is_file() && !visible.contains(&flag) {
                    visible.push(flag);
                    break;
                }
            }
        }
        for lib in p.list("extra-libraries") {
            let flag = format!("-hsc-lib={lib}");
            if !visible.contains(&flag) {
                visible.push(flag);
            }
        }
    }
    Ok((dirs, includes, visible))
}

/// An input that names a source file (rather than a module).
fn is_file_input(s: &str) -> bool {
    s.ends_with(".hs") || s.ends_with(".lhs") || s.contains('/')
}

fn is_c_input(s: &str) -> bool {
    [".c", ".cc", ".cpp", ".cxx", ".s", ".S"].iter().any(|e| s.ends_with(e))
}

/// The object file of a module or input in the output directory: `dir/A/B.o`.
fn out_path(dir: &Option<PathBuf>, module: &str, source: &Path, suffix: &str) -> PathBuf {
    match dir {
        Some(d) => d.join(format!("{}.{suffix}", module.replace('.', "/"))),
        None => source.with_extension(suffix),
    }
}

/// An empty object file (written where GHC writes a module's object code: hsc keeps the code of a
/// package in its sources until a program is built).
fn empty_object(path: &Path) -> Result<(), String> {
    if path.is_file() {
        return Ok(());
    }
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
    }
    let st = std::process::Command::new("gcc")
        .args(["-c", "-x", "c", "/dev/null", "-o"])
        .arg(path)
        .status();
    if !st.is_ok_and(|s| s.success()) {
        std::fs::write(path, b"").map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(())
}

/// Runs a compilation. Answers the exit status.
pub fn compile(o: &Opts) -> Result<i32, String> {
    // C inputs go to the C compiler.
    let c_inputs: Vec<&String> = o.inputs.iter().filter(|i| is_c_input(i)).collect();
    for c in &c_inputs {
        // A C file's object keeps its path under the output directory (`cbits/a.c` gives
        // `odir/cbits/a.o`), as GHC places it.
        let out = match (&o.output, o.no_link, &o.odir) {
            (Some(out), true, _) if c_inputs.len() == 1 => PathBuf::from(out),
            (_, _, Some(dir)) if Path::new(c.as_str()).is_relative() => dir.join(Path::new(c.as_str()).with_extension(&o.osuf)),
            _ => Path::new(c.as_str()).with_extension(&o.osuf),
        };
        if let Some(d) = out.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
        }
        let st = std::process::Command::new("gcc")
            .arg("-c")
            .arg(c.as_str())
            .arg("-o")
            .arg(&out)
            .args(&o.c_opts)
            .args(o.flags.iter().filter(|f| f.starts_with("-I") || f.starts_with("-D")))
            // The RTS headers (HsFFI.h), as GHC passes its own include directory.
            .arg(format!("-I{}", driver::default_lib_dir().join("include").display()))
            .status()
            .map_err(|e| format!("gcc: {e}"))?;
        if !st.success() {
            return Ok(1);
        }
    }
    let hs_inputs: Vec<&String> = o.inputs.iter().filter(|i| !is_c_input(i) && !i.ends_with(".o") && !i.ends_with(".a")).collect();
    if hs_inputs.is_empty() {
        if c_inputs.is_empty() {
            return Err("no input files".into());
        }
        return Ok(0);
    }
    if o.shared {
        // A shared library of a package: hsc keeps a package's code in its sources, so the library is
        // an empty object.
        if let Some(out) = &o.output {
            empty_object(Path::new(out))?;
        }
        return Ok(0);
    }
    let search: Vec<PathBuf> = match &o.search {
        Some(dirs) => dirs.clone(),
        None => vec![PathBuf::from(".")],
    };
    if o.abi_hash {
        return abi_hash(o, &search, &hs_inputs);
    }
    let mut with_env;
    let o = if o.package_env.as_deref() != Some("-") {
        with_env = o.clone();
        apply_environment(&mut with_env)?;
        &with_env
    } else {
        o
    };
    let (pkg_dirs, pkg_includes, visible) = package_dirs(o)?;
    // The packages' include directories come after the command line's; the versions of the
    // packages in scope are the preprocessor's MIN_VERSION_ macros.
    let with_includes;
    let o = {
        let mut c = o.clone();
        c.flags.extend(pkg_includes.iter().map(|d| format!("-I{}", d.display())));
        c.flags.extend(visible);
        with_includes = c;
        &with_includes
    };
    // The modules to build: the files given, and the modules named (found in the search path).
    let mut targets: Vec<(String, PathBuf)> = Vec::new();
    for inp in &hs_inputs {
        if is_file_input(inp) {
            let path = PathBuf::from(inp.as_str());
            if !path.is_file() {
                return Err(format!("can't find file: {inp}"));
            }
            targets.push((driver::module_name_of(&path, &o.flags)?, path));
        } else {
            let mut d = driver::Driver::new(search.clone(), pkg_dirs.clone(), o.flags.clone());
            // The program's module of that name, before a package's (see `home_wired_key`).
            let key = d.home_wired_key(inp).unwrap_or_else(|| inp.to_string());
            let f = d.find_module(&key).filter(|f| f.home).ok_or_else(|| {
                format!("<no location info>: error:\n    Could not find module ‘{inp}’\n    It is not a module in the current program, or in any known package.")
            })?;
            targets.push((inp.to_string(), f.path));
        }
    }
    let main_target = targets.iter().position(|(m, _)| m == "Main");
    let link = !o.no_link && main_target.is_some();
    // A program: its sources are compiled together, from its main module. A file given with its own
    // directory is searched there too, as GHC does when no -i is given.
    if link {
        let (_, main_path) = targets[main_target.unwrap_or(0)].clone();
        let mut search = search.clone();
        if o.search.is_none() {
            if let Some(dir) = main_path.parent().filter(|d| !d.as_os_str().is_empty()) {
                search.insert(0, dir.to_path_buf());
            }
        }
        let exe = match &o.output {
            Some(out) => PathBuf::from(out),
            None => main_path.with_extension(""),
        };
        return build_program(o, search, pkg_dirs, &main_path, &exe, &targets);
    }
    // Modules of a package: each is checked (with everything it imports) and gets its interface.
    let mut d = driver::Driver::new(search.clone(), pkg_dirs, o.flags.clone());
    for (m, path) in &targets {
        if m == "Main" {
            driver::compile_with(&mut d, path, &o.flags)?;
        } else {
            let key = d.home_wired_key(m).unwrap_or_else(|| m.clone());
            d.load(&key)?;
        }
    }
    for (m, path, home) in d.sources.clone() {
        if !home {
            continue;
        }
        write_outputs(o, d.real_name(&m), &path)?;
    }
    Ok(0)
}

/// `--abi-hash`: a hash of the modules' interfaces. An interface of hsc is its module's source and
/// flags, so the hash is over those (the interface files written in `hidir`, or the sources).
fn abi_hash(o: &Opts, search: &[PathBuf], modules: &[&String]) -> Result<i32, String> {
    let hi_dir = o.hidir.clone().or_else(|| o.odir.clone());
    let d = driver::Driver::new(search.to_vec(), Vec::new(), o.flags.clone());
    let mut ctx = [0u8; 88];
    crate::interp::md5::init(&mut ctx);
    for m in modules {
        let iface = hi_dir.as_ref().map(|h| h.join(format!("{}.{}", m.replace('.', "/"), o.hisuf)));
        let source = match iface.as_ref().and_then(|i| driver::read_interface(i)) {
            Some((src, _)) => src,
            None => d.find_module(m).map(|f| f.path).ok_or_else(|| format!("Could not find module ‘{m}’"))?,
        };
        let text = std::fs::read(&source).map_err(|e| format!("{}: {e}", source.display()))?;
        crate::interp::md5::update(&mut ctx, m.as_bytes());
        crate::interp::md5::update(&mut ctx, &text);
    }
    for f in &o.flags {
        crate::interp::md5::update(&mut ctx, f.as_bytes());
    }
    let digest = crate::interp::md5::finish(&mut ctx);
    println!("{}", digest.iter().map(|b| format!("{b:02x}")).collect::<String>());
    Ok(0)
}

/// The interface and object files of a module of the program.
fn write_outputs(o: &Opts, module: &str, path: &Path) -> Result<(), String> {
    let abs = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let hi_dir = o.hidir.clone().or_else(|| o.odir.clone());
    driver::write_interface(&out_path(&hi_dir, module, path, &o.hisuf), module, &abs, path, &o.flags)?;
    empty_object(&out_path(&o.odir, module, path, &o.osuf))?;
    if o.dynamic_too {
        driver::write_interface(&out_path(&hi_dir, module, path, &o.dynhisuf), module, &abs, path, &o.flags)?;
        empty_object(&out_path(&o.odir, module, path, &o.dynosuf))?;
    }
    Ok(())
}

/// `runghc`: compiles the program rooted at `main` with the options' packages and runs it in the
/// interpreter, with `args` as its arguments. Answers its exit code.
pub fn run_program(o: &Opts, main: &Path, args: Vec<String>) -> Result<i32, String> {
    let mut search: Vec<PathBuf> = match &o.search {
        Some(dirs) => dirs.clone(),
        None => vec![PathBuf::from(".")],
    };
    if o.search.is_none() {
        if let Some(dir) = main.parent().filter(|d| !d.as_os_str().is_empty()) {
            search.insert(0, dir.to_path_buf());
        }
    }
    let mut with_env = o.clone();
    if o.package_env.as_deref() != Some("-") {
        apply_environment(&mut with_env)?;
    }
    let (pkg_dirs, pkg_includes, visible) = package_dirs(&with_env)?;
    let mut o = with_env;
    o.flags.extend(pkg_includes.iter().map(|d| format!("-I{}", d.display())));
    o.flags.extend(visible);
    let mut d = driver::Driver::new(search, pkg_dirs, o.flags.clone());
    let binds = driver::compile_with(&mut d, main, &o.flags)?;
    let prog_name = main.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "main".into());
    crate::ccall::set_program_args(std::iter::once(prog_name.clone()).chain(args).collect());
    Ok(driver::run_binds_code(binds, prog_name))
}

/// Builds the executable of a program, unless it is up to date: the record beside it lists the
/// sources it was built from (with their sizes and modification times) and the flags.
fn build_program(
    o: &Opts,
    search: Vec<PathBuf>,
    pkg_dirs: Vec<PathBuf>,
    main_path: &Path,
    exe: &Path,
    targets: &[(String, PathBuf)],
) -> Result<i32, String> {
    let record_path = {
        let name = exe.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        match &o.odir {
            Some(d) => d.join(format!("{name}.hsc-deps")),
            None => exe.with_file_name(format!(".{name}.hsc-deps")),
        }
    };
    let mut key = format!("hsc {VERSION} {}\nnative {}\n", env!("CARGO_PKG_VERSION"), o.native);
    for f in &o.flags {
        key.push_str(&format!("flag {f}\n"));
    }
    for d in search.iter().chain(pkg_dirs.iter()) {
        key.push_str(&format!("dir {}\n", d.display()));
    }
    key.push_str(&format!("compiler {}\n", stamp(&std::env::current_exe().unwrap_or_default())));
    if exe.is_file() {
        if let Ok(old) = std::fs::read_to_string(&record_path) {
            if let Some(rest) = old.strip_prefix(&key) {
                let fresh = rest.lines().all(|l| match l.split_once(' ') {
                    Some((st, p)) => stamp(Path::new(p)) == st,
                    None => false,
                });
                if fresh {
                    return Ok(0);
                }
            }
        }
    }
    let mut d = driver::Driver::new(search, pkg_dirs, o.flags.clone());
    let binds = driver::compile_with(&mut d, main_path, &o.flags)?;
    // The program's own modules get their interface and object files, as GHC writes them.
    for (m, path, home) in d.sources.clone() {
        if home && (o.odir.is_some() || o.hidir.is_some()) {
            write_outputs(o, &m, &path)?;
        }
    }
    let _ = targets;
    if o.dump_core {
        for b in &binds {
            println!("{} = {}", b.name, crate::core::pretty::show(&b.body));
        }
        return Ok(0);
    }
    if let Some(dir) = exe.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let out = exe.display().to_string();
    let mut c_sources = d.c_sources.clone();
    // The capi wrappers include the packages' headers: their include directories are searched.
    let mut capi_flags = d.c_flags.clone();
    capi_flags.extend(o.flags.iter().filter(|f| f.starts_with("-I")).cloned());
    if let Some(stub) = driver::capi_source(&binds, &capi_flags)? {
        c_sources.push(stub);
    }
    // Objects and archives named on the command line (a build tool's compiled C sources).
    for inp in &o.inputs {
        if inp.ends_with(".o") || inp.ends_with(".a") {
            c_sources.push((PathBuf::from(inp), Vec::new()));
        }
    }
    // The C code of the packages' library archives, and the C libraries they need.
    for f in &o.flags {
        if let Some(a) = f.strip_prefix("-hsc-archive=") {
            c_sources.push((PathBuf::from(a), Vec::new()));
        } else if let Some(l) = f.strip_prefix("-hsc-lib=") {
            c_sources.push((PathBuf::from(format!("-l{l}")), Vec::new()));
        }
    }
    // The RTS functions library C code calls (include/Rts.h) and base's C helpers: their (weak)
    // definitions, in every program.
    c_sources.push((driver::default_lib_dir().join("rts").join("hsc-rts-shim.c"), Vec::new()));
    let written = if o.native {
        driver::write_native_with(&out, &binds, &c_sources)
    } else {
        // The interpreter loads the C sources as a shared library named in the program.
        let mut binds = binds;
        if let Some(lib) = driver::c_sources_library(&c_sources)? {
            binds.push(crate::core::TopBind {
                name: driver::CBITS_BIND.into(),
                body: crate::core::Core::Lit(crate::core::Lit::Str(lib.display().to_string())),
            });
        }
        write_executable(&out, &binds).map_err(|e| e.to_string())
    };
    written.map_err(|e| format!("hsc: cannot write {out}: {e}"))?;
    let mut record = key;
    for (_, p, _) in &d.sources {
        record.push_str(&format!("{} {}\n", stamp(p), p.display()));
    }
    if let Some(dir) = record_path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(&record_path, record);
    Ok(0)
}

/// A file's size and modification time, as one word.
fn stamp(p: &Path) -> String {
    match std::fs::metadata(p) {
        Ok(m) => {
            let t = m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_nanos()).unwrap_or(0);
            format!("{}:{t}", m.len())
        }
        Err(_) => "missing".into(),
    }
}

/// The magic trailer of an executable that carries its program.
pub const MAGIC: &[u8; 8] = b"HSCPROG1";

/// Writes an executable: this binary, then the serialised program, its length, and a magic trailer.
pub fn write_executable(path: &str, binds: &[crate::core::TopBind]) -> std::io::Result<()> {
    let me = std::env::current_exe()?;
    let mut bytes = std::fs::read(me)?;
    if let Some(start) = payload_offset(&bytes) {
        bytes.truncate(start);
    }
    let payload = crate::core::serial::encode(binds);
    bytes.extend_from_slice(&payload);
    bytes.extend_from_slice(&(payload.len() as u64).to_le_bytes());
    bytes.extend_from_slice(MAGIC);
    // Written beside and renamed, so that a running copy of the program is not disturbed.
    let tmp = format!("{path}.hsc-tmp");
    std::fs::write(&tmp, &bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))?;
    }
    std::fs::rename(&tmp, path)
}

/// Where an embedded payload starts, if this file has one.
pub fn payload_offset(bytes: &[u8]) -> Option<usize> {
    if bytes.len() < 16 || &bytes[bytes.len() - 8..] != MAGIC {
        return None;
    }
    let len_bytes: [u8; 8] = bytes[bytes.len() - 16..bytes.len() - 8].try_into().ok()?;
    let len = u64::from_le_bytes(len_bytes) as usize;
    (bytes.len() - 16).checked_sub(len)
}

