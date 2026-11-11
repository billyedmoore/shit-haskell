//! The driver: finds and loads modules, runs the renamer and type checker over them in
//! dependency order, and assembles the program's Core.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};

use crate::core::TopBind;
use crate::rename::iface::Iface;
use crate::syntax::ast::{Decl, Module};
use crate::syntax::extensions::Extensions;
use crate::tc::Checker;

pub struct Driver {
    /// The directories of the program's own modules (`-i`), searched first.
    pub search: Vec<PathBuf>,
    /// The import directories of the visible packages: each holds a module's source, or the
    /// interface file that names its source (see `Iface pointer` in `write_interface`).
    pub pkg_dirs: Vec<PathBuf>,
    pub ifaces: BTreeMap<String, Iface>,
    pub checker: Checker,
    pub binds: Vec<TopBind>,
    loaded: BTreeSet<String>,
    loading: Vec<String>,
    /// Modules imported through `{-# SOURCE #-}`, known so far by their `.hs-boot` file.
    booted: BTreeSet<String>,
    /// The flags of the program's own modules.
    flags: Vec<String>,
    /// Every module loaded: its name, source file, and whether it is one of the program's own.
    pub sources: Vec<(String, PathBuf, bool)>,
    /// The C sources of the packages the program uses, with the C compiler's flags for each.
    pub c_sources: Vec<(PathBuf, Vec<String>)>,
    /// The C include and macro flags of every package module loaded (for the capi wrappers).
    pub c_flags: Vec<String>,
}

/// Where a module was found: its source, the flags it is compiled with, and whether it is one of
/// the program's own modules (found in a `-i` directory).
pub struct Found {
    pub path: PathBuf,
    pub flags: Vec<String>,
    pub home: bool,
}

/// The import directories of the packages in the global database (every one is exposed).
pub fn default_package_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(db) = crate::pkg::Db::load(&crate::pkg::global_db_path()) {
        for p in &db.packages {
            for d in p.list("import-dirs") {
                let d = PathBuf::from(d);
                if !dirs.contains(&d) {
                    dirs.push(d);
                }
            }
        }
    }
    dirs
}

/// The flags of a package's modules found by source in an import directory: its
/// `hsc-package.flags` file, one flag per line; relative `-I` paths are relative to the directory.
pub fn package_flags(dir: &Path) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(dir.join("hsc-package.flags")) else { return Vec::new() };
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|l| {
            if let Some(rel) = l.strip_prefix("-I").filter(|r| !Path::new(r).is_absolute()) {
                return format!("-I{}", dir.join(rel).display());
            }
            if let Some(rel) = l.strip_prefix("-c-source=").filter(|r| !Path::new(r).is_absolute()) {
                return format!("-c-source={}", dir.join(rel).display());
            }
            l.to_string()
        })
        .collect()
}

/// The first line of an interface file written by hsc.
pub const IFACE_MAGIC: &str = "hsc-interface 2";

/// The flag that marks a source as already preprocessed (the text an interface carries).
pub const PREPROCESSED_FLAG: &str = "-hsc-preprocessed";

/// hsc's cache directory for `what` (under `$XDG_CACHE_HOME/hsc` or `~/.cache/hsc`).
pub fn cache_dir(what: &str) -> PathBuf {
    std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .unwrap_or_else(std::env::temp_dir)
        .join("hsc")
        .join(what)
}

fn md5_hex(bytes: &[u8]) -> String {
    let mut ctx = [0u8; 88];
    crate::interp::md5::init(&mut ctx);
    crate::interp::md5::update(&mut ctx, bytes);
    crate::interp::md5::finish(&mut ctx).iter().map(|b| format!("{b:02x}")).collect()
}

