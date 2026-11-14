//! The driver: finds and loads modules, runs the renamer and type checker over them in
//! dependency order, and assembles the program's Core.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
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
    /// The key of each module loaded (see `cache_key`).
    keys: HashMap<String, String>,
    /// Imports observed when a boot interface was checked. SOURCE edges are also
    /// revalidated against their current boot/full context when the key is used.
    boot_deps: HashMap<String, Vec<String>>,
    /// Modules a package-qualified import names that the plain name would not find (such as a
    /// module importing its namesake from another package): the internal name each is loaded
    /// under, with its real name and the package directory it is in.
    renamed: BTreeMap<String, (String, PathBuf)>,
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

/// Compile-time accounting for `HSC_TIME=1`: the time spent in each phase, printed at exit.
pub mod timing {
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    static TOTALS: Mutex<Vec<(&'static str, Duration)>> = Mutex::new(Vec::new());

    /// An operation's elapsed time, including early returns. Nested operations have inclusive
    /// totals, so the reported phases need not add up to the wall-clock total.
    pub struct Scope(Option<(&'static str, Instant)>);

    pub fn scope(phase: &'static str) -> Scope {
        Scope(enabled().then(|| (phase, Instant::now())))
    }

    impl Drop for Scope {
        fn drop(&mut self) {
            if let Some((phase, start)) = self.0 {
                add(phase, start);
            }
        }
    }

    pub fn enabled() -> bool {
        std::env::var_os("HSC_TIME").is_some()
    }

    /// Adds the time since `start` to `phase`.
    pub fn add(phase: &'static str, start: Instant) {
        if !enabled() {
            return;
        }
        let d = start.elapsed();
        let mut t = TOTALS.lock().unwrap_or_else(|e| e.into_inner());
        match t.iter_mut().find(|(p, _)| *p == phase) {
            Some((_, total)) => *total += d,
            None => t.push((phase, d)),
        }
    }

    /// Prints the totals (to stderr).
    pub fn report() {
        if !enabled() {
            return;
        }
        let t = TOTALS.lock().unwrap_or_else(|e| e.into_inner());
        for (p, d) in t.iter() {
            eprintln!("hsc time: {p:<12} {:>8.3}s", d.as_secs_f64());
        }
    }
}

/// The first line of an interface file written by hsc.
pub const IFACE_MAGIC: &str = "hsc-interface 2";

/// The flag that marks a source as already preprocessed (the text an interface carries).
pub const PREPROCESSED_FLAG: &str = "-hsc-preprocessed";

/// The flag of a reexported module of a package in scope: `-hsc-reexport=NAME=MODULE=DIR`.
pub const REEXPORT_FLAG: &str = "-hsc-reexport=";

/// The flag naming an interface module's source as its package's build was given it.
pub const SOURCE_NAME_FLAG: &str = "-hsc-source-name=";

/// hsc's cache directory for `what` (under `$XDG_CACHE_HOME/hsc` or `~/.cache/hsc`).
pub fn cache_dir(what: &str) -> PathBuf {
    std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .unwrap_or_else(std::env::temp_dir)
        .join("hsc")
        .join(what)
}

/// The key of a checked module: its source's hash and the keys of the modules it imports.
fn cache_key(own_hash: &str, deps: &[(String, String)]) -> String {
    let mut text = own_hash.to_string();
    for (d, k) in deps {
        text.push_str(&format!("|{d}={k}"));
    }
    md5_hex(text.as_bytes())
}

pub(crate) fn md5_hex(bytes: &[u8]) -> String {
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
    let _time = timing::scope("interfaces");
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
    let mut name: Option<String> = None;
    let (mut text, mut boot): (Option<Vec<u8>>, Option<Vec<u8>>) = (None, None);
    let mut headers: Vec<(String, Vec<u8>)> = Vec::new();
    while let Some(l) = line(&mut pos) {
        if let Some(v) = l.strip_prefix("header: ") {
            let (name, n) = v.rsplit_once(' ')?;
            let n: usize = n.parse().ok()?;
            headers.push((name.to_string(), bytes.get(pos..pos + n)?.to_vec()));
            pos += n;
            continue;
        }
        if let Some(v) = l.strip_prefix("module: ") {
            module = v.to_string();
        } else if let Some(v) = l.strip_prefix("source: ") {
            source = Some(PathBuf::from(v));
        } else if let Some(v) = l.strip_prefix("name: ") {
            name = Some(v.to_string());
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
    // (A literate source's text is already its code: the copy is plain Haskell.)
    let ext = source.extension().and_then(|e| e.to_str()).map(|e| if e == "lhs" { "hs" } else { e }).unwrap_or("hs").to_string();
    let mut key = text.clone();
    key.extend_from_slice(boot.as_deref().unwrap_or_default());
    for (n, h) in &headers {
        key.extend_from_slice(n.as_bytes());
        key.extend_from_slice(h);
    }
    let dir = cache_dir("ifaces").join(md5_hex(&key));
    // The headers the interface carries, in an include directory of their own.
    if !headers.is_empty() {
        let inc = dir.join("%include");
        std::fs::create_dir_all(&inc).ok()?;
        for (n, h) in &headers {
            let p = inc.join(n);
            if !p.is_file() {
                let tmp = inc.join(format!("{n}.tmp{}", std::process::id()));
                std::fs::write(&tmp, h).ok()?;
                std::fs::rename(&tmp, &p).ok()?;
            }
        }
        flags.push(format!("-I{}", inc.display()));
    }
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
    if let Some(n) = name {
        flags.push(format!("{SOURCE_NAME_FLAG}{n}"));
    }
    Some((file, flags))
}

/// Writes the interface file of a module of a package being built. hsc compiles programs from
/// source, so the interface holds the module's preprocessed text and the flags it is compiled
/// with (those of the preprocessor dropped). The file is left alone when it already says the same,
/// so that its time stamp shows when the module last changed.
pub fn write_interface(path: &Path, module: &str, source: &Path, name: &Path, flags: &[String]) -> Result<(), String> {
    let pre = |p: &Path| -> Result<String, String> {
        let src = read_source(p).map_err(|e| format!("{}: {e}", p.display()))?;
        preprocess_if_needed(src, p, flags)
    };
    let text = pre(source)?;
    let boot_path = source.with_extension("hs-boot");
    let boot = if boot_path.is_file() { Some(pre(&boot_path)?) } else { None };
    // `name`: the source as the compiler was given it (relative to the build), which Template
    // Haskell's `location` and call stacks show, as GHC's do.
    let mut out = format!("{IFACE_MAGIC}\nmodule: {module}\nsource: {}\nname: {}\n", source.display(), name.display());
    for f in flags {
        let cpp = f == "-cpp" || f == "-XCPP" || f.starts_with("-I") || f.starts_with("-D") || f.starts_with("-U")
            || f.starts_with("-include=") || f.starts_with("-optP") || f.starts_with("-hsc-pkgver=")
            || f.starts_with("-hsc-archive=") || f.starts_with("-hsc-lib=") || f.starts_with(REEXPORT_FLAG);
        if !cpp {
            out.push_str(&format!("flag: {f}\n"));
        }
    }
    out.push_str(&format!("text: {}\n{text}", text.len()));
    if let Some(b) = boot {
        out.push_str(&format!("boot: {}\n{b}", b.len()));
    }
    // A module with capi imports: the headers of its include directories go with it, since the
    // wrappers are compiled with the program, after the package's sources are gone.
    if text.contains("capi") {
        for f in flags {
            let Some(dir) = f.strip_prefix("-I") else { continue };
            let Ok(entries) = std::fs::read_dir(dir) else { continue };
            let mut names: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|x| x == "h")).collect();
            names.sort();
            for h in names {
                let (Some(name), Ok(content)) = (h.file_name().and_then(|n| n.to_str()), std::fs::read_to_string(&h)) else { continue };
                out.push_str(&format!("header: {name} {}\n{content}", content.len()));
            }
        }
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
        // The built-in constructors' wrappers belong to the program, not to the first module
        // checked (whose cache entry would carry them).
        let mut checker = Checker::new();
        let builtin_binds = std::mem::take(&mut checker.out);
        Driver {
            search,
            pkg_dirs,
            ifaces: BTreeMap::new(),
            checker,
            binds: builtin_binds,
            loaded: BTreeSet::new(),
            booted: BTreeSet::new(),
            loading: Vec::new(),
            flags,
            sources: Vec::new(),
            c_sources: Vec::new(),
            c_flags: Vec::new(),
            renamed: BTreeMap::new(),
            keys: HashMap::new(),
            boot_deps: HashMap::new(),
        }
    }

    /// Finds a module: among the program's own first, then in the packages (by interface file or
    /// by source).
    pub fn find_module(&self, module: &str) -> Option<Found> {
        if let Some((real, dir)) = self.renamed.get(module) {
            if self.search.contains(dir) {
                return self.find_home_in(dir, &real.replace('.', "/"));
            }
            return Self::find_in_package_dir(dir, &real.replace('.', "/"));
        }
        let stem = module.replace('.', "/");
        // A module of ghc-internal (the package the compiler wires in) is always its own: a
        // program's module of the same name is loaded under an internal name (see
        // `resolve_home_wired_imports`).
        if let Some(found) = self.wired_dir().and_then(|d| Self::find_in_package_dir(d, &stem)) {
            return Some(found);
        }
        for d in &self.search {
            if let Some(found) = self.find_home_in(d, &stem) {
                return Some(found);
            }
        }
        self.pkg_dirs.iter().find_map(|d| Self::find_in_package_dir(d, &stem))
    }

    /// The import directory of ghc-internal, if it is among the packages.
    fn wired_dir(&self) -> Option<&PathBuf> {
        self.pkg_dirs.iter().find(|d| Self::dir_of_package(d, "ghc-internal"))
    }

    /// A program's module (by its path stem) in one of its `-i` directories.
    fn find_home_in(&self, d: &Path, stem: &str) -> Option<Found> {
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
        None
    }

    /// Points a program module's imports of its own modules named like ghc-internal's
    /// (ghc-lib-parser's `GHC.Internal.TH.Syntax`) at those: each is loaded under an internal
    /// name, as the compiler and the libraries refer to ghc-internal's by the real one.
    fn resolve_home_wired_imports(&mut self, m: &mut Module) {
        for imp in &mut m.imports {
            if imp.package.is_some() || self.renamed.contains_key(&imp.module) {
                continue;
            }
            if let Some(key) = self.home_wired_key(&imp.module) {
                if imp.alias.is_none() {
                    imp.alias = Some(imp.module.clone());
                }
                imp.module = key;
            }
        }
    }

    /// The internal name a program's module named like one of ghc-internal's is loaded under
    /// (registering it), or None when the name is not such a module.
    pub fn home_wired_key(&mut self, module: &str) -> Option<String> {
        let stem = module.replace('.', "/");
        let wired = self.wired_dir().and_then(|d| Self::find_in_package_dir(d, &stem))?;
        let dir = self.search.iter().find(|d| self.find_home_in(d, &stem).is_some_and(|f| f.path != wired.path))?.clone();
        let key = format!("{module}_home");
        self.renamed.insert(key.clone(), (module.to_string(), dir));
        Some(key)
    }

    /// The real name of a module loaded under an internal one.
    pub fn real_name<'a>(&'a self, module: &'a str) -> &'a str {
        self.renamed.get(module).map(|(r, _)| r.as_str()).unwrap_or(module)
    }

    /// A module (by its path stem) in one package import directory.
    fn find_in_package_dir(d: &Path, stem: &str) -> Option<Found> {
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
        None
    }

    /// Whether a package import directory belongs to package `pkg` (a store directory is named
    /// `<pkg>-<version>-<hash>`, a vendored package's `libs/<pkg>`).
    fn dir_of_package(d: &Path, pkg: &str) -> bool {
        d.components().any(|c| {
            let c = c.as_os_str().to_string_lossy();
            c == pkg || c.strip_prefix(pkg).and_then(|r| r.strip_prefix('-')).is_some_and(|r| r.starts_with(|ch: char| ch.is_ascii_digit()))
        })
    }

    /// Points the package-qualified imports of a module at the package they name. An import the
    /// plain module name would resolve elsewhere is loaded under an internal name of its own
    /// (still referred to by its real name in the importing module).
    fn resolve_package_imports(&mut self, m: &mut Module) {
        // A reexported module (`reexported-modules: Paths_happy_lib as Happy.Paths`) is its module
        // of the providing package, loaded under an internal name and imported as the name.
        let reexports: Vec<(String, String, PathBuf)> = self
            .flags
            .iter()
            .filter_map(|f| f.strip_prefix(REEXPORT_FLAG))
            .filter_map(|r| {
                let (name, rest) = r.split_once('=')?;
                let (real, dir) = rest.split_once('=')?;
                Some((name.to_string(), real.to_string(), PathBuf::from(dir)))
            })
            .collect();
        for imp in &mut m.imports {
            if imp.package.is_some() || self.renamed.contains_key(&imp.module) {
                continue;
            }
            let Some((_, real, dir)) = reexports.iter().find(|(n, _, _)| *n == imp.module) else { continue };
            if self.find_module(&imp.module).is_some() {
                continue;
            }
            let key = format!("{}_reexport_{}", real, md5_hex(dir.to_string_lossy().as_bytes()).get(..8).unwrap_or("x"));
            self.renamed.insert(key.clone(), (real.clone(), dir.clone()));
            if imp.alias.is_none() {
                imp.alias = Some(imp.module.clone());
            }
            imp.module = key;
        }
        for imp in &mut m.imports {
            let Some(pkg) = imp.package.clone() else { continue };
            if pkg == "this" {
                continue;
            }
            let stem = imp.module.replace('.', "/");
            let Some((dir, found)) = self
                .pkg_dirs
                .iter()
                .filter(|d| Self::dir_of_package(d, &pkg))
                .find_map(|d| Self::find_in_package_dir(d, &stem).map(|f| (d.clone(), f)))
            else {
                continue;
            };
            let plain = self.find_module(&imp.module).map(|f| f.path);
            if plain.as_ref() == Some(&found.path) && imp.module != m.name {
                continue;
            }
            let key = format!("{}_pkg_{}", imp.module, pkg.replace('-', "_"));
            self.renamed.insert(key.clone(), (imp.module.clone(), dir));
            if imp.alias.is_none() {
                imp.alias = Some(imp.module.clone());
            }
            imp.module = key;
        }
    }

    /// Points a package module's imports at its own package's modules: a module of the same name
    /// in the program (time-compat's `Data.Time.Calendar.Private` beside time's) is not the one
    /// it means. Such an import is loaded under an internal name, as a package-qualified one is.
    fn resolve_own_package_imports(&mut self, m: &mut Module, path: &Path) {
        let Some(dir) = self.pkg_dirs.iter().filter(|d| path.starts_with(d)).max_by_key(|d| d.as_os_str().len()).cloned() else {
            return;
        };
        for imp in &mut m.imports {
            if imp.package.is_some() || imp.module == m.name || self.renamed.contains_key(&imp.module) {
                continue;
            }
            let stem = imp.module.replace('.', "/");
            let found = self.find_module(&imp.module);
            // The module of its own package; else, when the plain name finds a home module (one of
            // the program's, as ghc-lib-parser's GHC.Internal.TH.Syntax), the packages' one: a
            // package's modules never see the program's.
            let (from_dir, own) = match Self::find_in_package_dir(&dir, &stem) {
                Some(own) => (dir.clone(), own),
                None if found.as_ref().is_some_and(|f| f.home) => {
                    match self.pkg_dirs.iter().find_map(|d| Self::find_in_package_dir(d, &stem).map(|f| (d.clone(), f))) {
                        Some(hit) => hit,
                        None => continue,
                    }
                }
                None => continue,
            };
            if found.map(|f| f.path) == Some(own.path) {
                continue;
            }
            let dtag: String = from_dir.to_string_lossy().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
            let key = format!("{}_pkg_{}", imp.module, md5_hex(dtag.as_bytes()).get(..8).unwrap_or("x"));
            self.renamed.insert(key.clone(), (imp.module.clone(), from_dir.clone()));
            if imp.alias.is_none() {
                imp.alias = Some(imp.module.clone());
            }
            imp.module = key;
        }
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
        let path = self.find_boot(module).ok_or_else(|| format!("Could not find module ‘{module}’ (no .hs-boot file for a SOURCE import)"))?;
        if self.loaded.contains(module) {
            return Ok(());
        }
        if self.booted.contains(module) {
            return Ok(());
        }
        let flags = self.find_module(module).map(|f| f.flags).unwrap_or_default();
        let src = read_source(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let src = preprocess_if_needed(src, &path, &flags)?;
        let (mut parsed, mut exts) =
            crate::syntax::parse_module_source(&src, &flags).map_err(|e| format!("{}:{}:{}: error:\n    {}", path.display(), e.line, e.col, e.msg))?;
        strip_boot_bodies(&mut parsed);
        let path_text = path.to_string_lossy();
        if ["/libs/ghc-internal/", "/libs/ghc-prim/", "/libs/ghc-bignum/", "/libs/base/"].iter().any(|p| path_text.contains(p)) {
            exts.apply("NoImplicitPrelude")?;
        }
        let source = Self::source_imports(&src);
        let mut dep_names = Vec::new();
        let mut deps: Vec<_> = parsed.imports.iter().map(|i| i.module.clone()).collect();
        if exts.is_on("ImplicitPrelude") && module != "Prelude" && !deps.iter().any(|d| d == "Prelude") {
            deps.insert(0, "Prelude".into());
        }
        for dep in deps {
            if source.contains(&dep) {
                self.load_boot(&dep)?;
            } else {
                self.load(&dep)?;
            }
            let name = if source.contains(&dep) { format!("{{boot}}{dep}") } else { dep };
            dep_names.push(name);
        }
        let dep_keys: Option<Vec<_>> = dep_names.iter().map(|name| self.dependency_key(name).map(|key| (name.clone(), key))).collect();
        self.checker.boot = true;
        let result = self.compile_module(parsed, exts, &path);
        self.checker.boot = false;
        result?;
        let own = format!("boot\0{module}\0{}\0{}\0{src}\0{}", path.display(), flags.join("\u{1}"), self.instance_key(module));
        if let Some(deps) = dep_keys {
            self.keys.insert(format!("{{boot}}{module}"), cache_key(&md5_hex(own.as_bytes()), &deps));
            self.boot_deps.insert(module.to_string(), dep_names);
        }
        self.booted.insert(module.to_string());
        Ok(())
    }

    fn instance_key(&self, module: &str) -> String {
        crate::cache::instance_key(&self.checker.env, module, |t| self.checker.metas.zonk(t))
    }

    fn boot_key(&self, module: &str) -> Option<String> {
        self.booted.contains(module).then(|| self.dependency_key(&format!("{{boot}}{module}"))).flatten()
    }

    /// SOURCE dependencies use the interface actually present after *all* imports
    /// have loaded. Later imports may complete a boot module or add orphan instances.
    fn dependency_key(&self, name: &str) -> Option<String> {
        self.dependency_key_inner(name, &mut HashSet::new())
    }

    fn dependency_key_inner(&self, name: &str, visiting: &mut HashSet<String>) -> Option<String> {
        let Some(module) = name.strip_prefix("{boot}") else { return self.keys.get(name).cloned() };
        if self.loaded.contains(module) {
            return Some(md5_hex(format!("full\0{}\0{}", self.keys.get(module)?, self.instance_key(module)).as_bytes()));
        }
        if !visiting.insert(module.to_string()) { return None; }
        let deps: Option<Vec<_>> = self.boot_deps.get(module)?.iter().map(|dep| {
            self.dependency_key_inner(dep, visiting).map(|key| (dep.clone(), key))
        }).collect();
        visiting.remove(module);
        Some(cache_key(&format!("boot\0{}\0{}", self.keys.get(name)?, self.instance_key(module)), &deps?))
    }

    fn completed_key(&self, module: &str, own: &str, deps: &[(String, String)], boot: &Option<String>) -> String {
        cache_key(&format!("{own}\0{boot:?}\0{}", self.instance_key(module)), deps)
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
        let t_parse = std::time::Instant::now();
        let src = read_source(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let src = preprocess_if_needed(src, &path, &flags)?;
        // The module cache: an entry for this source (and flags) whose imports are unchanged
        // stands for the module (see `crate::cache`).
        // An implementation's cache also depends on the declarations in its boot file.
        let boot_src = if let Some(boot_path) = self.find_boot(module) {
            let text = read_source(&boot_path).map_err(|e| format!("{}: {e}", boot_path.display()))?;
            preprocess_if_needed(text, &boot_path, &flags)?
        } else { String::new() };
        let own_hash = md5_hex(format!("{module}\0{}\0{}\0{src}\0{boot_src}", path.display(), flags.join("\u{1}")).as_bytes());
        let cache_file = crate::cache::dir().map(|d| d.join(format!("{own_hash}.bin")));
        if let Some(cf) = &cache_file {
            if std::env::var_os("HSC_DEBUG_CACHE").is_some() && cf.is_file() && crate::cache::load(cf).is_none() {
                eprintln!("cache: {module}: entry {} does not decode", cf.display());
            }
            if let Some(entry) = crate::cache::load(cf) {
                let t = std::time::Instant::now();
                let used = self.use_cached(module, entry, &own_hash, &path, home);
                timing::add("cache load", t);
                if used {
                    return Ok(());
                }
            }
        }
        let (mut parsed, mut exts) = crate::syntax::parse_module_source(&src, &flags).map_err(|e| {
            format!("{}:{}:{}: error:\n    {}", path.display(), e.line, e.col, e.msg)
        })?;
        timing::add("parse", t_parse);
        if let Some((real, _)) = self.renamed.get(module) {
            // Loaded under an internal name: `module M` of its own name in its export list (base's
            // `module GHC.Internal.TH.Syntax`) is still the module itself.
            let real = real.clone();
            if let Some(items) = &mut parsed.exports {
                for item in items.iter_mut() {
                    if let crate::syntax::ast::IEItem::Module(m) = item {
                        if *m == real {
                            *m = module.to_string();
                        }
                    }
                }
            }
            parsed.name = module.to_string();
        }
        self.resolve_package_imports(&mut parsed);
        if home {
            self.resolve_home_wired_imports(&mut parsed);
        } else {
            self.resolve_own_package_imports(&mut parsed, &path);
        }
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
        if exts.is_on("MonadComprehensions") && !path_text.contains("/libs/ghc-internal/") && !path_text.contains("/libs/ghc-prim/") {
            deps.push("GHC.Internal.Control.Monad".into());
        }
        if exts.is_on("OverloadedLists") && module != "GHC.Internal.IsList" && !path_text.contains("/libs/ghc-prim/") {
            deps.push("GHC.Internal.IsList".into());
        }
        if exts.is_on("ImplicitPrelude") && module != "Prelude" && !deps.iter().any(|d| d == "Prelude") {
            deps.insert(0, "Prelude".into());
        }
        let source = Self::source_imports(&src);
        if std::env::var_os("HSC_DEBUG_CACHE").is_some() {
            eprintln!("full: {module} needs {deps:?} (loading {:?})", self.loading);
        }
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
        let has_splices = (exts.is_on("TemplateHaskell") || exts.is_on("QuasiQuotes"))
            && (parsed.decls.iter().any(|d| matches!(d, Decl::Splice(..))) || crate::th::has_splices(&parsed));
        // A module from an interface is shown by the name its package's build gave it (in call
        // stacks and messages, as GHC shows it). Template Haskell's `location` also reads the file
        // it names, so its splices see that name only when the file is there, and otherwise the
        // interface's own copy of the text (whose path is not the same each time).
        let named: Option<PathBuf> = flags.iter().find_map(|f| f.strip_prefix(SOURCE_NAME_FLAG)).map(PathBuf::from);
        let shown: PathBuf = named.clone().unwrap_or_else(|| path.clone());
        let splice_path: PathBuf = named.filter(|p| p.is_file()).unwrap_or_else(|| path.clone());
        self.expand_splices(&mut parsed, &exts, &splice_path)?;
        // What the module adds is saved when it can be loaded back on its own: no splices
        // (their results depend on more than the source).
        // (A `{-# SOURCE #-}` import depends on the boot file, keyed `{boot}M`.)
        let dep_keys: Option<Vec<(String, String)>> = deps
            .iter()
            .map(|d| {
                let name = if source.contains(d) { format!("{{boot}}{d}") } else { d.clone() };
                self.dependency_key(&name).map(|k| (name, k))
            })
            .collect();
        let completing_boot = self.booted.contains(module);
        let boot_key = self.boot_key(module);
        let cacheable = cache_file.is_some() && !has_splices && dep_keys.is_some() && (!completing_boot || boot_key.is_some());
        let mut snapshot = cacheable.then(|| {
            let _time = timing::scope("cache snapshot");
            if completing_boot {
                crate::cache::snapshot_boot(&self.checker.env, &self.checker.kinds.tycons, module)
            } else {
                Some(crate::cache::snapshot(&self.checker.env, &self.checker.kinds.tycons))
            }
        }).flatten();
        let boot_instances: Vec<_> = if completing_boot {
            let prefix = format!("{module}.$f");
            // Standalone deriving in a SOURCE consumer can create an A-prefixed
            // dictionary with an implementation already present. Only stubs need
            // supplying by A itself.
            let implemented: HashSet<&str> = self.binds.iter().map(|b| b.name.as_str()).collect();
            self.checker.env.instances.iter().filter(|i| i.dict.starts_with(&prefix) && !implemented.contains(i.dict.as_str())).cloned().collect()
        } else { Vec::new() };
        let binds_before = self.binds.len();
        self.compile_module(parsed, exts, &shown)?;
        let mut new_binds = self.binds.split_off(binds_before);
        if completing_boot {
            if !self.checker.complete_boot_instances(&boot_instances, &mut new_binds).map_err(|e| e.msg)? {
                snapshot = None;
            }
            let names: HashSet<&str> = new_binds.iter().map(|b| b.name.as_str()).collect();
            self.binds.retain(|b| !names.contains(b.name.as_str()));
        }
        let binds_before = self.binds.len();
        self.binds.extend(new_binds);
        if let Some(dep_keys) = dep_keys.filter(|_| !completing_boot || boot_key.is_some()) {
            self.keys.insert(module.to_string(), self.completed_key(module, &own_hash, &dep_keys, &boot_key));
            if let Some(snap) = snapshot {
                if let (Some(cf), Some(iface)) = (&cache_file, self.ifaces.get(module)) {
                    let kinds = &self.checker.kinds;
                    let mut env = crate::cache::delta(&self.checker.env, &kinds.tycons, &snap, |k| kinds.resolve(k));
                    let metas = &self.checker.metas;
                    let complete = crate::cache::zonk_delta(&mut env, &|t| metas.zonk(t));
                    let entry = crate::cache::Entry {
                        boot_key,
                        deps: dep_keys,
                        iface: iface.clone(),
                        binds: self.binds[binds_before..].to_vec(),
                        env,
                        explicit_sigs: Vec::new(),
                    };
                    if std::env::var_os("HSC_DEBUG_CACHE").is_some() {
                        let prefix = format!("{module}.");
                        let foreign: Vec<&str> = entry.binds.iter().map(|b| b.name.as_str()).filter(|n| !n.starts_with(&prefix)).take(6).collect();
                        if !foreign.is_empty() {
                            eprintln!("cache: {module} saves others' bindings {foreign:?}");
                        }
                    }
                    if complete {
                        crate::cache::save(cf, &entry);
                    } else if std::env::var_os("HSC_DEBUG_CACHE").is_some() {
                        eprintln!("cache: {module} not saved: unsolved type variables");
                    }
                }
            }
        }
        self.loaded.insert(module.to_string());
        self.sources.push((module.to_string(), path.clone(), home));
        if let Some(n) = std::env::var_os("HSC_DEBUG_SCHEME") {
            let n = n.to_string_lossy().to_string();
            if let Some(s) = self.checker.env.values.get(&n) {
                eprintln!("checked {module}: {n} :: {s:?}");
            }
        }
        Ok(())
    }

    /// Loads a module from its cache entry, if the modules it imports (loaded first) are the
    /// ones it was checked against. Answers whether it did.
    fn use_cached(&mut self, module: &str, entry: crate::cache::Entry, own_hash: &str, path: &Path, home: bool) -> bool {
        if std::env::var_os("HSC_DEBUG_CACHE").is_some() {
            eprintln!("cache: {module} needs {:?}", entry.deps.iter().map(|(d, _)| d.as_str()).collect::<Vec<_>>());
        }
        self.loading.push(module.to_string());
        for (dep, _) in &entry.deps {
            let loaded = match dep.strip_prefix("{boot}") {
                Some(b) => self.load_boot(b),
                None => self.load(dep),
            };
            if let Err(e) = loaded {
                if std::env::var_os("HSC_DEBUG_CACHE").is_some() {
                    eprintln!("cache: {module} not used: {dep}: {e}");
                }
                self.loading.pop();
                return false;
            }
        }
        self.loading.pop();
        if !entry.deps.iter().all(|(d, k)| self.dependency_key(d).as_ref() == Some(k)) {
            return false;
        }
        let boot_key = self.boot_key(module);
        if entry.boot_key != boot_key || (self.booted.contains(module) && boot_key.is_none()) {
            return false;
        }
        if self.booted.contains(module) && !crate::cache::boot_cacheable(&self.checker.env, module) {
            return false;
        }
        // (The unifier keeps its own set of the families' names.)
        for (name, _) in &entry.env.families {
            self.checker.metas.families.insert(name.clone());
        }
        crate::cache::apply(&mut self.checker.env, &mut self.checker.kinds.tycons, entry.env);
        self.keys.insert(module.to_string(), self.completed_key(module, own_hash, &entry.deps, &entry.boot_key));
        self.ifaces.insert(module.to_string(), entry.iface);
        if self.booted.contains(module) {
            let names: HashSet<&str> = entry.binds.iter().map(|b| b.name.as_str()).collect();
            self.binds.retain(|b| !names.contains(b.name.as_str()));
        }
        self.binds.extend(entry.binds);
        self.loaded.insert(module.to_string());
        self.sources.push((module.to_string(), path.to_path_buf(), home));
        if let Some(n) = std::env::var_os("HSC_DEBUG_SCHEME") {
            let n = n.to_string_lossy().to_string();
            if let Some(s) = self.checker.env.values.get(&n) {
                eprintln!("cached {module}: {n} :: {s:?}");
            }
        }
        true
    }

    /// Runs the Template Haskell splices and quasi-quotations of a module, whose imports are
    /// loaded, and puts what they produce in their place (see `crate::th`).
    fn expand_splices(&mut self, m: &mut Module, exts: &Extensions, path: &Path) -> Result<(), String> {
        let groups_end = self.expand_decl_splices(m, exts, path)?;
        self.expand_expr_splices(m, exts, path, groups_end)
    }

    /// The declaration splices of a module, in order: each runs with the declarations before it
    /// compiled (under the module's own name, so that `''T` names them as they will be named),
    /// and the declarations it produces take its place.
    /// Answers where the last declaration group ends (what a declaration splice produced last),
    /// which the expression splices after it may refer to.
    fn expand_decl_splices(&mut self, m: &mut Module, exts: &Extensions, path: &Path) -> Result<usize, String> {
        let mut groups_end = 0;
        while let Some(i) = m.decls.iter().position(|d| matches!(d, Decl::Splice(..))) {
            let Decl::Splice(body, loc) = m.decls[i].clone() else { unreachable!() };
            // (The top handler too, for a native run of the splices.)
            for lib in ["GHC.Internal.TH.Syntax", "GHC.Internal.IO.Unsafe", "GHC.Internal.TopHandler"] {
                self.load(lib)?;
            }
            self.load_booted()?;
            // The declarations before it may have expression splices of their own.
            let mut before = Module { decls: m.decls[..i].to_vec(), ..m.clone() };
            self.expand_expr_splices(&mut before, exts, path, groups_end)?;
            m.decls.splice(..i, before.decls.iter().cloned());
            let imported = self.quoted_data(&[&body], &m.name);
            let table = crate::th::reify_table(&before.decls, &m.name, &imported, loc);
            let smod = crate::th::decl_splice_module(m, before.decls, body, loc, table, &path.display().to_string());
            let mark = self.binds.len();
            let user_module = self.checker.user_module;
            self.compile_module(smod, exts.clone(), path)?;
            let pre = self.reify_values(&m.name, exts, path, loc)?;
            self.checker.user_module = user_module;
            let pre_io = pre.as_ref().map(|p| format!("{}IO", p));
            let generated = match crate::th::run_decl_splice(&self.binds, &m.name, loc, pre) {
                Err(e) if e == crate::th::SPLICE_BUDGET => {
                    self.native_splices(&m.name, 1, pre_io).and_then(|out| crate::th::native_decs(&out, loc))
                }
                r => r,
            }
                .map_err(|e| format!("{}:{}:{}: error:\n    {e}", path.display(), loc.line, loc.col))?;
            // The module is compiled for real afterwards.
            self.binds.truncate(mark);
            self.ifaces.remove(&m.name);
            groups_end = i + generated.len();
            m.decls.splice(i..=i, generated);
        }
        Ok(groups_end)
    }

    /// Runs the splices of a compiled splice module as a native program (built beside the current
    /// directory and removed after), answering what it printed: one result per splice.
    fn native_splices(&self, module: &str, count: usize, pre_io: Option<String>) -> Result<String, String> {
        let t = std::time::Instant::now();
        let mut binds = self.binds.clone();
        binds.push(crate::core::TopBind { name: "hsc$main".into(), body: crate::th::native_splice_main(module, count, pre_io) });
        static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        // In hsc's cache directory (a compilation killed while it runs leaves nothing in the
        // project); old ones are removed.
        let base = cache_dir("splices");
        if let Ok(entries) = std::fs::read_dir(&base) {
            for e in entries.flatten() {
                let old = e.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|age| age.as_secs() > 3600);
                if old {
                    let _ = std::fs::remove_dir_all(e.path());
                }
            }
        }
        let dir = base.join(format!("{}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let exe = dir.join("splices");
        // As in every program: the RTS functions C code calls, and base's C helpers.
        let mut c_sources = self.c_sources.clone();
        c_sources.push((default_lib_dir().join("rts").join("hsc-rts-shim.c"), Vec::new()));
        let result = write_native_with(&exe.display().to_string(), &binds, &c_sources).and_then(|()| {
            let t_run = std::time::Instant::now();
            let out = std::process::Command::new(&exe).output().map_err(|e| format!("{}: {e}", exe.display()))?;
            if std::env::var_os("HSC_TIME_SPLICES").is_some() {
                eprintln!("splice program built in {:?}, ran in {:?}", t_run - t, t_run.elapsed());
            }
            let stderr = String::from_utf8_lossy(&out.stderr);
            if !out.status.success() {
                return Err(format!("Exception when trying to run compile-time code:\n  {}", stderr.trim_end()));
            }
            eprint!("{stderr}");
            Ok(String::from_utf8_lossy(&out.stdout).to_string())
        });
        let _ = std::fs::remove_dir_all(&dir);
        if std::env::var_os("HSC_TIME_SPLICES").is_some() {
            eprintln!("splices of {module} run natively in {:?}", t.elapsed());
        }
        result
    }

    /// Compiles the module that gives `reify` the types of the values of `module` checked so far
    /// (the declarations before its splices), answering the binding that sets them; the caller
    /// drops the module's bindings with the splice module's.
    fn reify_values(&mut self, module: &str, exts: &Extensions, path: &Path, loc: crate::syntax::ast::Loc) -> Result<Option<String>, String> {
        let prefix = format!("{module}.");
        let mut values: Vec<(String, crate::tc::types::Scheme)> = self
            .checker
            .env
            .values
            .iter()
            .filter_map(|(k, s)| {
                let occ = k.strip_prefix(&prefix)?;
                let plain = occ.starts_with(|c: char| c.is_lowercase() || c == '_') && occ.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '\'');
                (plain && !occ.starts_with("hsc'splice")).then(|| (occ.to_string(), s.clone()))
            })
            .collect();
        if values.is_empty() {
            return Ok(None);
        }
        values.sort_by(|a, b| a.0.cmp(&b.0));
        static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let name = format!("HscReifyValues{}", COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
        let refs: Vec<(String, &crate::tc::types::Scheme)> = values.iter().map(|(n, s)| (n.clone(), s)).collect();
        let vm = crate::th::reify_values_module(&refs, module, &name, loc);
        let mut exts = exts.clone();
        exts.apply("NoImplicitPrelude")?;
        self.compile_module(vm, exts, path)?;
        self.ifaces.remove(&name);
        Ok(Some(format!("{name}.{}", crate::th::REIFY_VALUES)))
    }

    /// The data types of other modules that splices quote by name (`''T`), which `reify` may be
    /// asked about: their declarations, with their modules.
    fn quoted_data(&self, bodies: &[&crate::syntax::ast::Expr], current: &str) -> Vec<(String, crate::syntax::ast::DataDecl)> {
        let mut out: Vec<(String, crate::syntax::ast::DataDecl)> = Vec::new();
        for q in crate::th::quoted_types(bodies) {
            for (module, dd) in self.checker.env.data_decls.values() {
                if dd.name == q.name && module != current && !out.iter().any(|(m, d)| m == module && d.name == dd.name) {
                    out.push((module.clone(), dd.clone()));
                }
            }
        }
        out
    }

    /// The expression splices of a module. Those after a declaration splice may refer to the
    /// declarations of the groups before it (the first `groups_end`): they are compiled with
    /// those, under the module's own name.
    fn expand_expr_splices(&mut self, m: &mut Module, exts: &Extensions, path: &Path, groups_end: usize) -> Result<(), String> {
        if !(exts.is_on("TemplateHaskell") || exts.is_on("QuasiQuotes")) || !crate::th::has_splices(m) {
            return Ok(());
        }
        let groups_end = groups_end.min(m.decls.len());
        for lib in ["GHC.Internal.TH.Syntax", "GHC.Internal.TH.Quote", "GHC.Internal.IO.Unsafe", "GHC.Internal.TopHandler"] {
            self.load(lib)?;
        }
        // The splices run with everything loaded so far, so the modules known only by their
        // boot files are compiled first.
        let tb = std::time::Instant::now();
        self.load_booted()?;
        if std::env::var_os("HSC_TIME_SPLICES").is_some() {
            eprintln!("booted modules: {:?}", tb.elapsed());
        }
        let t0 = std::time::Instant::now();
        let timing = std::env::var_os("HSC_TIME_SPLICES").is_some();
        // The splices of the declarations after the last group; those before it have run.
        let mut later = Module { decls: m.decls.split_off(groups_end), ..m.clone() };
        let splices = crate::th::take_splices(&mut later);
        let earlier = m.decls.clone();
        m.decls.extend(later.decls);
        static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let own = !earlier.is_empty();
        let name = if own { m.name.clone() } else { format!("HscSplice{n}") };
        let locs: Vec<crate::syntax::ast::Loc> = splices.iter().map(|s| s.loc).collect();
        let first = locs.first().copied().unwrap_or(m.loc);
        let bodies: Vec<&crate::syntax::ast::Expr> = splices.iter().map(|s| &s.body).collect();
        let imported = self.quoted_data(&bodies, &m.name);
        let table = crate::th::reify_table(&earlier, &m.name, &imported, first);
        let mut smod = crate::th::splice_module(m, &splices, &name, table, &path.display().to_string());
        smod.decls.splice(0..0, earlier);
        let user_module = self.checker.user_module;
        let mark = self.binds.len();
        self.compile_module(smod, exts.clone(), path)?;
        let pre = if own { self.reify_values(&m.name, exts, path, first)? } else { None };
        self.checker.user_module = user_module;
        if timing {
            eprintln!("splices compiled: {:?}", t0.elapsed());
        }
        let pre_io = pre.as_ref().map(|p| format!("{}IO", p));
        let results = match crate::th::run_splices(&self.binds, &name, splices.len(), locs.clone(), pre) {
            // Too costly for the interpreter: the splices run as a native program.
            Err(e) if e == crate::th::SPLICE_BUDGET => {
                self.native_splices(&name, splices.len(), pre_io).and_then(|out| crate::th::native_exps(&out, locs))
            }
            r => r,
        }
        .map_err(|e| format!("{}:{}:{}: error:\n    {e}", path.display(), first.line, first.col))?;
        crate::th::fill_splices(m, &results);
        // The splice module is not part of the program (the module itself is compiled next).
        self.binds.truncate(mark);
        if !own {
            self.ifaces.remove(&name);
        } else {
            self.ifaces.remove(&m.name);
        }
        Ok(())
    }

    fn compile_module(&mut self, m: Module, exts: Extensions, path: &Path) -> Result<(), String> {
        let name = m.name.clone();
        // HSC_PROGRESS: each module compiled, with the compiler's resident memory then.
        if std::env::var_os("HSC_PROGRESS").is_some() {
            let rss = std::fs::read_to_string("/proc/self/statm").ok().and_then(|s| s.split_whitespace().nth(1)?.parse::<u64>().ok()).unwrap_or(0);
            eprintln!("compiling {name} ({} decls, rss {} MB)", m.decls.len(), rss * 4096 / (1 << 20));
        }
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
        self.checker.overloaded_lists = exts.is_on("OverloadedLists");
        self.checker.monomorphism_restriction = exts.is_on("MonomorphismRestriction");
        self.checker.mono_local_binds = exts.is_on("MonoLocalBinds");
        self.checker.applicative_do = exts.is_on("ApplicativeDo");
        self.checker.newtype_deriving = exts.is_on("GeneralizedNewtypeDeriving");
        let t = std::time::Instant::now();
        let (renamed, iface) = crate::rename::rename_module(m, &self.ifaces, exts.is_on("ImplicitPrelude"), boot)
            .map_err(|e| located(e.loc, &e.msg))?;
        timing::add("rename", t);
        let t = std::time::Instant::now();
        let binds = self.checker.check_module(&renamed).map_err(|e| located(e.loc, &e.msg))?;
        timing::add("typecheck", t);
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
    let (mut parsed, exts) = crate::syntax::parse_module_source(&src, flags).map_err(|e| {
        format!("{}:{}:{}: error:\n    {}", main_file.display(), e.line, e.col, e.msg)
    })?;
    d.resolve_package_imports(&mut parsed);
    d.resolve_home_wired_imports(&mut parsed);
    let mut deps: Vec<String> = parsed.imports.iter().map(|i| i.module.clone()).collect();
    if exts.is_on("ImplicitPrelude") && !deps.iter().any(|x| x == "Prelude") {
        deps.insert(0, "Prelude".into());
    }
    if exts.is_on("OverloadedStrings") {
        deps.push("GHC.Internal.Data.String".into());
    }
    if exts.is_on("OverloadedLists") {
        deps.push("GHC.Internal.IsList".into());
    }
    if exts.is_on("MonadComprehensions") {
        deps.push("GHC.Internal.Control.Monad".into());
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
    let mut parsed = parsed;
    d.expand_splices(&mut parsed, &exts, main_file)?;
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
    // HSC_DEBUG_BINDS=file: each binding's name and the size of its Core (to compare builds).
    if let Some(f) = std::env::var_os("HSC_DEBUG_BINDS") {
        // The Core's hash, with the numbers of generated names left out.
        let norm = |t: &str| -> String {
            let mut out = String::with_capacity(t.len());
            let mut digits = false;
            for c in t.chars() {
                if c == '%' || c == '_' {
                    digits = true;
                    out.push(c);
                } else if digits && c.is_ascii_digit() {
                } else {
                    digits = false;
                    out.push(c);
                }
            }
            out
        };
        let text: String = binds.iter().map(|b| format!("{} {}\n", norm(&b.name), md5_hex(norm(&crate::core::pretty::show(&b.body)).as_bytes()))).collect();
        let _ = std::fs::write(f, text);
    }
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
    let text = match String::from_utf8(bytes) {
        Ok(s) => s,
        Err(e) => String::from_utf8_lossy(e.as_bytes()).into_owned(),
    };
    // A literate source (`.lhs`) is its code, as GHC's unlit leaves it.
    Ok(if path.extension().is_some_and(|e| e == "lhs") { unlit(&text) } else { text })
}

/// The code of a literate Haskell source, as GHC's `unlit` gives it: in LaTeX style the lines
/// between `\begin{code}` and `\end{code}`, otherwise the lines starting with `>` (Bird style),
/// whose `>` becomes a space. Every other line becomes empty, so lines and columns are kept.
pub fn unlit(text: &str) -> String {
    let latex = text.lines().any(|l| l.trim_start().starts_with("\\begin{code}"));
    let mut out = String::with_capacity(text.len());
    let mut in_code = false;
    for line in text.lines() {
        if latex {
            let t = line.trim_start();
            if t.starts_with("\\begin{code}") {
                in_code = true;
            } else if t.starts_with("\\end{code}") {
                in_code = false;
            } else if in_code {
                out.push_str(line);
            }
        } else if let Some(code) = line.strip_prefix('>') {
            out.push(' ');
            out.push_str(code);
        } else if line.starts_with('#') {
            // A CPP directive (`#if`, `#include`) stays, as unlit leaves it.
            out.push_str(line);
        }
        out.push('\n');
    }
    out
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

#[cfg(test)]
mod cache_tests {
    use super::*;

    struct Sources(PathBuf);
    impl Sources {
        fn new() -> Self {
            static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!("hsc-boot-cache-{}-{}", std::process::id(), NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
        fn write(&self, name: &str, text: &str) {
            std::fs::write(self.0.join(name), format!("{{-# LANGUAGE NoImplicitPrelude #-}}\n{text}\n")).unwrap();
        }
        fn driver(&self) -> Driver {
            Driver::new(vec![self.0.clone()], Vec::new(), Vec::new())
        }
    }
    impl Drop for Sources {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn boot_key_tracks_transitive_imports_and_full_interface_mode() {
        let s = Sources::new();
        s.write("Dep.hs", "module Dep where\ndata D = D");
        s.write("A.hs", "module A where\nimport Dep\ndata T = T D");
        s.write("A.hs-boot", "module A where\nimport Dep\ndata T = T D");
        let mut first = s.driver();
        first.load_boot("A").unwrap();
        let boot = first.dependency_key("{boot}A").unwrap();
        first.load_boot("A").unwrap();
        assert_eq!(first.dependency_key("{boot}A"), Some(boot.clone()));
        first.load("A").unwrap();
        first.load_boot("A").unwrap();
        assert_ne!(first.dependency_key("{boot}A"), Some(boot.clone()));
        s.write("Dep.hs", "module Dep where\ndata D = Changed");
        let mut second = s.driver();
        second.load_boot("A").unwrap();
        assert_ne!(second.dependency_key("{boot}A"), Some(boot));
    }

    #[test]
    fn implementation_hash_tracks_boot_file_resolved_in_another_search_directory() {
        let source = Sources::new();
        let boot = Sources::new();
        source.write("A.hs", "module A where\ndata T = T");
        boot.write("A.hs-boot", "module A where\ndata T");
        let driver = || Driver::new(vec![source.0.clone(), boot.0.clone()], Vec::new(), Vec::new());
        let mut first = driver();
        assert_eq!(first.find_boot("A"), Some(boot.0.join("A.hs-boot")));
        first.load("A").unwrap();
        boot.write("A.hs-boot", "module A where\ndata T = T");
        let mut second = driver();
        second.load("A").unwrap();
        assert_ne!(first.keys["A"], second.keys["A"]);
    }

    #[test]
    fn nested_source_boot_context_tracks_completion_and_shared_dependencies() {
        let s = Sources::new();
        s.write("D.hs", "module D where\ndata T = T");
        s.write("D.hs-boot", "module D where\ndata T");
        s.write("B.hs-boot", "module B where\nimport {-# SOURCE #-} D\ndata B");
        s.write("C.hs-boot", "module C where\nimport {-# SOURCE #-} D\ndata C");
        s.write("A.hs", "module A where\nimport D\ndata U = U T");
        s.write("A.hs-boot", "module A where\nimport {-# SOURCE #-} B\nimport {-# SOURCE #-} C\ndata U");
        let mut d = s.driver();
        d.load_boot("A").unwrap();
        let first = d.boot_key("A").unwrap();
        d.boot_deps.get_mut("D").unwrap().push("{boot}A".into());
        assert!(d.boot_key("A").is_none());
        d.boot_deps.get_mut("D").unwrap().pop();
        assert_eq!(d.boot_key("A"), Some(first.clone()));
        let base = d.keys.remove("{boot}D").unwrap();
        assert!(d.boot_key("A").is_none());
        d.keys.insert("{boot}D".into(), base);
        assert_eq!(d.boot_key("A"), Some(first.clone()));
        d.load("D").unwrap();
        assert_ne!(d.boot_key("A"), Some(first));
        d.load("A").unwrap();
        assert!(d.keys.contains_key("A"));
    }

    #[test]
    fn cache_rejects_different_precompletion_boot_context() {
        let s = Sources::new();
        s.write("A.hs-boot", "module A where\ndata T");
        let mut d = s.driver();
        d.load_boot("A").unwrap();
        let before = d.boot_key("A");
        d.checker.env.instances.push(crate::tc::env::InstInfo {
            class: "External.C".into(),
            nvars: 0,
            ctx: Vec::new(),
            head: vec![crate::tc::types::Ty::con("A.T")],
            dict: "A.$fOrphan".into(),
            kinds: Vec::new(),
        });
        assert_ne!(d.boot_key("A"), before);
        let entry = crate::cache::Entry {
            boot_key: None,
            deps: Vec::new(),
            iface: d.ifaces["A"].clone(),
            binds: Vec::new(),
            env: crate::cache::EnvDelta::default(),
            explicit_sigs: Vec::new(),
        };
        assert!(!d.use_cached("A", entry, "source", &s.0.join("A.hs"), true));
        assert!(!d.loaded.contains("A"));
        let entry = crate::cache::Entry {
            boot_key: d.boot_key("A"),
            deps: Vec::new(),
            iface: d.ifaces["A"].clone(),
            binds: Vec::new(),
            env: crate::cache::EnvDelta::default(),
            explicit_sigs: Vec::new(),
        };
        assert!(d.use_cached("A", entry, "source", &s.0.join("A.hs"), true));
    }
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
        // A C library to link (`-lm`) or a compiled object is passed on as it is.
        if src.to_string_lossy().starts_with("-l") || src.extension().is_some_and(|e| e == "o") {
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
    // The wrappers pass array and state arguments (`MutableByteArray# s`) as 64-bit integers, which
    // a pointer parameter receives the same way on x86-64: newer compilers would make that an error.
    let mut flags = flags.to_vec();
    flags.extend(["-cc-option=-Wno-int-conversion".to_string(), "-cc-option=-Wno-incompatible-pointer-types".to_string()]);
    Ok(Some((path, flags)))
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
    if !has("hsc$main") && has("GHC.Internal.TopHandler.runMainIO") && has("Main.main") && has("GHC.Internal.TopHandler.flushStdHandles") && has("GHC.Internal.Base.thenIO") {
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
    let t = std::time::Instant::now();
    let binds = crate::native::reachable(binds, &roots);
    let prog = crate::native::stg::Lowering::program(&binds);
    let mut text = crate::native::codegen::Gen::program(&prog);
    text.push_str(&crate::native::program_main());
    std::fs::write(&asm, text).map_err(|e| e.to_string())?;
    timing::add("codegen", t);
    let t_link = std::time::Instant::now();
    let run = |args: &[&str]| -> Result<(), String> {
        let st = std::process::Command::new("gcc").args(args).status().map_err(|e| format!("gcc: {e}"))?;
        if st.success() { Ok(()) } else { Err(format!("gcc {} failed", args.join(" "))) }
    };
    // HSC_DEBUG keeps the symbols and the generated assembly (its directory is printed).
    let debug = std::env::var_os("HSC_DEBUG").is_some();
    let c_objs = compile_c_sources(c_sources, &dir)?;
    let c_objs: Vec<String> = c_objs.iter().map(|p| p.display().to_string()).collect();
    let result = run(&["-c", &asm.display().to_string(), "-o", &obj.display().to_string()]).and_then(|()| {
        timing::add("assemble", t_link);
        let t_link = std::time::Instant::now();
        let r = (|| {
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
        })();
        timing::add("link", t_link);
        r
    });
    if debug {
        eprintln!("hsc: assembly kept in {}", dir.display());
    } else {
        let _ = std::fs::remove_dir_all(&dir);
    }
    result
}
