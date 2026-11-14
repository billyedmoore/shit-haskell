//! Module-level declarations: types, constructors, classes, instances, foreign imports.

use std::collections::HashMap;

use super::convert::{global_tycon_name, tvar_index};
use super::env::{ClassInfo, InstInfo, Synonym, TyConInfo};
use super::types::{subst_gen, Pred, Scheme, Ty};
use super::{count_args, Checker, Given, TcError, Wanted, TR};
use crate::core::{Alt, Core, TopBind};
use crate::syntax::ast::{ClassDecl, ConFields, DataDecl, Decl, InstDecl, Module, QName, Type};

impl Checker {
    /// Checks a renamed module and returns its Core bindings. The environment is
    /// extended with everything the module defines.
    pub fn check_module(&mut self, m: &Module) -> TR<Vec<TopBind>> {
        self.module = m.name.clone();
        self.coercions.clear();
        // Equalities a previous module left stuck are of no use to this one.
        self.deferred_eqs.clear();
        self.closed_locals.clear();
        self.dict_counter = 0;
        let decls = &m.decls;

        // 0. Kinds of this module's type constructors.
        self.check_kinds(decls)?;

        // Type families: declared first, so that equations and uses can refer to them.
        let mut family_decls: Vec<&Decl> = Vec::new();
        for d in decls {
            match d {
                Decl::TypeFamily { .. } => family_decls.push(d),
                Decl::Class(cd) => family_decls.extend(cd.decls.iter().filter(|x| matches!(x, Decl::TypeFamily { .. }))),
                _ => {}
            }
        }
        for d in &family_decls {
            if let Decl::TypeFamily { name, params, equations, injective, .. } = d {
                let key = format!("{}.{name}", self.module);
                self.register_family(&key, params.len(), equations.is_some());
                let positions: Vec<usize> =
                    params.iter().enumerate().filter(|(_, p)| injective.contains(&p.name)).map(|(i, _)| i).collect();
                if let Some(f) = self.env.families.get_mut(&key) {
                    f.injective = positions;
                }
            }
        }

        // 1. Type synonyms and type constructors. A synonym may mention one declared after it, so the
        // right-hand sides are converted again until nothing changes (a chain has at most one
        // link per synonym).
        let synonym_decls: Vec<&Decl> = decls.iter().filter(|d| matches!(d, Decl::TypeSyn { .. })).collect();
        for _ in 0..=synonym_decls.len() {
            let mut changed = false;
            for d in &synonym_decls {
                if let Decl::TypeSyn { name, tyvars, rhs, loc } = d {
                    self.loc = *loc;
                    let mut tvs: Vec<String> = tyvars.iter().map(|t| t.name.clone()).collect();
                    let rhs_ty = self.convert_type(rhs, &mut tvs)?;
                    let key = format!("{}.{name}", self.module);
                    let syn = Synonym { nparams: tyvars.len() as u32, rhs: rhs_ty };
                    let same = self.env.synonyms.get(&key).is_some_and(|old| old.rhs == syn.rhs && old.nparams == syn.nparams);
                    if !same {
                        changed = true;
                        self.env.synonyms.insert(key, syn);
                    }
                }
            }
            if !changed {
                break;
            }
        }
        for d in decls {
            if let Decl::Data(dd) = d {
                let cons: Vec<String> = dd.cons.iter().map(|c| format!("{}.{}", self.module, c.name)).collect();
                self.env.tycons.insert(
                    format!("{}.{}", self.module, dd.name),
                    TyConInfo {
                        arity: dd.tyvars.len(),
                        cons,
                    },
                );
            }
        }

        // Equations: closed families in order, then the instances of open families.
        for d in &family_decls {
            if let Decl::TypeFamily { name, equations: Some(eqs), .. } = d {
                let key = format!("{}.{name}", self.module);
                for e in eqs {
                    // A closed family's equations are written with its own name.
                    let _ = name;
                    self.add_family_equation(&key, &e.args, &e.rhs, e.loc)?;
                }
            }
        }
        let mut instance_eqs: Vec<&Decl> = Vec::new();
        for d in decls {
            match d {
                Decl::TypeInstance { .. } => instance_eqs.push(d),
                Decl::Instance(id) => instance_eqs.extend(id.decls.iter().filter(|x| matches!(x, Decl::TypeInstance { .. }))),
                Decl::Class(cd) => instance_eqs.extend(cd.decls.iter().filter(|x| matches!(x, Decl::TypeInstance { .. }))),
                _ => {}
            }
        }
        for d in instance_eqs {
            if let Decl::TypeInstance { name, equation, .. } = d {
                self.add_family_equation(&global_tycon_name(name), &equation.args, &equation.rhs, equation.loc)?;
            }
        }

        // 2. Data constructors and field selectors.
        let mut derived: Vec<Decl> = Vec::new();
        // The declaration each instance derived in a `deriving` clause is for.
        let mut derived_from: Vec<DataDecl> = Vec::new();
        for d in decls {
            if let Decl::Data(dd) = d {
                self.data_decl(dd)?;
                self.env.data_decls.insert(
                    format!("{}.{}", self.module, dd.name),
                    (self.module.clone(), dd.clone()),
                );
                let insts = self.derive_instances(dd)?;
                derived_from.extend(insts.iter().map(|_| dd.clone()));
                derived.extend(insts);
            }
        }

        // 3. Classes: dictionary layout, selectors, and default-method names.
        let mut class_decls: Vec<&ClassDecl> = Vec::new();
        for d in decls {
            if let Decl::Class(cd) = d {
                self.class_header(cd)?;
                class_decls.push(cd);
            }
        }
        // Newtype deriving and standalone deriving, once this module's classes are known.
        for d in decls {
            if let Decl::Data(dd) = d {
                self.register_newtype_deriving(dd)?;
            }
        }
        for d in decls {
            if let Decl::Deriving(sd) = d {
                self.loc = sd.loc;
                derived.extend(self.standalone_derive(sd)?);
            }
        }
        // The associated type equations of derived instances (`Rep T` of a derived Generic).
        for d in &derived {
            if let Decl::Instance(id) = d {
                for x in &id.decls {
                    if let Decl::TypeInstance { name, equation, .. } = x {
                        self.add_family_equation(&global_tycon_name(name), &equation.args, &equation.rhs, equation.loc)?;
                    }
                }
            }
        }

        // 4. Foreign imports.
        for d in decls {
            if let Decl::Foreign { import: true, prim, entity, name, ty, loc } = d {
                self.loc = *loc;
                self.foreign_import(name, entity.as_deref(), ty, *prim)?;
            }
        }

        // 5. Instance headers, visible before any body is checked.
        let mut all_insts: Vec<InstDecl> = Vec::new();
        let mut derived_insts: Vec<(usize, DataDecl)> = Vec::new();
        for d in decls {
            if let Decl::Instance(id) = d {
                self.instance_header(id)?;
                all_insts.push(id.clone());
            }
        }
        for (i, d) in derived.iter().enumerate() {
            if let Decl::Instance(id) = d {
                let before = self.env.instances.len();
                self.instance_header(id)?;
                all_insts.push(id.clone());
                if let Some(dd) = derived_from.get(i).filter(|_| self.env.instances.len() > before) {
                    derived_insts.push((self.env.instances.len() - 1, dd.clone()));
                }
            }
        }
        self.reduce_derived_contexts(&derived_insts);
        // 6. Top-level values.
        let value_decls: Vec<Decl> = decls
            .iter()
            .filter(|d| matches!(d, Decl::Fun { .. } | Decl::PatBind { .. } | Decl::Sig { .. }))
            .cloned()
            .collect();
        if self.boot {
            // A boot file declares types only: each signature is the value's scheme.
            for d in &value_decls {
                if let Decl::Sig { names, ty, loc } = d {
                    self.loc = *loc;
                    let scheme = self.sig_scheme(ty, &[])?;
                    for n in names {
                        self.env.values.insert(format!("{}.{n}", self.module), scheme.clone());
                    }
                }
            }
        } else {
            let t_binds = std::time::Instant::now();
            let mut binds = self.infer_binds(&value_decls, true)?;
            if std::env::var_os("HSC_TIME_TC").is_some() && t_binds.elapsed().as_millis() >= 200 {
                eprintln!("tc {}: top-level values: {} ms", self.module, t_binds.elapsed().as_millis());
            }
            // Evidence that ends up at top level (`d%12` of an ungeneralised group) is named
            // after the module, like everything at top level (see `top_binds_from_evidence`).
            let prefix = format!("{}.", self.module);
            let map: HashMap<String, String> = binds
                .iter()
                .filter(|(n, _)| !n.starts_with(&prefix) && !n.contains('.'))
                .map(|(n, _)| (n.clone(), format!("{prefix}ev%{n}")))
                .collect();
            if !map.is_empty() {
                for (name, body) in binds.iter_mut() {
                    crate::core::rename_vars(body, &map);
                    if let Some(n) = map.get(name.as_str()) {
                        *name = n.clone();
                    }
                }
                for b in self.out.iter_mut() {
                    crate::core::rename_vars(&mut b.body, &map);
                }
            }
            for (name, body) in binds {
                self.out.push(TopBind { name, body });
            }
        }

        // 7. Class default methods and 8. instance bodies: a boot file has none.
        if !self.boot {
            for cd in &class_decls {
                self.class_defaults(cd)?;
            }
            let slow = std::env::var_os("HSC_TIME_TC").is_some();
            for id in &all_insts {
                let t = std::time::Instant::now();
                self.instance_body(id)?;
                if slow && t.elapsed().as_millis() >= 200 {
                    eprintln!("tc {}: instance {:?}: {} ms", self.module, id.head, t.elapsed().as_millis());
                }
            }
        }

        // `main` is an IO action: a type the monomorphism restriction left open (`main = return
        // ()`) is fixed to `IO t` before the remaining constraints are solved, as GHC checks it.
        if self.module == "Main" {
            if let Some(scheme) = self.vars.get("Main.main").cloned() {
                let ty = self.metas.zonk(&scheme.ty);
                let mut ms = Vec::new();
                self.metas.free_metas(&ty, &mut ms);
                if !ms.is_empty() {
                    let t = self.meta();
                    let _ = self.unify_at(&ty, &Ty::app(Ty::con("GHC.Types.IO"), t));
                }
            }
        }

        // 9. Remaining constraints at top level: defaulting, then error.
        let wanted = std::mem::take(&mut self.wanted);
        let (binds, residual) = self.solve(wanted)?;
        self.top_binds_from_evidence(binds);
        let residual = self.default_residual(residual)?;
        if let Some(w) = residual.first() {
            let p = self.metas_zonk_pred(&w.pred);
            return Err(TcError {
                loc: w.loc,
                msg: format!(
                    "Ambiguous type variable arising from a use requiring ({})",
                    self.show_pred(&p)
                ),
            });
        }

        // 10. Publish this module's schemes to the global environment.
        let prefix = format!("{}.", self.module);
        let publish: Vec<(String, Scheme)> = self
            .vars
            .iter()
            .filter(|(k, _)| k.starts_with(&prefix))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        for (k, v) in publish {
            self.env.values.insert(k, v);
        }
        // GHC's desugarer patches these magic definitions: the equality proof is the constructor
        // (its source is self-referential), and unsafeCoerce# is the identity.
        if self.module == "GHC.Internal.Unsafe.Coerce" {
            for b in self.out.iter_mut() {
                if b.name == "GHC.Internal.Unsafe.Coerce.unsafeEqualityProof" {
                    b.body = Core::Con("GHC.Internal.Unsafe.Coerce.UnsafeRefl".into(), vec![]);
                } else if b.name == "GHC.Internal.Unsafe.Coerce.unsafeCoerce#" {
                    b.body = Core::Lam("uc%".into(), Box::new(Core::Var("uc%".into())));
                }
            }
        }
        Ok(std::mem::take(&mut self.out))
    }

