//! Core: the untyped, explicitly-dictionary-passing intermediate language.
//!
//! Type classes are already elaborated into ordinary function arguments, pattern
//! matching is compiled to `Case`, and `do`, list comprehensions and sections are
//! desugared. Variables are unique strings: globals are `Module.name`, locals carry a
//! `%N` suffix. Application and abstraction are single-argument.

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
