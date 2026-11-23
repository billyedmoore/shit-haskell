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

use crate::diagnostic::one_edit_apart;
use crate::syntax::ast::*;
use crate::syntax::extensions::Extensions;
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

#[derive(Debug)]
pub struct RnErrors(pub Vec<RnError>);

impl From<RnError> for RnErrors {
    fn from(err: RnError) -> Self {
        Self(vec![err])
    }
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
    /// RebindableSyntax: literals, negation, `if` and literal patterns use the names in scope.
    rebindable: bool,
    partial_type_signatures: bool,
    named_wildcards: bool,
    data_kinds: bool,
    report_unticked_promotions: bool,
    warnings: Vec<(Loc, String, crate::diagnostic::Warning)>,
    empty_data_decls: bool,
    /// Fixities declared for local operators (`where infixl 3 <!>`), by their unique local name.
    local_fixities: HashMap<String, (Assoc, u8)>,
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
    /// The depth of quotations being renamed (see `value_ref`).
    in_quote: usize,
    patsyns: HashMap<GName, PatSynDef>,
    duplicate_record_fields: bool,
    warn_duplicate_exports: bool,
    /// Renaming a `.hs-boot` file: signatures declare values without bindings.
    boot: bool,
}

/// Renames a module against the interfaces of its imports.
/// The prefix of a rewrite rule's binding name: `$rule%<index>%<activation>`, the activation
/// being `A` (always), `F<n>` (from phase n), `B<n>` (before phase n) or `N` (never).
pub const RULE_PREFIX: &str = "$rule%";

/// A rewrite rule as a top-level binding: its binders are the parameters (with their
/// signatures), its body the list of its two sides.
/// An activation as rule and inline marker names end: `A`, `F<n>`, `B<n>` or `N`.
fn activation_code(a: crate::syntax::ast::Activation) -> String {
    use crate::syntax::ast::Activation;
    match a {
        Activation::Always => "A".to_string(),
        Activation::From(n) => format!("F{n}"),
        Activation::Before(n) => format!("B{n}"),
        Activation::Never => "N".to_string(),
    }
}

/// An INLINE (`I`) or NOINLINE (`N`) pragma on the global `target`, as a binding
/// `$rule%inl<i>%<target>%<kind>%<activation> = 'i'` (checked, and kept private, as rules are;
/// see `core::simplify`, which reads it).
fn inline_binding(i: usize, target: &str, p: &crate::syntax::ast::InlinePragma) -> Decl {
    use crate::syntax::ast::{Body, Clause, Expr, Lit, Rhs};
    let kind = if p.inline { "I" } else { "N" };
    let body = Expr::Lit(Lit::Char('i'), p.loc);
    Decl::Fun {
        name: format!("{RULE_PREFIX}inl{i}%{target}%{kind}%{}", activation_code(p.activation)),
        clauses: vec![Clause { pats: Vec::new(), rhs: Rhs { body: Body::Plain(body), wheres: Vec::new() }, loc: p.loc, end: p.loc }],
        loc: p.loc,
    }
}

fn rule_binding(i: usize, r: crate::syntax::ast::Rule) -> Decl {
    use crate::syntax::ast::{Body, Clause, Expr, Pat, Rhs};
    let act = activation_code(r.activation);
    let pats = r
        .binders
        .into_iter()
        .map(|(v, ty)| {
            let var = Pat::Var(v, r.loc);
            match ty {
                Some(t) => Pat::Sig(Box::new(var), t, r.loc),
                None => var,
            }
        })
        .collect();
    let body = Expr::List(vec![r.lhs, r.rhs], r.loc);
    Decl::Fun {
        name: format!("{RULE_PREFIX}{i}%{act}"),
        clauses: vec![Clause { pats, rhs: Rhs { body: Body::Plain(body), wheres: Vec::new() }, loc: r.loc, end: r.loc }],
        loc: r.loc,
    }
}

pub fn rename_module(
    m: Module,
    ifaces: &BTreeMap<String, Iface>,
    extensions: &Extensions,
    boot: bool,
) -> Result<(Module, Iface), RnErrors> {
    let (module, iface, _) = rename_module_with_warnings(m, ifaces, extensions, boot)?;
    Ok((module, iface))
}

pub(crate) fn rename_module_with_warnings(
    mut m: Module,
    ifaces: &BTreeMap<String, Iface>,
    extensions: &Extensions,
    boot: bool,
) -> Result<(Module, Iface, Vec<(Loc, String, crate::diagnostic::Warning)>), RnErrors> {
    let duplicate_record_fields = extensions.is_on("DuplicateRecordFields");
    if duplicate_record_fields {
        name_duplicate_fields(&mut m.decls);
    }
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
        rebindable: extensions.is_on("RebindableSyntax"),
        partial_type_signatures: extensions.is_on("PartialTypeSignatures"),
        named_wildcards: extensions.is_on("NamedWildCards"),
        data_kinds: extensions.is_on("DataKinds"),
        report_unticked_promotions: extensions.warnings.level(crate::diagnostic::Warning::UntickedPromotedConstructors).is_some(),
        warnings: Vec::new(),
        empty_data_decls: extensions.is_on("EmptyDataDecls"),
        local_fixities: HashMap::new(),
        vals: HashMap::new(),
        types: HashMap::new(),
        qvals: HashMap::new(),
        qtypes: HashMap::new(),
        own: Vec::new(),
        locals: Vec::new(),
        counter: 0,
        pat_seen: None,
        duplicate_record_fields,
        warn_duplicate_exports: extensions.warnings.level(crate::diagnostic::Warning::DuplicateExports).is_some(),
        boot,
        in_quote: 0,
        patsyns: ifaces
            .values()
            .flat_map(|i| i.patsyns.iter().map(|(g, d)| (g.clone(), d.clone())))
            .collect(),
    };
    rn.fixities.insert(GName::new(BUILTIN, ":"), (Assoc::Right, 5));
    // The promoted cons in types (`x ': xs`, or `x : xs` with DataKinds) associates as `:` does.
    rn.fixities.insert(GName::new(BUILTIN, "':"), (Assoc::Right, 5));
    rn.run(m, extensions.is_on("ImplicitPrelude"))
}