/// Reads an interface file written by `write_interface`: the module's source and its flags. The
/// interface carries the module's preprocessed text (and its boot file's), since the package's
/// build directory is gone once it is installed: the text is put in a cache file named by its
/// contents, whose path is answered.
pub fn read_interface(path: &Path) -> Option<(PathBuf, Vec<String>)> {
    let bytes = std::fs::read(path).ok()?;
    let mut pos = 0usize;
    let mut line = |pos: &mut usize| -> Option<String> {
        let rest = bytes.get(*pos..)?;
        let end = rest.iter().position(|&b| b == b'\n')?;
        let l = String::from_utf8_lossy(&rest[..end]).into_owned();
        *pos += end + 1;
        Some(l)
    };
    if line(&mut pos)? != IFACE_MAGIC {
        return None;
    }
    let (mut module, mut source, mut flags) = (String::new(), None, Vec::new());
    let (mut text, mut boot): (Option<Vec<u8>>, Option<Vec<u8>>) = (None, None);
    while let Some(l) = line(&mut pos) {
        if let Some(v) = l.strip_prefix("module: ") {
            module = v.to_string();
        } else if let Some(v) = l.strip_prefix("source: ") {
            source = Some(PathBuf::from(v));
        } else if let Some(v) = l.strip_prefix("flag: ") {
            flags.push(v.to_string());
        } else if let Some(v) = l.strip_prefix("text: ").or_else(|| l.strip_prefix("boot: ")) {
            let n: usize = v.parse().ok()?;
            let chunk = bytes.get(pos..pos + n)?.to_vec();
            pos += n;
            if l.starts_with("text") { text = Some(chunk) } else { boot = Some(chunk) }
        }
    }
    let source = source?;
    let Some(text) = text else { return Some((source, flags)) };
    // The cached copy keeps the module's file name (so it is found as the module) and extension.
    let ext = source.extension().and_then(|e| e.to_str()).unwrap_or("hs").to_string();
    let mut key = text.clone();
    key.extend_from_slice(boot.as_deref().unwrap_or_default());
    let dir = cache_dir("ifaces").join(md5_hex(&key));
    let file = dir.join(format!("{}.{ext}", module.replace('.', "/")));
    if !file.is_file() {
        std::fs::create_dir_all(file.parent()?).ok()?;
        if let Some(b) = &boot {
            std::fs::write(file.with_extension("hs-boot"), b).ok()?;
        }
        // Written beside and renamed, for builds running at once.
        let tmp = file.with_extension(format!("tmp{}", std::process::id()));
        std::fs::write(&tmp, &text).ok()?;
        std::fs::rename(&tmp, &file).ok()?;
    }
    flags.push(PREPROCESSED_FLAG.to_string());
    Some((file, flags))
}

/// Writes the interface file of a module of a package being built. hsc compiles programs from
/// source, so the interface holds the module's preprocessed text and the flags it is compiled
/// with (those of the preprocessor dropped). The file is left alone when it already says the same,
/// so that its time stamp shows when the module last changed.
pub fn write_interface(path: &Path, module: &str, source: &Path, flags: &[String]) -> Result<(), String> {
    let pre = |p: &Path| -> Result<String, String> {
        let src = read_source(p).map_err(|e| format!("{}: {e}", p.display()))?;
        preprocess_if_needed(src, p, flags)
    };
    let text = pre(source)?;
    let boot_path = source.with_extension("hs-boot");
    let boot = if boot_path.is_file() { Some(pre(&boot_path)?) } else { None };
    let mut out = format!("{IFACE_MAGIC}\nmodule: {module}\nsource: {}\n", source.display());
    for f in flags {
        let cpp = f == "-cpp" || f == "-XCPP" || f.starts_with("-I") || f.starts_with("-D") || f.starts_with("-U")
            || f.starts_with("-include=") || f.starts_with("-optP") || f.starts_with("-hsc-pkgver=")
            || f.starts_with("-hsc-archive=") || f.starts_with("-hsc-lib=");
        if !cpp {
            out.push_str(&format!("flag: {f}\n"));
        }
    }
    out.push_str(&format!("text: {}\n{text}", text.len()));
    if let Some(b) = boot {
        out.push_str(&format!("boot: {}\n{b}", b.len()));
    }
    if std::fs::read_to_string(path).is_ok_and(|t| t == out) {
        return Ok(());
    }
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
    }
    std::fs::write(path, out).map_err(|e| format!("{}: {e}", path.display()))
}

/// The default library directory: `libs/` at the project root.
pub fn default_lib_dir() -> PathBuf {
    let dir = match std::env::var("HSC_LIBDIR") {
        Ok(dir) => PathBuf::from(dir),
        Err(_) => PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../libs"),
    };
    std::fs::canonicalize(&dir).unwrap_or(dir)
}

impl Driver {
    /// A driver whose own modules are in `search` and whose packages' modules are in `pkg_dirs`.
    pub fn new(search: Vec<PathBuf>, pkg_dirs: Vec<PathBuf>, flags: Vec<String>) -> Driver {
        Driver {
            search,
            pkg_dirs,
            ifaces: BTreeMap::new(),
            checker: Checker::new(),
            binds: Vec::new(),
            loaded: BTreeSet::new(),
            booted: BTreeSet::new(),
            loading: Vec::new(),
            flags,
            sources: Vec::new(),
            c_sources: Vec::new(),
            c_flags: Vec::new(),
        }
    }

