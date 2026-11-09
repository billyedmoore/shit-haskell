//! Global type-checking environment, accumulated across modules.

use super::types::{Pred, Scheme, Ty};
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct TyConInfo {
    pub arity: usize,
    /// Global names of the constructors, in declaration order.
    pub cons: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct DataCon {
    /// Global name, e.g. `Prelude.Just`.
    pub name: String,
    pub tycon: String,
    /// Zero-based position among the type's constructors.
    pub tag: usize,
    pub arity: usize,
    /// Type of the constructor as a function, quantified over the type's parameters.
    pub scheme: Scheme,
    /// Record field global names by position, if the constructor has labelled fields.
    pub fields: Vec<String>,
    /// Newtype constructors are erased at run time: they are the identity.
    pub newtype: bool,
}

#[derive(Clone, Debug)]
pub struct Synonym {
    pub nparams: u32,
    /// Right-hand side with the parameters as `Gen(0..nparams)`.
    pub rhs: Ty,
}

#[derive(Clone, Debug)]
pub struct ClassInfo {
    pub name: String,
    pub nparams: u32,
    /// Superclass predicates over the class parameters `Gen(0..nparams)`.
    pub supers: Vec<Pred>,
    /// Methods: global name, and the full scheme (class parameters first, then the
    /// method's own variables; the class predicate is the first predicate).
    pub methods: Vec<(String, Scheme)>,
    /// Methods that have a default implementation, as `$dm` global names.
    pub defaults: HashMap<String, String>,
    /// Constructor of the dictionary record.
    pub dict_con: String,
    /// Functional dependencies, as parameter positions: determining and determined.
    pub fundeps: Vec<(Vec<usize>, Vec<usize>)>,
}

impl ClassInfo {
    /// Number of dictionary fields before the methods (the superclass dictionaries).
    pub fn nsupers(&self) -> usize {
        self.supers.len()
    }
}

#[derive(Clone, Debug)]
pub struct InstInfo {
    pub class: String,
    pub nvars: u32,
    /// Instance context, over `Gen(0..nvars)`.
    pub ctx: Vec<Pred>,
    /// Class arguments of the instance head, over `Gen(0..nvars)`.
    pub head: Vec<Ty>,
    /// Global name of the dictionary function (a value when `ctx` is empty).
    pub dict: String,
}

/// One equation of a type family: patterns over `Gen(0..nvars)`, and the right-hand side.
#[derive(Clone, Debug)]
pub struct FamEqn {
    pub nvars: u32,
    pub args: Vec<Ty>,
    pub rhs: Ty,
}

#[derive(Clone, Debug)]
pub struct FamilyInfo {
    pub arity: usize,
    /// Closed families try their equations in order.
    pub closed: bool,
    pub equations: Vec<FamEqn>,
}

#[derive(Default)]
pub struct Env {
    /// Type families by global name.
    pub families: HashMap<String, FamilyInfo>,
    pub tycons: HashMap<String, TyConInfo>,
    pub synonyms: HashMap<String, Synonym>,
    pub datacons: HashMap<String, DataCon>,
    pub classes: HashMap<String, ClassInfo>,
    pub instances: Vec<InstInfo>,
    /// Types of all top-level values, including constructors, selectors and methods.
    pub values: HashMap<String, Scheme>,
    /// Record field name to (constructor, position) pairs.
    pub fields: HashMap<String, Vec<(String, usize)>>,
}