    fn data_decl(&mut self, dd: &DataDecl) -> TR<()> {
        self.loc = dd.loc;
        let tycon = format!("{}.{}", self.module, dd.name);
        let tvs: Vec<String> = dd.tyvars.iter().map(|t| t.name.clone()).collect();
        let n = tvs.len() as u32;
        let result_head = Ty::apps(Ty::Con(tycon.clone()), (0..n).map(Ty::Gen).collect());

        // Field types per constructor, with occurrence names of record fields.
        let mut per_con: Vec<(String, Vec<String>, Vec<Ty>, Scheme)> = Vec::new();
        for (tag, c) in dd.cons.iter().enumerate() {
            self.loc = c.loc;
            let mut vars = tvs.clone();
            for e in &c.exist {
                tvar_index(&mut vars, &e.name);
            }
            let mut field_tys = Vec::new();
            let mut occs = Vec::new();
            match &c.fields {
                ConFields::Positional(fs) => {
                    for f in fs {
                        field_tys.push(self.convert_type(&f.ty, &mut vars)?);
                    }
                }
                ConFields::Record(groups) => {
                    for (names, f) in groups {
                        let t = self.convert_type(&f.ty, &mut vars)?;
                        for nm in names {
                            occs.push(nm.clone());
                            field_tys.push(t.clone());
                        }
                    }
                }
            }
            let res = match &c.gadt_result {
                Some(t) => self.convert_type(t, &mut vars)?,
                None => result_head.clone(),
            };
            let mut preds = Vec::new();
            for ctx in &c.context {
                preds.extend(self.convert_context_item(ctx, &mut vars)?);
            }
            let name = format!("{}.{}", self.module, c.name);
            let scheme = Scheme {
                nvars: vars.len() as u32,
                names: vars,
                preds,
                ty: Ty::funs(field_tys.clone(), res),
            };
            per_con.push((name, occs, field_tys, scheme));
            let _ = tag;
        }

        for (tag, (name, occs, _, scheme)) in per_con.iter().enumerate() {
            let strict = strict_flags(dd, tag);
            self.add_con(name, &tycon, tag, scheme.clone(), occs.clone(), dd.newtype, strict);
            for (pos, f) in occs.iter().enumerate() {
                let key = format!("{}.{f}", self.module);
                self.env.fields.entry(key).or_default().push((name.clone(), pos));
            }
        }

        // Field selectors: one per distinct field name.
        let mut seen: Vec<String> = Vec::new();
        for (_, occs, field_tys, _) in &per_con {
            for (pos, f) in occs.iter().enumerate() {
                if seen.contains(f) {
                    continue;
                }
                seen.push(f.clone());
                let sel = format!("{}.{f}", self.module);
                let r = self.fresh_name("rec");
                let b = self.fresh_name("rb");
                let mut alts = Vec::new();
                for (cname, cocc, _, cscheme) in per_con.iter() {
                    if let Some(p) = cocc.iter().position(|o| o == f) {
                        // The constructor's context dictionaries come before its fields.
                        let ndicts = cscheme.preds.len();
                        let arity = cocc.len();
                        let vars: Vec<String> = (0..ndicts + arity).map(|_| self.fresh_name("sf")).collect();
                        alts.push(Alt::Con(cname.clone(), vars.clone(), Core::Var(vars[ndicts + p].clone())));
                    }
                }
                alts.push(Alt::Default(Core::Fail(format!("No match in record selector {f}"))));
                let body = if dd.newtype {
                    // A newtype selector is the identity on the erased value.
                    Core::Lam(r.clone(), Box::new(Core::Var(r)))
                } else {
                    Core::Lam(r.clone(), Box::new(Core::Case(Box::new(Core::Var(r)), b, alts)))
                };
                self.out.push(TopBind {
                    name: sel.clone(),
                    body,
                });
                let _ = pos;
                let field_ty = per_con
                    .iter()
                    .find_map(|(_, occs, tys, _)| occs.iter().position(|o| o == f).map(|p| tys[p].clone()))
                    .expect("field occurs in some constructor");
                self.env.values.insert(
                    sel.clone(),
                    Scheme {
                        nvars: n,
                        names: tvs.clone(),
                        preds: Vec::new(),
                        ty: Ty::fun(result_head.clone(), field_ty),
                    },
                );
                self.vars.insert(
                    sel,
                    self.env.values[&format!("{}.{f}", self.module)].clone(),
                );
            }
        }
        Ok(())
    }

