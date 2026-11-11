//! Package databases, in GHC's format: a database is a directory of `.conf` files, each one the
//! text of an installed package (`name: value` fields, continuation lines indented). `hsc-pkg`
//! edits them and the driver reads them to find the modules of the packages a program uses.
//!
//! hsc compiles programs from source, so a package's `import-dirs` hold, for each module, either
//! its source file or the interface file written when the package was built, which names the
//! source and the flags it was compiled with (see `driver`).

use std::path::{Path, PathBuf};

/// An installed package: its fields in order, with the raw (multi-line) values.
#[derive(Clone, Debug, Default)]
pub struct Package {
    pub fields: Vec<(String, String)>,
}

impl Package {
    /// A field's value (field names are case-insensitive), trimmed.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.fields.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.trim())
    }

    /// A list field: the words of the value, separated by commas or white space, with quoted
    /// strings (Haskell syntax) unquoted.
    pub fn list(&self, name: &str) -> Vec<String> {
        self.get(name).map(split_list).unwrap_or_default()
    }

    pub fn id(&self) -> String {
        self.get("id").or_else(|| self.get("key")).unwrap_or("").to_string()
    }

    pub fn name(&self) -> String {
        self.get("name").unwrap_or("").to_string()
    }

    pub fn version(&self) -> String {
        self.get("version").unwrap_or("").to_string()
    }

    /// The exposed modules, without the `from` part of reexports.
    pub fn exposed_modules(&self) -> Vec<String> {
        let text = self.get("exposed-modules").unwrap_or("");
        let mut out = Vec::new();
        for item in text.split(',') {
            for (i, w) in item.split_whitespace().enumerate() {
                if i == 0 {
                    out.push(w.to_string());
                } else {
                    break;
                }
            }
        }
        out
    }

    pub fn set(&mut self, name: &str, value: &str) {
        match self.fields.iter_mut().find(|(n, _)| n.eq_ignore_ascii_case(name)) {
            Some(f) => f.1 = value.to_string(),
            None => self.fields.push((name.to_string(), value.to_string())),
        }
    }

    /// The package's text, as it is written to a database.
    pub fn render(&self) -> String {
        let mut out = String::new();
        for (n, v) in &self.fields {
            let v = v.trim();
            if v.contains('\n') {
                out.push_str(&format!("{n}:\n"));
                for line in v.lines() {
                    out.push_str(&format!("    {}\n", line.trim()));
                }
            } else {
                out.push_str(&format!("{n}: {v}\n"));
            }
        }
        out
    }

    /// Replaces `${pkgroot}` (the directory that holds the database) in the path fields.
    pub fn expand_pkgroot(&mut self, root: &Path) {
        let r = root.display().to_string();
        for (_, v) in self.fields.iter_mut() {
            if v.contains("${pkgroot}") {
                *v = v.replace("${pkgroot}", &r);
            }
        }
    }
}

/// Parses the text of one or more packages (separated by `---` lines).
pub fn parse(text: &str) -> Result<Vec<Package>, String> {
    let mut out = Vec::new();
    let mut cur = Package::default();
    for (lineno, line) in text.lines().enumerate() {
        if line.trim() == "---" {
            if !cur.fields.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            continue;
        }
        if line.trim().is_empty() {
            continue;
        }
        if line.starts_with(' ') || line.starts_with('\t') {
            match cur.fields.last_mut() {
                Some((_, v)) => {
                    v.push('\n');
                    v.push_str(line.trim());
                }
                None => return Err(format!("line {}: continuation without a field", lineno + 1)),
            }
            continue;
        }
        let Some((name, value)) = line.split_once(':') else {
            return Err(format!("line {}: expected `field: value`", lineno + 1));
        };
        cur.fields.push((name.trim().to_string(), value.trim().to_string()));
    }
    if !cur.fields.is_empty() {
        out.push(cur);
    }
    Ok(out)
}

/// The words of a list field.
pub fn split_list(v: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = v.chars().peekable();
    let mut word = String::new();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                // A Haskell string literal.
                while let Some(d) = chars.next() {
                    match d {
                        '"' => break,
                        '\\' => {
                            if let Some(e) = chars.next() {
                                word.push(match e {
                                    'n' => '\n',
                                    't' => '\t',
                                    other => other,
                                });
                            }
                        }
                        other => word.push(other),
                    }
                }
            }
            ',' => {
                if !word.is_empty() {
                    out.push(std::mem::take(&mut word));
                }
            }
            c if c.is_whitespace() => {
                if !word.is_empty() {
                    out.push(std::mem::take(&mut word));
                }
            }
            c => word.push(c),
        }
    }
    if !word.is_empty() {
        out.push(word);
    }
    out
}

/// A package database: a directory of `.conf` files, or a single file of packages.
pub struct Db {
    pub path: PathBuf,
    pub packages: Vec<Package>,
}

