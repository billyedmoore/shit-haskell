//! Global type-checking environment, accumulated across modules.

use super::types::{Pred, Scheme, Ty};
use std::collections::HashMap;

/// A global environment only gains keys. Remember their insertion order so a module's
/// additions can be collected without copying or scanning every earlier module's keys.
pub struct EnvMap<V> {
    entries: HashMap<String, V>,
    added: Vec<String>,
}

impl<V> Default for EnvMap<V> {
    fn default() -> Self {
        Self { entries: HashMap::new(), added: Vec::new() }
    }
}

impl<V> EnvMap<V> {
    pub fn mark(&self) -> usize {
        self.added.len()
    }

    pub fn added_since(&self, mark: usize) -> impl Iterator<Item = (&String, &V)> {
        self.added[mark..].iter().map(|k| (k, &self.entries[k]))
    }

    pub fn insert(&mut self, key: String, value: V) -> Option<V> {
        match self.entries.entry(key) {
            std::collections::hash_map::Entry::Occupied(mut e) => Some(e.insert(value)),
            std::collections::hash_map::Entry::Vacant(e) => {
                self.added.push(e.key().clone());
                e.insert(value);
                None
            }
        }
    }

    pub fn get_mut(&mut self, key: &str) -> Option<&mut V> {
        self.entries.get_mut(key)
    }

    pub fn get_or_insert_default(&mut self, key: String) -> &mut V where V: Default {
        match self.entries.entry(key) {
            std::collections::hash_map::Entry::Occupied(e) => e.into_mut(),
            std::collections::hash_map::Entry::Vacant(e) => {
                self.added.push(e.key().clone());
                e.insert(V::default())
            }
        }
    }
}

// Expose read-only HashMap operations; every insertion must go through the journal above.
impl<V> std::ops::Deref for EnvMap<V> {
    type Target = HashMap<String, V>;
    fn deref(&self) -> &Self::Target { &self.entries }
}

impl<'a, V> IntoIterator for &'a EnvMap<V> {
    type Item = (&'a String, &'a V);
    type IntoIter = std::collections::hash_map::Iter<'a, String, V>;
    fn into_iter(self) -> Self::IntoIter { self.entries.iter() }
}

impl<V> Extend<(String, V)> for EnvMap<V> {
    fn extend<T: IntoIterator<Item = (String, V)>>(&mut self, iter: T) {
        for (key, value) in iter { self.insert(key, value); }
    }
}

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
    /// Strictness of each field: strict fields are forced when the constructor is applied.
    pub strict: Vec<bool>,
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
    /// Default signatures (DefaultSignatures): the scheme of each default, class predicate
    /// first, then the predicates the default needs beyond the class.
    pub default_schemes: HashMap<String, Scheme>,
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
    /// The runtime representation each head argument's kind annotation names (`(a :: TYPE
    /// 'IntRep)` is "IntRep"): instances can differ only in these (ghc-heap's `HasHeapRep`).
    pub kinds: Vec<Option<String>>,
}

/// Instances in registration order, indexed by class. Keeping the index here ensures that
/// cached modules, derived instances and boot-file replacements all update it together.
#[derive(Default)]
pub struct Instances {
    entries: Vec<InstInfo>,
    by_class: HashMap<String, Vec<usize>>,
}

impl Instances {
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn for_class(&self, class: &str) -> impl DoubleEndedIterator<Item = &InstInfo> {
        self.by_class.get(class).into_iter().flatten().map(|&i| &self.entries[i])
    }

    pub fn since(&self, start: usize) -> &[InstInfo] {
        &self.entries[start..]
    }

    pub fn push(&mut self, inst: InstInfo) {
        self.by_class.entry(inst.class.clone()).or_default().push(self.entries.len());
        self.entries.push(inst);
    }