    fn class_header(&mut self, cd: &ClassDecl) -> TR<()> {
        self.loc = cd.loc;
        let class_key = format!("{}.{}", self.module, cd.name);
        let class_vars: Vec<String> = cd.tyvars.iter().map(|t| t.name.clone()).collect();
        let nparams = class_vars.len() as u32;
        let mut tvs = class_vars.clone();
        let mut supers = Vec::new();
        // A superclass may be a quantified constraint (`class (forall a. Eq a => Eq (f a)) => Eq1 f`).
        let outer = std::mem::replace(&mut self.allow_quantified, true);
        for c in &cd.context {
            match self.convert_context_item(c, &mut tvs) {
                Ok(ps) => supers.extend(ps),
                Err(e) => {
                    self.allow_quantified = outer;
                    return Err(e);
                }
            }
        }
        self.allow_quantified = outer;
        let class_pred = Pred {
            class: class_key.clone(),
            args: (0..nparams).map(Ty::Gen).collect(),
        };

        let mut methods: Vec<(String, Scheme)> = Vec::new();
        for d in &cd.decls {
            if let Decl::Sig { names, ty, loc } = d {
                self.loc = *loc;
                for name in names {
                    let mut mtvs = class_vars.clone();
                    let (own_preds, mty) = self.sig_parts(ty, &mut mtvs)?;
                    let mut preds = vec![class_pred.clone()];
                    preds.extend(own_preds);
                    let key = format!("{}.{name}", self.module);
                    methods.push((
                        key,
                        Scheme {
                            nvars: mtvs.len() as u32,
                            names: mtvs,
                            preds,
                            ty: mty,
                        },
                    ));
                }
            }
        }

        let dict_con = format!("{}.D:{}", self.module, cd.name);
        let nfields = supers.len() + methods.len();
        let mut defaults = HashMap::new();
        for d in &cd.decls {
            if let Decl::Fun { name, .. } = d {
                defaults.insert(format!("{}.{name}", self.module), format!("{}.$dm{name}", self.module));
            }
        }
        // The dictionary constructor, and a selector for each method.
        self.env.tycons.insert(
            class_key.clone(),
            TyConInfo {
                arity: nparams as usize,
                cons: vec![dict_con.clone()],
            },
        );
        for (i, (mkey, scheme)) in methods.iter().enumerate() {
            let d = self.fresh_name("dict");
            let b = self.fresh_name("mb");
            let vars: Vec<String> = (0..nfields).map(|_| self.fresh_name("mf")).collect();
            let field = Core::Var(vars[supers.len() + i].clone());
            self.out.push(TopBind {
                name: mkey.clone(),
                body: Core::Lam(
                    d.clone(),
                    Box::new(Core::Case(
                        Box::new(Core::Var(d)),
                        b,
                        vec![Alt::Con(dict_con.clone(), vars, field)],
                    )),
                ),
            });
            self.env.values.insert(mkey.clone(), scheme.clone());
        }
        let position = |n: &str| class_vars.iter().position(|v| v == n);
        let fundeps: Vec<(Vec<usize>, Vec<usize>)> = cd
            .fundeps
            .iter()
            .map(|(from, to)| {
                (
                    from.iter().filter_map(|n| position(n)).collect(),
                    to.iter().filter_map(|n| position(n)).collect(),
                )
            })
            .collect();
        // Default signatures, each as a scheme over the class parameters.
        let mut default_schemes: HashMap<String, Scheme> = HashMap::new();
        for d in &cd.decls {
            if let Decl::DefaultSig { names, ty, loc } = d {
                self.loc = *loc;
                for name in names {
                    let mut mtvs = class_vars.clone();
                    let (own_preds, mty) = self.sig_parts(ty, &mut mtvs)?;
                    let mut preds = vec![class_pred.clone()];
                    preds.extend(own_preds);
                    default_schemes.insert(
                        format!("{}.{name}", self.module),
                        Scheme { nvars: mtvs.len() as u32, names: mtvs, preds, ty: mty },
                    );
                }
            }
        }
        let info = ClassInfo {
            name: class_key.clone(),
            nparams,
            supers,
            methods,
            defaults,
            default_schemes,
            dict_con,
            fundeps,
        };
        self.add_class_info(info);
        Ok(())
    }

    /// Default method implementations: `$dm<m> = \dict -> \own-dicts -> body`.
    fn class_defaults(&mut self, cd: &ClassDecl) -> TR<()> {
        // GADT refinements made inside the declaration end with it.
        let refinements = self.metas.skol_binds.clone();
        let r = self.class_defaults_body(cd);
        self.metas.skol_binds = refinements;
        r
    }

    fn class_defaults_body(&mut self, cd: &ClassDecl) -> TR<()> {
        let class_key = format!("{}.{}", self.module, cd.name);
        let info = self.class(&class_key).cloned().expect("class registered");
        for d in &cd.decls {
            if let Decl::Fun { name, clauses, loc } = d {
                self.loc = *loc;
                let mkey = format!("{}.{name}", self.module);
                let scheme = match info.default_schemes.get(&mkey) {
                    Some(s) => s.clone(),
                    None => info
                        .methods
                        .iter()
                        .find(|(k, _)| *k == mkey)
                        .map(|(_, s)| s.clone())
                        .ok_or_else(|| TcError {
                            loc: *loc,
                            msg: format!("‘{name}’ is not a method of class ‘{}’", cd.name),
                        })?,
                };
                let dm = format!("{}.$dm{name}", self.module);
                // The class's variables scope over its default methods.
                self.explicit_sigs.insert(dm.clone());
                let core = self.check_method(clauses, &scheme, &dm, *loc)?;
                self.out.push(TopBind { name: dm, body: core });
            }
        }
        Ok(())
    }

    /// Checks a method implementation against its full scheme. The class predicate is
    /// the first predicate, so the result takes the class dictionary first.
    fn check_method(&mut self, clauses: &[crate::syntax::ast::Clause], scheme: &Scheme, name: &str, loc: crate::syntax::ast::Loc) -> TR<Core> {
        let binds = self.check_signed(name, clauses, scheme, loc)?;
        Ok(binds.into_iter().next().expect("one binding").1)
    }

    /// How a foreign result of this type is turned into a Haskell value: a `Bool` tests the raw
    /// answer against zero, a one-field box (CInt, Word8, ...) wraps it, and anything else (Int,
    /// Double, the identity boxes) is the raw value already. Newtypes around the type are followed.
    fn ffi_result_kind(&self, ty: &Ty) -> Option<FfiResult> {
        let mut t = ty.clone();
        for _ in 0..8 {
            let Ty::Con(name) = t.split_apps().0.clone() else { return None };
            if name == "GHC.Types.Bool" {
                return Some(FfiResult::Bool);
            }
            let info = self.env.tycons.get(&name)?;
            if info.cons.len() != 1 {
                return None;
            }
            let dc = self.env.datacons.get(&info.cons[0])?;
            if dc.newtype {
                if dc.scheme.nvars != 0 {
                    return None;
                }
                t = dc.scheme.ty.as_fun()?.0.clone();
                continue;
            }
            if dc.arity != 1 || !dc.scheme.preds.is_empty() || crate::tc::expr::is_boxing_con(&info.cons[0]) {
                return None;
            }
            return Some(FfiResult::Box(info.cons[0].clone()));
        }
        None
    }

    fn ffi_wrap(&mut self, kind: FfiResult, raw: Core) -> Core {
        match kind {
            FfiResult::Box(con) => Core::Con(con, vec![raw]),
            FfiResult::Bool => {
                let b = self.fresh_name("fb");
                Core::Case(
                    Box::new(raw),
                    b,
                    vec![
                        Alt::Lit(crate::core::Lit::Int(0), Core::Con("GHC.Types.False".into(), vec![])),
                        Alt::Default(Core::Con("GHC.Types.True".into(), vec![])),
                    ],
                )
            }
        }
    }

