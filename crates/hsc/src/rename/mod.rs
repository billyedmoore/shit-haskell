//! The renamer: resolves every identifier to its binding site, applies operator
//! fixities, and computes the module's interface.
//!
//! Naming convention after renaming:
//! - a top-level entity of module `M` is `QName { qual: Some("M"), name }`, except
//!   that top-level binding sites keep their occurrence name;
//! - a local binder is `QName { qual: None, name: "x%N" }`, where `%` cannot occur in
//!   a Haskell identifier, so locals never clash with source names;
//! - built-in syntax (`()`, `[]`, `:`, tuples, `->`) is qualified with `Builtin`.

pub mod fixity;
pub mod iface;
mod quote;

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::syntax::ast::*;
use fixity::{resolve, Tk};
use iface::{EntKind, Entity, GName, Iface, PatSynDef};

/// The module users import implicitly.
pub const PRELUDE: &str = "Prelude";
/// The module where the Prelude's definitions live.
pub const BASE: &str = "Base";
/// Pseudo-module qualifying built-in syntax.
pub const BUILTIN: &str = "Builtin";

#[derive(Debug, Clone)]
pub struct RnError {
    pub loc: Loc,
    pub msg: String,
}

type RR<T> = Result<T, RnError>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ns {
    Value,
    Type,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BindMode {
    /// Top-level binding site: the occurrence name is kept.
    Top,
    /// A new local binder with a fresh unique name.
    Fresh,
    /// A binder that was already introduced by a binding group.
    Existing,
}

pub fn is_builtin_con(name: &str) -> bool {
    name == "()" || name == "[]" || name == ":" || name.starts_with("(,") || name.starts_with("(#_")
}

pub fn is_builtin_tycon(name: &str) -> bool {
    name == "()" || name == "[]" || name == "->" || name == "~" || name == "~~" || name.starts_with("(,") || name.starts_with("(#|")
}

/// Constructor-like names (constructors, `:`, built-in syntax).
fn is_con_name(name: &str) -> bool {
    name.starts_with(':')
        || name.starts_with('(')
        || name.starts_with('[')
        || name.chars().next().is_some_and(|c| c.is_uppercase())
}

fn builtin(name: &str, loc: Loc) -> QName {
    let _ = loc;
    QName {
        qual: Some(BUILTIN.to_string()),
        name: name.to_string(),
    }
}

pub struct Renamer<'a> {
    module: String,
    ifaces: &'a BTreeMap<String, Iface>,
    fixities: HashMap<GName, (Assoc, u8)>,
    vals: HashMap<String, Vec<Entity>>,
    types: HashMap<String, Vec<Entity>>,
    qvals: HashMap<(String, String), Vec<Entity>>,
    qtypes: HashMap<(String, String), Vec<Entity>>,
    /// Top-level entities defined by this module, with their sub-entities.
    own: Vec<Entity>,
    /// Stack of local scopes: occurrence name to unique name.
    locals: Vec<HashMap<String, String>>,
    counter: usize,
    /// Binders seen so far in the pattern group being renamed (for duplicate detection).
    pat_seen: Option<HashSet<String>>,
    /// Pattern synonyms of every loaded module and of this one.
    patsyns: HashMap<GName, PatSynDef>,
    /// Renaming a `.hs-boot` file: signatures declare values without bindings.
    boot: bool,
}

/// Renames a module against the interfaces of its imports.
pub fn rename_module(
    mut m: Module,
    ifaces: &BTreeMap<String, Iface>,
    implicit_prelude: bool,
    boot: bool,
) -> RR<(Module, Iface)> {
    name_duplicate_fields(&mut m.decls);
    // Global names are unique, so the fixities of every loaded module can be merged. This
    // matters for re-exports: `Prelude` exports operators whose fixities `Base` declares.
    let mut fixities = HashMap::new();
    for iface in ifaces.values() {
        for (g, f) in &iface.fixities {
            fixities.insert(g.clone(), *f);
        }
    }
    let mut rn = Renamer {
        module: m.name.clone(),
        ifaces,
        fixities,
        vals: HashMap::new(),
        types: HashMap::new(),
        qvals: HashMap::new(),
        qtypes: HashMap::new(),
        own: Vec::new(),
        locals: Vec::new(),
        counter: 0,
        pat_seen: None,
        boot,
        patsyns: ifaces
            .values()
            .flat_map(|i| i.patsyns.iter().map(|(g, d)| (g.clone(), d.clone())))
            .collect(),
    };
    rn.fixities.insert(GName::new(BUILTIN, ":"), (Assoc::Right, 5));
    rn.run(m, implicit_prelude)
}

impl<'a> Renamer<'a> {
    fn run(&mut self, m: Module, implicit_prelude: bool) -> RR<(Module, Iface)> {
        // Imports, including the implicit Prelude import.
        let explicit_prelude = m.imports.iter().any(|i| i.module == PRELUDE);
        let mut imports = m.imports.clone();
        if implicit_prelude && !explicit_prelude && self.module != PRELUDE {
            imports.insert(
                0,
                Import {
                    module: PRELUDE.to_string(),
                    package: None,
                    qualified: false,
                    alias: None,
                    hiding: false,
                    items: None,
                    loc: m.loc,
                },
            );
        }
        for imp in &imports {
            self.process_import(imp)?;
        }

        // Top-level entities of this module.
        self.collect_own(&m.decls)?;
        let own = self.own.clone();
        for e in &own {
            self.add_entity(e.clone(), true, Some(&self.module.clone()));
            for s in &e.subs {
                self.add_entity(s.clone(), true, Some(&self.module.clone()));
            }
        }
        // Fixities declared in this module.
        for d in &m.decls {
            if let Decl::Fixity { assoc, prec, ops, loc } = d {
                for op in ops {
                    if !self.defines_value_or_method(op) {
                        return Err(RnError {
                            loc: *loc,
                            msg: format!("The fixity signature for ‘{op}’ lacks an accompanying binding"),
                        });
                    }
                    self.fixities
                        .insert(GName::new(self.module.clone(), op.clone()), (*assoc, *prec));
                }
            }
        }

        // Pattern synonyms: each definition is renamed now, so that uses in patterns can expand
        // it; implicitly bidirectional ones and explicit builders become functions `$bP`.
        let mut builders = Vec::new();
        for d in &m.decls {
            if let Decl::PatSyn { name, args, pat, dir, loc } = d {
                let sig = m.decls.iter().find_map(|s| match s {
                    Decl::PatSynSig { names, ty, .. } if names.iter().any(|n| n == name) => Some(ty.clone()),
                    _ => None,
                });
                self.patsyn_definition(name, args, pat, dir, *loc)?;
                if let Some(sig) = sig {
                    let tys = closed_arg_types(&sig);
                    let mut renamed = Vec::with_capacity(tys.len());
                    for t in tys {
                        renamed.push(match t {
                            Some(t) => self.ty(t).ok(),
                            None => None,
                        });
                    }
                    if let Some(def) = self.patsyns.get_mut(&GName::new(self.module.clone(), name)) {
                        def.arg_types = renamed;
                    }
                }
                let defs = builder_decls(name, args, pat, dir, *loc)?;
                // A builder has the synonym's signature, with both of its contexts required.
                if !defs.is_empty() {
                    for d in &m.decls {
                        if let Decl::PatSynSig { names, ty, loc } = d {
                            if names.iter().any(|n| n == name) {
                                builders.push(Decl::Sig {
                                    names: vec![builder_name(name)],
                                    ty: merge_contexts(ty),
                                    loc: *loc,
                                });
                            }
                        }
                    }
                }
                builders.extend(defs);
            }
        }

        // Rename declarations.
        let mut decls = Vec::with_capacity(m.decls.len());
        for d in m.decls.into_iter().filter(|d| !matches!(d, Decl::PatSyn { .. } | Decl::PatSynSig { .. })) {
            decls.push(self.top_decl(d)?);
        }
        for d in builders {
            decls.push(self.top_decl(d)?);
        }

        // Exports.
        let exports = self.compute_exports(&m.exports, m.loc)?;
        let mut fixities = BTreeMap::new();
        for (g, f) in self.fixities.iter() {
            if g.module == self.module {
                fixities.insert(g.clone(), *f);
            }
        }
        let patsyns = self
            .patsyns
            .iter()
            .filter(|(g, _)| g.module == self.module)
            .map(|(g, d)| (g.clone(), d.clone()))
            .collect();
        let iface = Iface {
            module: self.module.clone(),
            exports,
            fixities,
            patsyns,
        };
        let module = Module {
            name: m.name,
            exports: m.exports,
            imports,
            decls,
            loc: m.loc,
        };
        Ok((module, iface))
    }

    /// Whether a fixity declaration can refer to this name: a value, a constructor,
    /// a field or a method defined by this module.
    fn defines_value_or_method(&self, name: &str) -> bool {
        // Type-level operators (`->`, `~`) are built in; their fixity is fixed by GHC.
        if matches!(name, "->" | "~" | "~~") {
            return true;
        }
        // Type-level operators (type families, synonyms, classes) may also carry a fixity.
        self.own.iter().any(|e| {
            e.occ() == name || e.subs.iter().any(|s| s.kind.is_value() && s.occ() == name)
        })
    }

    // ----- scope construction ------------------------------------------------

    fn add_entity(&mut self, e: Entity, unqualified: bool, qualifier: Option<&str>) {
        let is_value = e.kind.is_value();
        let occ = e.occ().to_string();
        if unqualified {
            let map = if is_value { &mut self.vals } else { &mut self.types };
            let list = map.entry(occ.clone()).or_default();
            if !list.iter().any(|x| x.global == e.global) {
                list.push(e.clone());
            }
        }
        if let Some(q) = qualifier {
            let map = if is_value { &mut self.qvals } else { &mut self.qtypes };
            let list = map.entry((q.to_string(), occ)).or_default();
            if !list.iter().any(|x| x.global == e.global) {
                list.push(e);
            }
        }
    }