    /// Finds a module: among the program's own first, then in the packages (by interface file or
    /// by source).
    pub fn find_module(&self, module: &str) -> Option<Found> {
        let stem = module.replace('.', "/");
        for d in &self.search {
            for ext in ["hs", "lhs"] {
                let p = d.join(format!("{stem}.{ext}"));
                if p.is_file() {
                    return Some(Found { path: p, flags: self.flags.clone(), home: true });
                }
            }
            // A `.hsc` module is translated by hsc2hs first.
            let p = d.join(format!("{stem}.hsc"));
            if p.is_file() {
                if let Ok(hs) = crate::hsc2hs::translate(&p, &self.flags) {
                    return Some(Found { path: hs, flags: self.flags.clone(), home: true });
                }
            }
        }
        for d in &self.pkg_dirs {
            if let Some((path, flags)) = read_interface(&d.join(format!("{stem}.hi"))) {
                if path.is_file() {
                    return Some(Found { path, flags, home: false });
                }
            }
            let p = d.join(format!("{stem}.hs"));
            if p.is_file() {
                return Some(Found { path: p, flags: package_flags(d), home: false });
            }
            let p = d.join(format!("{stem}.hsc"));
            if p.is_file() {
                let flags = package_flags(d);
                match crate::hsc2hs::translate(&p, &flags) {
                    Ok(hs) => return Some(Found { path: hs, flags, home: false }),
                    Err(e) => eprintln!("{e}"),
                }
            }
        }
        None
    }

    fn find_source(&self, module: &str) -> Option<PathBuf> {
        self.find_module(module).map(|f| f.path)
    }

    fn find_boot(&self, module: &str) -> Option<PathBuf> {
        let rel = format!("{}.hs-boot", module.replace('.', "/"));
        // A package module's boot file is beside its source.
        if let Some(f) = self.find_module(module) {
            let p = f.path.with_extension("hs-boot");
            if p.is_file() {
                return Some(p);
            }
        }
        self.search.iter().chain(self.pkg_dirs.iter()).map(|d| d.join(&rel)).find(|p| p.is_file())
    }