    /// The C class of a foreign argument or result type (see `crate::ccall`): newtypes and one-field
    /// boxes are followed to the primitive type, whose width and signedness give the letter.
    fn ffi_class(&self, ty: &Ty) -> char {
        let mut t = ty.clone();
        for _ in 0..12 {
            let Ty::Con(name) = t.split_apps().0.clone() else { return 'i' };
            let prim = match name.as_str() {
                "GHC.Prim.Int8#" => Some('b'),
                "GHC.Prim.Word8#" => Some('B'),
                "GHC.Prim.Int16#" => Some('h'),
                "GHC.Prim.Word16#" => Some('H'),
                "GHC.Prim.Int32#" => Some('w'),
                "GHC.Prim.Word32#" | "GHC.Prim.Char#" => Some('W'),
                "GHC.Prim.Addr#" => Some('p'),
                "GHC.Prim.Double#" => Some('d'),
                "GHC.Prim.Float#" => Some('f'),
                "Builtin.()" => Some('v'),
                "GHC.Types.Bool" => Some('w'),
                _ => None,
            };
            if let Some(c) = prim {
                return c;
            }
            let Some(info) = self.env.tycons.get(&name) else { return 'i' };
            if info.cons.len() != 1 {
                return 'i';
            }
            let Some(dc) = self.env.datacons.get(&info.cons[0]) else { return 'i' };
            if dc.arity != 1 {
                return 'i';
            }
            match dc.scheme.ty.as_fun() {
                Some((field, _)) => t = field.clone(),
                None => return 'i',
            }
        }
        'i'
    }

    fn foreign_import(&mut self, name: &str, entity: Option<&str>, ty: &Type, is_prim: bool) -> TR<()> {
        let mut tvs = Vec::new();
        let (preds, body) = self.sig_parts(ty, &mut tvs)?;
        let scheme = Scheme {
            nvars: tvs.len() as u32,
            names: tvs,
            preds,
            ty: body.clone(),
        };
        let key = format!("{}.{name}", self.module);
        // The C entity: an optional `static`, an optional header (`HsBase.h`), then the symbol.
        let capi = entity.is_some_and(|e| e.split_whitespace().any(|w| w == "%capi"));
        // A safe call is marked with a leading `!`: the runtime lets other threads run during it.
        let safe = !entity.is_some_and(|e| e.split_whitespace().any(|w| w == "%unsafe"));
        let interruptible = entity.is_some_and(|e| e.split_whitespace().any(|w| w == "%interruptible"));
        let header = entity
            .and_then(|e| e.split_whitespace().find(|w| w.ends_with(".h")).map(str::to_string))
            .unwrap_or_default();
        let value = entity.is_some_and(|e| e.split_whitespace().any(|w| w == "value"));
        let prim = entity
            .and_then(|e| {
                e.split_whitespace()
                    .filter(|w| *w != "static" && *w != "%capi" && *w != "%unsafe" && *w != "%interruptible" && *w != "value" && !w.ends_with(".h"))
                    .last()
                    .map(str::to_string)
            })
            .unwrap_or_else(|| name.to_string());
        let arity = count_args(&body);
        // An IO result is a function of the state token, which the primitive takes last and
        // answers with the (# state, result #) pair, like the other IO primitives.
        let mut result = body.clone();
        for _ in 0..arity {
            if let Some((_, r)) = result.as_fun() {
                result = r.clone();
            }
        }
        let io = matches!(result.split_apps().0, Ty::Con(c) if c == "GHC.Types.IO");
        let total = arity + usize::from(io);
        let params: Vec<String> = (0..total).map(|_| self.fresh_name("fa")).collect();
        let args: Vec<Core> = params.iter().map(|p| Core::Var(p.clone())).collect();
        // The primitive answers with the raw machine value; a boxed result type (CInt, Word8,
        // ...) wraps it in its one-field constructor, inside the IO pair when there is one.
        let value_ty = if io {
            result.split_apps().1.first().map(|t| (*t).clone())
        } else {
            Some(result.clone())
        };
        // A C result is raw: a Bool, or a box around one, is made here. A runtime primitive
        // already answers with Haskell values.
        let boxed = if is_prim { None } else { value_ty.clone().and_then(|t| self.ffi_result_kind(&t)) };
        // A C call carries its signature, for the runtimes' generic call (`crate::ccall`).
        let prim = if is_prim {
            prim
        } else {
            let mut arg_tys = Vec::new();
            let mut t = body.clone();
            for _ in 0..arity {
                if let Some((a, r)) = t.as_fun() {
                    arg_tys.push(a.clone());
                    t = r.clone();
                }
            }
            let args: String = arg_tys
                .iter()
                .map(|a| match self.ffi_class(a) {
                    c @ ('d' | 'f') => c,
                    _ => 'i',
                })
                .collect();
            let ret = value_ty.as_ref().map(|v| self.ffi_class(v)).unwrap_or('v');
            let mark = if !safe || value || prim.starts_with('&') {
                ""
            } else if interruptible {
                "!!"
            } else {
                "!"
            };
            if capi {
                // The arguments keep their C classes (pointers apart), for the C wrapper.
                let full: String = arg_tys.iter().map(|a| self.ffi_class(a)).collect();
                format!("{mark}capi:{full}>{ret}:{prim}:{header}:{}", if value { "value" } else { "" })
            } else {
                format!("{mark}ccall:{args}>{ret}:{prim}")
            }
        };
        // The runtime answers a C call with the (# state, value #) pair; a pure call takes the value.
        let call = Core::Prim(prim, args);
        let call = if !io && !is_prim {
            let (s, v, r) = (self.fresh_name("fs"), self.fresh_name("fv"), self.fresh_name("fr"));
            Core::Case(
                Box::new(call),
                r,
                vec![Alt::Con("Builtin.(,)".into(), vec![s, v.clone()], Core::Var(v))],
            )
        } else {
            call
        };
        let call = match (io, boxed) {
            (_, None) => call,
            (false, Some(kind)) => self.ffi_wrap(kind, call),
            (true, Some(kind)) => {
                let (s, v, r) = (self.fresh_name("fs"), self.fresh_name("fv"), self.fresh_name("fr"));
                let inner = self.ffi_wrap(kind, Core::Var(v.clone()));
                Core::Case(
                    Box::new(call),
                    r,
                    vec![Alt::Con(
                        "Builtin.(,)".into(),
                        vec![s.clone(), v],
                        Core::Con("Builtin.(,)".into(), vec![Core::Var(s), inner]),
                    )],
                )
            }
        };
        let mut core = call;
        for p in params.iter().rev() {
            core = Core::Lam(p.clone(), Box::new(core));
        }
        self.out.push(TopBind { name: key.clone(), body: core });
        self.env.values.insert(key.clone(), scheme.clone());
        self.vars.insert(key, scheme);
        Ok(())
    }

    /// Registers an instance: its context, head and dictionary name.
    fn instance_header(&mut self, id: &InstDecl) -> TR<()> {
        self.loc = id.loc;
        let (class_name, args) = instance_head_parts(&id.head).ok_or_else(|| TcError {
            loc: id.loc,
            msg: "Malformed instance head".into(),
        })?;
        let class = match class_name {
            Type::Con(q, _) => global_tycon_name(q),
            _ => return self.err("Malformed instance head"),
        };
        if !self.env.classes.contains_key(&class) {
            if id.overlap.as_deref() == Some(super::derive::DERIVED_MARK) {
                // A derived instance of a class this library does not provide.
                return Ok(());
            }
            return self.err(format!("‘{}’ is not a class", super::types::unqualify(&class)));
        }
        let mut tvs = Vec::new();
        let mut head = Vec::new();
        for a in &args {
            // A head may name a data family instance: it is the representation type then.
            let converted = self.convert_type(a, &mut tvs)?;
            head.push(self.normalize(&converted));
        }
        let mut ctx = Vec::new();
        // An instance's context may hold quantified constraints (`(forall a. Show a => Show (m a))
        // => Show (T m)`): they are wanted where the instance is used and given in its methods.
        let outer = std::mem::replace(&mut self.allow_quantified, true);
        let converted: TR<()> = (|| {
            for c in &id.context {
                ctx.extend(self.convert_context_item(c, &mut tvs)?);
            }
            Ok(())
        })();
        self.allow_quantified = outer;
        converted?;
        // A module can be registered twice (first as a boot file, then for real): the same instance
        // keeps its dictionary name, so that what was compiled against the first still refers to it.
        let kinds: Vec<Option<String>> = args.iter().map(|a| annotated_rep(a)).collect();
        let earlier = self
            .env
            .instances
            .iter()
            .position(|i| i.class == class && i.head == head && i.kinds == kinds && i.dict.starts_with(&format!("{}.$f", self.module)));
        let dict = match earlier {
            Some(i) => self.env.instances.remove(i).dict,
            None => {
                self.dict_counter += 1;
                format!(
                    "{}.$f{}{}_{}",
                    self.module,
                    super::types::unqualify(&class),
                    head.iter().map(|t| head_name(t)).collect::<String>(),
                    self.dict_counter
                )
            }
        };
        self.env.instances.push(InstInfo {
            class,
            nvars: tvs.len() as u32,
            ctx,
            head,
            dict,
            kinds,
        });
        Ok(())
    }