    fn process_import(&mut self, imp: &Import) -> RR<()> {
        let iface = self.ifaces.get(&imp.module).ok_or_else(|| RnError {
            loc: imp.loc,
            msg: format!(
                "Could not find module ‘{}’\n    It is not a module in the current program, or in any known package.",
                imp.module
            ),
        })?;
        for (g, f) in &iface.fixities {
            self.fixities.entry(g.clone()).or_insert(*f);
        }
        let qualifier = imp.alias.clone().unwrap_or_else(|| imp.module.clone());

        let selected: Vec<Entity> = match &imp.items {
            None => iface.exports.clone(),
            Some(items) if !imp.hiding => {
                let mut out = Vec::new();
                for item in items {
                    // Built-in type operators (`type (~)`) are always in scope; nothing to import.
                    if is_builtin_tycon(&item_name(item)) {
                        continue;
                    }
                    let found = select_item(&iface.exports, item);
                    match found {
                        Some(list) => out.extend(list),
                        None => {
                            return Err(RnError {
                                loc: imp.loc,
                                msg: format!(
                                    "Module ‘{}’ does not export ‘{}’",
                                    imp.module,
                                    item_name(item)
                                ),
                            })
                        }
                    }
                }
                out
            }
            Some(items) => {
                let mut hidden: Vec<Entity> = Vec::new();
                for item in items {
                    if let Some(list) = select_item(&iface.exports, item) {
                        hidden.extend(list);
                    }
                }
                // Hidden names are removed both from the exports and from their sub-entities,
                // so that hiding a method does not bring in its class's other methods. Hiding a
                // type by its bare name hides the type only: its constructors stay in scope, as
                // entities of their own (`hiding (T(..))` hides them too).
                let mut hidden_globals: HashSet<GName> = HashSet::new();
                for h in &hidden {
                    hidden_globals.insert(h.global.clone());
                    for s in &h.subs {
                        hidden_globals.insert(s.global.clone());
                    }
                }
                // `hiding (C(..))` hides every method of the class, also those the module exports on
                // their own (Prelude's `null` is a Foldable method exported outside `Foldable(..)`).
                for item in items {
                    if let IEItem::Thing { name, subs: Subs::All } = item {
                        let class = iface.exports.iter().find(|e| e.kind == EntKind::Class && e.occ() == name.name);
                        if let Some(c) = class {
                            let defining = self.ifaces.get(&c.global.module).and_then(|i| {
                                i.exports.iter().find(|e| e.kind == EntKind::Class && e.global == c.global).cloned()
                            });
                            for s in defining.iter().flat_map(|d| d.subs.iter()) {
                                hidden_globals.insert(s.global.clone());
                            }
                        }
                    }
                }
                let mut out: Vec<Entity> = Vec::new();
                for e in &iface.exports {
                    if hidden_globals.contains(&e.global) {
                        if !e.kind.is_value() {
                            out.extend(e.subs.iter().filter(|s| s.kind.is_value() && !hidden_globals.contains(&s.global)).cloned());
                        }
                        continue;
                    }
                    let mut kept = e.clone();
                    kept.subs.retain(|s| !hidden_globals.contains(&s.global));
                    out.push(kept);
                }
                out
            }
        };

        for e in selected {
            let mut all = vec![e.clone()];
            all.extend(e.subs.iter().cloned());
            if let Some(items) = &imp.items {
                // Sub-entities come in with `T(..)` or with `T(a, b)`.
                let _ = items;
            }
            for x in all {
                let qual = if imp.qualified { None } else { Some(qualifier.as_str()) };
                let unqual = !imp.qualified;
                self.add_entity(x.clone(), unqual, qual);
                if imp.qualified {
                    self.add_entity(x, false, Some(qualifier.as_str()));
                }
            }
        }
        Ok(())
    }

    fn collect_own(&mut self, decls: &[Decl]) -> RR<()> {
        let module = self.module.clone();
        let mut seen_vals: HashSet<String> = HashSet::new();
        let mut seen_types: HashSet<String> = HashSet::new();
        let mut own = Vec::new();

        let add_val = |name: &str, loc: Loc, seen: &mut HashSet<String>| -> RR<()> {
            if !seen.insert(name.to_string()) {
                return Err(RnError {
                    loc,
                    msg: format!("Multiple declarations of ‘{name}’"),
                });
            }
            Ok(())
        };

        for d in decls {
            match d {
                Decl::Fun { name, loc, .. } => {
                    add_val(name, *loc, &mut seen_vals)?;
                    own.push(entity(EntKind::Var, &module, name, vec![], vec![]));
                }
                Decl::PatBind { pat, loc, .. } => {
                    for (v, vl) in pat_vars(pat) {
                        let _ = loc;
                        add_val(&v, vl, &mut seen_vals)?;
                        own.push(entity(EntKind::Var, &module, &v, vec![], vec![]));
                    }
                }
                Decl::Foreign { import: true, name, loc, .. } => {
                    add_val(name, *loc, &mut seen_vals)?;
                    own.push(entity(EntKind::Var, &module, name, vec![], vec![]));
                }
                Decl::PatSyn { name, loc, .. } => {
                    add_val(name, *loc, &mut seen_vals)?;
                    own.push(entity(EntKind::PatSyn, &module, name, vec![], vec![]));
                }
                Decl::Sig { names, loc, .. } if self.boot => {
                    for n in names {
                        add_val(n, *loc, &mut seen_vals)?;
                        own.push(entity(EntKind::Var, &module, n, vec![], vec![]));
                    }
                }
                Decl::Data(dd) => {
                    add_type(&mut seen_types, &dd.name, dd.loc)?;
                    let mut subs = Vec::new();
                    let mut fields_seen: Vec<String> = Vec::new();
                    for c in &dd.cons {
                        add_val(&c.name, c.loc, &mut seen_vals)?;
                        let fields = match &c.fields {
                            ConFields::Record(groups) => groups
                                .iter()
                                .flat_map(|(names, _)| names.iter().cloned())
                                .collect::<Vec<_>>(),
                            ConFields::Positional(_) => Vec::new(),
                        };
                        for f in &fields {
                            if !fields_seen.contains(f) {
                                fields_seen.push(f.clone());
                                add_val(f, c.loc, &mut seen_vals)?;
                                subs.push(entity(EntKind::Field, &module, f, vec![], vec![]));
                            }
                        }
                        subs.push(entity(EntKind::Con, &module, &c.name, vec![], fields));
                    }
                    own.push(entity(EntKind::TyCon, &module, &dd.name, subs, vec![]));
                }
                Decl::TypeSyn { name, loc, .. } => {
                    add_type(&mut seen_types, name, *loc)?;
                    own.push(entity(EntKind::TyCon, &module, name, vec![], vec![]));
                }
                Decl::TypeFamily { name, loc, .. } => {
                    add_type(&mut seen_types, name, *loc)?;
                    own.push(entity(EntKind::TyCon, &module, name, vec![], vec![]));
                }
                Decl::Class(cd) => {
                    add_type(&mut seen_types, &cd.name, cd.loc)?;
                    // Associated families are top-level type constructors of the module.
                    for cdecl in &cd.decls {
                        if let Decl::TypeFamily { name, loc, .. } = cdecl {
                            add_type(&mut seen_types, name, *loc)?;
                            own.push(entity(EntKind::TyCon, &module, name, vec![], vec![]));
                        }
                    }
                    // `C(..)` also names the associated families.
                    let mut subs: Vec<Entity> = cd
                        .decls
                        .iter()
                        .filter_map(|d| match d {
                            Decl::TypeFamily { name, .. } => Some(entity(EntKind::TyCon, &module, name, vec![], vec![])),
                            _ => None,
                        })
                        .collect();
                    for cdecl in &cd.decls {
                        if let Decl::Sig { names, loc, .. } = cdecl {
                            for n in names {
                                add_val(n, *loc, &mut seen_vals)?;
                                subs.push(entity(EntKind::Method, &module, n, vec![], vec![]));
                            }
                        }
                    }
                    own.push(entity(EntKind::Class, &module, &cd.name, subs, vec![]));
                }
                _ => {}
            }
        }
        // The constructors of a data instance (`R:` types) also belong to its family: `F(..)`
        // names them, as in GHC.
        for d in decls {
            if let Decl::TypeInstance { name: family, equation, .. } = d {
                let mut head = &equation.rhs;
                while let Type::App(f, _) = head {
                    head = f;
                }
                let Type::Con(rep, _) = head else { continue };
                if !rep.name.starts_with("R:") {
                    continue;
                }
                let Some(cons) = own.iter().find(|e| e.kind == EntKind::TyCon && e.occ() == rep.name).map(|e| e.subs.clone()) else {
                    continue;
                };
                if let Some(fam) = own.iter_mut().find(|e| e.kind == EntKind::TyCon && e.occ() == family.name) {
                    for c in cons {
                        if !fam.subs.contains(&c) {
                            fam.subs.push(c);
                        }
                    }
                }
            }
        }
        self.own = own;
        Ok(())
    }

    // ----- lookup -----------------------------------------------------------

    /// A record field named in a pattern, an expression or an update: the field itself, even where a
    /// local variable of the same name is in scope (a record wildcard binds such variables).
    fn field_ref(&mut self, f: &QName, loc: Loc) -> RR<QName> {
        match self.lookup_global(f, Ns::Value) {
            Ok(e) => Ok(QName { qual: Some(e.global.module.clone()), name: e.global.name.clone() }),
            Err(_) => self.value_ref(f, loc, false),
        }
    }

    /// A field of a record construction or pattern: an unqualified name the constructor has is
    /// its field, in scope or not (DisambiguateRecordFields; `M.C { f = x }` with `M` qualified).
    fn con_field_ref(&mut self, con: &Entity, f: &QName, loc: Loc) -> RR<QName> {
        // A qualified name (`M.f`, a duplicate field of the module `M` names) is the
        // constructor's field when that is in scope under the qualifier.
        if let Some(x) = con.fields.iter().find(|x| iface::field_label(x) == f.name) {
            let global = QName { qual: Some(con.global.module.clone()), name: x.clone() };
            let visible = match &f.qual {
                None => true,
                Some(m) => self
                    .qvals
                    .get(&(m.clone(), f.name.clone()))
                    .is_some_and(|es| es.iter().any(|e| e.global.module == con.global.module && e.global.name == *x)),
            };
            if visible {
                return Ok(global);
            }
        }
        self.field_ref(f, loc)
    }

    fn lookup_global(&self, q: &QName, ns: Ns) -> Result<Entity, Vec<Entity>> {
        let list = match (&q.qual, ns) {
            (None, Ns::Value) => self.vals.get(&q.name),
            (None, Ns::Type) => self.types.get(&q.name),
            (Some(m), Ns::Value) => self.qvals.get(&(m.clone(), q.name.clone())),
            (Some(m), Ns::Type) => self.qtypes.get(&(m.clone(), q.name.clone())),
        };
        match list.map(|l| l.as_slice()) {
            None | Some([]) => Err(Vec::new()),
            Some([one]) => Ok(one.clone()),
            Some(many) => Err(many.to_vec()),
        }
    }

    fn lookup_local(&self, occ: &str) -> Option<String> {
        self.locals.iter().rev().find_map(|s| s.get(occ).cloned())
    }

