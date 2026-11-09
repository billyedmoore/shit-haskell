//! The driver: finds and loads modules, runs the renamer and type checker over them in
//! dependency order, and assembles the program's Core.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use crate::core::TopBind;
use crate::rename::iface::Iface;
use crate::syntax::ast::Module;
use crate::syntax::extensions::Extensions;
use crate::tc::Checker;

pub struct Driver {
    pub search: Vec<PathBuf>,
    pub ifaces: BTreeMap<String, Iface>,
    pub checker: Checker,
    pub binds: Vec<TopBind>,
    loaded: HashSet<String>,
    loading: Vec<String>,
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
        Driver {
            search: vec![main_dir.to_path_buf(), default_lib_dir()],
            ifaces: BTreeMap::new(),
            checker: Checker::new(),
            binds: Vec::new(),
            loaded: HashSet::new(),
            loading: Vec::new(),
            flags,
        }
    }

    fn find_source(&self, module: &str) -> Option<PathBuf> {
        let rel = format!("{}.hs", module.replace('.', "/"));
        self.search.iter().map(|d| d.join(&rel)).find(|p| p.is_file())
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
        let src = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let (parsed, exts) = crate::syntax::parse_module_source(&src, &self.flags).map_err(|e| {
            format!("{}:{}:{}: error:\n    {}", path.display(), e.line, e.col, e.msg)
        })?;
        self.loading.push(module.to_string());
        let mut deps: Vec<String> = parsed.imports.iter().map(|i| i.module.clone()).collect();
        if exts.is_on("ImplicitPrelude") && module != "Prelude" && !deps.iter().any(|d| d == "Prelude") {
            deps.insert(0, "Prelude".into());
        }
        for d in &deps {
            if let Err(e) = self.load(d) {
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
        self.checker.source_name = path.display().to_string();
        let located = |loc: crate::syntax::ast::Loc, msg: &str| {
            format!("{}:{}:{}: error:\n    {}", path.display(), loc.line, loc.col, msg)
        };
        let (renamed, iface) = crate::rename::rename_module(m, &self.ifaces, exts.is_on("ImplicitPrelude"))
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
    let src = std::fs::read_to_string(main_file).map_err(|e| format!("{}: {e}", main_file.display()))?;
    let (parsed, exts) = crate::syntax::parse_module_source(&src, &flags).map_err(|e| {
        format!("{}:{}:{}: error:\n    {}", main_file.display(), e.line, e.col, e.msg)
    })?;
    let mut deps: Vec<String> = parsed.imports.iter().map(|i| i.module.clone()).collect();
    if exts.is_on("ImplicitPrelude") && !deps.iter().any(|x| x == "Prelude") {
        deps.insert(0, "Prelude".into());
    }
    for dep in &deps {
        d.load(dep)?;
    }
    let name = parsed.name.clone();
    d.compile_module(parsed, exts, main_file)?;
    if name == "Main" && !d.checker.env.values.contains_key("Main.main") {
        return Err(format!(
            "{}:1:1: error:\n    The IO action ‘main’ is not defined in module ‘Main’",
            main_file.display()
        ));
    }
    let mut binds = std::mem::take(&mut d.binds);
    crate::core::opt::specialise_literals(&mut binds);
    Ok(binds)
}