    /// The contexts of the instances derived in `deriving` clauses, as GHC infers them: the class
    /// at each field's type, reduced through the instances to constraints on type variables
    /// (`deriving Eq` of a newtype of `[Range v]` needs what `Eq (Range v)` needs). The derived
    /// instances of a module may need each other's, so this repeats until nothing changes. An
    /// instance whose fields do not reduce keeps the context the deriving pass guessed.
    fn reduce_derived_contexts(&mut self, insts: &[(usize, DataDecl)]) {
        for _round in 0..8 {
            let mut changed = false;
            for (idx, dd) in insts {
                let inst = self.env.instances[*idx].clone();
                let short = super::types::unqualify(&inst.class);
                if !matches!(short, "Eq" | "Ord" | "Show" | "Read" | "Data") || inst.head.len() != 1 {
                    continue;
                }
                let Some(ctx) = self.derived_context(&inst, dd) else { continue };
                if ctx != inst.ctx {
                    self.env.instances[*idx].ctx = ctx;
                    changed = true;
                }
            }
            if !changed {
                return;
            }
        }
    }

    fn derived_context(&mut self, inst: &InstInfo, dd: &DataDecl) -> Option<Vec<Pred>> {
        if dd.tyvars.len() != inst.nvars as usize || dd.cons.iter().any(|c| !c.exist.is_empty() || c.gadt_result.is_some() || !c.context.is_empty()) {
            return None;
        }
        let mut wanted = Vec::new();
        for c in &dd.cons {
            for t in super::derive::field_types(c) {
                let mut tvs: Vec<String> = dd.tyvars.iter().map(|tv| tv.name.clone()).collect();
                let ty = self.convert_type(&t, &mut tvs).ok()?;
                if tvs.len() != dd.tyvars.len() {
                    return None;
                }
                wanted.push(Pred { class: inst.class.clone(), args: vec![self.normalize(&ty)] });
            }
        }
        // `Data` has `Typeable` as a superclass: every parameter must be Typeable (a phantom one
        // included), unless its `Data` constraint implies it.
        if super::types::unqualify(&inst.class) == "Data" {
            for i in 0..inst.nvars {
                wanted.push(Pred { class: "GHC.Internal.Data.Typeable.Internal.Typeable".into(), args: vec![Ty::Gen(i)] });
            }
        }
        let this = Pred { class: inst.class.clone(), args: inst.head.clone() };
        let mut out: Vec<Pred> = Vec::new();
        let mut stack = vec![this];
        for w in wanted {
            self.reduce_derived_pred(w, &mut stack, &mut out, 0)?;
        }
        // A constraint another one implies through its superclasses is left out.
        let mut implied = Vec::new();
        for (i, p) in out.iter().enumerate() {
            let mut closure = Vec::new();
            self.superclass_closure(p, &mut closure, 0);
            for q in closure.into_iter().skip(1) {
                if q != *p && out.iter().enumerate().any(|(j, o)| j != i && *o == q) {
                    implied.push(q);
                }
            }
        }
        out.retain(|p| !implied.contains(p));
        Some(out)
    }

    fn reduce_derived_pred(&mut self, p: Pred, stack: &mut Vec<Pred>, out: &mut Vec<Pred>, depth: usize) -> Option<()> {
        if depth > 40 {
            return None;
        }
        let head_is_var = |t: &Ty| {
            let mut h = t;
            while let Ty::App(f, _) = h {
                h = f;
            }
            matches!(h, Ty::Gen(_))
        };
        if p.args.iter().all(head_is_var) {
            if !out.contains(&p) {
                out.push(p);
            }
            return Some(());
        }
        // An instance being derived needs nothing more of itself (a recursive type).
        if stack.contains(&p) {
            return Some(());
        }
        let (_, subs) = self.find_instance(&p)?;
        stack.push(p);
        for s in subs {
            let s = Pred { class: s.class.clone(), args: s.args.iter().map(|t| self.normalize(&self.metas.zonk(t))).collect() };
            self.reduce_derived_pred(s, stack, out, depth + 1)?;
        }
        stack.pop();
        Some(())
    }

    /// Builds the dictionary for an instance and checks its methods.
    fn instance_body(&mut self, id: &InstDecl) -> TR<()> {
        // GADT refinements made inside the declaration end with it.
        let refinements = self.metas.skol_binds.clone();
        let r = self.instance_body_body(id);
        self.metas.skol_binds = refinements;
        r
    }

