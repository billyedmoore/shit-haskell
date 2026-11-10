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
    pub search: Vec<PathBuf>,
    pub ifaces: BTreeMap<String, Iface>,
    pub checker: Checker,
    pub binds: Vec<TopBind>,
    loaded: BTreeSet<String>,
    loading: Vec<String>,
    /// Modules imported through `{-# SOURCE #-}`, known so far by their `.hs-boot` file.
    booted: BTreeSet<String>,
    flags: Vec<String>,
}

/// The default library directory: `libs/` at the project root.
pub fn default_lib_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("HSC_LIBDIR") {
        return PathBuf::from(dir);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../libs")
}

impl Driver {
    pub fn new(main_dir: &Path, flags: Vec<String>) -> Driver {
        // The Haskell libraries are the vendored GHC packages: base (Prelude included), ghc-prim,
        // ghc-internal and ghc-bignum.
        let libs = default_lib_dir();
        let mut search = vec![main_dir.to_path_buf(), libs.join("base").join("src")];
        search.push(libs.join("ghc-prim"));
        search.push(libs.join("ghc-internal").join("src"));
        search.push(libs.join("ghc-bignum").join("src"));
        Driver {
            search,
            ifaces: BTreeMap::new(),
            checker: Checker::new(),
            binds: Vec::new(),
            loaded: BTreeSet::new(),
            booted: BTreeSet::new(),
            loading: Vec::new(),
            flags,
        }
    }

    fn find_source(&self, module: &str) -> Option<PathBuf> {
        let rel = format!("{}.hs", module.replace('.', "/"));
        self.search.iter().map(|d| d.join(&rel)).find(|p| p.is_file())
    }

    fn find_boot(&self, module: &str) -> Option<PathBuf> {
        let rel = format!("{}.hs-boot", module.replace('.', "/"));
        self.search.iter().map(|d| d.join(&rel)).find(|p| p.is_file())
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
        let src = read_source(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let src = preprocess_if_needed(src, &path, &self.flags)?;
        let (mut parsed, exts) = crate::syntax::parse_module_source(&src, &self.flags).map_err(|e| {
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
        let path = self
            .find_source(module)
            .ok_or_else(|| format!("Could not find module ‘{module}’"))?;
        let src = read_source(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let src = preprocess_if_needed(src, &path, &self.flags)?;
        let (parsed, mut exts) = crate::syntax::parse_module_source(&src, &self.flags).map_err(|e| {
            format!("{}:{}:{}: error:\n    {}", path.display(), e.line, e.col, e.msg)
        })?;
        // The GHC packages compile with NoImplicitPrelude as a package default (their cabal files).
        let path_text = path.to_string_lossy();
        if ["/libs/ghc-internal/", "/libs/ghc-prim/", "/libs/ghc-bignum/", "/libs/base/"].iter().any(|p| path_text.contains(p)) {
            exts.apply("NoImplicitPrelude")?;
        }
        self.loading.push(module.to_string());
        let mut deps: Vec<String> = parsed.imports.iter().map(|i| i.module.clone()).collect();
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
        let (renamed, iface) = crate::rename::rename_module(m, &self.ifaces, exts.is_on("ImplicitPrelude"), boot)
            .map_err(|e| located(e.loc, &e.msg))?;
        let binds = self.checker.check_module(&renamed).map_err(|e| located(e.loc, &e.msg))?;
        self.binds.extend(binds);
        self.ifaces.insert(name, iface);
        Ok(())
    }
}

/// Compiles a program rooted at `main_file`, returning all its Core bindings.
pub fn compile_program(main_file: &Path, flags: Vec<String>) -> Result<Vec<TopBind>, String> {
    let dir = main_file.parent().unwrap_or(Path::new(".")).to_path_buf();
    let mut d = Driver::new(&dir, flags.clone());
    let src = read_source(main_file).map_err(|e| format!("{}: {e}", main_file.display()))?;
    let src = preprocess_if_needed(src, main_file, &flags)?;
    let (parsed, exts) = crate::syntax::parse_module_source(&src, &flags).map_err(|e| {
        format!("{}:{}:{}: error:\n    {}", main_file.display(), e.line, e.col, e.msg)
    })?;
    let mut deps: Vec<String> = parsed.imports.iter().map(|i| i.module.clone()).collect();
    if exts.is_on("ImplicitPrelude") && !deps.iter().any(|x| x == "Prelude") {
        deps.insert(0, "Prelude".into());
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
    if !crate::syntax::cpp::wants_cpp(&src, flags) {
        return Ok(src);
    }
    let dir = path.parent().unwrap_or(Path::new(".")).to_path_buf();
    let mut pp = crate::syntax::cpp::Preprocessor::new(vec![default_lib_dir().join("include")]);
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
    let prog_name = std::env::args()
        .next()
        .and_then(|a| std::path::Path::new(&a).file_name().map(|s| s.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "main".into());
    run_binds_code(binds, prog_name)
}

/// Writes a native executable for a program: its code is generated as assembly, assembled, and
/// linked with the hsc runtime library next to this binary.
pub fn write_native(out: &str, binds: &[crate::core::TopBind]) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let runtime = exe.parent().map(|d| d.join("libhsc.a")).unwrap_or_default();
    if !runtime.is_file() {
        return Err(format!("runtime library not found: {}", runtime.display()));
    }
    let dir = std::env::temp_dir().join(format!("hsc-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let asm = dir.join("program.s");
    let obj = dir.join("program.o");
    let prog = crate::native::stg::Lowering::program(binds);
    let mut text = crate::native::codegen::Gen::program(&prog);
    text.push_str(&crate::native::program_main());
    std::fs::write(&asm, text).map_err(|e| e.to_string())?;
    let run = |args: &[&str]| -> Result<(), String> {
        let st = std::process::Command::new("gcc").args(args).status().map_err(|e| format!("gcc: {e}"))?;
        if st.success() { Ok(()) } else { Err(format!("gcc {} failed", args.join(" "))) }
    };
    let result = run(&["-c", &asm.display().to_string(), "-o", &obj.display().to_string()]).and_then(|()| {
        run(&[
            &obj.display().to_string(),
            &runtime.display().to_string(),
            "-o",
            out,
            "-lpthread",
            "-ldl",
            "-lm",
            "-lgcc_s",
        ])
    });
    let _ = std::fs::remove_dir_all(&dir);
    result
}