impl Db {
    /// Reads a database; `${pkgroot}` is its parent directory.
    pub fn load(path: &Path) -> Result<Db, String> {
        let mut packages = Vec::new();
        if path.is_dir() {
            let mut files: Vec<PathBuf> = std::fs::read_dir(path)
                .map_err(|e| format!("{}: {e}", path.display()))?
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().is_some_and(|x| x == "conf"))
                .collect();
            files.sort();
            for f in files {
                let text = std::fs::read_to_string(&f).map_err(|e| format!("{}: {e}", f.display()))?;
                packages.extend(parse(&text).map_err(|e| format!("{}: {e}", f.display()))?);
            }
        } else if path.is_file() {
            let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
            // An empty old-style database is `[]`.
            if text.trim() != "[]" {
                packages = parse(&text).map_err(|e| format!("{}: {e}", path.display()))?;
            }
        } else {
            return Err(format!("{}: package database does not exist", path.display()));
        }
        let root = path.parent().map(Path::to_path_buf).unwrap_or_default();
        for p in packages.iter_mut() {
            p.expand_pkgroot(&root);
        }
        Ok(Db { path: path.to_path_buf(), packages })
    }

    /// Adds a package, replacing one with the same id.
    pub fn register(&mut self, p: Package) -> Result<(), String> {
        let id = p.id();
        if id.is_empty() {
            return Err("package has no id".into());
        }
        if !self.path.is_dir() {
            return Err(format!("{}: not a package database directory", self.path.display()));
        }
        let file = self.path.join(format!("{id}.conf"));
        std::fs::write(&file, p.render()).map_err(|e| format!("{}: {e}", file.display()))?;
        self.packages.retain(|q| q.id() != id);
        self.packages.push(p);
        Ok(())
    }

    /// Removes the packages with a name or id; answers how many.
    pub fn unregister(&mut self, name_or_id: &str) -> Result<usize, String> {
        let gone: Vec<String> = self
            .packages
            .iter()
            .filter(|p| p.id() == name_or_id || p.name() == name_or_id || format!("{}-{}", p.name(), p.version()) == name_or_id)
            .map(|p| p.id())
            .collect();
        for id in &gone {
            let file = self.path.join(format!("{id}.conf"));
            let _ = std::fs::remove_file(file);
        }
        self.packages.retain(|p| !gone.contains(&p.id()));
        Ok(gone.len())
    }
}

/// The global package database of hsc: the vendored packages, in the library directory.
pub fn global_db_path() -> PathBuf {
    let dir = crate::driver::default_lib_dir();
    std::fs::canonicalize(&dir).unwrap_or(dir).join("package.conf.d")
}

/// The user package database (`~/.hsc/x86_64-linux-9.12.2/package.conf.d`), when it exists.
pub fn user_db_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let p = PathBuf::from(home).join(".hsc").join("x86_64-linux-9.12.2").join("package.conf.d");
    p.is_dir().then_some(p)
}

/// The packages a compilation can see: the databases in order, later ones shadowing earlier ones.
pub struct Stack {
    pub dbs: Vec<Db>,
}

impl Stack {
    pub fn load(paths: &[PathBuf]) -> Result<Stack, String> {
        let mut dbs = Vec::new();
        for p in paths {
            dbs.push(Db::load(p)?);
        }
        Ok(Stack { dbs })
    }

    /// The package with an id (the last database that has it).
    pub fn by_id(&self, id: &str) -> Option<&Package> {
        self.dbs.iter().rev().flat_map(|d| d.packages.iter()).find(|p| p.id() == id)
    }

    /// The package named `name` (or `name-version`) with the highest version.
    pub fn by_name(&self, name: &str) -> Option<&Package> {
        let mut best: Option<&Package> = None;
        for p in self.dbs.iter().flat_map(|d| d.packages.iter()) {
            let full = format!("{}-{}", p.name(), p.version());
            if p.name() != name && full != name && p.id() != name {
                continue;
            }
            if best.is_none_or(|b| version_key(&p.version()) >= version_key(&b.version())) {
                best = Some(p);
            }
        }
        best
    }

    /// A package and everything it depends on, each once, dependencies first.
    pub fn closure(&self, roots: &[String]) -> Result<Vec<Package>, String> {
        let mut out: Vec<Package> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        fn visit(s: &Stack, id: &str, seen: &mut std::collections::HashSet<String>, out: &mut Vec<Package>) -> Result<(), String> {
            if !seen.insert(id.to_string()) {
                return Ok(());
            }
            let p = s.by_id(id).ok_or_else(|| format!("cannot satisfy -package-id {id}"))?.clone();
            for d in p.list("depends") {
                visit(s, &d, seen, out)?;
            }
            out.push(p);
            Ok(())
        }
        for r in roots {
            visit(self, r, &mut seen, &mut out)?;
        }
        Ok(out)
    }
}

/// A version as numbers, for comparison.
pub fn version_key(v: &str) -> Vec<u64> {
    v.split('.').map(|x| x.parse().unwrap_or(0)).collect()
}