    fn fresh(&mut self, occ: &str) -> String {
        // Unique across modules: a pattern synonym's definition, renamed in its module, is
        // substituted into others, whose own local names must not capture its names.
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(1);
        self.counter += 1;
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed) as u64 + crate::core::name_base();
        format!("{occ}%{n}")
    }

    fn ambiguous(&self, q: &QName, cands: &[Entity], loc: Loc) -> RnError {
        let names: Vec<String> = cands
            .iter()
            .map(|e| format!("‘{}.{}’", e.global.module, e.global.name))
            .collect();
        RnError {
            loc,
            msg: format!(
                "Ambiguous occurrence ‘{}’\n    It could refer to {}",
                q.name,
                names.join(" or ")
            ),
        }
    }

    /// Resolves a variable or constructor in the value namespace.
    fn value_ref(&mut self, q: &QName, loc: Loc, con: bool) -> RR<QName> {
        // `%G:M.x` names the global `x` of module `M` directly (a Template Haskell `NameG`).
        if let Some(m) = q.qual.as_deref().and_then(|x| x.strip_prefix("%G:")) {
            return Ok(QName { qual: Some(m.to_string()), name: q.name.clone() });
        }
        // `%Base.x` names a Base global directly, whatever the module imports (used by
        // desugarings the parser introduces).
        if q.qual.as_deref() == Some("%Base") {
            return Ok(QName {
                qual: Some(BASE.to_string()),
                name: q.name.clone(),
            });
        }
        if q.qual.is_none() {
            if con && is_builtin_con(&q.name) {
                return Ok(builtin(&q.name, loc));
            }
            if !con {
                if let Some(u) = self.lookup_local(&q.name) {
                    return Ok(QName::unqual(u));
                }
            }
        }
        match self.lookup_global(q, Ns::Value) {
            Ok(e) => Ok(QName {
                qual: Some(e.global.module.clone()),
                name: e.global.name.clone(),
            }),
            Err(cands) if cands.is_empty() => Err(RnError {
                loc,
                msg: match (con, &q.qual) {
                    (true, _) => format!("Data constructor not in scope: {}", display_qname(q)),
                    (false, _) => format!("Variable not in scope: {}", display_qname(q)),
                },
            }),
            Err(cands) => Err(self.ambiguous(q, &cands, loc)),
        }
    }

    fn lookup_entity(&self, q: &QName, loc: Loc) -> RR<Entity> {
        // A Template Haskell global name: the entity its module exports (or a part of one).
        if let Some(m) = q.qual.as_deref().and_then(|x| x.strip_prefix("%G:")) {
            let found = self.ifaces.get(m).and_then(|i| {
                i.exports.iter().flat_map(|e| std::iter::once(e).chain(e.subs.iter())).find(|e| e.global.name == q.name && e.global.module == m).cloned()
            });
            if let Some(e) = found {
                return Ok(e);
            }
        }
        match self.lookup_global(q, Ns::Value) {
            Ok(e) => Ok(e),
            Err(c) if c.is_empty() => Err(RnError {
                loc,
                msg: format!("Data constructor not in scope: {}", display_qname(q)),
            }),
            Err(c) => Err(self.ambiguous(q, &c, loc)),
        }
    }

    fn tycon_ref(&mut self, q: &QName, loc: Loc) -> RR<QName> {
        if let Some(m) = q.qual.as_deref().and_then(|x| x.strip_prefix("%G:")) {
            return Ok(QName { qual: Some(m.to_string()), name: q.name.clone() });
        }
        if q.qual.is_none() && is_builtin_tycon(&q.name) {
            return Ok(builtin(&q.name, loc));
        }
        // Implicit-parameter constraints `?x t` are named by the parameter.
        if q.name.starts_with('?') {
            return Ok(q.clone());
        }
        // A promoted data constructor (DataKinds) is looked up among the values; its global
        // name keeps the tick so that it is distinct from a type of the same name.
        if let Some(bare) = q.name.strip_prefix('\'') {
            // The built-in promoted constructors (list cons and nil, tuples) are always in scope.
            if q.qual.is_none() && (bare == ":" || bare == "[]" || bare == "()" || bare.starts_with("(,")) {
                return Ok(QName { qual: Some(BUILTIN.to_string()), name: format!("'{bare}") });
            }
            if bare.starts_with(|c: char| c.is_uppercase() || c == ':') {
                let vq = QName { qual: q.qual.clone(), name: bare.to_string() };
                return match self.lookup_global(&vq, Ns::Value) {
                    Ok(e) => Ok(QName {
                        qual: Some(e.global.module.clone()),
                        name: format!("'{}", e.global.name),
                    }),
                    Err(_) => Err(RnError {
                        loc,
                        msg: format!("Data constructor not in scope: {}", bare),
                    }),
                };
            }
            return Ok(q.clone());
        }
        match self.lookup_global(q, Ns::Type) {
            Ok(e) => Ok(QName {
                qual: Some(e.global.module.clone()),
                name: e.global.name.clone(),
            }),
            // DataKinds: an unticked data constructor used as a type is promoted.
            Err(c) if c.is_empty() && self.lookup_global(q, Ns::Value).is_ok() => {
                let e = self.lookup_global(q, Ns::Value).unwrap_or_else(|_| unreachable!());
                Ok(QName {
                    qual: Some(e.global.module.clone()),
                    name: format!("'{}", e.global.name),
                })
            }
            Err(c) if c.is_empty() => Err(RnError {
                loc,
                msg: format!("Not in scope: type constructor or class ‘{}’", display_qname(q)),
            }),
            Err(c) => Err(self.ambiguous(q, &c, loc)),
        }
    }

    fn fixity_of(&self, q: &QName) -> (Assoc, u8) {
        match &q.qual {
            Some(m) => self
                .fixities
                .get(&GName::new(m.clone(), q.name.clone()))
                .copied()
                .unwrap_or((Assoc::Left, 9)),
            None => (Assoc::Left, 9),
        }
    }

    // ----- scopes -----------------------------------------------------------

    fn push(&mut self) {
        self.locals.push(HashMap::new());
    }

    /// The definition of the pattern synonym a resolved constructor name refers to, if any.
    /// A record update whose fields all belong to one bidirectional record pattern synonym, as
    /// `case e of P x1 .. xn -> P x1' .. xn'` (the updated fields replaced).
    fn patsyn_update(&mut self, e: &Expr, fields: &[(QName, Option<Expr>)], loc: Loc) -> RR<Option<Expr>> {
        let mut globals = Vec::with_capacity(fields.len());
        for (f, _) in fields {
            match self.field_ref(f, loc) {
                Ok(q) => globals.push(q),
                Err(_) => return Ok(None),
            }
        }
        let owner = self.patsyns.iter().find(|(g, def)| {
            def.builder
                && globals.iter().all(|q| q.qual.as_deref() == Some(g.module.as_str()) && def.names.iter().any(|n| *n == q.name))
        });
        let Some((g, def)) = owner.map(|(g, d)| (g.clone(), d.clone())) else { return Ok(None) };
        let vars: Vec<String> = def.names.iter().map(|n| self.fresh(n)).collect();
        let pats: Vec<Pat> = vars.iter().map(|v| Pat::Var(v.clone(), loc)).collect();
        let pat = subst_pat(&def.pat, &def.params, &pats);
        let mut body = Expr::Var(QName { qual: Some(g.module.clone()), name: builder_name(&g.name) }, loc);
        for (i, name) in def.names.iter().enumerate() {
            let arg = match fields.iter().zip(&globals).find(|(_, q)| q.name == *name) {
                Some(((f, Some(v)), _)) => {
                    let _ = f;
                    self.expr(v.clone())?
                }
                Some(((f, None), _)) => self.expr(Expr::Var(QName::unqual(f.name.clone()), loc))?,
                None => Expr::Var(QName::unqual(vars[i].clone()), loc),
            };
            body = Expr::App(Box::new(body), Box::new(arg));
        }
        let alt = Alt { pat, rhs: Rhs { body: Body::Plain(body), wheres: Vec::new() }, loc };
        Ok(Some(Expr::Case(Box::new(e.clone()), vec![alt], loc)))
    }

    fn patsyn_of(&self, q: &QName) -> Option<PatSynDef> {
        let module = q.qual.clone()?;
        self.patsyns.get(&GName::new(module, q.name.clone())).cloned()
    }

    /// Renames a pattern synonym's pattern with its arguments bound, and records it.
    fn patsyn_definition(
        &mut self,
        name: &str,
        args: &[String],
        pat: &Pat,
        dir: &PatSynDir,
        loc: Loc,
    ) -> RR<()> {
        let depth = self.locals.len();
        self.push();
        let renamed = self.pat_group(vec![pat.clone()], BindMode::Fresh)?.remove(0);
        let mut params = Vec::with_capacity(args.len());
        for a in args {
            match self.lookup_local(a) {
                Some(u) => params.push(u),
                None => {
                    self.locals.truncate(depth);
                    return Err(RnError {
                        loc,
                        msg: format!("Pattern synonym argument ‘{a}’ is not bound by the pattern"),
                    });
                }
            }
        }
        self.locals.truncate(depth);
        let def = PatSynDef {
            params,
            names: args.to_vec(),
            pat: renamed,
            builder: !matches!(dir, PatSynDir::Unidirectional),
            arg_types: Vec::new(),
        };
        self.patsyns.insert(GName::new(self.module.clone(), name), def);
        Ok(())
    }

    fn bind(&mut self, occ: &str, loc: Loc, mode: BindMode) -> RR<String> {
        if let Some(seen) = &mut self.pat_seen {
            if !seen.insert(occ.to_string()) {
                return Err(RnError {
                    loc,
                    msg: format!("Conflicting definitions for ‘{occ}’"),
                });
            }
        }
        match mode {
            BindMode::Top => Ok(occ.to_string()),
            BindMode::Fresh => {
                let u = self.fresh(occ);
                if self.locals.is_empty() {
                    self.push();
                }
                self.locals.last_mut().expect("scope").insert(occ.to_string(), u.clone());
                Ok(u)
            }
            BindMode::Existing => Ok(self
                .locals
                .last()
                .and_then(|s| s.get(occ).cloned())
                .unwrap_or_else(|| occ.to_string())),
        }
    }

    // ----- declarations -----------------------------------------------------

    fn top_decl(&mut self, d: Decl) -> RR<Decl> {
        Ok(match d {
            Decl::IPBind { loc, .. } => {
                return Err(RnError { loc, msg: "Implicit-parameter bindings are only allowed in a let or where".into() })
            }
            Decl::DefaultSig { loc, .. } => {
                return Err(RnError { loc, msg: "A default signature outside a class".into() })
            }
            // (The driver runs declaration splices before the module is renamed.)
            Decl::Splice(_, loc) => {
                return Err(RnError { loc, msg: "A declaration splice needs TemplateHaskell".into() })
            }
            // Pattern synonyms are resolved away before declarations are renamed.
            Decl::PatSyn { loc, .. } | Decl::PatSynSig { loc, .. } => {
                return Err(RnError { loc, msg: "internal: pattern synonym reached the declaration pass".into() })
            }
            Decl::Many(_) => return Err(RnError { loc: Loc::default(), msg: "internal: grouped declarations reached the renamer".into() }),
            Decl::Sig { names, ty, loc } => {
                for n in &names {
                    // Pattern synonym builders (`$bP`) are generated with their own signatures.
                    let generated = n.starts_with("$b");
                    if !self.boot && !generated && !self.own.iter().any(|e| e.kind == EntKind::Var && e.occ() == n) {
                        return Err(RnError {
                            loc,
                            msg: format!("The type signature for ‘{n}’ lacks an accompanying binding"),
                        });
                    }
                }
                Decl::Sig {
                    names,
                    ty: self.ty(ty)?,
                    loc,
                }
            }
            Decl::Fixity { assoc, prec, ops, loc } => Decl::Fixity { assoc, prec, ops, loc },
            Decl::Fun { name, clauses, loc } => {
                check_arities(&name, &clauses, loc)?;
                Decl::Fun {
                name,
                clauses: clauses
                    .into_iter()
                    .map(|c| self.clause(c))
                    .collect::<RR<Vec<_>>>()?,
                loc,
                }
            }
            Decl::PatBind { pat, rhs, loc } => {
                let pat = self.pat_group(vec![pat], BindMode::Top)?.remove(0);
                Decl::PatBind {
                    pat,
                    rhs: self.rhs(rhs)?,
                    loc,
                }
            }
            Decl::Data(mut dd) => {
                dd.context = self.types(dd.context)?;
                for c in &mut dd.cons {
                    c.exist = self.tyvar_binds(std::mem::take(&mut c.exist))?;
                    c.context = self.types(std::mem::take(&mut c.context))?;
                    c.gadt_result = match c.gadt_result.take() {
                        Some(t) => Some(self.ty(t)?),
                        None => None,
                    };
                    c.fields = match std::mem::replace(&mut c.fields, ConFields::Positional(vec![])) {
                        ConFields::Positional(fs) => ConFields::Positional(self.fields(fs)?),
                        ConFields::Record(groups) => ConFields::Record(
                            groups
                                .into_iter()
                                .map(|(names, f)| Ok((names, self.field(f)?)))
                                .collect::<RR<Vec<_>>>()?,
                        ),
                    };
                }
                for dv in &mut dd.deriving {
                    dv.classes = self.types(std::mem::take(&mut dv.classes))?;
                    dv.via = match dv.via.take() {
                        Some(t) => Some(self.ty(t)?),
                        None => None,
                    };
                }
                Decl::Data(dd)
            }
            Decl::TypeSyn { name, tyvars, rhs, loc } => Decl::TypeSyn {
                name,
                tyvars: self.tyvar_binds(tyvars)?,
                rhs: self.ty(rhs)?,
                loc,
            },
            Decl::Class(mut cd) => {
                cd.context = self.types(cd.context)?;
                let mut decls = Vec::new();
                for d in std::mem::take(&mut cd.decls) {
                    decls.push(match d {
                        Decl::Sig { names, ty, loc } => Decl::Sig {
                            names,
                            ty: self.ty(ty)?,
                            loc,
                        },
                        Decl::DefaultSig { names, ty, loc } => Decl::DefaultSig {
                            names,
                            ty: self.ty(ty)?,
                            loc,
                        },
                        other => self.top_decl(other)?,
                    });
                }
                cd.decls = decls;
                Decl::Class(cd)
            }
            Decl::Instance(mut id) => {
                id.context = self.types(id.context)?;
                id.head = self.ty(id.head)?;
                let class = instance_class(&id.head);
                let methods = class.as_ref().and_then(|g| self.class_methods(g));
                let mut decls = Vec::new();
                for d in std::mem::take(&mut id.decls) {
                    match &d {
                        Decl::Fun { name, loc, .. } => {
                            if let Some(ms) = &methods {
                                if !ms.contains(name) {
                                    return Err(RnError {
                                        loc: *loc,
                                        msg: format!("‘{name}’ is not a (visible) method of the class"),
                                    });
                                }
                            }
                        }
                        _ => {}
                    }
                    // An associated type's equation names the family as the class does, whatever
                    // the imports (`type Item (Set a) = a` under `instance GHC.Exts.IsList`).
                    if let Decl::TypeInstance { name, equation, loc } = &d {
                        if name.qual.is_none() {
                            if let Some(g) = class.as_ref().and_then(|g| self.class_assoc(g, &name.name)) {
                                decls.push(Decl::TypeInstance { name: g, equation: self.type_equation(equation.clone())?, loc: *loc });
                                continue;
                            }
                        }
                    }
                    decls.push(match d {
                        Decl::Sig { names, ty, loc } => Decl::Sig {
                            names,
                            ty: self.ty(ty)?,
                            loc,
                        },
                        other => self.top_decl(other)?,
                    });
                }
                id.decls = decls;
                Decl::Instance(id)
            }
            Decl::Deriving(mut sd) => {
                sd.context = self.types(sd.context)?;
                sd.head = self.ty(sd.head)?;
                sd.via = match sd.via.take() {
                    Some(t) => Some(self.ty(t)?),
                    None => None,
                };
                Decl::Deriving(sd)
            }
            Decl::Default { types, loc } => Decl::Default {
                types: self.types(types)?,
                loc,
            },
            Decl::Foreign {
                import,
                prim,
                entity,
                name,
                ty,
                loc,
            } => Decl::Foreign {
                import,
                prim,
                entity,
                name,
                ty: self.ty(ty)?,
                loc,
            },
            Decl::KindSig { name, kind, loc } => Decl::KindSig {
                name,
                kind: self.ty(kind)?,
                loc,
            },
            Decl::TypeFamily {
                name,
                params,
                equations,
                injective,
                loc,
            } => {
                let equations = match equations {
                    Some(eqs) => Some(
                        eqs.into_iter()
                            .map(|e| self.type_equation(e))
                            .collect::<RR<Vec<_>>>()?,
                    ),
                    None => None,
                };
                Decl::TypeFamily {
                    name,
                    params,
                    equations,
                    injective,
                    loc,
                }
            }
            Decl::TypeInstance { name, equation, loc } => {
                let qname = self.tycon_ref(&name, loc)?;
                Decl::TypeInstance {
                    name: qname,
                    equation: self.type_equation(equation)?,
                    loc,
                }
            }
        })
    }

    fn type_equation(&mut self, e: TypeEquation) -> RR<TypeEquation> {
        Ok(TypeEquation {
            args: self.types(e.args)?,
            rhs: self.ty(e.rhs)?,
            loc: e.loc,
        })
    }

    /// An associated family of a class, by its occurrence name: its global name.
    fn class_assoc(&self, g: &QName, occ: &str) -> Option<QName> {
        let module = g.qual.as_deref()?;
        let class = if module == self.module {
            self.own.iter().find(|e| e.kind == EntKind::Class && e.occ() == g.name)
        } else {
            self.ifaces.values().find_map(|i| {
                i.exports
                    .iter()
                    .find(|e| e.kind == EntKind::Class && e.global.module == module && e.occ() == g.name)
            })
        }?;
        let sub = class.subs.iter().find(|s| s.kind == EntKind::TyCon && s.occ() == occ)?;
        Some(QName { qual: Some(sub.global.module.clone()), name: sub.global.name.clone() })
    }

    fn class_methods(&self, g: &QName) -> Option<Vec<String>> {
        let module = g.qual.as_deref()?;
        if module == self.module {
            let class = self.own.iter().find(|e| e.kind == EntKind::Class && e.occ() == g.name)?;
            return Some(class.subs.iter().filter(|s| s.kind.is_value()).map(|s| s.occ().to_string()).collect());
        }
        // The methods any module exports the class with: a re-export of the bare class
        // (`module P (C)`) does not hide the methods another import brings.
        let mut found = false;
        let mut out: Vec<String> = Vec::new();
        for i in self.ifaces.values() {
            for e in &i.exports {
                if e.kind == EntKind::Class && e.global.module == module && e.occ() == g.name {
                    found = true;
                    for s in e.subs.iter().filter(|s| s.kind.is_value()) {
                        if !out.iter().any(|o| o == s.occ()) {
                            out.push(s.occ().to_string());
                        }
                    }
                }
            }
        }
        found.then_some(out)
    }

    fn tyvar_binds(&mut self, binds: Vec<TyVarBind>) -> RR<Vec<TyVarBind>> {
        binds
            .into_iter()
            .map(|b| {
                Ok(TyVarBind {
                    name: b.name,
                    kind: match b.kind {
                        Some(k) => Some(self.ty(k)?),
                        None => None,
                    },
                })
            })
            .collect()
    }

    fn fields(&mut self, fs: Vec<Field>) -> RR<Vec<Field>> {
        fs.into_iter().map(|f| self.field(f)).collect()
    }

    fn field(&mut self, f: Field) -> RR<Field> {
        Ok(Field {
            ty: self.ty(f.ty)?,
            strict: f.strict,
        })
    }

    fn types(&mut self, ts: Vec<Type>) -> RR<Vec<Type>> {
        ts.into_iter().map(|t| self.ty(t)).collect()
    }

    fn clause(&mut self, c: Clause) -> RR<Clause> {
        self.push();
        let pats = self.pat_group(c.pats, BindMode::Fresh)?;
        let rhs = self.rhs(c.rhs)?;
        self.locals.pop();
        Ok(Clause {
            pats,
            rhs,
            loc: c.loc,
            end: c.end,
        })
    }

    /// Right-hand side. Where-bindings scope over guards and bodies.
    fn rhs(&mut self, rhs: Rhs) -> RR<Rhs> {
        let depth = self.locals.len();
        self.push();
        let wheres = self.local_decls(rhs.wheres)?;
        let body = match rhs.body {
            Body::Plain(e) => Body::Plain(self.expr(e)?),
            Body::Guarded(alts) => {
                let mut out = Vec::with_capacity(alts.len());
                for (conds, e) in alts {
                    let d = self.locals.len();
                    let conds = self.stmts_into_scope(conds)?;
                    let e = self.expr(e)?;
                    self.locals.truncate(d);
                    out.push((conds, e));
                }
                Body::Guarded(out)
            }
        };
        self.locals.truncate(depth);
        Ok(Rhs { body, wheres })
    }

    /// Binds and renames a local declaration group in the current (already pushed) scope.
    fn local_decls(&mut self, decls: Vec<Decl>) -> RR<Vec<Decl>> {
        let mut seen: HashSet<String> = HashSet::new();
        if self.locals.is_empty() {
            self.push();
        }
        for d in &decls {
            match d {
                Decl::Fun { name, loc, .. } => {
                    if !seen.insert(name.clone()) {
                        return Err(RnError {
                            loc: *loc,
                            msg: format!("Conflicting definitions for ‘{name}’"),
                        });
                    }
                    let u = self.fresh(name);
                    self.locals.last_mut().expect("scope").insert(name.clone(), u);
                }
                Decl::PatBind { pat, .. } => {
                    for (v, loc) in pat_vars(pat) {
                        if !seen.insert(v.clone()) {
                            return Err(RnError {
                                loc,
                                msg: format!("Conflicting definitions for ‘{v}’"),
                            });
                        }
                        let u = self.fresh(&v);
                        self.locals.last_mut().expect("scope").insert(v, u);
                    }
                }
                _ => {}
            }
        }
        let mut out = Vec::with_capacity(decls.len());
        for d in decls {
            out.push(match d {
                Decl::IPBind { name, expr, loc } => Decl::IPBind { name, expr: self.expr(expr)?, loc },
                Decl::Fun { name, clauses, loc } => {
                    check_arities(&name, &clauses, loc)?;
                    let u = self.lookup_local(&name).expect("bound above");
                    let clauses = clauses
                        .into_iter()
                        .map(|c| self.clause(c))
                        .collect::<RR<Vec<_>>>()?;
                    Decl::Fun {
                        name: u,
                        clauses,
                        loc,
                    }
                }
                Decl::PatBind { pat, rhs, loc } => {
                    let pat = self.pat_group(vec![pat], BindMode::Existing)?.remove(0);
                    Decl::PatBind {
                        pat,
                        rhs: self.rhs(rhs)?,
                        loc,
                    }
                }
                Decl::Sig { names, ty, loc } => {
                    let mut us = Vec::new();
                    for n in names {
                        match self.lookup_local(&n) {
                            Some(u) => us.push(u),
                            None => {
                                return Err(RnError {
                                    loc,
                                    msg: format!("The type signature for ‘{n}’ lacks an accompanying binding"),
                                })
                            }
                        }
                    }
                    Decl::Sig {
                        names: us,
                        ty: self.ty(ty)?,
                        loc,
                    }
                }
                Decl::Fixity { assoc, prec, ops, loc } => Decl::Fixity { assoc, prec, ops, loc },
                other => {
                    return Err(RnError {
                        loc: decl_loc(&other),
                        msg: "Illegal declaration in a local binding group".into(),
                    })
                }
            });
        }
        Ok(out)
    }

    // ----- statements -------------------------------------------------------

    /// Renames statements, leaving the scopes they introduce pushed.
    fn stmts_into_scope(&mut self, stmts: Vec<Stmt>) -> RR<Vec<Stmt>> {
        let mut out = Vec::with_capacity(stmts.len());
        for s in stmts {
            out.push(match s {
                Stmt::Bind { pat, expr, loc } => {
                    let expr = self.expr(expr)?;
                    self.push();
                    let pat = self.pat_group(vec![pat], BindMode::Fresh)?.remove(0);
                    Stmt::Bind { pat, expr, loc }
                }
                Stmt::Let { decls, loc } => {
                    self.push();
                    Stmt::Let {
                        decls: self.local_decls(decls)?,
                        loc,
                    }
                }
                Stmt::Expr { expr, loc } => Stmt::Expr {
                    expr: self.expr(expr)?,
                    loc,
                },
            });
        }
        Ok(out)
    }

    fn stmts_scoped(&mut self, stmts: Vec<Stmt>) -> RR<Vec<Stmt>> {
        let depth = self.locals.len();
        let out = self.stmts_into_scope(stmts)?;
        self.locals.truncate(depth);
        Ok(out)
    }

    fn alt(&mut self, a: Alt) -> RR<Alt> {
        let depth = self.locals.len();
        self.push();
        let pat = self.pat_group(vec![a.pat], BindMode::Fresh)?.remove(0);
        let rhs = self.rhs(a.rhs)?;
        self.locals.truncate(depth);
        Ok(Alt {
            pat,
            rhs,
            loc: a.loc,
        })
    }

    // ----- expressions ------------------------------------------------------

    fn expr(&mut self, e: Expr) -> RR<Expr> {
        Ok(match e {
            Expr::IPVar(name, loc) => Expr::IPVar(name, loc),
            Expr::NameQuote(is_type, q, loc) => {
                // `'f`, `'C`, `''T`: the TH `Name` of the global the name refers to, built by
                // `mkNameG_v`, `mkNameG_d` or `mkNameG_tc` from its package, module and name.
                let global = q.qual.as_deref().and_then(|x| x.strip_prefix("%G:")).map(str::to_string);
                let (fun, resolved) = if let (true, Some(m)) = (is_type, global) {
                    // A global name the compiler wrote (the reify table of a splice).
                    ("mkNameG_tc", Some((m, q.name.clone())))
                } else if is_type && q.qual.is_none() && matches!(q.name.as_str(), "~" | "->") {
                    // The built-in type operators: GHC names them in GHC.Types.
                    ("mkNameG_tc", Some(("GHC.Types".to_string(), q.name.clone())))
                } else if is_type {
                    let e = self.lookup_global(&q, Ns::Type).map_err(|c| {
                        if c.is_empty() {
                            RnError { loc, msg: format!("Not in scope: type constructor or class ‘{}’", q.name) }
                        } else {
                            self.ambiguous(&q, &c, loc)
                        }
                    })?;
                    ("mkNameG_tc", Some((e.global.module.clone(), e.global.name.clone())))
                } else {
                    let con = q.name.starts_with(|c: char| c.is_uppercase() || c == ':') || q.name == "[]";
                    if !con && q.qual.is_none() && self.lookup_local(&q.name).is_some() {
                        ("mkName", None)
                    } else {
                        let r = self.value_ref(&q, loc, con)?;
                        (if con { "mkNameG_d" } else { "mkNameG_v" }, Some((r.qual.clone().unwrap_or_else(|| self.module.clone()), r.name.clone())))
                    }
                };
                let th = |n: &str| Expr::Var(QName { qual: Some("GHC.Internal.TH.Syntax".into()), name: n.into() }, loc);
                let lit = |s: String| Expr::Lit(Lit::Str(s), loc);
                match resolved {
                    None => Expr::App(Box::new(th(fun)), Box::new(lit(q.name.clone()))),
                    Some((module, name)) => {
                        let module = if module == "Builtin" { "GHC.Types".to_string() } else { module };
                        let pkg = package_of_module(&module, &self.module);
                        let f = Expr::App(Box::new(th(fun)), Box::new(lit(pkg)));
                        let f = Expr::App(Box::new(f), Box::new(lit(module)));
                        Expr::App(Box::new(f), Box::new(lit(name)))
                    }
                }
            }
            Expr::Splice(typed, e, loc) => Expr::Splice(typed, Box::new(self.expr(*e)?), loc),
            Expr::Quote(kind, body, loc) => {
                // The names in a quoted expression are resolved as in any expression; the
                // quotation is then the code that builds its syntax (`quote.rs`).
                match (kind, body) {
                    ('e' | 'T', Some(b)) => {
                        let renamed = self.expr(*b)?;
                        let built = self.quote_exp(&renamed, loc)?;
                        if kind == 'T' {
                            let coerce = Expr::Var(QName { qual: Some("GHC.Internal.TH.Syntax".into()), name: "unsafeCodeCoerce".into() }, loc);
                            Expr::App(Box::new(coerce), Box::new(built))
                        } else {
                            built
                        }
                    }
                    (kind, body) => {
                        let body = match body {
                            Some(b) => Some(Box::new(self.expr(*b)?)),
                            None => None,
                        };
                        Expr::Quote(kind, body, loc)
                    }
                }
            }
            Expr::Var(q, loc) => Expr::Var(self.value_ref(&q, loc, false)?, loc),
            Expr::Con(q, loc) => {
                let cq = self.value_ref(&q, loc, true)?;
                match self.patsyn_of(&cq) {
                    Some(def) if def.builder => Expr::Var(
                        QName {
                            qual: cq.qual.clone(),
                            name: builder_name(&cq.name),
                        },
                        loc,
                    ),
                    Some(_) => {
                        return Err(RnError {
                            loc,
                            msg: format!(
                                "non-bidirectional pattern synonym ‘{}’ used in an expression",
                                cq.name
                            ),
                        })
                    }
                    None => Expr::Con(cq, loc),
                }
            }
            Expr::Lit(l, loc) => Expr::Lit(l, loc),
            Expr::App(f, a) => Expr::App(Box::new(self.expr(*f)?), Box::new(self.expr(*a)?)),
            Expr::TyApp(e, t, loc) => Expr::TyApp(Box::new(self.expr(*e)?), self.ty(t)?, loc),
            Expr::OpChain(items, loc) => self.op_chain(items, loc)?,
            Expr::Lam(pats, body, loc) => {
                let depth = self.locals.len();
                self.push();
                let pats = self.pat_group(pats, BindMode::Fresh)?;
                let body = self.expr(*body)?;
                self.locals.truncate(depth);
                Expr::Lam(pats, Box::new(body), loc)
            }
            Expr::Let(decls, body, loc) => {
                let depth = self.locals.len();
                self.push();
                let decls = self.local_decls(decls)?;
                let body = self.expr(*body)?;
                self.locals.truncate(depth);
                Expr::Let(decls, Box::new(body), loc)
            }
            Expr::If(c, t, e, loc) => Expr::If(
                Box::new(self.expr(*c)?),
                Box::new(self.expr(*t)?),
                Box::new(self.expr(*e)?),
                loc,
            ),
            Expr::MultiIf(alts, loc) => {
                let mut out = Vec::with_capacity(alts.len());
                for (conds, e) in alts {
                    let depth = self.locals.len();
                    let conds = self.stmts_into_scope(conds)?;
                    let e = self.expr(e)?;
                    self.locals.truncate(depth);
                    out.push((conds, e));
                }
                Expr::MultiIf(out, loc)
            }
            Expr::Case(scrut, alts, loc) => {
                let scrut = self.expr(*scrut)?;
                let alts = alts.into_iter().map(|a| self.alt(a)).collect::<RR<Vec<_>>>()?;
                Expr::Case(Box::new(scrut), alts, loc)
            }
            Expr::LamCase(alts, loc) => {
                let alts = alts.into_iter().map(|a| self.alt(a)).collect::<RR<Vec<_>>>()?;
                Expr::LamCase(alts, loc)
            }
            Expr::Do(stmts, loc) => Expr::Do(self.stmts_scoped(stmts)?, loc),
            Expr::Tuple(elems, loc) => {
                let mut out = Vec::with_capacity(elems.len());
                for el in elems {
                    out.push(match el {
                        Some(x) => Some(self.expr(x)?),
                        None => None,
                    });
                }
                Expr::Tuple(out, loc)
            }
            Expr::List(elems, loc) => Expr::List(
                elems
                    .into_iter()
                    .map(|x| self.expr(x))
                    .collect::<RR<Vec<_>>>()?,
                loc,
            ),
            Expr::ArithSeq { from, then, to, loc } => Expr::ArithSeq {
                from: Box::new(self.expr(*from)?),
                then: match then {
                    Some(t) => Some(Box::new(self.expr(*t)?)),
                    None => None,
                },
                to: match to {
                    Some(t) => Some(Box::new(self.expr(*t)?)),
                    None => None,
                },
                loc,
            },
            Expr::ListComp(e, branches, loc) => {
                let depth = self.locals.len();
                let mut out_branches = Vec::with_capacity(branches.len());
                for b in branches {
                    out_branches.push(self.stmts_into_scope(b)?);
                }
                let e = self.expr(*e)?;
                self.locals.truncate(depth);
                Expr::ListComp(Box::new(e), out_branches, loc)
            }
            Expr::LeftSection(e, op, loc) => {
                let e = self.expr(*e)?;
                let op = self.op_name(&op, loc)?;
                Expr::LeftSection(Box::new(e), op, loc)
            }
            Expr::RightSection(op, e, loc) => {
                let op = self.op_name(&op, loc)?;
                let e = self.expr(*e)?;
                Expr::RightSection(op, Box::new(e), loc)
            }
            Expr::RecCon {
                con,
                fields,
                dotdot,
                loc,
            } => {
                // A record pattern synonym builds by its builder, its fields in the synonym's order.
                let con_q = self.value_ref(&con, loc, true)?;
                if let Some(def) = self.patsyn_of(&con_q).filter(|d| d.builder) {
                    let mut f = Expr::Var(QName { qual: con_q.qual.clone(), name: builder_name(&con_q.name) }, loc);
                    for p in &def.params {
                        let occ = p.split('%').next().unwrap_or(p).to_string();
                        let value = fields.iter().find(|(q, _)| q.name == occ).map(|(_, v)| v.clone());
                        let arg = match value {
                            Some(Some(x)) => self.expr(x)?,
                            Some(None) => self.expr(Expr::Var(QName::unqual(occ.clone()), loc))?,
                            None if dotdot => self.expr(Expr::Var(QName::unqual(occ.clone()), loc))?,
                            None => self.expr(Expr::App(
                                Box::new(Expr::Var(QName { qual: Some("%Base".into()), name: "error".into() }, loc)),
                                Box::new(Expr::Lit(Lit::Str(format!("Missing field in record construction {occ}")), loc)),
                            ))?,
                        };
                        f = Expr::App(Box::new(f), Box::new(arg));
                    }
                    return Ok(f);
                }
                let entity = self.lookup_entity(&con, loc)?;
                let mut out = Vec::new();
                let mut given: HashSet<String> = HashSet::new();
                for (f, v) in fields {
                    given.insert(f.name.clone());
                    let fq = self.con_field_ref(&entity, &f, loc)?;
                    let v = match v {
                        Some(x) => Some(self.expr(x)?),
                        None => Some(self.expr(Expr::Var(QName::unqual(f.name.clone()), loc))?),
                    };
                    out.push((fq, v));
                }
                if dotdot {
                    for f in &entity.fields {
                        let label = iface::field_label(f).to_string();
                        if !given.contains(&label) {
                            let fq = QName { qual: Some(entity.global.module.clone()), name: f.clone() };
                            let v = self.expr(Expr::Var(QName::unqual(label), loc))?;
                            out.push((fq, Some(v)));
                        }
                    }
                }
                Expr::RecCon {
                    con: con_q,
                    fields: out,
                    dotdot: false,
                    loc,
                }
            }
            Expr::RecUpd(e, fields, loc) => {
                let e = self.expr(*e)?;
                // An update of a record pattern synonym's fields (`lbi { installedPkgs = i }`
                // with `pattern LocalBuildInfo {..}`): a match on the synonym and its builder.
                if let Some(upd) = self.patsyn_update(&e, &fields, loc)? {
                    return Ok(upd);
                }
                let mut out = Vec::with_capacity(fields.len());
                for (f, v) in fields {
                    // A field name in an update is the field itself, even where a local variable of
                    // the same name (a record wildcard's, say) is in scope.
                    let fq = self.field_ref(&f, loc)?;
                    let v = match v {
                        Some(x) => Some(self.expr(x)?),
                        None => Some(self.expr(Expr::Var(QName::unqual(f.name.clone()), loc))?),
                    };
                    out.push((fq, v));
                }
                Expr::RecUpd(Box::new(e), out, loc)
            }
            Expr::Sig(e, t, loc) => Expr::Sig(Box::new(self.expr(*e)?), self.ty(t)?, loc),
            Expr::Wild(loc) => {
                return Err(RnError {
                    loc,
                    msg: "Found hole: _".into(),
                })
            }
            Expr::AsPat(_, _, loc) | Expr::LazyPat(_, loc) | Expr::BangPat(_, loc) | Expr::ViewPat(_, _, loc) => {
                return Err(RnError {
                    loc,
                    msg: "Pattern syntax in expression context".into(),
                })
            }
        })
    }

    fn op_name(&mut self, q: &QName, loc: Loc) -> RR<QName> {
        if is_con_name(&q.name) {
            self.value_ref(q, loc, true)
        } else {
            self.value_ref(q, loc, false)
        }
    }

    fn op_expr(op: QName, loc: Loc) -> Expr {
        if is_con_name(&op.name) {
            Expr::Con(op, loc)
        } else {
            Expr::Var(op, loc)
        }
    }

    fn op_chain(&mut self, items: Vec<OpItem>, loc: Loc) -> RR<Expr> {
        let mut toks: Vec<Tk<Expr, QName>> = Vec::with_capacity(items.len());
        for it in items {
            match it {
                OpItem::Operand(e) => toks.push(Tk::Operand(self.expr(e)?)),
                OpItem::Neg(l) => toks.push(Tk::Neg(l)),
                OpItem::Op(q, l) => {
                    let qn = self.op_name(&q, l)?;
                    let (assoc, prec) = self.fixity_of(&qn);
                    toks.push(Tk::Op {
                        op: qn,
                        name: q.name.clone(),
                        assoc,
                        prec,
                        loc: l,
                    });
                }
            }
        }
        let bin = |op: QName, l: Loc, a: Expr, b: Expr| {
            Expr::App(
                Box::new(Expr::App(Box::new(Self::op_expr(op, l)), Box::new(a))),
                Box::new(b),
            )
        };
        let neg = |l: Loc, e: Expr| {
            Expr::App(
                Box::new(Expr::Var(
                    QName {
                        qual: Some(BASE.to_string()),
                        name: "negate".into(),
                    },
                    l,
                )),
                Box::new(e),
            )
        };
        resolve(toks, &bin, &neg).map_err(|e| RnError {
            loc: if e.loc == Loc::default() { loc } else { e.loc },
            msg: e.msg,
        })
    }

    // ----- patterns ---------------------------------------------------------

    /// Renames a group of patterns that bind variables together (arguments, one
    /// statement, one alternative). Duplicate binders within the group are errors.
    fn pat_group(&mut self, pats: Vec<Pat>, mode: BindMode) -> RR<Vec<Pat>> {
        let saved = self.pat_seen.replace(HashSet::new());
        let mut out = Vec::with_capacity(pats.len());
        let mut result = Ok(());
        for p in pats {
            match self.pat(p, mode) {
                Ok(p) => out.push(p),
                Err(e) => {
                    result = Err(e);
                    break;
                }
            }
        }
        self.pat_seen = saved;
        result.map(|_| out)
    }

    fn pat(&mut self, p: Pat, mode: BindMode) -> RR<Pat> {
        Ok(match p {
            Pat::Var(occ, loc) => Pat::Var(self.bind(&occ, loc, mode)?, loc),
            Pat::Wild(loc) => Pat::Wild(loc),
            Pat::Lit(l, loc) => Pat::Lit(l, loc),
            Pat::NegLit(l, loc) => Pat::NegLit(l, loc),
            Pat::Con(q, args, loc) => {
                let cq = self.value_ref(&q, loc, true)?;
                let mut out = Vec::with_capacity(args.len());
                for a in args {
                    out.push(self.pat(a, mode)?);
                }
                if let Some(def) = self.patsyn_of(&cq) {
                    if out.len() != def.params.len() {
                        return Err(RnError {
                            loc,
                            msg: format!(
                                "The pattern synonym ‘{}’ should have {} arguments, but has been given {}",
                                cq.name,
                                def.params.len(),
                                out.len()
                            ),
                        });
                    }
                    let out = typed_args(out, &def.arg_types, loc);
                    return Ok(subst_pat(&def.pat, &def.params, &out));
                }
                Pat::Con(cq, out, loc)
            }
            Pat::Rec {
                con,
                fields,
                dotdot,
                loc,
            } => {
                // `P{}` or `P{f = p}` on a pattern synonym: its prefix form, the named parameters
                // given and the others wildcards.
                let cq = self.value_ref(&con, loc, true)?;
                if let Some(def) = self.patsyn_of(&cq) {
                    let mut args: Vec<Pat> = def.params.iter().map(|_| Pat::Wild(loc)).collect();
                    for (f, p) in fields {
                        let i = def.names.iter().position(|x| *x == f.name).ok_or_else(|| RnError {
                            loc,
                            msg: format!("Pattern synonym ‘{}’ does not have field ‘{}’", cq.name, f.name),
                        })?;
                        args[i] = match p {
                            Some(p) => p,
                            None => Pat::Var(f.name.clone(), loc),
                        };
                    }
                    if dotdot {
                        for (i, x) in def.names.iter().enumerate() {
                            if matches!(args[i], Pat::Wild(_)) {
                                args[i] = Pat::Var(x.clone(), loc);
                            }
                        }
                    }
                    return self.pat(Pat::Con(con, args, loc), mode);
                }
                let entity = self.lookup_entity(&con, loc)?;
                let mut out = Vec::new();
                let mut given: HashSet<String> = HashSet::new();
                for (f, p) in fields {
                    given.insert(f.name.clone());
                    let fq = self.con_field_ref(&entity, &f, loc)?;
                    let p = match p {
                        Some(p) => Some(self.pat(p, mode)?),
                        None => Some(Pat::Var(self.bind(&f.name, loc, mode)?, loc)),
                    };
                    out.push((fq, p));
                }
                if dotdot {
                    for f in &entity.fields {
                        let label = iface::field_label(f).to_string();
                        if !given.contains(&label) {
                            let fq = QName { qual: Some(entity.global.module.clone()), name: f.clone() };
                            let v = Pat::Var(self.bind(&label, loc, mode)?, loc);
                            out.push((fq, Some(v)));
                        }
                    }
                }
                Pat::Rec {
                    con: cq,
                    fields: out,
                    dotdot: false,
                    loc,
                }
            }
            Pat::Tuple(ps, loc) => Pat::Tuple(self.pats(ps, mode)?, loc),
            Pat::List(ps, loc) => Pat::List(self.pats(ps, mode)?, loc),
            Pat::As(occ, p, loc) => {
                let name = self.bind(&occ, loc, mode)?;
                Pat::As(name, Box::new(self.pat(*p, mode)?), loc)
            }
            Pat::Lazy(p, loc) => Pat::Lazy(Box::new(self.pat(*p, mode)?), loc),
            Pat::Bang(p, loc) => Pat::Bang(Box::new(self.pat(*p, mode)?), loc),
            Pat::Sig(p, t, loc) => Pat::Sig(Box::new(self.pat(*p, mode)?), self.ty(t)?, loc),
            Pat::View(e, p, loc) => Pat::View(Box::new(self.expr(*e)?), Box::new(self.pat(*p, mode)?), loc),
            Pat::OpChain(items, loc) => {
                let mut toks: Vec<Tk<Pat, QName>> = Vec::with_capacity(items.len());
                for it in items {
                    match it {
                        PatOpItem::Operand(p) => toks.push(Tk::Operand(self.pat(p, mode)?)),
                        PatOpItem::Op(q, l) => {
                            let qn = self.value_ref(&q, l, true)?;
                            let (assoc, prec) = self.fixity_of(&qn);
                            toks.push(Tk::Op {
                                op: qn,
                                name: q.name.clone(),
                                assoc,
                                prec,
                                loc: l,
                            });
                        }
                    }
                }
                let bin = |op: QName, l: Loc, a: Pat, b: Pat| Pat::Con(op, vec![a, b], l);
                let neg = |_: Loc, p: Pat| p;
                let resolved = resolve(toks, &bin, &neg).map_err(|e| RnError {
                    loc: if e.loc == Loc::default() { loc } else { e.loc },
                    msg: e.msg,
                })?;
                // Infix uses of pattern synonyms (`x :<| xs`) are expanded like prefix ones.
                self.expand_infix_patsyns(resolved)
            }
        })
    }

    /// Expands the pattern synonyms at the binary nodes of a resolved infix pattern.
    fn expand_infix_patsyns(&self, p: Pat) -> Pat {
        match p {
            Pat::Con(q, args, loc) if args.len() == 2 => {
                let args: Vec<Pat> = args.into_iter().map(|a| self.expand_infix_patsyns(a)).collect();
                match self.patsyn_of(&q) {
                    Some(def) if def.params.len() == 2 => subst_pat(&def.pat, &def.params, &typed_args(args, &def.arg_types, loc)),
                    _ => Pat::Con(q, args, loc),
                }
            }
            other => other,
        }
    }

    fn pats(&mut self, ps: Vec<Pat>, mode: BindMode) -> RR<Vec<Pat>> {
        ps.into_iter().map(|p| self.pat(p, mode)).collect()
    }

    // ----- types ------------------------------------------------------------

    fn ty(&mut self, t: Type) -> RR<Type> {
        Ok(match t {
            Type::Var(v, loc) => Type::Var(v, loc),
            Type::Con(q, loc) => Type::Con(self.tycon_ref(&q, loc)?, loc),
            Type::App(f, a) => Type::App(Box::new(self.ty(*f)?), Box::new(self.ty(*a)?)),
            Type::Fun(a, b) => Type::Fun(Box::new(self.ty(*a)?), Box::new(self.ty(*b)?)),
            Type::List(t, loc) => Type::List(Box::new(self.ty(*t)?), loc),
            Type::Tuple(ts, loc) => Type::Tuple(self.types(ts)?, loc),
            Type::Forall(vs, body) => Type::Forall(self.tyvar_binds(vs)?, Box::new(self.ty(*body)?)),
            Type::Qual(ctx, body) => Type::Qual(self.types(ctx)?, Box::new(self.ty(*body)?)),
            Type::Sig(t, k) => Type::Sig(Box::new(self.ty(*t)?), Box::new(self.ty(*k)?)),
            Type::OpChain(items, loc) => {
                let mut toks: Vec<Tk<Type, QName>> = Vec::with_capacity(items.len());
                for it in items {
                    match it {
                        TypeOpItem::Operand(t) => toks.push(Tk::Operand(self.ty(t)?)),
                        TypeOpItem::Op(q, l) => {
                            let qn = self.tycon_ref(&q, l)?;
                            let (assoc, prec) = self.fixity_of(&qn);
                            toks.push(Tk::Op {
                                op: qn,
                                name: q.name.clone(),
                                assoc,
                                prec,
                                loc: l,
                            });
                        }
                    }
                }
                let bin = |op: QName, l: Loc, a: Type, b: Type| {
                    Type::App(
                        Box::new(Type::App(Box::new(Type::Con(op, l)), Box::new(a))),
                        Box::new(b),
                    )
                };
                let neg = |_: Loc, t: Type| t;
                resolve(toks, &bin, &neg).map_err(|e| RnError {
                    loc: if e.loc == Loc::default() { loc } else { e.loc },
                    msg: e.msg,
                })?
            }
            Type::Lit(l, loc) => Type::Lit(l, loc),
            Type::Wild(loc) => Type::Wild(loc),
        })
    }

    // ----- exports ----------------------------------------------------------

    /// All the children of a type or class (its constructors, fields or methods), as its defining
    /// module (or any other) exports them, beyond the ones an import brought into scope.
    fn all_children(&self, e: &Entity) -> Vec<Entity> {
        let mut out = e.subs.clone();
        let defining = self.ifaces.get(&e.global.module).into_iter();
        for iface in defining.chain(self.ifaces.values()) {
            for x in iface.exports.iter().filter(|x| x.global == e.global) {
                for c in &x.subs {
                    if !out.iter().any(|o| o.global == c.global) {
                        out.push(c.clone());
                    }
                }
            }
        }
        out
    }

    fn compute_exports(&self, exports: &Option<Vec<IEItem>>, loc: Loc) -> RR<Vec<Entity>> {
        let Some(items) = exports else {
            return Ok(self.own.clone());
        };
        let mut out: Vec<Entity> = Vec::new();
        // The exported types and classes with all their children, so that a child exported on its
        // own (`StateT(StateT), runStateT`) is still one of its parent's (imported by `StateT(..)`).
        let mut parents: Vec<(GName, Vec<Entity>)> = Vec::new();
        for item in items {
            match item {
                IEItem::Var(q) if q.qual.is_none() && is_builtin_tycon(&q.name) => {}
                IEItem::Var(q) | IEItem::Pattern(q) => {
                    let e = self.lookup_global(q, Ns::Value).map_err(|c| RnError {
                        loc,
                        msg: if c.is_empty() {
                            format!("Not in scope: ‘{}’", display_qname(q))
                        } else {
                            format!("Ambiguous occurrence ‘{}’", display_qname(q))
                        },
                    })?;
                    out.push(e);
                }
                IEItem::Thing { name, .. } if name.qual.is_none() && is_builtin_tycon(&name.name) => {
                    // Built-in syntax and operators have no defining entity to export.
                }
                IEItem::Thing { name, subs } => {
                    let e = self.lookup_global(name, Ns::Type).map_err(|c| RnError {
                        loc,
                        msg: if c.is_empty() {
                            format!("Not in scope: type constructor or class ‘{}’", display_qname(name))
                        } else {
                            format!("Ambiguous occurrence ‘{}’", display_qname(name))
                        },
                    })?;
                    parents.push((e.global.clone(), self.all_children(&e)));
                    let mut with_subs = e.clone();
                    with_subs.subs = match subs {
                        Subs::None => Vec::new(),
                        Subs::All => e.subs.clone(),
                        Subs::AllPlus(extra) => {
                            let mut all = e.subs.clone();
                            for n in extra {
                                if let Ok(s) = self.lookup_global(&QName::unqual(n.clone()), Ns::Value) {
                                    all.push(s);
                                }
                            }
                            all
                        }
                        Subs::Some(names) => {
                            let mut some: Vec<Entity> =
                                e.subs.iter().filter(|s| names.contains(&s.global.name)).cloned().collect();
                            // A name that is not one of the type's own is a pattern synonym bundled
                            // with it (`ByteString(BS, PS)`).
                            for n in names {
                                if !some.iter().any(|s| s.global.name == *n) {
                                    if let Ok(s) = self.lookup_global(&QName::unqual(n.clone()), Ns::Value) {
                                        some.push(s);
                                    }
                                }
                            }
                            some
                        }
                    };
                    out.push(with_subs);
                }
                IEItem::Module(m) => {
                    if *m == self.module {
                        out.extend(self.own.iter().cloned());
                    } else {
                        // `module M` exports the entities in scope both unqualified and as `M.x`.
                        for (map, unqual) in [(&self.qvals, &self.vals), (&self.qtypes, &self.types)] {
                            for ((q, _occ), list) in map {
                                if q == m {
                                    for e in list {
                                        let unqualified = unqual
                                            .get(e.occ())
                                            .is_some_and(|l| l.iter().any(|x| x.global == e.global));
                                        if !unqualified {
                                            continue;
                                        }
                                        // A type and a constructor can share a global name; they
                                        // are different entities.
                                        if !out.iter().any(|x| x.global == e.global && x.kind.is_value() == e.kind.is_value()) {
                                            out.push(e.clone());
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        let alone: Vec<Entity> = out.iter().filter(|e| e.kind.is_value()).cloned().collect();
        for (parent, children) in &parents {
            for v in &alone {
                if children.iter().any(|c| c.global == v.global) {
                    if let Some(t) = out.iter_mut().find(|t| t.global == *parent && !t.kind.is_value()) {
                        if !t.subs.iter().any(|c| c.global == v.global) {
                            t.subs.push(v.clone());
                        }
                    }
                }
            }
        }
        Ok(out)
    }
}

/// GHC's rules for the equations of one function: a variable binding takes exactly one
/// equation, and all equations must have the same number of arguments.
fn check_arities(name: &str, clauses: &[Clause], loc: Loc) -> RR<()> {
    if clauses.len() > 1 && clauses[0].pats.is_empty() {
        return Err(RnError {
            loc: clauses[1].loc,
            msg: format!("Conflicting definitions for ‘{name}’"),
        });
    }
    if let Some(first) = clauses.first() {
        let n = first.pats.len();
        if let Some(bad) = clauses.iter().find(|c| c.pats.len() != n) {
            return Err(RnError {
                loc: bad.loc,
                msg: format!("Equations for ‘{name}’ have different numbers of arguments"),
            });
        }
    }
    let _ = loc;
    Ok(())
}

/// DuplicateRecordFields: a field name that several data types of the module declare is
/// given an internal name per type (`f%T`), which programs still refer to as `f` (see
/// `field_label`): through its constructor in record syntax, or as a selector where only one
/// is in scope.
fn name_duplicate_fields(decls: &mut [Decl]) {
    fn record_fields(dd: &DataDecl) -> HashSet<String> {
        dd.cons
            .iter()
            .flat_map(|c| match &c.fields {
                ConFields::Record(groups) => groups.iter().flat_map(|(ns, _)| ns.iter().cloned()).collect::<Vec<_>>(),
                ConFields::Positional(_) => Vec::new(),
            })
            .collect()
    }
    fn count(decls: &[Decl], out: &mut HashMap<String, usize>) {
        for d in decls {
            match d {
                Decl::Data(dd) => {
                    for f in record_fields(dd) {
                        *out.entry(f).or_default() += 1;
                    }
                }
                Decl::Many(ds) => count(ds, out),
                _ => {}
            }
        }
    }
    fn rename(decls: &mut [Decl], dup: &HashMap<String, usize>) {
        for d in decls {
            match d {
                Decl::Data(dd) => {
                    let ty = dd.name.clone();
                    for c in &mut dd.cons {
                        if let ConFields::Record(groups) = &mut c.fields {
                            for (ns, _) in groups.iter_mut() {
                                for n in ns.iter_mut() {
                                    if dup.get(n.as_str()).is_some_and(|&k| k > 1) {
                                        *n = format!("{n}%{ty}");
                                    }
                                }
                            }
                        }
                    }
                }
                Decl::Many(ds) => rename(ds, dup),
                _ => {}
            }
        }
    }
    let mut counts = HashMap::new();
    count(decls, &mut counts);
    if counts.values().any(|&n| n > 1) {
        rename(decls, &counts);
    }
}

/// Builds a top-level entity.
fn entity(kind: EntKind, module: &str, name: &str, subs: Vec<Entity>, fields: Vec<String>) -> Entity {
    Entity {
        kind,
        global: GName::new(module, name),
        subs,
        fields,
    }
}

fn add_type(seen: &mut HashSet<String>, name: &str, loc: Loc) -> RR<()> {
    if !seen.insert(name.to_string()) {
        return Err(RnError {
            loc,
            msg: format!("Multiple declarations of ‘{name}’"),
        });
    }
    Ok(())
}

/// Chooses the entities an import or export item denotes.
fn select_item(exports: &[Entity], item: &IEItem) -> Option<Vec<Entity>> {
    match item {
        IEItem::Var(q) | IEItem::Pattern(q) => {
            // A value may be exported on its own or as a method, field or constructor of a
            // type or class exported with its sub-entities.
            let found: Vec<Entity> = exports
                .iter()
                .flat_map(|e| std::iter::once(e).chain(e.subs.iter()))
                .filter(|e| e.kind.is_value() && e.occ() == q.name)
                .cloned()
                .collect();
            if found.is_empty() {
                // Constructors and types are also reachable by a bare name in hiding lists.
                let types: Vec<Entity> = exports
                    .iter()
                    .filter(|e| !e.kind.is_value() && e.occ() == q.name)
                    .cloned()
                    .collect();
                if types.is_empty() {
                    None
                } else {
                    Some(types)
                }
            } else {
                Some(found)
            }
        }
        IEItem::Thing { name, subs } => {
            // A type is exported on its own, or as the associated type of an exported class
            // (`Item` of `IsList(..)`), and can be imported by its name either way.
            let t = exports.iter().find(|e| !e.kind.is_value() && e.occ() == name.name).or_else(|| {
                exports.iter().flat_map(|e| e.subs.iter()).find(|e| !e.kind.is_value() && e.occ() == name.name)
            });
            let Some(t) = t
            else {
                // No type of this name: a constructor (possibly inside an exported type).
                return select_item(exports, &IEItem::Var(name.clone()));
            };
            // The type or class, with the sub-entities the item names (none for a bare `C`).
            let mut kept = t.clone();
            kept.subs = match subs {
                Subs::None => Vec::new(),
                Subs::All | Subs::AllPlus(_) => t.subs.clone(),
                Subs::Some(names) => t.subs.iter().filter(|s| names.contains(&s.global.name)).cloned().collect(),
            };
            Some(vec![kept])
        }
        IEItem::Module(_) => Some(Vec::new()),
    }
}

fn item_name(item: &IEItem) -> String {
    match item {
        IEItem::Var(q) | IEItem::Pattern(q) => q.name.clone(),
        IEItem::Thing { name, .. } => name.name.clone(),
        IEItem::Module(m) => m.clone(),
    }
}

pub fn display_qname(q: &QName) -> String {
    match &q.qual {
        Some(m) => format!("{m}.{}", q.name),
        None => q.name.clone(),
    }
}

/// Variables bound by a pattern, with their locations.
pub fn pat_vars(p: &Pat) -> Vec<(String, Loc)> {
    let mut out = Vec::new();
    collect_pat_vars(p, &mut out);
    out
}

fn collect_pat_vars(p: &Pat, out: &mut Vec<(String, Loc)>) {
    match p {
        Pat::Var(v, l) => out.push((v.clone(), *l)),
        Pat::Wild(_) | Pat::Lit(..) | Pat::NegLit(..) | Pat::View(..) => {
            if let Pat::View(_, inner, _) = p {
                collect_pat_vars(inner, out);
            }
        }
        Pat::Con(_, args, _) => args.iter().for_each(|a| collect_pat_vars(a, out)),
        Pat::Rec { fields, .. } => {
            for (f, sub) in fields {
                match sub {
                    Some(s) => collect_pat_vars(s, out),
                    None => out.push((f.name.clone(), Loc::default())),
                }
            }
        }
        Pat::Tuple(ps, _) | Pat::List(ps, _) => ps.iter().for_each(|a| collect_pat_vars(a, out)),
        Pat::As(v, inner, l) => {
            out.push((v.clone(), *l));
            collect_pat_vars(inner, out);
        }
        Pat::Lazy(inner, _) | Pat::Bang(inner, _) | Pat::Sig(inner, _, _) => collect_pat_vars(inner, out),
        Pat::OpChain(items, _) => {
            for it in items {
                if let PatOpItem::Operand(p) = it {
                    collect_pat_vars(p, out);
                }
            }
        }
    }
}

fn decl_loc(d: &Decl) -> Loc {
    match d {
        Decl::PatSyn { loc, .. } | Decl::PatSynSig { loc, .. } | Decl::IPBind { loc, .. } | Decl::DefaultSig { loc, .. } => *loc,
        Decl::Splice(_, loc) => *loc,
        Decl::Many(ds) => ds.first().map(decl_loc).unwrap_or_default(),
        Decl::Sig { loc, .. }
        | Decl::Fixity { loc, .. }
        | Decl::Fun { loc, .. }
        | Decl::PatBind { loc, .. }
        | Decl::TypeSyn { loc, .. }
        | Decl::Default { loc, .. }
        | Decl::Foreign { loc, .. }
        | Decl::KindSig { loc, .. }
        | Decl::TypeFamily { loc, .. }
        | Decl::TypeInstance { loc, .. } => *loc,
        Decl::Data(dd) => dd.loc,
        Decl::Class(cd) => cd.loc,
        Decl::Instance(id) => id.loc,
        Decl::Deriving(sd) => sd.loc,
    }
}

/// The class name at the head of an instance type, if it is a constructor application.
fn instance_class(head: &Type) -> Option<QName> {
    let mut t = head;
    loop {
        match t {
            Type::App(f, _) => t = f,
            Type::Con(q, _) => return Some(q.clone()),
            _ => return None,
        }
    }
}

/// The global name of a pattern synonym's builder function.
/// The type of a pattern synonym's builder: `forall vs. req => forall ws. prov => t` becomes
/// `forall vs ws. (req, prov) => t`, dropping empty contexts.
fn merge_contexts(t: &Type) -> Type {
    let mut binds: Vec<TyVarBind> = Vec::new();
    let mut ctx: Vec<Type> = Vec::new();
    let mut cur = t;
    loop {
        match cur {
            Type::Forall(bs, body) => {
                binds.extend(bs.iter().cloned());
                cur = body;
            }
            Type::Qual(cs, body) => {
                ctx.extend(cs.iter().cloned());
                cur = body;
            }
            _ => break,
        }
    }
    ctx.retain(|c| !matches!(c, Type::Con(q, _) if q.name == "()") && !matches!(c, Type::Tuple(v, _) if v.is_empty()));
    let body = if ctx.is_empty() { cur.clone() } else { Type::Qual(ctx, Box::new(cur.clone())) };
    if binds.is_empty() {
        body
    } else {
        Type::Forall(binds, Box::new(body))
    }
}

pub fn builder_name(synonym: &str) -> String {
    format!("$b{synonym}")
}

/// The functions that build a pattern synonym's values (before renaming).
fn builder_decls(name: &str, args: &[String], pat: &Pat, dir: &PatSynDir, loc: Loc) -> RR<Vec<Decl>> {
    let fname = builder_name(name);
    match dir {
        PatSynDir::Unidirectional => Ok(Vec::new()),
        PatSynDir::Implicit => {
            let body = pat_to_expr(pat).ok_or_else(|| RnError {
                loc,
                msg: format!("The right-hand side of the implicitly bidirectional pattern synonym ‘{name}’ is not invertible"),
            })?;
            let clause = Clause {
                pats: args.iter().map(|a| Pat::Var(a.clone(), loc)).collect(),
                rhs: Rhs { body: Body::Plain(body), wheres: Vec::new() },
                loc,
                end: loc,
            };
            Ok(vec![Decl::Fun { name: fname, clauses: vec![clause], loc }])
        }
        PatSynDir::Explicit(decls) => {
            let mut funs = Vec::new();
            for d in decls {
                match d {
                    Decl::PatBind { pat: Pat::Con(_, pats, _), rhs, loc } => funs.push(Decl::Fun {
                        name: fname.clone(),
                        clauses: vec![Clause { pats: pats.clone(), rhs: rhs.clone(), loc: *loc, end: *loc }],
                        loc: *loc,
                    }),
                    // An infix builder `x :<| xs = ..`: the operands are its arguments.
                    Decl::PatBind { pat: Pat::OpChain(items, _), rhs, loc }
                        if items.len() == 3 && matches!(&items[1], PatOpItem::Op(q, _) if q.name == *name) =>
                    {
                        let pats: Vec<Pat> = [&items[0], &items[2]]
                            .iter()
                            .filter_map(|it| match it {
                                PatOpItem::Operand(p) => Some(p.clone()),
                                _ => None,
                            })
                            .collect();
                        funs.push(Decl::Fun {
                            name: fname.clone(),
                            clauses: vec![Clause { pats, rhs: rhs.clone(), loc: *loc, end: *loc }],
                            loc: *loc,
                        })
                    }
                    other => {
                        return Err(RnError {
                            loc: decl_loc(other),
                            msg: "The builder of a pattern synonym must define it by its constructor syntax".into(),
                        })
                    }
                }
            }
            Ok(crate::syntax::parser::group_binds(funs))
        }
    }
}

/// The expression that builds the value a pattern describes, if the pattern is invertible.
fn pat_to_expr(p: &Pat) -> Option<Expr> {
    Some(match p {
        Pat::Var(v, loc) => Expr::Var(QName::unqual(v.clone()), *loc),
        Pat::Lit(l, loc) => Expr::Lit(l.clone(), *loc),
        Pat::NegLit(l, loc) => Expr::App(
            Box::new(Expr::Var(QName { qual: Some(BASE.into()), name: "negate".into() }, *loc)),
            Box::new(Expr::Lit(l.clone(), *loc)),
        ),
        Pat::Con(q, args, loc) => {
            let mut e = Expr::Con(q.clone(), *loc);
            for a in args {
                e = Expr::App(Box::new(e), Box::new(pat_to_expr(a)?));
            }
            e
        }
        Pat::Tuple(ps, loc) => Expr::Tuple(ps.iter().map(|x| pat_to_expr(x).map(Some)).collect::<Option<_>>()?, *loc),
        Pat::List(ps, loc) => Expr::List(ps.iter().map(pat_to_expr).collect::<Option<_>>()?, *loc),
        Pat::Sig(q, t, loc) => Expr::Sig(Box::new(pat_to_expr(q)?), t.clone(), *loc),
        Pat::Bang(q, _) | Pat::Lazy(q, _) => pat_to_expr(q)?,
        Pat::OpChain(items, loc) => Expr::OpChain(
            items
                .iter()
                .map(|it| match it {
                    PatOpItem::Operand(x) => pat_to_expr(x).map(OpItem::Operand),
                    PatOpItem::Op(q, l) => Some(OpItem::Op(q.clone(), *l)),
                })
                .collect::<Option<_>>()?,
            *loc,
        ),
        // A record pattern is a record construction (a pun stays a pun).
        Pat::Rec { con, fields, dotdot, loc } => Expr::RecCon {
            con: con.clone(),
            fields: fields
                .iter()
                .map(|(f, p)| match p {
                    Some(p) => pat_to_expr(p).map(|e| (f.clone(), Some(e))),
                    None => Some((f.clone(), None)),
                })
                .collect::<Option<_>>()?,
            dotdot: *dotdot,
            loc: *loc,
        },
        Pat::Wild(_) | Pat::As(..) | Pat::View(..) => return None,
    })
}

/// Substitutes the argument patterns of a pattern synonym use for its parameters.
fn subst_pat(p: &Pat, params: &[String], args: &[Pat]) -> Pat {
    let s = |q: &Pat| subst_pat(q, params, args);
    match p {
        Pat::Var(v, loc) => match params.iter().position(|x| x == v) {
            Some(i) => args[i].clone(),
            None => Pat::Var(v.clone(), *loc),
        },
        Pat::Wild(_) | Pat::Lit(..) | Pat::NegLit(..) | Pat::OpChain(..) => p.clone(),
        Pat::Con(q, ps, loc) => Pat::Con(q.clone(), ps.iter().map(s).collect(), *loc),
        Pat::Rec { con, fields, dotdot, loc } => Pat::Rec {
            con: con.clone(),
            fields: fields.iter().map(|(n, fp)| (n.clone(), fp.as_ref().map(s))).collect(),
            dotdot: *dotdot,
            loc: *loc,
        },
        Pat::Tuple(ps, loc) => Pat::Tuple(ps.iter().map(s).collect(), *loc),
        Pat::List(ps, loc) => Pat::List(ps.iter().map(s).collect(), *loc),
        Pat::As(v, q, loc) => Pat::As(v.clone(), Box::new(s(q)), *loc),
        Pat::Lazy(q, loc) => Pat::Lazy(Box::new(s(q)), *loc),
        Pat::Bang(q, loc) => Pat::Bang(Box::new(s(q)), *loc),
        Pat::Sig(q, t, loc) => Pat::Sig(Box::new(s(q)), t.clone(), *loc),
        Pat::View(e, q, loc) => Pat::View(e.clone(), Box::new(s(q)), *loc),
    }
}

/// The package a module belongs to, for Template Haskell names: the vendored GHC packages by
/// their module prefixes, `main` for the module being compiled and others.
fn package_of_module(module: &str, current: &str) -> String {
    const PRIM: &[&str] = &["GHC.Types", "GHC.Classes", "GHC.CString", "GHC.Prim", "GHC.Tuple", "GHC.Magic", "GHC.PrimopWrappers"];
    if module == current {
        return "main".into();
    }
    if module.starts_with("GHC.Internal.") {
        "ghc-internal".into()
    } else if PRIM.iter().any(|p| module == *p || module.starts_with(&format!("{p}."))) {
        "ghc-prim".into()
    } else if module.starts_with("GHC.Num.") {
        "ghc-bignum".into()
    } else {
        "base".into()
    }
}

/// The argument types of a pattern synonym's signature that mention no type variable.
fn closed_arg_types(sig: &Type) -> Vec<Option<Type>> {
    let mut t = sig;
    loop {
        match t {
            Type::Forall(_, b) => t = b,
            Type::Qual(_, b) => t = b,
            _ => break,
        }
    }
    fn has_var(t: &Type) -> bool {
        match t {
            Type::Var(..) => true,
            Type::App(f, a) | Type::Fun(f, a) => has_var(f) || has_var(a),
            Type::List(e, _) => has_var(e),
            Type::Tuple(ts, _) => ts.iter().any(has_var),
            Type::Sig(t, _) => has_var(t),
            Type::OpChain(items, _) => items.iter().any(|i| matches!(i, TypeOpItem::Operand(x) if has_var(x))),
            _ => false,
        }
    }
    let mut out = Vec::new();
    while let Type::Fun(a, r) = t {
        out.push(if has_var(a) { None } else { Some((**a).clone()) });
        t = r;
    }
    out
}

/// The arguments of a pattern synonym use, with the types its signature gives them.
fn typed_args(args: Vec<Pat>, tys: &[Option<Type>], loc: Loc) -> Vec<Pat> {
    args.into_iter()
        .enumerate()
        .map(|(i, a)| match tys.get(i).cloned().flatten() {
            Some(t) => Pat::Sig(Box::new(a), t, loc),
            None => a,
        })
        .collect()
}
