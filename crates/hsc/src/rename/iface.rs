//! Module interfaces: what a module exports, and the fixities it declares.

use crate::syntax::ast::Assoc;
use std::collections::BTreeMap;

/// A global entity name: the defining module and the occurrence name.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GName {
    pub module: String,
    pub name: String,
}

impl GName {
    pub fn new(module: impl Into<String>, name: impl Into<String>) -> Self {
        GName {
            module: module.into(),
            name: name.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntKind {
    /// A pattern synonym (in the value namespace, like a constructor).
    PatSyn,
    /// A value binding: function, variable, or foreign import.
    Var,
    /// A data constructor.
    Con,
    /// A record field selector (also a value).
    Field,
    /// A class method (also a value).
    Method,
    /// A type constructor or type synonym.
    TyCon,
    /// A class.
    Class,
}

impl EntKind {
    /// True for entities that live in the value namespace.
    pub fn is_value(self) -> bool {
        matches!(self, EntKind::Var | EntKind::Con | EntKind::Field | EntKind::Method | EntKind::PatSyn)
    }
}

/// An exported or in-scope entity, with the sub-entities that `T(..)` brings along.
#[derive(Clone, Debug, PartialEq)]
pub struct Entity {
    pub kind: EntKind,
    pub global: GName,
    /// For types and classes: constructors and fields, or methods.
    pub subs: Vec<Entity>,
    /// For constructors: record field names in declaration order.
    pub fields: Vec<String>,
}

impl Entity {
    /// The name a program refers to the entity by (a duplicate record field's internal name
    /// `f%T` is referred to as `f`).
    pub fn occ(&self) -> &str {
        if self.kind == EntKind::Field {
            return field_label(&self.global.name);
        }
        &self.global.name
    }
}

/// The source name of a record field: a field several types of a module declare
/// (DuplicateRecordFields) is named `f%T` internally.
pub fn field_label(name: &str) -> &str {
    name.split('%').next().unwrap_or(name)
}

/// A pattern synonym after renaming: its argument variables (as bound in `pat`) and its
/// pattern. Uses in patterns are expanded by substitution; uses in expressions call the builder
/// `$bP` when `builder` is set.
#[derive(Clone, Debug, PartialEq)]
pub struct PatSynDef {
    pub params: Vec<String>,
    /// The parameters as the source names them (a record synonym's field names).
    pub names: Vec<String>,
    pub pat: crate::syntax::ast::Pat,
    pub builder: bool,
    /// The argument types of the synonym's signature that mention no type variable (renamed): the
    /// arguments of a use get them as pattern signatures, so that the expansion keeps their types.
    pub arg_types: Vec<Option<crate::syntax::ast::Type>>,
}

#[derive(Clone, Debug, Default)]
pub struct Iface {
    pub module: String,
    pub exports: Vec<Entity>,
    pub fixities: BTreeMap<GName, (Assoc, u8)>,
    pub patsyns: BTreeMap<GName, PatSynDef>,
}