impl<'a> Renamer<'a> {
    fn run(
        &mut self, m: Module, implicit_prelude: bool,
    ) -> Result<(Module, Iface, Vec<(Loc, String, crate::diagnostic::Warning)>), RnErrors> {
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
        check_group_signatures(&m.decls)?;
        self.collect_own(&m.decls)?;
        let mut previous_default: Option<Loc> = None;
        for decl in &m.decls {
            match decl {
                Decl::PatSynSig { names, loc, .. } => {
                    for name in names {
                        if !self.own.iter().any(|e| e.kind == EntKind::PatSyn && e.occ() == name) {
                            return Err(RnError {
                                loc: *loc,
                                msg: format!("The pattern synonym signature for ‘{name}’ lacks an accompanying binding"),
                            }.into());
                        }
                    }
                }
                Decl::Default { loc, .. } => {
                    if let Some(first) = previous_default.replace(*loc) {
                        return Err(RnError {
                            loc: *loc,
                            msg: format!("Multiple default declarations for class ‘Num’\nThe first declaration is at line {}, column {}.", first.line, first.col),
                        }.into());
                    }
                }
                _ => {}
            }
        }
        let own = self.own.clone();
        for e in &own {
            self.add_entity(e.clone(), true, Some(&self.module.clone()));
            for s in &e.subs {
                self.add_entity(s.clone(), true, Some(&self.module.clone()));
            }
        }
        for decl in &m.decls {
            let Decl::RoleAnnotation { name, roles, loc } = decl else { continue };
            let local = self.lookup_global(name, Ns::Type).ok().filter(|entity| entity.global.module == self.module);
            let Some(entity) = local else {
                return Err(RnError {
                    loc: *loc,
                    msg: format!(
                        "The role annotation for ‘{}’ needs a declaration in this module\nWrite the annotation in the module that declares the datatype or class.",
                        name.name
                    ),
                }.into());
            };
            let datatype = m.decls.iter().find_map(|decl| match decl {
                Decl::Data(data) if data.name == entity.global.name => Some(data),
                _ => None,
            });
            if entity.kind != EntKind::Class && datatype.is_none() {
                return Err(RnError {
                    loc: *loc,
                    msg: format!("A role annotation applies to a datatype or class. ‘{}’ is a type synonym or family.", name.name),
                }.into());
            }
            let arity = if entity.kind == EntKind::Class {
                m.decls.iter().find_map(|decl| match decl {
                    Decl::Class(class) if class.name == entity.global.name => Some(class.tyvars.len()),
                    _ => None,
                })
            } else {
                // GADT result kinds can add visible arguments through kind synonyms.
                datatype.filter(|data| {
                    !data.cons.is_empty() && data.cons.iter().all(|con| con.gadt_result.is_none())
                }).map(|data| data.tyvars.len())
            };
            if let Some(arity) = arity.filter(|arity| arity != roles) {
                return Err(RnError {
                    loc: *loc,
                    msg: format!(
                        "Wrong number of roles in the annotation for ‘{}’\nThe declaration expects {arity}. This annotation supplies {roles}.",
                        name.name
                    ),
                }.into());
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
                        }.into());
                    }
                    self.fixities
                        .insert(GName::new(self.module.clone(), op.clone()), (*assoc, *prec));
                }
            }
        }

        // Pattern synonyms: each definition is renamed now, so that uses in patterns can expand
        // it; implicitly bidirectional ones and explicit builders become functions `$bP`.
        let mut builders = Vec::new();
        // Each after the synonyms its pattern uses (`pattern BinOpApp x op y = OpVal op `App` ..`
        // before `pattern OpVal ..`), so that their uses in it expand.
        let mut order: Vec<&Decl> = Vec::new();
        {
            let syns: Vec<&Decl> = m.decls.iter().filter(|d| matches!(d, Decl::PatSyn { .. })).collect();
            let name_of = |d: &Decl| if let Decl::PatSyn { name, .. } = d { name.clone() } else { String::new() };
            let mut pending: Vec<&Decl> = syns.clone();
            while !pending.is_empty() {
                let names: Vec<String> = pending.iter().map(|d| name_of(d)).collect();
                let ready = pending.iter().position(|d| {
                    let Decl::PatSyn { name, pat, .. } = d else { return true };
                    let mut used = Vec::new();
                    pat_con_names(pat, &mut used);
                    !used.iter().any(|u| u != name && names.contains(u))
                });
                // A cycle: the rest in the order written.
                let i = ready.unwrap_or(0);
                order.push(pending.remove(i));
            }
        }
        for d in order {
            if let Decl::PatSyn { name, args, pat, dir, loc, .. } = d {
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
        let mut errors = Vec::new();
        let ordinary = m.decls.into_iter().filter(|d| !matches!(d, Decl::PatSyn { .. } | Decl::PatSynSig { .. }));
        for d in ordinary.chain(builders) {
            match self.rename_top_decl(d) {
                Ok(decl) => decls.push(decl),
                Err(err) => errors.push(err),
            }
        }
        if !errors.is_empty() {
            errors.sort_by_key(|e| e.loc);
            return Err(RnErrors(errors));
        }
        // Rewrite rules, as bindings `$rule%i%act binders = [lhs, rhs]` (the list makes the two
        // sides one type; see `tc::check_module`). A rule that does not rename is left out:
        // rules only make programs faster.
        if std::env::var_os("HSC_DEBUG_RULES").is_some() && !m.rules.is_empty() {
            eprintln!("rules: {} has {}", self.module, m.rules.len());
        }
        for (i, r) in m.rules.into_iter().enumerate() {
            // A binder of a polymorphic (rank-2) type, as "fold/build"'s `g`, cannot be checked
            // as a parameter here: those rules are built into the simplifier instead.
            if r.binders.iter().any(|(_, t)| t.as_ref().is_some_and(|t| matches!(t, crate::syntax::ast::Type::Forall { .. }))) {
                continue;
            }
            match self.rename_top_decl(rule_binding(i, r)) {
                Ok(d) => decls.push(d),
                Err(e) => {
                    if std::env::var_os("HSC_DEBUG_RULES").is_some() {
                        eprintln!("rule in {} not renamed: {}", self.module, e.msg);
                    }
                }
            }
        }

        // INLINE and NOINLINE pragmas on the module's globals, as marker bindings.
        let mut inline_sites = HashMap::new();
        for (i, p) in m.inlines.iter().enumerate() {
            let pragma = if p.inline { "INLINE" } else { "NOINLINE" };
            let q = self.value_ref_in_scope(&p.name, p.loc, false).ok();
            let module = q.as_ref().and_then(|q| q.qual.as_deref()).filter(|m| *m == self.module);
            let (Some(q), Some(module)) = (&q, module) else {
                return Err(RnError {
                    loc: p.loc,
                    msg: format!("The {pragma} pragma for ‘{}’ lacks an accompanying binding", display_qname(&p.name)),
                }.into());
            };
            if let Some(first) = inline_sites.insert(q.name.clone(), p.loc) {
                return Err(RnError {
                    loc: p.loc,
                    msg: format!("Duplicate {pragma} pragma for ‘{}’\nThe earlier pragma is at line {}, column {}.", q.name, first.line, first.col),
                }.into());
            }
            let target = format!("{module}.{}", q.name);
            if let Ok(d) = self.rename_top_decl(inline_binding(i, &target, p)) {
                decls.push(d);
            }
        }

        // Exports.
        let mut warnings = std::mem::take(&mut self.warnings);
        let exports = self.compute_exports(m.exports.as_deref(), &m.export_locs, m.loc, &imports, &mut warnings)?;
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
            export_locs: m.export_locs,
            imports,
            decls,
            loc: m.loc,
            // Rules are among the declarations now (`rule_binding`).
            rules: Vec::new(),
            inlines: Vec::new(),
        };
        Ok((module, iface, warnings))
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
            merge_entity(map.entry(occ.clone()).or_default(), e.clone());
        }
        if let Some(q) = qualifier {
            let map = if is_value { &mut self.qvals } else { &mut self.qtypes };
            merge_entity(map.entry((q.to_string(), occ)).or_default(), e);
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
                Decl::PatSyn { name, args, record, loc, .. } => {
                    add_val(name, *loc, &mut seen_vals)?;
                    let fields = if *record { args.clone() } else { Vec::new() };
                    own.push(entity(EntKind::PatSyn, &module, name, Vec::new(), fields));
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
                        let mut constructor_fields = HashSet::new();
                        for f in &fields {
                            if !constructor_fields.insert(f) {
                                let field = iface::field_label(f);
                                return Err(RnError {
                                    loc: c.loc,
                                    msg: format!("Constructor ‘{}’ declares field ‘{field}’ more than once", c.name),
                                });
                            }
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
                    check_group_signatures(&cd.decls)?;
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
                "Ambiguous occurrence ‘{}’\nIt could refer to {}",
                q.name,
                names.join(" or ")
            ),
        }
    }

    fn unbound(&self, q: &QName, loc: Loc, ns: Ns, con: bool) -> RnError {
        let name = display_qname(q);
        let mut msg = match (ns, con) {
            (Ns::Type, _) => format!("Not in scope: type constructor or class ‘{name}’"),
            (Ns::Value, true) => format!("Data constructor not in scope: {name}"),
            (Ns::Value, false) => format!("Variable not in scope: {name}"),
        };
        let (plain, qualified) = match ns {
            Ns::Type => (&self.types, &self.qtypes),
            Ns::Value => (&self.vals, &self.qvals),
        };
        let eligible = |entities: &[Entity]| {
            entities.len() == 1 && (ns == Ns::Type
                || matches!(entities[0].kind, EntKind::Con | EntKind::PatSyn) == con)
        };
        let mut suggestions = Vec::new();
        if let Some(alias) = &q.qual {
            for ((module, occ), entities) in qualified {
                if module == alias && eligible(entities) && one_edit_apart(&q.name, occ) {
                    suggestions.push((1, format!("{alias}.{occ}")));
                }
            }
        } else {
            for ((alias, occ), entities) in qualified {
                if occ == &q.name && eligible(entities) {
                    suggestions.push((0, format!("{alias}.{occ}")));
                }
            }
            for (occ, entities) in plain {
                if eligible(entities) && one_edit_apart(&q.name, occ) {
                    suggestions.push((1, occ.clone()));
                }
            }
            if ns == Ns::Value && !con {
                for occ in self.locals.iter().flat_map(|scope| scope.keys()) {
                    if one_edit_apart(&q.name, occ) {
                        suggestions.push((1, occ.clone()));
                    }
                }
            }
        }
        suggestions.sort_unstable();
        suggestions.dedup();
        if !suggestions.is_empty() {
            let rendered: Vec<_> = suggestions.iter().take(3).map(|(_, n)| format!("‘{n}’")).collect();
            msg.push_str(&format!("\nPerhaps you meant {}.", rendered.join(" or ")));
        }
        RnError { loc, msg }
    }

    /// Resolves a variable or constructor in the value namespace.
    fn value_ref(&mut self, q: &QName, loc: Loc, con: bool) -> RR<QName> {
        let r = self.value_ref_in_scope(q, loc, con);
        // In a quotation, a variable not in scope is left for the splice site (`UnboundVarE`),
        // as GHC does.
        if r.is_err() && self.in_quote > 0 && !con && q.qual.is_none() {
            return Ok(QName { qual: Some(UNBOUND_IN_QUOTE.into()), name: q.name.clone() });
        }
        r
    }

    fn value_ref_in_scope(&mut self, q: &QName, loc: Loc, con: bool) -> RR<QName> {
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
            Err(cands) if cands.is_empty() && !con && self.in_quote == 0 && q.qual.is_none() && q.name.starts_with('_') => {
                Ok(QName { qual: Some(EXPR_HOLE.into()), name: q.name.clone() })
            }
            Err(cands) if cands.is_empty() => Err(self.unbound(q, loc, Ns::Value, con)),
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
            if m == self.module {
                if let Some(e) = self.vals.get(&q.name).and_then(|list| list.iter().find(|e| e.global.module == m).cloned()) {
                    return Ok(e);
                }
            }
        }
        match self.lookup_global(q, Ns::Value) {
            Ok(e) => Ok(e),
            Err(c) if c.is_empty() => Err(self.unbound(q, loc, Ns::Value, true)),
            Err(c) => Err(self.ambiguous(q, &c, loc)),
        }
    }

    fn tycon_ref(&self, q: &QName, loc: Loc, infix: bool) -> RR<QName> {
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
        // DataKinds: an unticked `:` in a type is the promoted list cons (optics-core's
        // `Reverse (x : xs) acc`); no type can have that name.
        if q.qual.is_none() && q.name == ":" {
            return Ok(QName { qual: Some(BUILTIN.to_string()), name: "':".to_string() });
        }
        // A promoted data constructor (DataKinds) is looked up among the values; its global
        // name keeps the tick so that it is distinct from a type of the same name.
        if let Some(bare) = q.name.strip_prefix('\'') {
            let value_name = QName { qual: q.qual.clone(), name: bare.to_string() };
            let constructor = self.lookup_global(&value_name, Ns::Value);
            // GHC also permits ticked infix syntax and its wired-in representation constructors.
            if !self.data_kinds && !infix && !constructor.as_ref().is_ok_and(|e| self.wired_promotable_constructor(e)) {
                let name = display_qname(&value_name);
                return Err(RnError { loc, msg: format!("Promoting ‘{name}’ requires DataKinds") });
            }
            // The built-in promoted constructors (list cons and nil, tuples) are always in scope.
            if q.qual.is_none() && (bare == ":" || bare == "[]" || bare == "()" || bare.starts_with("(,")) {
                return Ok(QName { qual: Some(BUILTIN.to_string()), name: format!("'{bare}") });
            }
            if bare.starts_with(|c: char| c.is_uppercase() || c == ':') {
                return match constructor {
                    Ok(e) if e.kind == EntKind::Con => Ok(QName {
                        qual: Some(e.global.module.clone()),
                        name: format!("'{}", e.global.name),
                    }),
                    Ok(e) if e.kind == EntKind::PatSyn => Err(RnError {
                        loc,
                        msg: format!("Pattern synonym ‘{bare}’ cannot be promoted to a type"),
                    }),
                    _ => Err(RnError {
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
            // With DataKinds, a constructor can also be promoted without a tick.
            Err(c) if c.is_empty() => match self.lookup_global(q, Ns::Value) {
                Ok(e) if e.kind == EntKind::Con && self.data_kinds => Ok(QName {
                    qual: Some(e.global.module.clone()),
                    name: format!("'{}", e.global.name),
                }),
                Ok(e) if e.kind == EntKind::Con => {
                    let mut error = self.unbound(q, loc, Ns::Type, false);
                    let name = display_qname(q);
                    error.msg.push_str(&format!("\nEnable DataKinds to promote the data constructor ‘{name}’."));
                    Err(error)
                }
                Ok(e) if e.kind == EntKind::PatSyn && self.data_kinds => Err(RnError {
                    loc,
                    msg: format!("Pattern synonym ‘{}’ cannot be promoted to a type", q.name),
                }),
                _ => Err(self.unbound(q, loc, Ns::Type, false)),
            },
            Err(c) => Err(self.ambiguous(q, &c, loc)),
        }
    }

    fn wired_promotable_constructor(&self, constructor: &Entity) -> bool {
        if constructor.kind != EntKind::Con || constructor.global.module != "GHC.Types" {
            return false;
        }
        let types = self.ifaces.get("GHC.Types").map_or(self.own.as_slice(), |iface| iface.exports.as_slice());
        types.iter().any(|parent| {
            matches!(parent.occ(), "RuntimeRep" | "Levity" | "VecCount" | "VecElem" | "Multiplicity")
                && parent.subs.iter().any(|sub| sub.global == constructor.global)
        })
    }

    fn rename_type_name(&mut self, q: &QName, loc: Loc, infix: bool) -> RR<QName> {
        let resolved = self.tycon_ref(q, loc, infix)?;
        if self.report_unticked_promotions && !q.name.starts_with('\'') && resolved.name.starts_with('\'') {
            let name = display_qname(q);
            let prefix_operator = !infix && q.name.chars().next().is_some_and(crate::syntax::lexer::is_symbol);
            let spelling = if prefix_operator { format!("'({name})") } else { format!("'{name}") };
            self.warnings.push((
                loc,
                format!("Unticked promoted constructor ‘{name}’\nWrite ‘{spelling}’ to make the promotion explicit."),
                crate::diagnostic::Warning::UntickedPromotedConstructors,
            ));
        }
        Ok(resolved)
    }

    /// RebindableSyntax: a numeric literal as `fromInteger (n :: Integer)` or `fromRational
    /// (r :: Rational)` with the `fromInteger`/`fromRational` in scope (renamed already).
    fn rebound_literal(&mut self, l: Lit, loc: Loc) -> RR<Expr> {
        let integer = Type::Con(QName { qual: Some("GHC.Num.Integer".into()), name: "Integer".into() }, loc);
        let (fun, ty) = match &l {
            Lit::Frac(_) => (
                "fromRational",
                Type::App(Box::new(Type::Con(QName { qual: Some("GHC.Internal.Real".into()), name: "Ratio".into() }, loc)), Box::new(integer)),
            ),
            _ => ("fromInteger", integer),
        };
        let f = self.value_ref(&QName::unqual(fun), loc, false)?;
        Ok(Expr::App(Box::new(Expr::Var(f, loc)), Box::new(Expr::Sig(Box::new(Expr::Lit(l, loc)), ty, loc))))
    }

    fn fixity_of(&self, q: &QName) -> (Assoc, u8) {
        match &q.qual {
            Some(m) => self
                .fixities
                .get(&GName::new(m.clone(), q.name.clone()))
                .copied()
                .unwrap_or((Assoc::Left, 9)),
            None => self.local_fixities.get(&q.name).copied().unwrap_or((Assoc::Left, 9)),
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

    /// A failed declaration may leave a partially renamed pattern, scope, or quotation.
    fn rename_top_decl(&mut self, decl: Decl) -> RR<Decl> {
        let depth = self.locals.len();
        let in_quote = self.in_quote;
        let result = self.top_decl(decl).and_then(|decl| {
            if let Decl::TypeInstance { name, equation, .. } = &decl {
                let entities = self.own.iter().chain(self.ifaces.values().flat_map(|iface| &iface.exports));
                let mut classes = entities.filter(|entity| entity.kind == EntKind::Class);
                if let Some(class) = classes.find(|class| class.subs.iter().any(|sub| {
                    sub.kind == EntKind::TyCon && sub.global.name == name.name
                        && Some(sub.global.module.as_str()) == name.qual.as_deref()
                })) {
                    return Err(RnError {
                        loc: equation.loc,
                        msg: format!("The associated type ‘{}’ must be defined inside an instance of ‘{}’", name.name, class.occ()),
                    });
                }
            }
            Ok(decl)
        });
        if result.is_err() {
            self.locals.truncate(depth);
            self.pat_seen = None;
            self.in_quote = in_quote;
        }
        result
    }

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
                    ty: self.partial_signature(ty, self.named_wildcards)?,
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
                check_type_binders(&dd.tyvars, dd.loc)?;
                if !dd.newtype && !self.boot && dd.cons.is_empty() && !self.empty_data_decls {
                    return Err(RnError {
                        loc: dd.loc,
                        msg: format!("Data type ‘{}’ has no constructors\nEnable EmptyDataDecls to permit this declaration.", dd.name),
                    });
                }
                if dd.newtype {
                    check_newtype(&dd)?;
                }
                dd.tyvars = self.tyvar_binds(std::mem::take(&mut dd.tyvars))?;
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
            Decl::TypeSyn { name, tyvars, rhs, loc } => {
                check_type_binders(&tyvars, loc)?;
                Decl::TypeSyn {
                    name,
                    tyvars: self.tyvar_binds(tyvars)?,
                    rhs: self.ty(rhs)?,
                    loc,
                }
            }
            Decl::Class(mut cd) => {
                check_type_binders(&cd.tyvars, cd.loc)?;
                cd.context = self.types(cd.context)?;
                let class = QName { qual: Some(self.module.clone()), name: cd.name.clone() };
                let mut decls = Vec::new();
                for d in std::mem::take(&mut cd.decls) {
                    decls.push(match d {
                        Decl::Sig { names, ty, loc } => Decl::Sig {
                            ty: self.complete_signature(ty, "a class method signature")?,
                            names,
                            loc,
                        },
                        Decl::DefaultSig { names, ty, loc } => Decl::DefaultSig {
                            ty: self.complete_signature(ty, "a default method signature")?,
                            names,
                            loc,
                        },
                        Decl::TypeInstance { name, equation, .. } if self.class_assoc(&class, &name.name).is_none() => {
                            return Err(RnError {
                                loc: equation.loc,
                                msg: format!("Class ‘{}’ has no associated type named ‘{}’", cd.name, name.name),
                            });
                        }
                        // Boot interfaces can describe a default without checking its body.
                        binding @ Decl::Fun { .. } if self.boot => binding,
                        other => self.top_decl(other)?,
                    });
                }
                cd.decls = decls;
                Decl::Class(cd)
            }
            Decl::Instance(mut id) => {
                check_group_signatures(&id.decls)?;
                check_type_binders(&id.binders, id.loc)?;
                let methods_with_bindings: HashSet<_> = id.decls.iter().filter_map(|decl| match decl {
                    Decl::Fun { name, .. } => Some(name.as_str()),
                    _ => None,
                }).collect();
                for decl in &id.decls {
                    if let Decl::Sig { names, loc, .. } = decl {
                        for name in names {
                            if !methods_with_bindings.contains(name.as_str()) {
                                return Err(RnError {
                                    loc: *loc,
                                    msg: format!("The instance method signature for ‘{name}’ lacks an accompanying binding"),
                                });
                            }
                        }
                    }
                }
                id.binders = self.tyvar_binds(id.binders)?;
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
                            if let Some(class) = class.as_ref().filter(|class| {
                                methods.is_some() && class.qual.as_deref() == Some(self.module.as_str())
                            }) {
                                return Err(RnError {
                                    loc: equation.loc,
                                    msg: format!("Class ‘{}’ has no associated type named ‘{}’", class.name, name.name),
                                });
                            }
                        }
                    }
                    decls.push(match d {
                        Decl::Sig { names, ty, loc } => Decl::Sig {
                            ty: self.complete_signature(ty, "an instance method signature")?,
                            names,
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
            Decl::KindSig { name, kind, loc } => {
                if !self.own.iter().any(|e| !e.kind.is_value() && e.occ() == name) {
                    return Err(RnError {
                        loc,
                        msg: format!("The standalone kind signature for ‘{name}’ lacks an accompanying declaration"),
                    });
                }
                Decl::KindSig {
                    name,
                    kind: self.complete_signature(kind, "a standalone kind signature")?,
                    loc,
                }
            }
            annotation @ Decl::RoleAnnotation { .. } => annotation,
            Decl::TypeFamily {
                name,
                params,
                equations,
                abstract_closed,
                data,
                injective,
                loc,
            } => {
                if let Some(loc) = abstract_closed.filter(|_| !self.boot) {
                    return Err(RnError {
                        loc,
                        msg: format!("The ‘..’ in closed type family ‘{name}’ is only allowed in an hs-boot file"),
                    });
                }
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
                    abstract_closed,
                    data,
                    injective,
                    loc,
                }
            }
            Decl::TypeInstance { name, equation, loc } => {
                let qname = self.tycon_ref(&name, loc, false)?;
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
    fn local_decls(&mut self, mut decls: Vec<Decl>) -> RR<Vec<Decl>> {
        check_group_signatures(&decls)?;
        let mut seen: HashSet<String> = HashSet::new();
        if self.locals.is_empty() {
            self.push();
        }
        // `T{..}` binds the fields it does not name: spelled out, so that they are in scope in
        // the whole group.
        for d in decls.iter_mut() {
            if let Decl::PatBind { pat, .. } = d {
                self.expand_wildcards(pat);
            }
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
        // The group's fixity declarations, for its operators.
        for d in &decls {
            if let Decl::Fixity { assoc, prec, ops, loc } = d {
                for op in ops {
                    match self.locals.last().and_then(|l| l.get(op)).cloned() {
                        Some(u) => {
                            self.local_fixities.insert(u, (*assoc, *prec));
                        }
                        None => {
                            return Err(RnError {
                                loc: *loc,
                                msg: format!("The fixity signature for ‘{op}’ lacks an accompanying binding"),
                            })
                        }
                    }
                }
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
                        match self.locals.last().and_then(|scope| scope.get(&n)) {
                            Some(u) => us.push(u.clone()),
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
                        ty: self.partial_signature(ty, false)?,
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
                } else if is_type && q.qual.is_none() && matches!(q.name.as_str(), "~" | "->" | "[]") {
                    // The built-in type operators and the list type: GHC names them in GHC.Types.
                    ("mkNameG_tc", Some(("GHC.Types".to_string(), q.name.clone())))
                } else if q.qual.is_none() && (q.name == "()" || q.name.starts_with("(,")) {
                    // Unit and the tuple types/constructors, in GHC.Tuple.
                    (if is_type { "mkNameG_tc" } else { "mkNameG_d" }, Some(("GHC.Tuple".to_string(), q.name.clone())))
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
            Expr::QuoteType(t, loc) => {
                let renamed = self.ty(t)?;
                self.quote_type(&renamed, loc)?
            }
            Expr::QuotePat(p, loc) => {
                let depth = self.locals.len();
                self.push();
                self.in_quote += 1;
                let renamed = self.pat(*p, BindMode::Fresh);
                self.in_quote -= 1;
                self.locals.truncate(depth);
                self.quote_pat(&renamed?, loc)?
            }
            Expr::Quote(kind, body, loc) => {
                // The names in a quoted expression are resolved as in any expression; the
                // quotation is then the code that builds its syntax (`quote.rs`).
                match (kind, body) {
                    ('e' | 'T', Some(b)) => {
                        self.in_quote += 1;
                        let renamed = self.expr(*b);
                        self.in_quote -= 1;
                        let renamed = renamed?;
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
            Expr::Var(q, loc) if q.qual.as_deref() == Some("%ThisModule") => Expr::Lit(Lit::Str(self.module.clone()), loc),
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
            Expr::Lit(l @ (Lit::Int(_) | Lit::Frac(_)), loc) if self.rebindable => self.rebound_literal(l, loc)?,
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
            // RebindableSyntax: `if c then t else e` is `ifThenElse c t e`.
            Expr::If(c, t, e, loc) if self.rebindable => {
                let f = Expr::Var(self.value_ref(&QName::unqual("ifThenElse"), loc, false)?, loc);
                let (c, t, e) = (self.expr(*c)?, self.expr(*t)?, self.expr(*e)?);
                [c, t, e].into_iter().fold(f, |acc, x| Expr::App(Box::new(acc), Box::new(x)))
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
                let op = self.builder_op(op);
                Expr::LeftSection(Box::new(e), op, loc)
            }
            Expr::RightSection(op, e, loc) => {
                let op = self.op_name(&op, loc)?;
                let op = self.builder_op(op);
                let e = self.expr(*e)?;
                Expr::RightSection(op, Box::new(e), loc)
            }
            Expr::RecCon {
                con,
                fields,
                dotdot,
                loc,
            } => {
                check_record_fields(&fields, loc)?;
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
                check_record_fields(&fields, loc)?;
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
            Expr::Sig(e, t, loc) => Expr::Sig(Box::new(self.expr(*e)?), self.partial_signature(t, false)?, loc),
            Expr::Wild(loc) => Expr::Wild(loc),
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

    /// A bidirectional pattern synonym used as an operator (`` x `P` y ``) is its builder.
    fn builder_op(&self, op: QName) -> QName {
        match self.patsyn_of(&op) {
            Some(def) if def.builder => QName { qual: op.qual.clone(), name: builder_name(&op.name) },
            _ => op,
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
                    let qn = self.builder_op(qn);
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
        // RebindableSyntax: the `negate` in scope.
        let negate = if self.rebindable && toks.iter().any(|t| matches!(t, fixity::Tk::Neg(_))) {
            self.value_ref(&QName::unqual("negate"), loc, false)?
        } else {
            QName { qual: Some(BASE.to_string()), name: "negate".into() }
        };
        let neg = |l: Loc, e: Expr| Expr::App(Box::new(Expr::Var(negate.clone(), l)), Box::new(e));
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
            // RebindableSyntax: a numeric literal pattern is `((== lit) -> True)` with the names in
            // scope (basement's `scaleEnum 0 _`, its own `fromInteger`).
            Pat::Lit(l @ (Lit::Int(_) | Lit::Frac(_)), loc) if self.rebindable => {
                let lit = Expr::Lit(l, loc);
                self.pat(rebound_literal_pat(lit, loc), mode)?
            }
            Pat::NegLit(l @ (Lit::Int(_) | Lit::Frac(_)), loc) if self.rebindable => {
                let lit = Expr::App(Box::new(Expr::Var(QName::unqual("negate"), loc)), Box::new(Expr::Lit(l, loc)));
                self.pat(rebound_literal_pat(lit, loc), mode)?
            }
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
                check_record_fields(&fields, loc)?;
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
            Pat::Sig(p, t, loc) => Pat::Sig(Box::new(self.pat(*p, mode)?), self.partial_signature(t, false)?, loc),
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

    fn partial_signature(&mut self, ty: Type, named: bool) -> RR<Type> {
        if !self.partial_type_signatures {
            if let Some((name, loc)) = ty.first_wildcard(named) {
                return Err(RnError {
                    loc,
                    msg: format!("Found type wildcard ‘{name}’\nEnable PartialTypeSignatures to use an inferred type here."),
                });
            }
        }
        self.ty(ty)
    }

    fn complete_signature(&mut self, ty: Type, context: &str) -> RR<Type> {
        if let Some((_, loc)) = ty.first_wildcard(false) {
            return Err(RnError { loc, msg: format!("Wildcard ‘_’ is not allowed in {context}") });
        }
        self.ty(ty)
    }

    fn ty(&mut self, t: Type) -> RR<Type> {
        Ok(match t {
            Type::Var(v, loc) => Type::Var(v, loc),
            Type::Con(q, loc) => Type::Con(self.rename_type_name(&q, loc, false)?, loc),
            Type::App(f, a) => Type::App(Box::new(self.ty(*f)?), Box::new(self.ty(*a)?)),
            Type::Fun(a, b) => Type::Fun(Box::new(self.ty(*a)?), Box::new(self.ty(*b)?)),
            Type::List(t, loc) => Type::List(Box::new(self.ty(*t)?), loc),
            Type::Tuple(ts, loc) => Type::Tuple(self.types(ts)?, loc),
            Type::Forall { binders, body, parenthesized } => Type::Forall {
                binders: self.tyvar_binds(binders)?, body: Box::new(self.ty(*body)?), parenthesized,
            },
            Type::Qual(ctx, body) => Type::Qual(self.types(ctx)?, Box::new(self.ty(*body)?)),
            Type::Sig(t, k) => Type::Sig(Box::new(self.ty(*t)?), Box::new(self.ty(*k)?)),
            Type::OpChain(items, loc) => {
                let mut toks: Vec<Tk<Type, QName>> = Vec::with_capacity(items.len());
                for it in items {
                    match it {
                        TypeOpItem::Operand(t) => toks.push(Tk::Operand(self.ty(t)?)),
                        TypeOpItem::Op(q, l) => {
                            let qn = self.rename_type_name(&q, l, true)?;
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
            Type::Splice(e, loc) => Type::Splice(Box::new(self.expr(*e)?), loc),
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

    /// Record wildcards (of constructors and record pattern synonyms) as named field puns.
    fn expand_wildcards(&mut self, p: &mut Pat) {
        match p {
            Pat::Rec { con, fields, dotdot, loc } => {
                for (_, sub) in fields.iter_mut() {
                    if let Some(s) = sub {
                        self.expand_wildcards(s);
                    }
                }
                if !*dotdot {
                    return;
                }
                let loc = *loc;
                let labels: Vec<String> = match self.value_ref(con, loc, true).ok().and_then(|q| self.patsyn_of(&q)) {
                    Some(def) => def.names.clone(),
                    None => {
                        let Ok(entity) = self.lookup_entity(con, loc) else { return };
                        entity.fields.iter().map(|f| iface::field_label(f).to_string()).collect()
                    }
                };
                for label in labels {
                    if !fields.iter().any(|(q, _)| q.name == label) {
                        fields.push((QName::unqual(label), None));
                    }
                }
                *dotdot = false;
            }
            Pat::View(_, inner, _) | Pat::As(_, inner, _) | Pat::Lazy(inner, _) | Pat::Bang(inner, _) | Pat::Sig(inner, _, _) => {
                self.expand_wildcards(inner)
            }
            Pat::Con(_, ps, _) | Pat::Tuple(ps, _) | Pat::List(ps, _) => ps.iter_mut().for_each(|x| self.expand_wildcards(x)),
            Pat::OpChain(items, _) => {
                for it in items {
                    if let PatOpItem::Operand(x) = it {
                        self.expand_wildcards(x);
                    }
                }
            }
            _ => {}
        }
    }

    /// The children `T (..)` exports: those of T in scope, however they were imported
    /// (`import M (C, m)` brings class C and its method m in separately).
    fn children_in_scope(&self, e: &Entity) -> Vec<Entity> {
        let mut out = e.subs.clone();
        for c in self.all_children(e) {
            if out.iter().any(|o| o.global == c.global) {
                continue;
            }
            let has = |l: &Vec<Entity>| l.iter().any(|x| x.global == c.global);
            let (unqualified, qualified) = if c.kind.is_value() {
                (&self.vals, &self.qvals)
            } else {
                (&self.types, &self.qtypes)
            };
            let scoped = unqualified.get(c.occ()).is_some_and(has)
                || qualified.iter().any(|((_, occ), l)| occ == c.occ() && has(l));
            if scoped {
                out.push(c);
            }
        }
        out
    }

    fn export_named_children(
        &self, parent: &Entity, names: &[String], loc: Loc, out: &mut Vec<Entity>, errors: &mut Vec<RnError>,
    ) {
        let available = self.children_in_scope(parent);
        let patterns: Vec<_> = names.iter()
            .filter_map(|name| self.lookup_global(&QName::unqual(name.clone()), Ns::Value).ok())
            .filter(|entity| entity.kind == EntKind::PatSyn)
            .collect();
        let is_pattern_field = |child: &Entity| {
            available.iter().chain(&patterns).any(|pattern| {
                pattern.kind == EntKind::PatSyn
                    && pattern.global.module == child.global.module
                    && pattern.fields.contains(&child.global.name)
            })
        };
        for name in names {
            let mut children = available.iter().filter(|child| child.occ() == name).peekable();
            if children.peek().is_some() {
                for child in children {
                    if !out.iter().any(|previous| previous.global == child.global && previous.kind == child.kind) {
                        out.push(child.clone());
                    }
                }
                continue;
            }

            // A pattern synonym can be bundled with a type that did not declare it,
            // together with the selectors of its record fields.
            let q = QName::unqual(name.clone());
            match self.lookup_global(&q, Ns::Value) {
                Ok(child) if child.kind == EntKind::PatSyn || is_pattern_field(&child) => {
                    if !out.iter().any(|previous| previous.global == child.global && previous.kind == child.kind) {
                        out.push(child);
                    }
                }
                Ok(_) => errors.push(RnError {
                    loc,
                    msg: format!(
                        "‘{}’ does not own ‘{name}’\nExport ‘{name}’ on its own or with its declaring type or class.",
                        parent.occ(),
                    ),
                }),
                Err(candidates) if candidates.is_empty() => {
                    let con = name.starts_with(|c: char| c.is_uppercase() || c == ':');
                    let mut error = self.unbound(&q, loc, Ns::Value, con);
                    error.msg.push_str(&format!("\nIn the export of ‘{}’.", parent.occ()));
                    errors.push(error);
                }
                Err(candidates) => errors.push(self.ambiguous(&q, &candidates, loc)),
            }
        }
    }

    fn compute_exports(
        &self, items: Option<&[IEItem]>, export_locs: &[Loc], module_loc: Loc, imports: &[Import],
        warnings: &mut Vec<(Loc, String, crate::diagnostic::Warning)>,
    ) -> Result<Vec<Entity>, RnErrors> {
        use crate::diagnostic::Warning;
        let Some(items) = items else {
            return Ok(self.own.clone());
        };
        #[derive(Clone, Copy, PartialEq, Eq, Hash)]
        enum Namespace {
            Type,
            Value,
            Field,
        }
        #[derive(Clone, Copy)]
        enum ExportStyle {
            Single,
            Grouped,
            Implicit,
        }
        let mut out: Vec<Entity> = Vec::new();
        let mut errors = Vec::new();
        let mut exported = HashMap::<(String, Namespace), (GName, Loc)>::new();
        let mut conflicts = HashSet::new();
        let mut exported_fields = HashSet::new();
        let mut export_sites = HashMap::<(GName, bool), [Option<Loc>; 3]>::new();
        let mut exported_modules = HashSet::new();
        let warn_repeated_children = |wildcard: &[Entity], names: &[String], loc, warnings: &mut Vec<_>| {
            if !self.warn_duplicate_exports {
                return;
            }
            let mut seen: HashSet<&str> = wildcard.iter().map(Entity::occ).collect();
            for name in names {
                if !seen.insert(name) {
                    warnings.push((
                        loc,
                        format!("‘{name}’ appears more than once in this export item"),
                        Warning::DuplicateExports,
                    ));
                }
            }
        };
        // A module can export a type or class more than once (`module System.Random, Uniform(..)`
        // where `System.Random` exported bare `Uniform` without its methods). Merge their
        // sub-entities into one entry so `select_item` sees all exported children.
        let mut push_export = |out: &mut Vec<Entity>, errors: &mut Vec<RnError>, warnings: &mut Vec<_>,
                               e: Entity, item: &IEItem, loc: Loc| {
            let covered = std::iter::once((&e, true)).chain(e.subs.iter().map(|child| (child, false)));
            for (entity, root) in covered {
                if self.warn_duplicate_exports {
                    let style = match item {
                        IEItem::Var(_) | IEItem::Pattern(_) | IEItem::Thing { subs: Subs::None, .. } => ExportStyle::Single,
                        IEItem::Thing { subs, .. } if root
                            || matches!(subs, Subs::Some(_))
                            || matches!(subs, Subs::AllPlus(names) if names.iter().any(|name| name == entity.occ())) => ExportStyle::Grouped,
                        _ => ExportStyle::Implicit,
                    };
                    let sites = export_sites.entry((entity.global.clone(), entity.kind.is_value())).or_default();
                    // A direct export can duplicate any earlier occurrence. Members of
                    // grouped exports warn when both items name the entity explicitly.
                    let first = match style {
                        ExportStyle::Single => sites.iter().flatten().min().copied(),
                        ExportStyle::Grouped => sites[..2].iter().flatten().min().copied(),
                        ExportStyle::Implicit => sites[ExportStyle::Single as usize],
                    };
                    if let Some(first) = first {
                        warnings.push((
                            loc,
                            format!(
                                "‘{}’ is exported more than once\nThe earlier export is at line {}, column {}.",
                                entity.occ(), first.line, first.col,
                            ),
                            Warning::DuplicateExports,
                        ));
                    }
                    sites[style as usize].get_or_insert(loc);
                }
                let namespace = match entity.kind {
                    EntKind::Field if self.duplicate_record_fields => continue,
                    EntKind::Field => Namespace::Field,
                    kind if kind.is_value() => Namespace::Value,
                    _ => Namespace::Type,
                };
                let name = entity.occ();
                if namespace == Namespace::Field && !exported_fields.insert(entity.global.clone()) {
                    continue;
                }
                let (first, first_loc) = exported.entry((name.to_string(), namespace))
                    .or_insert_with(|| (entity.global.clone(), loc));
                // A single module re-export can bring several fields with the same label.
                if namespace == Namespace::Field && *first_loc == loc {
                    continue;
                }
                if *first != entity.global && conflicts.insert((entity.global.clone(), namespace)) {
                    let earlier = format!("The earlier export is at line {}, column {}.", first_loc.line, first_loc.col);
                    let msg = if namespace == Namespace::Field {
                        format!("Duplicate record field ‘{name}’ in the export list\n{earlier}\nEnable DuplicateRecordFields to export both fields.")
                    } else {
                        let namespace = if namespace == Namespace::Value { "value" } else { "type" };
                        format!(
                            "Conflicting exports for {namespace} ‘{name}’\nBoth ‘{}.{name}’ and ‘{}.{name}’ would be exported as ‘{name}’.\n{earlier}",
                            first.module, entity.global.module,
                        )
                    };
                    errors.push(RnError { loc, msg });
                }
            }
            if let Some(existing) = out.iter_mut().find(|x| x.global == e.global && x.kind.is_value() == e.kind.is_value()) {
                for s in e.subs {
                    if !existing.subs.iter().any(|o| o.global == s.global) {
                        existing.subs.push(s);
                    }
                }
            } else {
                out.push(e);
            }
        };
        // The exported types and classes with all their children, so that a child exported on its
        // own (`StateT(StateT), runStateT`) is still one of its parent's (imported by `StateT(..)`).
        let mut parents: Vec<(GName, Vec<Entity>)> = Vec::new();
        for (index, item) in items.iter().enumerate() {
            let loc = export_locs.get(index).copied().unwrap_or(module_loc);
            match item {
                IEItem::Var(q) if q.qual.is_none() && is_builtin_tycon(&q.name) => {}
                IEItem::Var(q) | IEItem::Pattern(q) => {
                    let e = self.lookup_global(q, Ns::Value).map_err(|c| {
                        if c.is_empty() {
                            self.unbound(q, loc, Ns::Value, matches!(item, IEItem::Pattern(_)))
                        } else {
                            self.ambiguous(q, &c, loc)
                        }
                    });
                    match e {
                        Ok(e) => push_export(&mut out, &mut errors, warnings, e, item, loc),
                        Err(error) => errors.push(error),
                    }
                }
                IEItem::Thing { name, .. } if name.qual.is_none() && is_builtin_tycon(&name.name) => {
                    // Built-in syntax and operators have no defining entity to export.
                }
                IEItem::Thing { name, subs } => {
                    let e = self.lookup_global(name, Ns::Type).map_err(|c| {
                        if c.is_empty() {
                            self.unbound(name, loc, Ns::Type, false)
                        } else {
                            self.ambiguous(name, &c, loc)
                        }
                    });
                    let e = match e {
                        Ok(e) => e,
                        Err(error) => {
                            errors.push(error);
                            continue;
                        }
                    };
                    parents.push((e.global.clone(), self.all_children(&e)));
                    let mut with_subs = e.clone();
                    with_subs.subs = match subs {
                        Subs::None => Vec::new(),
                        Subs::All => self.children_in_scope(&e),
                        Subs::AllPlus(extra) => {
                            let mut all = self.children_in_scope(&e);
                            warn_repeated_children(&all, extra, loc, warnings);
                            self.export_named_children(&e, extra, loc, &mut all, &mut errors);
                            all
                        }
                        Subs::Some(names) => {
                            warn_repeated_children(&[], names, loc, warnings);
                            let mut some = Vec::new();
                            self.export_named_children(&e, names, loc, &mut some, &mut errors);
                            some
                        }
                    };
                    push_export(&mut out, &mut errors, warnings, with_subs, item, loc);
                }
                IEItem::Module(m) => {
                    if *m != self.module && !imports.iter().any(|import| {
                        import.alias.as_deref().unwrap_or(&import.module) == m
                    }) {
                        errors.push(RnError {
                            loc,
                            msg: format!("The export ‘module {m}’ does not name an imported module\nImport the module or use the alias in its import declaration."),
                        });
                        continue;
                    }
                    if self.warn_duplicate_exports && !exported_modules.insert(m) {
                        warnings.push((loc, format!("Module ‘{m}’ is exported more than once"), Warning::DuplicateExports));
                    }
                    if *m == self.module {
                        for e in &self.own {
                            push_export(&mut out, &mut errors, warnings, e.clone(), item, loc);
                        }
                    }
                    {
                        // `module M` exports the entities in scope both unqualified and as `M.x`
                        // (the module's own name too, when an import has it as its alias).
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
                                        push_export(&mut out, &mut errors, warnings, e.clone(), item, loc);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        if !errors.is_empty() {
            errors.sort_by(|a, b| (a.loc, &a.msg).cmp(&(b.loc, &b.msg)));
            return Err(RnErrors(errors));
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

/// A binding group can give a name one signature of each kind.
fn check_group_signatures(decls: &[Decl]) -> RR<()> {
    let mut seen = HashMap::new();
    for decl in decls {
        let (kind, names, loc): (&str, &[String], &Loc) = match decl {
            Decl::Sig { names, loc, .. } => ("type signature", names, loc),
            Decl::DefaultSig { names, loc, .. } => ("default signature", names, loc),
            Decl::Fixity { ops, loc, .. } => ("fixity declaration", ops, loc),
            Decl::KindSig { name, loc, .. } => ("standalone kind signature", std::slice::from_ref(name), loc),
            Decl::RoleAnnotation { name, loc, .. } => ("role annotation", std::slice::from_ref(&name.name), loc),
            Decl::PatSynSig { names, loc, .. } => ("pattern synonym signature", names, loc),
            _ => continue,
        };
        for name in names {
            if let Some(first) = seen.insert((kind, name), loc) {
                return Err(RnError {
                    loc: *loc,
                    msg: format!("Duplicate {kind}s for ‘{name}’\nThe first declaration is at line {}, column {}.", first.line, first.col),
                });
            }
        }
    }
    Ok(())
}

fn check_record_fields<T>(fields: &[(QName, Option<T>)], loc: Loc) -> RR<()> {
    let mut seen = HashSet::new();
    for (field, _) in fields {
        if !seen.insert(field.name.as_str()) {
            return Err(RnError { loc, msg: format!("Field ‘{}’ is specified more than once in this record", field.name) });
        }
    }
    Ok(())
}

fn check_type_binders(binders: &[TyVarBind], loc: Loc) -> RR<()> {
    let mut names = HashSet::new();
    for binder in binders {
        if !names.insert(binder.name.as_str()) {
            return Err(RnError { loc, msg: format!("Type variable ‘{}’ is bound more than once", binder.name) });
        }
    }
    Ok(())
}

fn check_newtype(decl: &DataDecl) -> RR<()> {
    let [con] = decl.cons.as_slice() else {
        return Err(RnError {
            loc: decl.loc,
            msg: format!("Newtype ‘{}’ must have exactly one constructor, but has {}", decl.name, decl.cons.len()),
        });
    };
    let (count, strict) = match &con.fields {
        ConFields::Positional(fields) => (fields.len(), fields.iter().any(|field| field.strict)),
        ConFields::Record(groups) => (
            groups.iter().map(|(names, _)| names.len()).sum(),
            groups.iter().any(|(_, field)| field.strict),
        ),
    };
    if count != 1 {
        return Err(RnError {
            loc: con.loc,
            msg: format!("Newtype constructor ‘{}’ must have exactly one field, but has {count}", con.name),
        });
    }
    if strict {
        return Err(RnError {
            loc: con.loc,
            msg: format!("Newtype constructor ‘{}’ cannot have a strictness annotation on its field", con.name),
        });
    }
    Ok(())
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
            let unit = if n == 1 { "argument" } else { "arguments" };
            return Err(RnError {
                loc: bad.loc,
                msg: format!(
                    "Equations for ‘{name}’ have different numbers of arguments\nThe equation on line {} takes {n} {unit}. This equation takes {}.",
                    first.loc.line, bad.pats.len()
                ),
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

/// The qualifier the renamer gives a variable a quotation uses but does not have in scope.
pub(crate) const UNBOUND_IN_QUOTE: &str = "%unbound";

/// A named expression hole survives renaming so the type checker can report its expected type.
pub(crate) const EXPR_HOLE: &str = "%hole";

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
        | Decl::RoleAnnotation { loc, .. }
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
            Type::Forall { binders: bs, body, .. } => {
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
        Type::Forall { binders: binds, body: Box::new(body), parenthesized: false }
    }
}

/// Adds an entity to the ones a name has in scope. The same entity imported again (through
/// another module) brings the children that import has: a type exported abstractly by one module
/// and with its constructors by another has them in scope.
fn merge_entity(list: &mut Vec<Entity>, e: Entity) {
    match list.iter_mut().find(|x| x.global == e.global) {
        Some(x) => {
            for s in e.subs {
                if !x.subs.iter().any(|y| y.global == s.global && y.kind.is_value() == s.kind.is_value()) {
                    x.subs.push(s);
                }
            }
        }
        None => list.push(e),
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
            Box::new(Expr::Var(QName { qual: Some("%Base".into()), name: "negate".into() }, *loc)),
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
            Type::Forall { body, .. } => t = body,
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

/// The constructor names (unqualified) a pattern mentions.
fn pat_con_names(p: &Pat, out: &mut Vec<String>) {
    match p {
        Pat::Con(q, ps, _) => {
            out.push(q.name.clone());
            ps.iter().for_each(|x| pat_con_names(x, out));
        }
        Pat::Rec { con, fields, .. } => {
            out.push(con.name.clone());
            fields.iter().filter_map(|(_, x)| x.as_ref()).for_each(|x| pat_con_names(x, out));
        }
        Pat::Tuple(ps, _) | Pat::List(ps, _) => ps.iter().for_each(|x| pat_con_names(x, out)),
        Pat::As(_, x, _) | Pat::Lazy(x, _) | Pat::Bang(x, _) | Pat::Sig(x, _, _) | Pat::View(_, x, _) => pat_con_names(x, out),
        Pat::OpChain(items, _) => {
            for it in items {
                match it {
                    crate::syntax::ast::PatOpItem::Operand(x) => pat_con_names(x, out),
                    crate::syntax::ast::PatOpItem::Op(q, _) => out.push(q.name.clone()),
                }
            }
        }
        _ => {}
    }
}

/// A literal pattern under RebindableSyntax: the view `(\x -> x == lit) -> True`, before renaming
/// (so that `==` and the literal are the ones in scope).
fn rebound_literal_pat(lit: Expr, loc: Loc) -> Pat {
    let x = "rebind%x".to_string();
    let eq = Expr::OpChain(
        vec![OpItem::Operand(Expr::Var(QName::unqual(x.clone()), loc)), OpItem::Op(QName::unqual("=="), loc), OpItem::Operand(lit)],
        loc,
    );
    let test = Expr::Lam(vec![Pat::Var(x, loc)], Box::new(eq), loc);
    let true_con = QName { qual: Some("%G:GHC.Types".into()), name: "True".into() };
    Pat::View(Box::new(test), Box::new(Pat::Con(true_con, Vec::new(), loc)), loc)
}
