//! hsc-pkg: the package manager of hsc, compatible with the `ghc-pkg` commands that cabal uses
//! (`--version`, `init`, `dump`, `list`, `field`, `describe`, `register`, `update`, `unregister`,
//! `recache`, `check`).

use hsc::pkg::{self, Db, Package};
use std::io::Read;
use std::path::PathBuf;
use std::process::ExitCode;

extern "C" {
    fn signal(sig: i32, handler: usize) -> usize;
}

const USAGE: &str = "usage: hsc-pkg [--global] [--user] [--package-db=DB]... COMMAND
commands: init DB | dump | list | field PKG FIELD | describe PKG | register FILE|- | update FILE|-
          unregister PKG | expose PKG | hide PKG | recache | check | --version";

fn main() -> ExitCode {
    // A reader that stops early (`| head`) ends the output quietly.
    // SAFETY: restores the default action of SIGPIPE before any output.
    unsafe {
        signal(13, 0);
    }
    match run(std::env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("hsc-pkg: {e}");
            ExitCode::from(1)
        }
    }
}

/// The databases a command reads (in order) and the one it changes (the last named).
struct DbFlags {
    global: bool,
    user: bool,
    dbs: Vec<PathBuf>,
}

impl DbFlags {
    fn stack(&self) -> Vec<PathBuf> {
        let mut out = Vec::new();
        if self.global || self.dbs.is_empty() {
            out.push(pkg::global_db_path());
        }
        if self.user {
            if let Some(u) = pkg::user_db_path() {
                out.push(u);
            }
        }
        out.extend(self.dbs.iter().cloned());
        out
    }

    /// The database a command writes: the last `--package-db`, else the global one.
    fn target(&self) -> PathBuf {
        self.dbs.last().cloned().unwrap_or_else(pkg::global_db_path)
    }
}

fn run(args: Vec<String>) -> Result<(), String> {
    let mut flags = DbFlags { global: false, user: false, dbs: Vec::new() };
    let mut words: Vec<String> = Vec::new();
    let mut field_sep_simple = false;
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        if let Some(db) = a.strip_prefix("--package-db=").or_else(|| a.strip_prefix("--package-conf=")).or_else(|| a.strip_prefix("--db=")) {
            flags.dbs.push(PathBuf::from(db));
            continue;
        }
        match a.as_str() {
            "--version" | "-V" => {
                println!("GHC package manager version 9.12.2");
                return Ok(());
            }
            "--numeric-version" => {
                println!("9.12.2");
                return Ok(());
            }
            "--help" | "-?" => {
                println!("{USAGE}");
                return Ok(());
            }
            "--package-db" | "--package-conf" => flags.dbs.push(PathBuf::from(it.next().ok_or("--package-db needs a value")?)),
            "--global" => flags.global = true,
            "--user" => flags.user = true,
            "--simple-output" => field_sep_simple = true,
            "--no-user-package-db" | "--no-user-package-conf" | "--force" | "--force-files" | "-f" | "--expand-pkgroot"
            | "--no-expand-pkgroot" | "--enable-multi-instance" | "--expand-env-vars" | "--ipid" | "--unit-id" => {}
            s if s.starts_with("-v") || s.starts_with("--verbose") => {}
            s if s.starts_with("--global-package-db=") => {}
            s if s.starts_with('-') && s.len() > 1 => return Err(format!("unknown option {s}")),
            _ => words.push(a),
        }
    }
    let Some(cmd) = words.first().cloned() else { return Err(USAGE.into()) };
    let rest = &words[1..];
    match cmd.as_str() {
        "init" => {
            let dir = rest.first().ok_or("init needs a path")?;
            let p = PathBuf::from(dir);
            if p.exists() {
                return Err(format!("{dir}: already exists"));
            }
            std::fs::create_dir_all(&p).map_err(|e| format!("{dir}: {e}"))
        }
        "dump" => {
            let stack = pkg::Stack::load(&flags.stack())?;
            let mut first = true;
            for db in &stack.dbs {
                for p in &db.packages {
                    if !first {
                        println!("---");
                    }
                    first = false;
                    print!("{}", p.render());
                }
            }
            Ok(())
        }
        "list" => {
            let stack = pkg::Stack::load(&flags.stack())?;
            for db in &stack.dbs {
                if field_sep_simple {
                    let names: Vec<String> = db.packages.iter().map(|p| format!("{}-{}", p.name(), p.version())).collect();
                    println!("{}", names.join(" "));
                } else {
                    println!("{}:", db.path.display());
                    for p in &db.packages {
                        println!("    {}-{}", p.name(), p.version());
                    }
                    println!();
                }
            }
            Ok(())
        }
        "field" | "describe" => {
            let name = rest.first().ok_or("which package?")?;
            let stack = pkg::Stack::load(&flags.stack())?;
            let p = stack.by_name(name).ok_or_else(|| format!("cannot find package {name}"))?;
            if cmd == "describe" {
                print!("{}", p.render());
                return Ok(());
            }
            let fields = rest.get(1).ok_or("which field?")?;
            for f in fields.split(',') {
                match p.get(f) {
                    Some(v) if field_sep_simple => println!("{v}"),
                    Some(v) => println!("{f}: {v}"),
                    None => return Err(format!("no field {f}")),
                }
            }
            Ok(())
        }
        "register" | "update" => {
            let src = rest.first().map(String::as_str).unwrap_or("-");
            let text = if src == "-" {
                let mut s = String::new();
                std::io::stdin().read_to_string(&mut s).map_err(|e| e.to_string())?;
                s
            } else {
                std::fs::read_to_string(src).map_err(|e| format!("{src}: {e}"))?
            };
            let target = flags.target();
            let mut db = Db::load(&target)?;
            for mut p in pkg::parse(&text)? {
                if p.get("id").is_none() {
                    let id = format!("{}-{}", p.name(), p.version());
                    p.set("id", &id);
                }
                if p.get("key").is_none() {
                    let id = p.id();
                    p.set("key", &id);
                }
                db.register(p)?;
            }
            Ok(())
        }
        "unregister" => {
            let name = rest.first().ok_or("which package?")?;
            let mut db = Db::load(&flags.target())?;
            match db.unregister(name)? {
                0 => Err(format!("cannot find package {name}")),
                _ => Ok(()),
            }
        }
        "expose" | "hide" | "trust" | "distrust" => {
            let name = rest.first().ok_or("which package?")?;
            let mut db = Db::load(&flags.target())?;
            let mut found: Vec<Package> =
                db.packages.iter().filter(|p| p.name() == *name || p.id() == *name).cloned().collect();
            if found.is_empty() {
                return Err(format!("cannot find package {name}"));
            }
            for p in found.iter_mut() {
                match cmd.as_str() {
                    "expose" => p.set("exposed", "True"),
                    "hide" => p.set("exposed", "False"),
                    "trust" => p.set("trusted", "True"),
                    _ => p.set("trusted", "False"),
                }
                db.register(p.clone())?;
            }
            Ok(())
        }
        // The databases are read as text: there is no cache to rebuild, and nothing to check.
        "recache" | "check" => Ok(()),
        other => Err(format!("unknown command {other}\n{USAGE}")),
    }
}
