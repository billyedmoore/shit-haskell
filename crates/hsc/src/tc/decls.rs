//! Module-level declarations: types, constructors, classes, instances, foreign imports.

use std::collections::HashMap;

use super::convert::{global_tycon_name, tvar_index};
use super::env::{ClassInfo, InstInfo, Synonym, TyConInfo};
use super::types::{subst_gen, Pred, Scheme, Ty};
use super::{count_args, Checker, Given, TcError, Wanted, TR};
use crate::core::{Alt, Core, TopBind};
use crate::syntax::ast::{ClassDecl, ConFields, DataDecl, Decl, InstDecl, Module, Type};

impl Checker {
    /// Checks a renamed module and returns its Core bindings. The environment is
    /// extended with everything the module defines.
    pub fn check_module(&mut self, m: &Module) -> TR<Vec<TopBind>> {
        self.module = m.name.clone();
        let decls = &m.decls;

        // 0. Kinds of this module's type constructors.
        self.check_kinds(decls)?;

        // 1. Type synonyms and type constructors.
        for d in decls {
            if let Decl::TypeSyn { name, tyvars, rhs, loc } = d {
                self.loc = *loc;
                let mut tvs: Vec<String> = tyvars.iter().map(|t| t.name.clone()).collect();
                let rhs_ty = self.convert_type(rhs, &mut tvs)?;
                self.env.synonyms.insert(
                    format!("{}.{name}", self.module),
                    Synonym {
                        nparams: tyvars.len() as u32,
                        rhs: rhs_ty,
                    },
                );
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

        // 2. Data constructors and field selectors.
        let mut derived: Vec<Decl> = Vec::new();
        for d in decls {
            if let Decl::Data(dd) = d {
                self.data_decl(dd)?;
                derived.extend(self.derive_instances(dd)?);
                self.register_newtype_deriving(dd)?;
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

        // 4. Foreign imports.
        for d in decls {
            if let Decl::Foreign { import: true, entity, name, ty, loc } = d {
                self.loc = *loc;
                self.foreign_import(name, entity.as_deref(), ty)?;
            }
        }

        // 5. Instance headers, visible before any body is checked.
        let mut all_insts: Vec<InstDecl> = Vec::new();
        for d in decls.iter().chain(derived.iter()) {
            match d {
                Decl::Instance(id) => {
                    self.instance_header(id)?;
                    all_insts.push(id.clone());
                }
                _ => {}
            }
        }
        // 6. Top-level values.
        let value_decls: Vec<Decl> = decls
            .iter()
            .filter(|d| matches!(d, Decl::Fun { .. } | Decl::PatBind { .. } | Decl::Sig { .. }))
            .cloned()
            .collect();
        let binds = self.infer_binds(&value_decls, true)?;
        for (name, body) in binds {
            self.out.push(TopBind { name, body });
        }

        // 7. Class default methods.
        for cd in &class_decls {
            self.class_defaults(cd)?;
        }

        // 8. Instance bodies.
        for id in &all_insts {
            self.instance_body(id)?;
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
                preds.push(self.convert_pred(ctx, &mut vars)?);
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
            self.add_con(name, &tycon, tag, scheme.clone(), occs.clone(), dd.newtype);
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
                    let _ = cscheme;
                    if let Some(p) = cocc.iter().position(|o| o == f) {
                        let arity = cocc.len();
                        let vars: Vec<String> = (0..arity).map(|_| self.fresh_name("sf")).collect();
                        alts.push(Alt::Con(cname.clone(), vars.clone(), Core::Var(vars[p].clone())));
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
        for c in &cd.context {
            supers.push(self.convert_pred(c, &mut tvs)?);
        }
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
        let info = ClassInfo {
            name: class_key.clone(),
            nparams,
            supers,
            methods,
            defaults,
            dict_con,
        };
        self.add_class_info(info);
        Ok(())
    }

    /// Default method implementations: `$dm<m> = \dict -> \own-dicts -> body`.
    fn class_defaults(&mut self, cd: &ClassDecl) -> TR<()> {
        let class_key = format!("{}.{}", self.module, cd.name);
        let info = self.class(&class_key).cloned().expect("class registered");
        for d in &cd.decls {
            if let Decl::Fun { name, clauses, loc } = d {
                self.loc = *loc;
                let mkey = format!("{}.{name}", self.module);
                let scheme = info
                    .methods
                    .iter()
                    .find(|(k, _)| *k == mkey)
                    .map(|(_, s)| s.clone())
                    .ok_or_else(|| TcError {
                        loc: *loc,
                        msg: format!("‘{name}’ is not a method of class ‘{}’", cd.name),
                    })?;
                let dm = format!("{}.$dm{name}", self.module);
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

    fn foreign_import(&mut self, name: &str, entity: Option<&str>, ty: &Type) -> TR<()> {
        let mut tvs = Vec::new();
        let (preds, body) = self.sig_parts(ty, &mut tvs)?;
        let scheme = Scheme {
            nvars: tvs.len() as u32,
            names: tvs,
            preds,
            ty: body.clone(),
        };
        let key = format!("{}.{name}", self.module);
        let prim = entity.map(str::to_string).unwrap_or_else(|| name.to_string());
        let arity = count_args(&body);
        let params: Vec<String> = (0..arity).map(|_| self.fresh_name("fa")).collect();
        let args: Vec<Core> = params.iter().map(|p| Core::Var(p.clone())).collect();
        let mut core = Core::Prim(prim, args);
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
        for a in args {
            head.push(self.convert_type(a, &mut tvs)?);
        }
        let mut ctx = Vec::new();
        for c in &id.context {
            ctx.push(self.convert_pred(c, &mut tvs)?);
        }
        let dict = format!(
            "{}.$f{}{}_{}",
            self.module,
            super::types::unqualify(&class),
            head.iter().map(|t| head_name(t)).collect::<String>(),
            self.counter
        );
        self.counter += 1;
        self.env.instances.push(InstInfo {
            class,
            nvars: tvs.len() as u32,
            ctx,
            head,
            dict,
        });
        Ok(())
    }

    /// Builds the dictionary for an instance and checks its methods.
    fn instance_body(&mut self, id: &InstDecl) -> TR<()> {
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
        for a in args {
            head_check.push(self.convert_type(a, &mut tvs)?);
        }
        let inst = self
            .env
            .instances
            .iter()
            .rev()
            .find(|i| i.class == class && i.head == head_check)
            .cloned()
            .expect("instance registered");
        let info = self.class(&class).cloned().expect("class registered");

        // Skolemise the instance variables; the context is given.
        let names: Vec<String> = (0..inst.nvars).map(|i| format!("i{i}")).collect();
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
        for (p, d) in ctx.iter().zip(&dparams) {
            self.givens.push(Given {
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
            let p = Pred {
                class: sup.class.clone(),
                args: sup.args.iter().map(|t| subst_gen(t, &head)).collect(),
            };
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
                    Some(dm) => Core::App(Box::new(Core::Var(dm.clone())), Box::new(Core::Var(this.clone()))),
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
        self.givens.truncate(givens_len);

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
        let (field_ty, rep_last) = match (dd.newtype, dd.cons.first()) {
            (true, Some(c)) => match &c.fields {
                ConFields::Positional(fs) if fs.len() == 1 => (fs[0].ty.clone(), true),
                ConFields::Record(groups) if groups.len() == 1 && groups[0].0.len() == 1 => {
                    (groups[0].1.ty.clone(), true)
                }
                _ => return Ok(()),
            },
            _ => return Ok(()),
        };
        let _ = rep_last;
        for d in &dd.deriving {
            for cls in &d.classes {
                let short = match cls {
                    Type::Con(q, _) => q.name.clone(),
                    Type::App(f, _) => match f.as_ref() {
                        Type::Con(q, _) => q.name.clone(),
                        _ => continue,
                    },
                    _ => continue,
                };
                let via_newtype = d.strategy == Some(Strategy::Newtype)
                    || (d.strategy.is_none() && !super::derive::is_stock(&short));
                if !via_newtype || d.strategy == Some(Strategy::Anyclass) {
                    continue;
                }
                let cls_name = match cls {
                    Type::Con(q, _) => global_tycon_name(q),
                    _ => continue,
                };
                if !self.env.classes.contains_key(&cls_name) {
                    continue;
                }
                // Constructor classes (Functor, Monad, ...) are derived for the type without
                // its last parameter, so the representation loses its last argument too.
                let last = dd.tyvars.last().map(|t| t.name.clone());
                let eta = is_constructor_class(&cls_name);
                let kept: Vec<String> = dd
                    .tyvars
                    .iter()
                    .map(|t| t.name.clone())
                    .take(dd.tyvars.len().saturating_sub(if eta { 1 } else { 0 }))
                    .collect();
                let rep_ast = if eta {
                    match (&field_ty, &last) {
                        (Type::App(g, arg), Some(l)) if matches!(arg.as_ref(), Type::Var(v, _) if v == l) => {
                            (**g).clone()
                        }
                        _ => continue,
                    }
                } else {
                    field_ty.clone()
                };
                let mut tvs = kept.clone();
                let rep = self.convert_type(&rep_ast, &mut tvs)?;
                let head_args: Vec<Ty> = (0..kept.len() as u32).map(Ty::Gen).collect();
                let head = Ty::apps(Ty::Con(format!("{}.{}", self.module, dd.name)), head_args);
                let dict = format!("{}.$fGND{}{}_{}", self.module, super::types::unqualify(&cls_name), dd.name, self.counter);
                self.counter += 1;
                self.env.instances.push(InstInfo {
                    class: cls_name.clone(),
                    nvars: tvs.len() as u32,
                    ctx: vec![Pred { class: cls_name.clone(), args: vec![rep] }],
                    head: vec![head],
                    dict: dict.clone(),
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
}

/// Classes whose parameter has kind `Type -> Type`, derived for a newtype's constructor.
fn is_constructor_class(cls: &str) -> bool {
    let short = super::types::unqualify(cls);
    matches!(
        short,
        "Functor" | "Applicative" | "Monad" | "Foldable" | "Traversable" | "Alternative" | "MonadIO"
            | "MonadFail" | "MonadPlus" | "MonadFix"
    )
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