    fn instance_body_body(&mut self, id: &InstDecl) -> TR<()> {
        self.loc = id.loc;
        if id.overlap.as_deref() == Some(super::derive::DERIVED_MARK) {
            let known = {
                let (class_name, _) = instance_head_parts(&id.head).expect("validated");
                matches!(class_name, Type::Con(q, _) if self.env.classes.contains_key(&global_tycon_name(q)))
            };
            if !known {
                return Ok(());
            }
        }
        let (class_name, args) = instance_head_parts(&id.head).expect("validated by header");
        let class = match class_name {
            Type::Con(q, _) => global_tycon_name(q),
            _ => unreachable!("validated by header"),
        };
        // Find this instance's registered info by its head.
        let mut tvs = Vec::new();
        let mut head_check = Vec::new();
        let kinds: Vec<Option<String>> = args.iter().map(|a| annotated_rep(a)).collect();
        for a in args {
            let converted = self.convert_type(a, &mut tvs)?;
            head_check.push(self.normalize(&converted));
        }
        let inst = self
            .env
            .instances
            .iter()
            .rev()
            .find(|i| i.class == class && i.head == head_check && (i.kinds.is_empty() || i.kinds == kinds))
            .cloned()
            .expect("instance registered");
        let info = self.class(&class).cloned().expect("class registered");
        // The variables only the context mentions (`gs` of `(gs ~ F opts) => C opts`) come after
        // the head's, as the header numbered them: they scope over the methods too.
        {
            let outer = std::mem::replace(&mut self.allow_quantified, true);
            for c in &id.context {
                let _ = self.convert_context_item(c, &mut tvs);
            }
            self.allow_quantified = outer;
        }

        // Skolemise the instance variables (named as in the source); the context is given.
        let names: Vec<String> =
            (0..inst.nvars as usize).map(|i| tvs.get(i).cloned().unwrap_or_else(|| format!("i{i}"))).collect();
        let skolems: Vec<Ty> = names.iter().map(|n| self.metas.fresh_skolem(n)).collect();
        let head: Vec<Ty> = inst.head.iter().map(|t| subst_gen(t, &skolems)).collect();
        let ctx: Vec<Pred> = inst
            .ctx
            .iter()
            .map(|p| Pred {
                class: p.class.clone(),
                args: p.args.iter().map(|t| subst_gen(t, &skolems)).collect(),
            })
            .collect();
        let dparams: Vec<String> = ctx.iter().map(|_| self.fresh_name("di")).collect();
        let givens_len = self.givens.len();
        // The instance head's variables scope over its methods.
        let scope_len = self.scoped.len();
        for (n, sk) in tvs.iter().zip(&skolems) {
            self.scoped.push((n.clone(), sk.clone()));
        }
        for (p, d) in ctx.iter().zip(&dparams) {
            self.add_given(Given {
                pred: p.clone(),
                ev: Core::Var(d.clone()),
            });
        }

        let this = self.fresh_name("this");
        // Superclass dictionaries.
        let mut super_binds: Vec<(String, Core)> = Vec::new();
        let mut super_vars: Vec<Core> = Vec::new();
        let outer_wanted = std::mem::take(&mut self.wanted);
        for sup in &info.supers {
            let p = super::types::subst_gen_pred(sup, &head);
            let ev = self.fresh_name("sd");
            self.wanted.push(Wanted {
                pred: p,
                ev: ev.clone(),
                loc: id.loc,
            });
            super_vars.push(Core::Var(ev));
        }
        let super_wanted = std::mem::replace(&mut self.wanted, outer_wanted);
        let (sb, residual) = self.solve(super_wanted)?;
        super_binds.extend(sb);
        if let Some(w) = residual.first() {
            let p = self.metas_zonk_pred(&w.pred);
            self.givens.truncate(givens_len);
            return Err(TcError {
                loc: w.loc,
                msg: format!("No instance for ({}) arising from the superclasses of an instance declaration", self.show_pred(&p)),
            });
        }

        // Methods.
        let mut fields: Vec<Core> = super_vars;
        let mut method_binds: Vec<(String, Core)> = Vec::new();
        // What the defaults need is solved here, while the instance context is given.
        let outer_wanted = std::mem::take(&mut self.wanted);
        for (mkey, mscheme) in &info.methods {
            let occ = super::types::unqualify(mkey).to_string();
            let impl_decl = id.decls.iter().find_map(|d| match d {
                Decl::Fun { name, clauses, loc } if *name == occ => Some((clauses, *loc)),
                _ => None,
            });
            let field = match impl_decl {
                Some((clauses, loc)) => {
                    let sch = instantiate_method(mscheme, info.nparams, &head);
                    let core = self.check_method(clauses, &sch, &format!("{}.{occ}@inst", self.module), loc)?;
                    let name = self.fresh_name("mi");
                    method_binds.push((name.clone(), core));
                    Core::Var(name)
                }
                None => match info.defaults.get(mkey) {
                    Some(dm) => {
                        // A default with its own context gets those dictionaries at this instance.
                        let mut core = Core::App(Box::new(Core::Var(dm.clone())), Box::new(Core::Var(this.clone())));
                        // The method's own dictionaries (`Monad f` of `m1 :: Monad f => ..`) are
                        // parameters of the dictionary field, passed on to the default.
                        let own: Vec<String> = mscheme.preds.iter().skip(1).map(|_| self.fresh_name("dm")).collect();
                        if let Some(ds) = info.default_schemes.get(mkey) {
                            let mut args: Vec<Ty> = head.clone();
                            while args.len() < ds.nvars as usize {
                                args.push(self.metas.fresh(self.level));
                            }
                            let class_level = |t: &Ty| -> bool {
                                fn ok(t: &Ty, n: u32) -> bool {
                                    match t {
                                        Ty::Gen(i) => *i < n,
                                        Ty::App(f, x) => ok(f, n) && ok(x, n),
                                        _ => true,
                                    }
                                }
                                ok(t, info.nparams as u32)
                            };
                            let mut used = vec![false; own.len()];
                            for p in ds.preds.iter().skip(1) {
                                if !p.args.iter().all(|a| class_level(a)) {
                                    let j = mscheme
                                        .preds
                                        .iter()
                                        .skip(1)
                                        .enumerate()
                                        .position(|(j, q)| !used[j] && q.class == p.class);
                                    if let Some(j) = j {
                                        used[j] = true;
                                        core = Core::App(Box::new(core), Box::new(Core::Var(own[j].clone())));
                                        continue;
                                    }
                                }
                                let pred = super::types::subst_gen_pred(p, &args);
                                let ev = self.fresh_name("dd");
                                self.wanted.push(super::Wanted { pred, ev: ev.clone(), loc: id.loc });
                                core = Core::App(Box::new(core), Box::new(Core::Var(ev)));
                            }
                        }
                        if own.is_empty() || !info.default_schemes.contains_key(mkey) {
                            // A default without its own signature takes the method's dictionaries itself.
                            core
                        } else {
                            own.into_iter().rev().fold(core, |acc, d| Core::Lam(d, Box::new(acc)))
                        }
                    }
                    // Exception types are identified by their type name, which stands in for Typeable.
                    None if occ == "exceptionTypeTag" => {
                        let name = inst.head.first().map(head_name).unwrap_or_default();
                        Core::Lam(
                            "tag%".into(),
                            Box::new(Core::Lit(crate::core::Lit::Str(name))),
                        )
                    }
                    None => Core::Fail(format!(
                        "No instance nor default method for class operation {occ}"
                    )),
                },
            };
            fields.push(field);
        }
        let default_wanted = std::mem::replace(&mut self.wanted, outer_wanted);
        let (db, residual) = self.solve(default_wanted)?;
        method_binds.extend(db);
        self.wanted.extend(residual);
        self.scoped.truncate(scope_len);
        self.givens.truncate(givens_len);

        if dparams.is_empty() {
            // A constant dictionary: its methods and superclass dictionaries are top-level
            // bindings, so that calls through the dictionary can be resolved statically.
            let mut fields = fields;
            for field in fields.iter_mut() {
                crate::core::rename_var(field, &this, &inst.dict);
            }
            // At top level they are named after the dictionary (so after its module), not by
            // this compilation's counter: a module loaded from the cache keeps its names.
            let mut binds: Vec<(String, Core)> = super_binds.into_iter().chain(method_binds).collect();
            let renames: Vec<(String, String)> = binds.iter().map(|(n, _)| (n.clone(), format!("{}%{n}", inst.dict))).collect();
            for (old, new) in &renames {
                for field in fields.iter_mut() {
                    crate::core::rename_var(field, old, new);
                }
                for (_, body) in binds.iter_mut() {
                    crate::core::rename_var(body, old, new);
                }
            }
            for ((_, body), (_, new)) in binds.into_iter().zip(renames) {
                self.out.push(TopBind { name: new, body });
            }
            self.out.push(TopBind {
                name: inst.dict.clone(),
                body: Core::Con(info.dict_con.clone(), fields),
            });
            return Ok(());
        }
        let dict_body = Core::Con(info.dict_con.clone(), fields);
        let mut bindings = super_binds;
        bindings.extend(method_binds);
        bindings.push((this.clone(), dict_body));
        let dict_core = Core::Let(bindings, Box::new(Core::Var(this)));
        let core = dparams.into_iter().rev().fold(dict_core, |acc, d| Core::Lam(d, Box::new(acc)));
        self.out.push(TopBind {
            name: inst.dict.clone(),
            body: core,
        });
        Ok(())
    }