    pub fn remove(&mut self, index: usize) -> InstInfo {
        let inst = self.entries.remove(index);
        // Removal is rare (a boot-file instance is replaced by its implementation). Preserve
        // the order of every class, including those whose entries follow the removed one.
        for indices in self.by_class.values_mut() {
            indices.retain(|&i| i != index);
            for i in indices {
                if *i > index {
                    *i -= 1;
                }
            }
        }
        inst
    }

    pub fn set_context(&mut self, index: usize, ctx: Vec<Pred>) {
        self.entries[index].ctx = ctx;
    }

    pub fn position(&self, class: &str, matches: impl Fn(&InstInfo) -> bool) -> Option<usize> {
        self.by_class.get(class)?.iter().copied().find(|&i| matches(&self.entries[i]))
    }
}

impl std::ops::Index<usize> for Instances {
    type Output = InstInfo;

    fn index(&self, index: usize) -> &InstInfo {
        &self.entries[index]
    }
}

impl Extend<InstInfo> for Instances {
    fn extend<T: IntoIterator<Item = InstInfo>>(&mut self, iter: T) {
        for inst in iter {
            self.push(inst);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn instance(class: &str, dict: &str) -> InstInfo {
        InstInfo {
            class: class.into(),
            nvars: 0,
            ctx: Vec::new(),
            head: vec![Ty::con(dict)],
            dict: dict.into(),
            kinds: Vec::new(),
        }
    }

    #[test]
    fn instance_index_survives_boot_replacement_and_cache_extension() {
        let mut insts = Instances::default();
        insts.extend([instance("C", "c0"), instance("D", "d0"), instance("C", "c1"), instance("D", "d1")]);
        // Replacing a boot-file entry shifts entries of every class, and registers the
        // replacement last. Cached additions use the same extension path.
        let old = insts.position("D", |i| i.dict == "d0").unwrap();
        assert_eq!(insts.remove(old).dict, "d0");
        insts.extend([instance("D", "d0"), instance("C", "c2")]);
        let names = |class| insts.for_class(class).map(|i| i.dict.as_str()).collect::<Vec<_>>();
        assert_eq!(names("C"), ["c0", "c1", "c2"]);
        assert_eq!(names("D"), ["d1", "d0"]);
        assert!(names("Missing").is_empty());
        assert_eq!(insts.position("D", |i| i.dict == "d0"), Some(3));
        assert_eq!(insts.since(3).iter().map(|i| i.dict.as_str()).collect::<Vec<_>>(), ["d0", "c2"]);
    }
}

/// One equation of a type family: patterns over `Gen(0..nvars)`, and the right-hand side.
#[derive(Clone, Debug)]
pub struct FamEqn {
    pub nvars: u32,
    pub args: Vec<Ty>,
    pub rhs: Ty,
    /// The kind each pattern is annotated with (`(s :: Sizing)`), as the name of a type: the
    /// equations of a poly-kinded family can differ only in the kinds of their arguments.
    pub kinds: Vec<Option<String>>,
}

#[derive(Clone, Debug)]
pub struct FamilyInfo {
    pub arity: usize,
    /// Closed families try their equations in order.
    pub closed: bool,
    pub equations: Vec<FamEqn>,
    /// The positions of the parameters the result determines (TypeFamilyDependencies).
    pub injective: Vec<usize>,
}

#[derive(Default)]
pub struct Env {
    /// Data declarations by global type name, with the module that declares them; standalone
    /// deriving needs the constructors of types declared elsewhere.
    pub data_decls: EnvMap<(String, crate::syntax::ast::DataDecl)>,
    /// Type families by global name.
    pub families: EnvMap<FamilyInfo>,
    pub tycons: EnvMap<TyConInfo>,
    pub synonyms: EnvMap<Synonym>,
    pub datacons: EnvMap<DataCon>,
    pub classes: EnvMap<ClassInfo>,
    pub instances: Instances,
    /// Types of all top-level values, including constructors, selectors and methods.
    pub values: EnvMap<Scheme>,
    /// Record field name to (constructor, position) pairs.
    pub fields: EnvMap<Vec<(String, usize)>>,
}
