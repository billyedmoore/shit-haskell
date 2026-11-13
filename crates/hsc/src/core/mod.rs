//! Core: the untyped, explicitly-dictionary-passing intermediate language.
//!
//! Type classes are already elaborated into ordinary function arguments, pattern
//! matching is compiled to `Case`, and `do`, list comprehensions and sections are
//! desugared. Variables are unique strings: globals are `Module.name`, locals carry a
//! `%N` suffix. Application and abstraction are single-argument.

pub mod lint;
pub mod matcher;
pub mod opt;
pub mod pretty;
pub mod serial;

#[derive(Clone, Debug, PartialEq)]
pub enum Lit {
    /// Arbitrary-precision integer in decimal.
    Integer(String),
    /// A literal already known to be an `Int`.
    Int(i64),
    /// A literal already known to be a `Double`.
    Double(f64),
    /// Floating literal as written, e.g. `1.5e-3`.
    Frac(String),
    Char(char),
    /// A string literal, which evaluates to a list of characters.
    Str(String),
    /// An `Addr#` literal: the bytes of the string, with a terminating NUL added at run time.
    Addr(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Core {
    Var(String),
    Lit(Lit),
    App(Box<Core>, Box<Core>),
    Lam(String, Box<Core>),
    /// Recursive let: every binding may refer to every other.
    Let(Vec<(String, Core)>, Box<Core>),
    /// Evaluates the scrutinee, binds it to the variable, and selects an alternative.
    Case(Box<Core>, String, Vec<Alt>),
    /// Saturated constructor application. The name is the constructor's global name.
    Con(String, Vec<Core>),
    /// Saturated primitive operation.
    Prim(String, Vec<Core>),
    /// Raises a pattern-match or other runtime failure with a message.
    Fail(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Alt {
    Con(String, Vec<String>, Core),
    Lit(Lit, Core),
    Default(Core),
}

/// A top-level binding.
#[derive(Clone, Debug, PartialEq)]
pub struct TopBind {
    pub name: String,
    pub body: Core,
}

/// Where this compilation starts numbering the local names it makes: a number of its own (from
/// the time and process), so that the names in Core loaded from the module cache, made by other
/// compilations, are not made again here (the program-wide passes take local names to be
/// unique).
pub fn name_base() -> u64 {
    static BASE: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
    *BASE.get_or_init(|| {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64);
        let mut x = nanos ^ ((std::process::id() as u64) << 32) ^ 0x9e37_79b9_7f4a_7c15;
        // A mix of the bits (splitmix64), then a range of 2^24 names for this compilation.
        x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        x ^= x >> 31;
        (x % (1 << 36)) << 24
    })
}

/// Replaces every occurrence of each variable the map names by its new name (in one pass).
pub fn rename_vars(c: &mut Core, map: &std::collections::HashMap<String, String>) {
    match c {
        Core::Var(v) => {
            if let Some(n) = map.get(v.as_str()) {
                *v = n.clone();
            }
        }
        Core::Lit(_) | Core::Fail(_) => {}
        Core::App(f, a) => {
            rename_vars(f, map);
            rename_vars(a, map);
        }
        Core::Lam(_, b) => rename_vars(b, map),
        Core::Let(binds, body) => {
            for (_, b) in binds.iter_mut() {
                rename_vars(b, map);
            }
            rename_vars(body, map);
        }
        Core::Case(s, _, alts) => {
            rename_vars(s, map);
            for a in alts.iter_mut() {
                match a {
                    Alt::Con(_, _, b) | Alt::Lit(_, b) | Alt::Default(b) => rename_vars(b, map),
                }
            }
        }
        Core::Con(_, args) | Core::Prim(_, args) => args.iter_mut().for_each(|a| rename_vars(a, map)),
    }
}

/// Replaces every occurrence of the variable `from` by `to`. Names are unique, so no
/// binder can capture the replacement.
pub fn rename_var(c: &mut Core, from: &str, to: &str) {
    match c {
        Core::Var(v) => {
            if v == from {
                *v = to.to_string();
            }
        }
        Core::Lit(_) | Core::Fail(_) => {}
        Core::App(f, a) => {
            rename_var(f, from, to);
            rename_var(a, from, to);
        }
        Core::Lam(_, b) => rename_var(b, from, to),
        Core::Let(binds, body) => {
            for (_, b) in binds.iter_mut() {
                rename_var(b, from, to);
            }
            rename_var(body, from, to);
        }
        Core::Case(s, _, alts) => {
            rename_var(s, from, to);
            for a in alts.iter_mut() {
                match a {
                    Alt::Con(_, _, b) | Alt::Lit(_, b) | Alt::Default(b) => rename_var(b, from, to),
                }
            }
        }
        Core::Con(_, args) | Core::Prim(_, args) => args.iter_mut().for_each(|a| rename_var(a, from, to)),
    }
}