    /// Newtype deriving: the dictionary of the derived class at the representation type
    /// serves as the dictionary of the newtype, because newtypes are erased at run time.
    fn register_newtype_deriving(&mut self, dd: &DataDecl) -> TR<()> {
        use crate::syntax::ast::Strategy;
        self.loc = dd.loc;
        // The representation of a newtype (its one field), for newtype deriving.
        let newtype_field: Option<Type> = match (dd.newtype, dd.cons.first()) {
            (true, Some(c)) => match &c.fields {
                ConFields::Positional(fs) if fs.len() == 1 => Some(fs[0].ty.clone()),
                ConFields::Record(groups) if groups.len() == 1 && groups[0].0.len() == 1 => Some(groups[0].1.ty.clone()),
                _ => None,
            },
            _ => None,
        };
        for d in &dd.deriving {
            // DerivingVia: the instance is the class's dictionary at the via type (a type of the
            // same representation), for a data type as for a newtype.
            if let Some(via) = &d.via {
                for cls in &d.classes {
                    let cls_name = match cls {
                        Type::Con(q, _) => global_tycon_name(q),
                        _ => continue,
                    };
                    if !self.env.classes.contains_key(&cls_name) {
                        continue;
                    }
                    let eta = self.constructor_class_arity(&cls_name);
                    let kept: Vec<String> = dd.tyvars.iter().map(|t| t.name.clone()).take(dd.tyvars.len().saturating_sub(eta)).collect();
                    let mut tvs = kept.clone();
                    let rep = self.convert_type(via, &mut tvs)?;
                    let head_args: Vec<Ty> = (0..kept.len() as u32).map(Ty::Gen).collect();
                    let head = Ty::apps(Ty::Con(format!("{}.{}", self.module, dd.name)), head_args);
                    let dict = format!("{}.$fVia{}{}_{}", self.module, super::types::unqualify(&cls_name), dd.name, self.dict_counter);
                    self.dict_counter += 1;
                    self.env.instances.push(InstInfo {
                        class: cls_name.clone(),
                        nvars: tvs.len() as u32,
                        ctx: vec![Pred { class: cls_name.clone(), args: vec![rep] }],
                        head: vec![head],
                        dict: dict.clone(),
                        kinds: Vec::new(),
                    });
                    let d_name = self.fresh_name("gd");
                    self.out.push(TopBind { name: dict, body: Core::Lam(d_name.clone(), Box::new(Core::Var(d_name))) });
                }
                continue;
            }
            let Some(field_ty) = newtype_field.clone() else { continue };
            for cls in &d.classes {
                let mut cls_con = cls;
                while let Type::App(f, _) = cls_con {
                    cls_con = f;
                }
                let short = match cls_con {
                    Type::Con(q, _) => q.name.clone(),
                    _ => continue,
                };
                // Generic, Generic1, Data and Lift are derived stock for a newtype too, as GHC does
                // without a strategy.
                let via_newtype = d.strategy == Some(Strategy::Newtype)
                    || (d.strategy.is_none() && !super::derive::stock_for_newtype(self.newtype_deriving, &short, dd));
                if !via_newtype || d.strategy == Some(Strategy::Anyclass) {
                    continue;
                }
                // A class applied to its leading arguments (`FilterableWithIndex i`): the instance
                // is at those, with the derived type last.
                let mut pre_args: Vec<Type> = Vec::new();
                let mut cls_head = cls;
                while let Type::App(f, x) = cls_head {
                    pre_args.insert(0, (**x).clone());
                    cls_head = f;
                }
                let cls_name = match cls_head {
                    Type::Con(q, _) => global_tycon_name(q),
                    _ => continue,
                };
                if !self.env.classes.contains_key(&cls_name) {
                    continue;
                }
                // Constructor classes (Functor, Monad, ...) are derived for the type without
                // its last parameter, so the representation loses its last argument too.
                let eta = self.constructor_class_arity(&cls_name);
                if eta > dd.tyvars.len() {
                    continue;
                }
                let kept: Vec<String> = dd.tyvars.iter().map(|t| t.name.clone()).take(dd.tyvars.len() - eta).collect();
                // The representation without the dropped parameters (`bi a b` is `bi` for a
                // Bifunctor), which it must end in.
                let mut rep_ast = field_ty.clone();
                let mut ok = true;
                for tv in dd.tyvars.iter().rev().take(eta) {
                    match rep_ast {
                        Type::App(g, arg) if matches!(arg.as_ref(), Type::Var(v, _) if *v == tv.name) => rep_ast = *g,
                        _ => {
                            ok = false;
                            break;
                        }
                    }
                }
                if !ok {
                    continue;
                }
                let mut tvs = kept.clone();
                let rep = self.convert_type(&rep_ast, &mut tvs)?;
                let mut pre: Vec<Ty> = Vec::with_capacity(pre_args.len());
                for t in &pre_args {
                    pre.push(self.convert_type(t, &mut tvs)?);
                }
                let head_args: Vec<Ty> = (0..kept.len() as u32).map(Ty::Gen).collect();
                let head = Ty::apps(Ty::Con(format!("{}.{}", self.module, dd.name)), head_args);
                let dict = format!("{}.$fGND{}{}_{}", self.module, super::types::unqualify(&cls_name), dd.name, self.dict_counter);
                self.dict_counter += 1;
                let mut ctx_args = pre.clone();
                ctx_args.push(rep);
                let mut heads = pre;
                heads.push(head);
                self.env.instances.push(InstInfo {
                    class: cls_name.clone(),
                    nvars: tvs.len() as u32,
                    ctx: vec![Pred { class: cls_name.clone(), args: ctx_args }],
                    head: heads,
                    dict: dict.clone(),
                    kinds: Vec::new(),
                });
                let d_name = self.fresh_name("gd");
                self.out.push(TopBind {
                    name: dict,
                    body: Core::Lam(d_name.clone(), Box::new(Core::Var(d_name))),
                });
            }
        }
        Ok(())
    }

    /// Generated instances for `deriving` clauses; see `derive`.
    fn derive_instances(&mut self, dd: &DataDecl) -> TR<Vec<Decl>> {
        super::derive::derive_instances(self, dd)
    }

    /// `deriving instance C (T ..)`: the stock instance for `T`, generated from its declaration
    /// (possibly in another module, whose names are then resolved in that module).
    /// The data type that a data family instance type reduces to, if it is one.
    fn family_representation(&mut self, t: &Type) -> Option<String> {
        let mut tvs = Vec::new();
        let ty = self.convert_type(t, &mut tvs).ok()?;
        let reduced = self.normalize(&ty);
        if let Some(rep) = self.partial_family_rep(&reduced) {
            return self.env.data_decls.contains_key(&rep).then_some(rep);
        }
        match reduced.split_apps().0 {
            Ty::Con(c) if self.env.data_decls.contains_key(c) => Some(c.clone()),
            _ => None,
        }
    }

    fn standalone_derive(&mut self, sd: &crate::syntax::ast::StandaloneDeriving) -> TR<Vec<Decl>> {
        use crate::syntax::ast::{Deriving, Type};
        let (cls, arg) = match &sd.head {
            Type::App(c, a) => (c.as_ref().clone(), a.as_ref().clone()),
            _ => return self.err("Malformed standalone deriving declaration"),
        };
        let mut head = &arg;
        while let Type::App(f, _) = head {
            head = f;
        }
        let tycon = match head {
            Type::Con(q, _) => match &q.qual {
                Some(m) => format!("{m}.{}", q.name),
                None => format!("{}.{}", self.module, q.name),
            },
            Type::Tuple(elems, _) => format!("Builtin.({})", ",".repeat(elems.len().saturating_sub(1))),
            // Lists are built in: their Generic instances are derived from a declaration written
            // here; their other instances are written in the libraries.
            Type::List(..) => {
                if !matches!(&cls, Type::Con(q, _) if q.name == "Generic" || q.name == "Generic1") {
                    return Ok(Vec::new());
                }
                "Builtin.[]".to_string()
            }
            // Unboxed tuples and sums: only Template Haskell's `Lift` derives instances for them,
            // which need Template Haskell to be used.
            _ if !matches!(&cls, Type::Con(q, _) if super::derive::is_stock(&q.name)) => return Ok(Vec::new()),
            _ => return self.err("Malformed standalone deriving declaration"),
        };
        // Unit and tuples are built in: their declaration is written here, so the stock
        // instance is generated for them as for any type. Lists keep their instances in Base.
        if let Some(occ) = tycon.strip_prefix("Builtin.") {
            use crate::syntax::ast::{ConDecl, ConFields, Field, Loc, TyVarBind};
            if occ == "[]" {
                let at = Loc { line: 0, col: 0 };
                let a = crate::syntax::ast::Type::Var("a".into(), at);
                let con = |name: &str, fields: Vec<Field>, infix: bool| ConDecl {
                    name: name.to_string(),
                    exist: Vec::new(),
                    context: Vec::new(),
                    fields: ConFields::Positional(fields),
                    gadt_result: None,
                    infix,
                    loc: at,
                };
                let synthetic = DataDecl {
                    newtype: false,
                    context: Vec::new(),
                    name: "[]".to_string(),
                    tyvars: vec![TyVarBind { name: "a".into(), kind: None }],
                    cons: vec![
                        con("[]", Vec::new(), false),
                        con(
                            ":",
                            vec![
                                Field { ty: a.clone(), strict: false },
                                Field { ty: crate::syntax::ast::Type::List(Box::new(a), at), strict: false },
                            ],
                            true,
                        ),
                    ],
                    deriving: vec![Deriving { strategy: sd.strategy.clone(), classes: vec![cls], via: None }],
                    loc: sd.loc,
                };
                let here = std::mem::replace(&mut self.module, "Builtin".to_string());
                let derived = self.derive_instances(&synthetic);
                self.module = here;
                return derived;
            }
            if !(occ == "()" || occ.starts_with("(,")) {
                return Ok(Vec::new());
            }
            let arity = if occ == "()" { 0 } else { occ.matches(',').count() + 1 };
            let tyvars: Vec<TyVarBind> = (0..arity)
                .map(|i| TyVarBind { name: format!("t{i}"), kind: None })
                .collect();
            let field_types: Vec<Field> = (0..arity)
                .map(|i| Field {
                    ty: crate::syntax::ast::Type::Var(format!("t{i}"), Loc { line: 0, col: 0 }),
                    strict: false,
                })
                .collect();
            let synthetic = DataDecl {
                newtype: false,
                context: Vec::new(),
                name: occ.to_string(),
                tyvars,
                cons: vec![ConDecl {
                    name: occ.to_string(),
                    exist: Vec::new(),
                    context: Vec::new(),
                    fields: ConFields::Positional(field_types),
                    gadt_result: None,
                    infix: false,
                    loc: Loc { line: 0, col: 0 },
                }],
                deriving: vec![Deriving { strategy: sd.strategy.clone(), classes: vec![cls], via: None }],
                loc: sd.loc,
            };
            let here = std::mem::replace(&mut self.module, "Builtin".to_string());
            let derived = self.derive_instances(&synthetic);
            self.module = here;
            return derived;
        }
        // A data family instance: the declaration is the representation type the family reduces to.
        let tycon = if self.env.data_decls.contains_key(&tycon) {
            tycon
        } else {
            self.family_representation(&arg).unwrap_or(tycon)
        };
        let Some((defining, dd)) = self.env.data_decls.get(&tycon).cloned() else {
            return self.err(format!("Cannot derive an instance for ‘{}’: its declaration is not known", super::types::unqualify(&tycon)));
        };
        let synthetic = DataDecl {
            deriving: vec![Deriving { strategy: sd.strategy.clone(), classes: vec![cls], via: sd.via.clone() }],
            ..dd
        };
        // The constructors and the instance head are named in the declaring module.
        let here = std::mem::replace(&mut self.module, defining);
        // A newtype's class instance by newtype deriving (no generated code).
        let via_newtype = self.register_newtype_deriving(&synthetic);
        let derived = via_newtype.and_then(|_| self.derive_instances(&synthetic));
        self.module = here;
        // A standalone deriving declaration states its own context and head; the inferred ones
        // of a stock deriving clause do not apply.
        Ok(derived?
            .into_iter()
            .map(|d| match d {
                Decl::Instance(mut id) => {
                    id.context = sd.context.clone();
                    id.head = sd.head.clone();
                    Decl::Instance(id)
                }
                other => other,
            })
            .collect())
    }
}