    /// Loads the `.hs-boot` file of a module imported through `{-# SOURCE #-}`: its signatures
    /// stand for the module until the module itself is loaded (see `load_booted`).
    fn load_boot(&mut self, module: &str) -> Result<(), String> {
        if self.loaded.contains(module) || self.booted.contains(module) {
            return Ok(());
        }
        let path = self
            .find_boot(module)
            .ok_or_else(|| format!("Could not find module ‘{module}’ (no .hs-boot file for a SOURCE import)"))?;
        let flags = self.find_module(module).map(|f| f.flags).unwrap_or_default();
        let src = read_source(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let src = preprocess_if_needed(src, &path, &flags)?;
        let (mut parsed, exts) = crate::syntax::parse_module_source(&src, &flags).map_err(|e| {
            format!("{}:{}:{}: error:\n    {}", path.display(), e.line, e.col, e.msg)
        })?;
        strip_boot_bodies(&mut parsed);
        let source = Self::source_imports(&src);
        for imp in parsed.imports.clone() {
            if source.contains(&imp.module) {
                self.load_boot(&imp.module)?;
            } else {
                self.load(&imp.module)?;
            }
        }
        self.checker.boot = true;
        let result = self.compile_module(parsed, exts, &path);
        self.checker.boot = false;
        result?;
        self.booted.insert(module.to_string());
        Ok(())
    }

    /// After the program is loaded: every module that was only known through its boot file is
    /// loaded for real, so that its definitions are compiled.
    fn load_booted(&mut self) -> Result<(), String> {
        loop {
            let pending: Vec<String> = self.booted.iter().filter(|m| !self.loaded.contains(*m)).cloned().collect();
            if pending.is_empty() {
                return Ok(());
            }
            for m in pending {
                self.load(&m)?;
            }
        }
    }

    /// The modules named in `import {-# SOURCE #-} M` lines of a source file.
    fn source_imports(src: &str) -> HashSet<String> {
        let mut out = HashSet::new();
        for line in src.lines() {
            let Some(rest) = line.strip_prefix("import") else { continue };
            let rest = rest.trim_start();
            let Some(rest) = rest.strip_prefix("{-#") else { continue };
            let Some(rest) = rest.trim_start().strip_prefix("SOURCE") else { continue };
            let Some(rest) = rest.trim_start().strip_prefix("#-}") else { continue };
            let mut rest = rest.trim_start();
            if let Some(r) = rest.strip_prefix("qualified") {
                rest = r.trim_start();
            }
            // The module name is what follows; an import list may come directly after it.
            let name: String = rest.chars().take_while(|c| c.is_alphanumeric() || *c == '.' || *c == '_').collect();
            if !name.is_empty() {
                out.insert(name);
            }
        }
        out
    }

    /// Loads a module by name, first loading its imports.
    pub fn load(&mut self, module: &str) -> Result<(), String> {
        if self.loaded.contains(module) {
            return Ok(());
        }
        if self.loading.iter().any(|m| m == module) {
            return Err(format!("Module imports form a cycle: {}", self.loading.join(" -> ")));
        }
        let found = self.find_module(module).ok_or_else(|| format!("Could not find module ‘{module}’"))?;
        let (path, flags, home) = (found.path, found.flags, found.home);
        // A package module brings the package's C sources into the program.
        let c_flags: Vec<String> = flags.iter().filter(|f| f.starts_with("-I") || f.starts_with("-D") || f.starts_with("-cc-option=")).cloned().collect();
        for f in &c_flags {
            if !self.c_flags.contains(f) {
                self.c_flags.push(f.clone());
            }
        }
        for f in &flags {
            if let Some(src) = f.strip_prefix("-c-source=") {
                let p = PathBuf::from(src);
                if !self.c_sources.iter().any(|(q, _)| *q == p) {
                    self.c_sources.push((p, c_flags.clone()));
                }
            }
        }
        let src = read_source(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let src = preprocess_if_needed(src, &path, &flags)?;
        let (parsed, mut exts) = crate::syntax::parse_module_source(&src, &flags).map_err(|e| {
            format!("{}:{}:{}: error:\n    {}", path.display(), e.line, e.col, e.msg)
        })?;
        // The GHC packages compile with NoImplicitPrelude as a package default (their cabal files).
        let path_text = path.to_string_lossy();
        if ["/libs/ghc-internal/", "/libs/ghc-prim/", "/libs/ghc-bignum/", "/libs/base/"].iter().any(|p| path_text.contains(p)) {
            exts.apply("NoImplicitPrelude")?;
        }
        self.loading.push(module.to_string());
        let mut deps: Vec<String> = parsed.imports.iter().map(|i| i.module.clone()).collect();
        if exts.is_on("OverloadedStrings") && module != "GHC.Internal.Data.String" && !path_text.contains("/libs/ghc-prim/") {
            deps.push("GHC.Internal.Data.String".into());
        }
        if exts.is_on("ImplicitPrelude") && module != "Prelude" && !deps.iter().any(|d| d == "Prelude") {
            deps.insert(0, "Prelude".into());
        }
        let source = Self::source_imports(&src);
        for d in &deps {
            let loaded = if source.contains(d) { self.load_boot(d) } else { self.load(d) };
            if let Err(e) = loaded {
                return Err(match self.find_source(d) {
                    Some(_) => e,
                    None => format!("{}:{}:{}: error:\n    {}", path.display(), parsed.loc.line, parsed.loc.col, e),
                });
            }
        }
        self.loading.pop();
        self.compile_module(parsed, exts, &path)?;
        self.loaded.insert(module.to_string());
        self.sources.push((module.to_string(), path.clone(), home));
        Ok(())
    }

    fn compile_module(&mut self, m: Module, exts: Extensions, path: &Path) -> Result<(), String> {
        let name = m.name.clone();
        self.checker.user_module = !path.starts_with(default_lib_dir());
        // Call stacks name a library file as GHC's build does (libraries/<package>/...).
        self.checker.source_name = if self.checker.user_module {
            path.display().to_string()
        } else {
            let text = path.display().to_string();
            match text.find("/libs/") {
                Some(i) => format!("libraries/{}", &text[i + "/libs/".len()..]),
                None => text,
            }
        };
        let located = |loc: crate::syntax::ast::Loc, msg: &str| {
            format!("{}:{}:{}: error:\n    {}", path.display(), loc.line, loc.col, msg)
        };
        let boot = self.checker.boot;
        self.checker.overloaded_strings = exts.is_on("OverloadedStrings");
        let (renamed, iface) = crate::rename::rename_module(m, &self.ifaces, exts.is_on("ImplicitPrelude"), boot)
            .map_err(|e| located(e.loc, &e.msg))?;
        let binds = self.checker.check_module(&renamed).map_err(|e| located(e.loc, &e.msg))?;
        self.binds.extend(binds);
        self.ifaces.insert(name, iface);
        Ok(())
    }
}

/// Compiles a program rooted at `main_file` with the vendored packages, returning its Core.
pub fn compile_program(main_file: &Path, flags: Vec<String>) -> Result<Vec<TopBind>, String> {
    let dir = main_file.parent().unwrap_or(Path::new(".")).to_path_buf();
    let mut d = Driver::new(vec![dir], default_package_dirs(), flags.clone());
    compile_with(&mut d, main_file, &flags)
}

/// Compiles the program rooted at `main_file` with a driver set up for its search path and
/// packages, returning all its Core bindings. The main module's flags are `flags`.
pub fn compile_with(d: &mut Driver, main_file: &Path, flags: &[String]) -> Result<Vec<TopBind>, String> {
    let src = read_source(main_file).map_err(|e| format!("{}: {e}", main_file.display()))?;
    let src = preprocess_if_needed(src, main_file, flags)?;
    let (parsed, exts) = crate::syntax::parse_module_source(&src, flags).map_err(|e| {
        format!("{}:{}:{}: error:\n    {}", main_file.display(), e.line, e.col, e.msg)
    })?;
    let mut deps: Vec<String> = parsed.imports.iter().map(|i| i.module.clone()).collect();
    if exts.is_on("ImplicitPrelude") && !deps.iter().any(|x| x == "Prelude") {
        deps.insert(0, "Prelude".into());
    }
    if exts.is_on("OverloadedStrings") {
        deps.push("GHC.Internal.Data.String".into());
    }
    let source = Driver::source_imports(&src);
    for dep in &deps {
        if source.contains(dep) {
            d.load_boot(dep)?;
        } else {
            d.load(dep)?;
        }
    }
    // A program on the real base runs under its top handler, which GHC links into every program.
    let real_base = d.loaded.iter().any(|m| m.starts_with("GHC.Internal."));
    if real_base && !d.loaded.contains("GHC.Internal.TopHandler") {
        d.load("GHC.Internal.TopHandler")?;
    }
    let name = parsed.name.clone();
    d.compile_module(parsed, exts, main_file)?;
    d.sources.push((name.clone(), main_file.to_path_buf(), true));
    d.load_booted()?;
    if name == "Main" && !d.checker.env.values.contains_key("Main.main") {
        return Err(format!(
            "{}:1:1: error:\n    The IO action ‘main’ is not defined in module ‘Main’",
            main_file.display()
        ));
    }
    let mut binds = std::mem::take(&mut d.binds);
    if flags.iter().any(|f| f == "-dcore-lint") {
        crate::core::lint::lint_program(&binds).map_err(|e| format!("Core lint error: {e}"))?;
    }
    crate::core::opt::inline_prim_wrappers(&mut binds);
    crate::core::opt::specialise_literals(&mut binds);
    crate::core::opt::resolve_known_methods(&mut binds);
    Ok(binds)
}

/// The name a source file gives its module (from its `module` line; `Main` when it has none).
pub fn module_name_of(path: &Path, flags: &[String]) -> Result<String, String> {
    let src = read_source(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let src = preprocess_if_needed(src, path, flags)?;
    let (parsed, _) = crate::syntax::parse_module_source(&src, flags)
        .map_err(|e| format!("{}:{}:{}: error:\n    {}", path.display(), e.line, e.col, e.msg))?;
    Ok(parsed.name)
}

/// Reads a source file. Bytes that are not valid UTF-8 (found in comments of some sources)
/// are replaced rather than rejecting the file.
pub fn read_source(path: &Path) -> std::io::Result<String> {
    let bytes = std::fs::read(path)?;
    Ok(match String::from_utf8(bytes) {
        Ok(s) => s,
        Err(e) => String::from_utf8_lossy(e.as_bytes()).into_owned(),
    })
}

/// Runs the C preprocessor on a source when its `CPP` extension is on. Includes are searched
/// in the source's directory and then in the library's `include/` directory.
pub fn preprocess_if_needed(src: String, path: &Path, flags: &[String]) -> Result<String, String> {
    if flags.iter().any(|f| f == PREPROCESSED_FLAG) || !crate::syntax::cpp::wants_cpp(&src, flags) {
        return Ok(src);
    }
    let dir = path.parent().unwrap_or(Path::new(".")).to_path_buf();
    let mut includes: Vec<PathBuf> = flags.iter().filter_map(|f| f.strip_prefix("-I")).filter(|d| !d.is_empty()).map(PathBuf::from).collect();
    includes.push(default_lib_dir().join("include"));
    let mut pp = crate::syntax::cpp::Preprocessor::new(includes);
    let mut prelude = String::new();
    for f in flags {
        if let Some(def) = f.strip_prefix("-D") {
            let (name, value) = def.split_once('=').unwrap_or((def, "1"));
            pp.define_macro(name, value);
        } else if let Some(name) = f.strip_prefix("-U") {
            pp.undefine(name);
        } else if let Some((name, ver)) = f.strip_prefix("-hsc-pkgver=").and_then(|v| v.split_once(':')) {
            pp.add_package_version(name, ver);
        } else if let Some(file) = f.strip_prefix("-include=") {
            prelude.push_str(&format!("#include \"{file}\"\n"));
        }
    }
    pp.set_file(&path.display().to_string());
    if prelude.is_empty() {
        return pp.run(&src, &dir).map_err(|e| format!("{}: error: {e}", path.display()));
    }
    // The files of `-include` come first; their lines are not part of the output.
    pp.run(&prelude, &dir).map_err(|e| format!("{}: error: {e}", path.display()))?;
    pp.run(&src, &dir).map_err(|e| format!("{}: error: {e}", path.display()))
}

/// A boot file declares; it does not define. Method definitions and instance bodies are removed.
fn strip_boot_bodies(m: &mut Module) {
    for d in &mut m.decls {
        match d {
            Decl::Class(cd) => cd.decls.retain(|x| !matches!(x, Decl::Fun { .. } | Decl::PatBind { .. })),
            Decl::Instance(id) => id.decls.clear(),
            _ => {}
        }
    }
    m.decls.retain(|d| !matches!(d, Decl::Fun { .. } | Decl::PatBind { .. }));
}

/// Runs a compiled program (its Core, decoded) on a thread with a large stack, since evaluation
/// is recursive. Returns the exit code.
pub fn run_binds_code(binds: Vec<crate::core::TopBind>, prog_name: String) -> i32 {
    let handle = std::thread::Builder::new()
        .stack_size(4 << 30)
        .spawn(move || {
            crate::interp::conc::set_stack_base();
            let rt = crate::interp::Runtime::new(binds, prog_name);
            crate::interp::run_main(&rt)
        })
        .expect("spawn interpreter thread");
    // This thread only waits: signals go to the program's threads.
    crate::signals::block_all();
    handle.join().unwrap_or(1)
}

/// Runs the program in a payload (serialised Core) that a compiled program carries.
pub fn run_payload_bytes(payload: Vec<u8>) -> i32 {
    let binds = match crate::core::serial::decode(&payload) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("corrupt program payload: {e}");
            return 1;
        }
    };
    load_cbits(&binds);
    let prog_name = std::env::args()
        .next()
        .and_then(|a| std::path::Path::new(&a).file_name().map(|s| s.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "main".into());
    run_binds_code(binds, prog_name)
}

/// Compiles the C sources of a program's packages into objects in `dir` (`-fPIC`, for a shared
/// library too). Answers the object files.
pub fn compile_c_sources(sources: &[(PathBuf, Vec<String>)], dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    for (i, (src, flags)) in sources.iter().enumerate() {
        // A C library to link (`-lm`) is passed on as it is.
        if src.to_string_lossy().starts_with("-l") {
            out.push(src.clone());
            continue;
        }
        // A library archive: its objects, all of them (the program finds its C functions by name).
        if src.extension().is_some_and(|e| e == "a") {
            let sub = dir.join(format!("a{i}"));
            std::fs::create_dir_all(&sub).map_err(|e| format!("{}: {e}", sub.display()))?;
            let abs = std::fs::canonicalize(src).map_err(|e| format!("{}: {e}", src.display()))?;
            let st = std::process::Command::new("ar").arg("x").arg(&abs).current_dir(&sub).status().map_err(|e| format!("ar: {e}"))?;
            if !st.success() {
                return Err(format!("ar failed on {}", src.display()));
            }
            let mut objs: Vec<PathBuf> = std::fs::read_dir(&sub)
                .map_err(|e| e.to_string())?
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().is_some_and(|e| e == "o"))
                .collect();
            objs.sort();
            out.extend(objs);
            continue;
        }
        let obj = dir.join(format!("c{i}-{}.o", src.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()));
        let mut cmd = std::process::Command::new("gcc");
        cmd.args(["-c", "-O2", "-fPIC", "-w"]).arg(src).arg("-o").arg(&obj);
        cmd.arg(format!("-I{}", default_lib_dir().join("include").display()));
        for f in flags {
            match f.strip_prefix("-cc-option=") {
                Some(o) => {
                    cmd.arg(o);
                }
                None => {
                    cmd.arg(f);
                }
            }
        }
        let st = cmd.status().map_err(|e| format!("gcc: {e}"))?;
        if !st.success() {
            return Err(format!("gcc failed on {}", src.display()));
        }
        out.push(obj);
    }
    Ok(out)
}

/// The shared library of a program's C sources, for the interpreter (cached by their contents and
/// flags under `~/.cache/hsc/cbits`). Answers its path.
pub fn c_sources_library(sources: &[(PathBuf, Vec<String>)]) -> Result<Option<PathBuf>, String> {
    if sources.is_empty() {
        return Ok(None);
    }
    let mut key = String::new();
    for (src, flags) in sources {
        let text = if src.to_string_lossy().starts_with("-l") {
            Vec::new()
        } else {
            std::fs::read(src).map_err(|e| format!("{}: {e}", src.display()))?
        };
        key.push_str(&format!("{}\n{}\n{}\n", src.display(), flags.join(" "), text.len()));
        key.push_str(&String::from_utf8_lossy(&text));
    }
    let mut ctx = [0u8; 88];
    crate::interp::md5::init(&mut ctx);
    crate::interp::md5::update(&mut ctx, key.as_bytes());
    let digest: String = crate::interp::md5::finish(&mut ctx).iter().map(|b| format!("{b:02x}")).collect();
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .unwrap_or_else(std::env::temp_dir)
        .join("hsc")
        .join("cbits");
    let lib = base.join(format!("{digest}.so"));
    if lib.is_file() {
        return Ok(Some(lib));
    }
    let dir = base.join(format!("build-{digest}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let objs = compile_c_sources(sources, &dir)?;
    let tmp = dir.join("lib.so");
    let st = std::process::Command::new("gcc")
        .arg("-shared")
        .args(&objs)
        .arg("-o")
        .arg(&tmp)
        .status()
        .map_err(|e| format!("gcc: {e}"))?;
    if !st.success() {
        return Err("gcc failed to link the C sources".into());
    }
    std::fs::rename(&tmp, &lib).map_err(|e| format!("{}: {e}", lib.display()))?;
    let _ = std::fs::remove_dir_all(&dir);
    Ok(Some(lib))
}

/// The C wrappers of a program's capi imports, as a C source to compile with the program (written
/// in the cache, by its contents), with the flags to compile it with.
pub fn capi_source(binds: &[TopBind], flags: &[String]) -> Result<Option<(PathBuf, Vec<String>)>, String> {
    fn walk(c: &crate::core::Core, out: &mut Vec<String>) {
        use crate::core::{Alt, Core};
        match c {
            Core::Prim(n, args) => {
                if n.trim_start_matches('!').starts_with("capi:") && !out.contains(n) {
                    out.push(n.clone());
                }
                args.iter().for_each(|a| walk(a, out));
            }
            Core::App(f, a) => {
                walk(f, out);
                walk(a, out);
            }
            Core::Lam(_, b) => walk(b, out),
            Core::Let(bs, b) => {
                bs.iter().for_each(|(_, e)| walk(e, out));
                walk(b, out);
            }
            Core::Case(s, _, alts) => {
                walk(s, out);
                for a in alts {
                    match a {
                        Alt::Con(_, _, e) | Alt::Lit(_, e) | Alt::Default(e) => walk(e, out),
                    }
                }
            }
            Core::Con(_, args) => args.iter().for_each(|a| walk(a, out)),
            Core::Var(_) | Core::Lit(_) | Core::Fail(_) => {}
        }
    }
    let mut prims = Vec::new();
    for b in binds {
        walk(&b.body, &mut prims);
    }
    let Some(src) = crate::ccall::capi_stubs(&prims) else { return Ok(None) };
    let mut ctx = [0u8; 88];
    crate::interp::md5::init(&mut ctx);
    crate::interp::md5::update(&mut ctx, src.as_bytes());
    let digest: String = crate::interp::md5::finish(&mut ctx).iter().map(|b| format!("{b:02x}")).collect();
    let dir = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .unwrap_or_else(std::env::temp_dir)
        .join("hsc")
        .join("capi");
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let path = dir.join(format!("capi-{digest}.c"));
    if !path.is_file() {
        std::fs::write(&path, src).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(Some((path, flags.to_vec())))
}

/// The name of the binding a program's payload carries for its C libraries (their paths).
pub const CBITS_BIND: &str = "hsc$cbits";

/// Loads the C libraries a program's payload names.
pub fn load_cbits(binds: &[TopBind]) {
    for b in binds.iter().filter(|b| b.name == CBITS_BIND) {
        if let crate::core::Core::Lit(crate::core::Lit::Str(paths)) = &b.body {
            for p in paths.lines() {
                if let Err(e) = crate::ccall::load(p) {
                    eprintln!("hsc: cannot load {p}: {e}");
                }
            }
        }
    }
}

/// Writes a native executable for a program: its code is generated as assembly, assembled, and
/// linked with the hsc runtime library next to this binary.
pub fn write_native(out: &str, binds: &[crate::core::TopBind]) -> Result<(), String> {
    write_native_with(out, binds, &[])
}

/// As `write_native`, with the C sources of the program's packages compiled and linked in.
pub fn write_native_with(out: &str, binds: &[crate::core::TopBind], c_sources: &[(PathBuf, Vec<String>)]) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let runtime = exe.parent().map(|d| d.join("libhsc.a")).unwrap_or_default();
    if !runtime.is_file() {
        return Err(format!("runtime library not found: {}", runtime.display()));
    }
    // The build directory sits beside the output, not in the temporary directory, which may be a small tmpfs.
    let out_dir = std::path::Path::new(out).parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(std::path::Path::new("."));
    let dir = out_dir.join(format!(".hsc-build-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let asm = dir.join("program.s");
    let obj = dir.join("program.o");
    // With the real base, main runs under its top handler, and stdout is flushed when the program
    // exits, as GHC's `hs_exit` does: `hsc$main` is `runMainIO Main.main >> flushStdHandles`.
    let mut binds = binds.to_vec();
    let has = |n: &str| binds.iter().any(|b| b.name == n);
    if has("GHC.Internal.TopHandler.runMainIO") && has("Main.main") && has("GHC.Internal.TopHandler.flushStdHandles") && has("GHC.Internal.Base.thenIO") {
        use crate::core::Core::{App, Var};
        let run_main = App(Box::new(Var("GHC.Internal.TopHandler.runMainIO".into())), Box::new(Var("Main.main".into())));
        let body = App(
            Box::new(App(Box::new(Var("GHC.Internal.Base.thenIO".into())), Box::new(run_main))),
            Box::new(Var("GHC.Internal.TopHandler.flushStdHandles".into())),
        );
        binds.push(crate::core::TopBind { name: "hsc$main".into(), body });
    }
    // Only what the program reaches is compiled: its entry, and the bindings the runtime uses.
    let entry = if binds.iter().any(|b| b.name == "hsc$main") { "hsc$main" } else { "Main.main" };
    let mut roots: Vec<&str> = vec![entry];
    roots.extend_from_slice(crate::native::RTS_GLOBALS);
    let binds = crate::native::reachable(binds, &roots);
    let prog = crate::native::stg::Lowering::program(&binds);
    let mut text = crate::native::codegen::Gen::program(&prog);
    text.push_str(&crate::native::program_main());
    std::fs::write(&asm, text).map_err(|e| e.to_string())?;
    let run = |args: &[&str]| -> Result<(), String> {
        let st = std::process::Command::new("gcc").args(args).status().map_err(|e| format!("gcc: {e}"))?;
        if st.success() { Ok(()) } else { Err(format!("gcc {} failed", args.join(" "))) }
    };
    // HSC_DEBUG keeps the symbols and the generated assembly (its directory is printed).
    let debug = std::env::var_os("HSC_DEBUG").is_some();
    let c_objs = compile_c_sources(c_sources, &dir)?;
    let c_objs: Vec<String> = c_objs.iter().map(|p| p.display().to_string()).collect();
    let result = run(&["-c", &asm.display().to_string(), "-o", &obj.display().to_string()]).and_then(|()| {
        let (o, r) = (obj.display().to_string(), runtime.display().to_string());
        let mut args = vec![o.as_str()];
        args.extend(c_objs.iter().map(String::as_str));
        // The program's C symbols are found by name at run time (`crate::ccall`).
        args.extend_from_slice(&[r.as_str(), "-rdynamic", "-o", out]);
        if !debug {
            args.push("-s");
        }
        args.extend_from_slice(&["-lpthread", "-ldl", "-lm", "-lgcc_s"]);
        run(&args)
    });
    if debug {
        eprintln!("hsc: assembly kept in {}", dir.display());
    } else {
        let _ = std::fs::remove_dir_all(&dir);
    }
    result
}