/// Classes whose parameter has kind `Type -> Type`, derived for a newtype's constructor.
impl Checker {
    /// How many arguments a class's (last) parameter takes, when it is a type constructor
    /// (1 for Functor or Contravariant, 2 for Bifunctor): what a method's type applies it to, or
    /// what a superclass's parameter takes.
    fn constructor_class_arity(&self, cls: &str) -> usize {
        fn applied(t: &Ty, p: u32) -> usize {
            match t {
                Ty::App(f, a) => {
                    let (head, args) = t.split_apps();
                    let here = if matches!(head, Ty::Gen(g) if *g == p) { args.len() } else { 0 };
                    here.max(applied(f, p)).max(applied(a, p))
                }
                Ty::Qual(ps, b) | Ty::Forall(_, ps, b) => {
                    ps.iter().flat_map(|q| q.args.iter()).map(|x| applied(x, p)).max().unwrap_or(0).max(applied(b, p))
                }
                _ => 0,
            }
        }
        let mut seen = std::collections::HashSet::new();
        let mut todo = vec![cls.to_string()];
        while let Some(c) = todo.pop() {
            if !seen.insert(c.clone()) {
                continue;
            }
            if is_constructor_class(&c) {
                return 1;
            }
            let Some(info) = self.env.classes.get(&c) else { continue };
            let p = info.nparams.saturating_sub(1);
            let n = info.methods.iter().map(|(_, m)| applied(&m.ty, p)).max().unwrap_or(0);
            if n > 0 {
                return n;
            }
            for sup in &info.supers {
                if sup.args.last() == Some(&Ty::Gen(p)) {
                    todo.push(sup.class.clone());
                }
            }
        }
        0
    }
}

fn is_constructor_class(cls: &str) -> bool {
    let short = super::types::unqualify(cls);
    matches!(
        short,
        "Functor" | "Applicative" | "Monad" | "Foldable" | "Traversable" | "Alternative" | "MonadIO"
            | "MonadFail" | "MonadPlus" | "MonadFix"
    )
}

/// The strictness of each field of the constructor with the given position.
fn strict_flags(dd: &DataDecl, tag: usize) -> Vec<bool> {
    match dd.cons.get(tag).map(|c| &c.fields) {
        Some(ConFields::Positional(fs)) => fs.iter().map(|f| f.strict).collect(),
        Some(ConFields::Record(groups)) => groups
            .iter()
            .flat_map(|(ns, f)| std::iter::repeat_n(f.strict, ns.len()))
            .collect(),
        None => Vec::new(),
    }
}

/// Splits an instance head `C t1 .. tn` into the class and argument types.
fn instance_head_parts(head: &Type) -> Option<(&Type, Vec<&Type>)> {
    let mut args = Vec::new();
    let mut t = head;
    while let Type::App(f, a) = t {
        args.push(a.as_ref());
        t = f;
    }
    args.reverse();
    Some((t, args))
}

fn head_name(t: &Ty) -> String {
    let (h, _) = t.split_apps();
    match h {
        Ty::Con(c) => super::types::unqualify(c).chars().filter(|c| c.is_alphanumeric()).collect(),
        _ => String::new(),
    }
}

/// Specialises a method's scheme to an instance: the class parameters are replaced by
/// the instance head, and the remaining variables are renumbered from zero.
pub fn instantiate_method(mscheme: &Scheme, nparams: u32, head: &[Ty]) -> Scheme {
    let mut args: Vec<Ty> = head.to_vec();
    let own = mscheme.nvars.saturating_sub(nparams);
    for i in 0..own {
        args.push(Ty::Gen(i));
    }
    let ty = subst_gen(&mscheme.ty, &args);
    // The first predicate is the class itself; it is provided by the instance.
    let preds = mscheme
        .preds
        .iter()
        .skip(1)
        .map(|p| Pred {
            class: p.class.clone(),
            args: p.args.iter().map(|t| subst_gen(t, &args)).collect(),
        })
        .collect();
    Scheme {
        nvars: own,
        names: mscheme.names.iter().skip(nparams as usize).cloned().collect(),
        preds,
        ty,
    }
}


/// How a foreign import's raw result becomes its Haskell value.
#[derive(Clone)]
enum FfiResult {
    Bool,
    Box(String),
}

/// The runtime representation a kind annotation names (`(a :: TYPE 'IntRep)`, `(a :: Type)`), as
/// `rep_of_type` names a type's.
fn annotated_rep(t: &Type) -> Option<String> {
    let Type::Sig(_, k) = t else { return None };
    fn last(q: &QName) -> String {
        q.name.trim_start_matches('\'').to_string()
    }
    match k.as_ref() {
        Type::Con(q, _) if last(q) == "Type" || q.name == "*" => Some("BoxedRep Lifted".into()),
        Type::Con(q, _) if last(q) == "UnliftedType" => Some("BoxedRep Unlifted".into()),
        Type::App(f, r) if matches!(f.as_ref(), Type::Con(q, _) if last(q) == "TYPE") => match r.as_ref() {
            Type::Con(q, _) => Some(match last(q).as_str() {
                "LiftedRep" => "BoxedRep Lifted".to_string(),
                "UnliftedRep" => "BoxedRep Unlifted".to_string(),
                other => other.to_string(),
            }),
            Type::App(b, l) => match (b.as_ref(), l.as_ref()) {
                (Type::Con(b, _), Type::Con(l, _)) if last(b) == "BoxedRep" => Some(format!("BoxedRep {}", last(l))),
                _ => None,
            },
            _ => None,
        },
        _ => None,
    }
}
