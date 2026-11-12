//! Top-level and local declarations.

use super::{group_binds, Ctx, PResult, Parser};
use crate::syntax::ast::*;
use crate::syntax::token::{Kw, ROp, Tok};
use crate::syntax::ast::TypeEquation;

impl Parser {
    pub(super) fn topdecl(&mut self) -> PResult<Decl> {
        let loc = self.loc();
        match self.tok().clone() {
            Tok::Kw(Kw::Data) if self.real() => {
                self.bump();
                if self.at_varid("family") {
                    // A data family is an open type family; its instances are data types.
                    self.bump();
                    return self.type_family(loc, true);
                }
                if self.at_kw(Kw::Instance) {
                    self.bump();
                    return self.data_instance(false, loc);
                }
                self.data_decl(false, loc)
            }
            Tok::Kw(Kw::Newtype) if self.real() => {
                self.bump();
                if self.at_kw(Kw::Instance) {
                    self.bump();
                    return self.data_instance(true, loc);
                }
                self.data_decl(true, loc)
            }
            Tok::Kw(Kw::Type) if self.real() => self.type_decl(loc),
            Tok::VarId(s) if s == "pattern" && self.real() && self.exts.is_on("PatternSynonyms") => {
                self.bump();
                self.pattern_decl(loc)
            }
            Tok::Kw(Kw::Class) if self.real() => self.class_decl(loc),
            Tok::Kw(Kw::Instance) if self.real() => self.instance_decl(loc),
            Tok::Kw(Kw::Deriving) if self.real() => self.standalone_deriving(loc),
            Tok::Kw(Kw::Default) if self.real() => {
                self.bump();
                self.expect_special('(')?;
                let mut types = Vec::new();
                while !self.at_special(')') {
                    types.push(self.sigtype()?);
                    if self.at_special(',') {
                        self.bump();
                    } else {
                        break;
                    }
                }
                self.expect_special(')')?;
                Ok(Decl::Default { types, loc })
            }
            Tok::Kw(Kw::Foreign) if self.real() => self.foreign_decl(loc),
            _ if self.exts.is_on("TemplateHaskell") => self.splice_or_value_decl(loc),
            _ => self.value_decl(),
        }
    }

    /// A top-level item that is an expression and nothing else (`$(deriveJSON ''T)`, or
    /// `makeLenses ''T` without the `$`) is a declaration splice; anything else a binding.
    fn splice_or_value_decl(&mut self, loc: Loc) -> PResult<Decl> {
        let saved = self.pos_state();
        if let Ok(e) = self.infix_expr() {
            let ends = self.at_semi() || self.at_vrbrace() || self.at_special('}') || *self.tok() == Tok::Eof;
            if ends {
                let e = match e {
                    Expr::Splice(false, inner, _) => *inner,
                    other => other,
                };
                return Ok(Decl::Splice(e, loc));
            }
        }
        self.restore_pos_state(saved);
        self.value_decl()
    }

    /// A declaration in a `let`, `where`, class or instance body.
    pub(super) fn local_decl(&mut self) -> PResult<Decl> {
        // ImplicitParams: `?x = e`.
        if self.real() && self.exts.is_on("ImplicitParams") && self.at_varsym("?") {
            if let Tok::VarId(name) = self.peek_raw(1).clone() {
                let loc = self.loc();
                self.bump();
                self.bump();
                self.expect_res(ROp::Eq)?;
                let expr = self.expr()?;
                return Ok(Decl::IPBind { name, expr, loc });
            }
        }
        self.value_decl()
    }

    // ----- value declarations ------------------------------------------------

    pub(super) fn value_decl(&mut self) -> PResult<Decl> {
        let loc = self.loc();
        if let Some(fixity) = self.fixity_decl(loc)? {
            return Ok(fixity);
        }
        if self.looks_like_sig() {
            let mut names = Vec::new();
            loop {
                names.push(self.binder_name()?);
                if self.at_special(',') {
                    self.bump();
                } else {
                    break;
                }
            }
            self.expect_res(ROp::DColon)?;
            let ty = self.sigtype()?;
            // `x :: t = e`: a pattern binding of a variable with a signature pattern.
            if names.len() == 1 && self.at_res(ROp::Eq) {
                let name = names.pop().expect("one name");
                let rhs = self.rhs(ROp::Eq)?;
                let pat = Pat::Sig(Box::new(Pat::Var(name, loc)), ty, loc);
                return Ok(Decl::PatBind { pat, rhs, loc });
            }
            return Ok(Decl::Sig { names, ty, loc });
        }
        let lhs = self.infix_expr()?;
        let rhs = self.rhs(ROp::Eq)?;
        self.make_binding(lhs, rhs, loc)
    }

    /// True if the upcoming tokens are `var {, var} ::` (or with operators in parens).
    fn looks_like_sig(&self) -> bool {
        let mut i = 0;
        loop {
            match self.peek_raw(i) {
                Tok::VarId(_) => i += 1,
                Tok::Special('(') => {
                    if !matches!(self.peek_raw(i + 1), Tok::VarSym(_)) {
                        return false;
                    }
                    if *self.peek_raw(i + 2) != Tok::Special(')') {
                        return false;
                    }
                    i += 3;
                }
                _ => return false,
            }
            match self.peek_raw(i) {
                Tok::Special(',') => i += 1,
                Tok::Res(ROp::DColon) => return true,
                _ => return false,
            }
        }
    }

    fn fixity_decl(&mut self, loc: Loc) -> PResult<Option<Decl>> {
        let assoc = match self.tok() {
            Tok::Kw(Kw::Infix) if self.real() => Assoc::None,
            Tok::Kw(Kw::Infixl) if self.real() => Assoc::Left,
            Tok::Kw(Kw::Infixr) if self.real() => Assoc::Right,
            _ => return Ok(None),
        };
        self.bump();
        let mut prec = 9u8;
        // GHC accepts precedence -1 (below every operator, used by `->`); it is kept as 0.
        if self.at_varsym("-") && matches!(self.peek_raw(1), Tok::Int(d) if d == "1") {
            self.bump();
            self.bump();
            prec = 0;
        } else if let Tok::Int(digits) = self.tok().clone() {
            if self.real() {
                prec = match digits.parse::<u8>() {
                    Ok(p) if p <= 9 => p,
                    _ => return self.err("Precedence out of range"),
                };
                self.bump();
            }
        }
        let mut ops = Vec::new();
        loop {
            let name = match self.tok().clone() {
                Tok::VarSym(s) | Tok::ConSym(s) if self.real() => {
                    self.bump();
                    s
                }
                // `->` is reserved syntax but may be given a fixity (GHC.Types does).
                Tok::Res(ROp::RArrow) if self.real() => {
                    self.bump();
                    "->".to_string()
                }
                Tok::Res(ROp::Tilde) if self.real() => {
                    self.bump();
                    "~".to_string()
                }
                Tok::Special('`') if self.real() => {
                    self.bump();
                    let n = match self.tok().clone() {
                        Tok::VarId(s) | Tok::ConId(s) => s,
                        _ => return self.unexpected(),
                    };
                    self.bump();
                    self.expect_special('`')?;
                    n
                }
                _ => return self.unexpected(),
            };
            ops.push(name);
            if self.at_special(',') {
                self.bump();
            } else {
                break;
            }
        }
        Ok(Some(Decl::Fixity {
            assoc,
            prec,
            ops,
            loc,
        }))
    }

    fn make_binding(&mut self, lhs: Expr, rhs: Rhs, loc: Loc) -> PResult<Decl> {
        // Infix definition: `p1 op p2 = ...`, where the operands may themselves be infix patterns
        // of constructor operators (`f :+ g <*> a :+ b = ...`): the one variable operator is defined.
        if let Expr::OpChain(items, span) = &lhs {
            let var_ops: Vec<usize> = items
                .iter()
                .enumerate()
                .filter(|(_, it)| matches!(it, OpItem::Op(op, _) if !is_con_op(&op.name)))
                .map(|(i, _)| i)
                .collect();
            if let [i] = var_ops.as_slice() {
                if let OpItem::Op(op, _) = &items[*i] {
                    if op.qual.is_none() {
                        let side = |part: &[OpItem]| -> Expr {
                            match part {
                                [OpItem::Operand(e)] => e.clone(),
                                _ => Expr::OpChain(part.to_vec(), *span),
                            }
                        };
                        let (l, r) = (side(&items[..*i]), side(&items[*i + 1..]));
                        let pats = vec![self.expr_to_pat(l)?, self.expr_to_pat(r)?];
                        return Ok(Decl::Fun {
                            name: op.name.clone(),
                            clauses: vec![Clause { pats, rhs, loc, end: self.prev_end() }],
                            loc,
                        });
                    }
                }
            }
        }
        // Flatten the application spine.
        let mut args = Vec::new();
        let mut head = lhs;
        while let Expr::App(f, a) = head {
            args.push(*a);
            head = *f;
        }
        args.reverse();

        match head {
            Expr::Var(q, _) if q.qual.is_none() => {
                let pats = args
                    .into_iter()
                    .map(|a| self.expr_to_pat(a))
                    .collect::<PResult<Vec<_>>>()?;
                Ok(Decl::Fun {
                    name: q.name,
                    clauses: vec![Clause { pats, rhs, loc, end: self.prev_end() }],
                    loc,
                })
            }
            Expr::OpChain(items, _) if !args.is_empty() => {
                // Parenthesised infix definition applied to more patterns: `(x op y) z = ...`.
                if let [OpItem::Operand(l), OpItem::Op(op, _), OpItem::Operand(r)] = items.as_slice() {
                    if !is_con_op(&op.name) && op.qual.is_none() {
                        let mut pats = vec![self.expr_to_pat(l.clone())?, self.expr_to_pat(r.clone())?];
                        for a in args {
                            pats.push(self.expr_to_pat(a)?);
                        }
                        return Ok(Decl::Fun {
                            name: op.name.clone(),
                            clauses: vec![Clause { pats, rhs, loc, end: self.prev_end() }],
                            loc,
                        });
                    }
                }
                self.err("parse error in function definition")
            }
            other => {
                if !args.is_empty() {
                    // `Con p1 p2 = ...` is a pattern binding.
                    let mut e = other;
                    for a in args {
                        e = Expr::App(Box::new(e), Box::new(a));
                    }
                    let pat = self.expr_to_pat(e)?;
                    return Ok(Decl::PatBind { pat, rhs, loc });
                }
                let pat = self.expr_to_pat(other)?;
                Ok(match pat {
                    Pat::Var(name, _) => Decl::Fun {
                        name,
                        clauses: vec![Clause {
                            pats: Vec::new(),
                            rhs,
                            loc,
                            end: self.prev_end(),
                        }],
                        loc,
                    },
                    pat => Decl::PatBind { pat, rhs, loc },
                })
            }
        }
    }

    /// Right-hand side after `=` (or `->` in case alternatives), with guards and `where`.
    pub(super) fn rhs(&mut self, eq: ROp) -> PResult<Rhs> {
        let body = if self.at_res(ROp::Bar) {
            let mut guards = Vec::new();
            while self.at_res(ROp::Bar) {
                self.bump();
                let conds = self.guard_stmts()?;
                self.expect_res(eq)?;
                let e = self.expr()?;
                guards.push((conds, e));
            }
            Body::Guarded(guards)
        } else {
            self.expect_res(eq)?;
            Body::Plain(self.expr()?)
        };
        let wheres = if self.at_kw(Kw::Where) {
            self.bump();
            group_binds(self.block(|p| p.local_decl())?)
        } else {
            Vec::new()
        };
        Ok(Rhs { body, wheres })
    }

    // ----- data, newtype, type synonyms ------------------------------------

    fn data_decl(&mut self, newtype: bool, loc: Loc) -> PResult<Decl> {
        let context = self.try_context();
        let head = self.btype()?;
        let (name, tyvars) = self.decl_head(head)?;
        self.data_body(newtype, context, name, tyvars, loc)
    }

    /// `data instance F t1 .. tn = cons`, after `data instance` (or `newtype instance`). Each
    /// instance becomes a data type of its own, named after the family and the instance's place,
    /// over the variables of the instance head; and the family equation `F t1 .. tn = R vs`.
    fn data_instance(&mut self, newtype: bool, loc: Loc) -> PResult<Decl> {
        let head = self.btype()?;
        let mut args = Vec::new();
        let mut t = head;
        while let Type::App(f, a) = t {
            args.push(*a);
            t = *f;
        }
        args.reverse();
        let family = match t {
            Type::Con(q, _) => q,
            _ => return self.err("malformed data instance head"),
        };
        let mut vars: Vec<String> = Vec::new();
        for a in &args {
            type_vars_of(a, &mut vars);
        }
        let name = format!("R:{}{}_{}", family.name, loc.line, loc.col);
        let tyvars: Vec<TyVarBind> = vars.iter().map(|v| TyVarBind { name: v.clone(), kind: None }).collect();
        let data = self.data_body(newtype, Vec::new(), name.clone(), tyvars, loc)?;
        let rhs = vars.iter().fold(Type::Con(QName::unqual(name), loc), |acc, v| {
            Type::App(Box::new(acc), Box::new(Type::Var(v.clone(), loc)))
        });
        let equation = TypeEquation { args, rhs, loc };
        Ok(Decl::Many(vec![
            data,
            Decl::TypeInstance { name: family, equation, loc },
        ]))
    }

    /// The constructors and deriving clauses of a data declaration whose head is parsed.
    fn data_body(&mut self, newtype: bool, context: Vec<Type>, name: String, tyvars: Vec<TyVarBind>, loc: Loc) -> PResult<Decl> {
        if self.at_res(ROp::DColon) {
            // Kind signature: accepted and ignored for now.
            self.bump();
            self.sigtype()?;
        }
        let mut cons = Vec::new();
        if self.at_res(ROp::Eq) {
            self.bump();
            // GHC.Types declares the unboxed tuple types as `data T# a b = (# a, b #)`. Unboxed
            // tuples are built in here, so the declaration adds nothing.
            if self.is_builtin_tuple_body() {
                // GHC.Tuple declares `data Tuple2 a b = (a, b)`; tuples are built in here.
                self.bump();
                if self.at_special(')') {
                    self.bump();
                } else {
                    while !self.at_special(')') {
                        self.bump();
                    }
                    self.bump();
                }
                return Ok(Decl::Data(DataDecl {
                    newtype: false,
                    context,
                    name,
                    tyvars,
                    cons: Vec::new(),
                    deriving: Vec::new(),
                    loc,
                }));
            }
            if self.at_special('(') && self.peek_raw(1) == &Tok::VarSym("#".into()) {
                // Each alternative is one parenthesised group `(# ... #)`.
                let mut depth = 0;
                loop {
                    if self.at_special('(') {
                        depth += 1;
                    } else if self.at_special(')') {
                        depth -= 1;
                    }
                    self.bump();
                    if depth == 0 {
                        if self.at_res(ROp::Bar) {
                            self.bump();
                            continue;
                        }
                        break;
                    }
                }
                return Ok(Decl::Data(DataDecl {
                    newtype: false,
                    context,
                    name,
                    tyvars,
                    cons: Vec::new(),
                    deriving: Vec::new(),
                    loc,
                }));
            }
            loop {
                cons.push(self.constr()?);
                if self.at_res(ROp::Bar) {
                    self.bump();
                } else {
                    break;
                }
            }
        } else if self.at_kw(Kw::Where) {
            self.bump();
            let groups = self.block(|p| p.gadt_con_group())?;
            cons = groups.into_iter().flatten().collect();
        }
        let deriving = self.deriving_clauses()?;
        Ok(Decl::Data(DataDecl {
            newtype,
            context,
            name,
            tyvars,
            cons,
            deriving,
            loc,
        }))
    }

    fn type_decl(&mut self, loc: Loc) -> PResult<Decl> {
        self.expect_kw(Kw::Type)?;
        if self.at_varid("family") {
            self.bump();
            return self.type_family(loc, true);
        }
        if self.at_kw(Kw::Instance) {
            self.bump();
            return self.type_instance(loc);
        }
        if self.at_varid("role") {
            // RoleAnnotations constrain coercions, which are not checked: the declaration is
            // skipped, up to the end of its line.
            self.bump();
            while self.real() {
                self.bump();
            }
            return Ok(Decl::Fixity { assoc: Assoc::None, prec: 9, ops: Vec::new(), loc });
        }
        let head = self.btype()?;
        if self.at_res(ROp::DColon) {
            // StandaloneKindSignatures: `type T :: kind`.
            self.bump();
            let kind = self.sigtype()?;
            let (name, _) = self.decl_head(head)?;
            return Ok(Decl::KindSig { name, kind, loc });
        }
        let (name, tyvars) = self.decl_head(head)?;
        self.expect_res(ROp::Eq)?;
        let rhs = self.sigtype()?;
        Ok(Decl::TypeSyn {
            name,
            tyvars,
            rhs,
            loc,
        })
    }

    /// `F a b [:: kind] [where equations]`, after `type family` (or `type` in a class).
    fn type_family(&mut self, loc: Loc, top: bool) -> PResult<Decl> {
        let head = self.btype()?;
        let (name, params) = self.decl_head(head)?;
        if self.at_res(ROp::DColon) {
            self.bump();
            self.sigtype()?;
        }
        // TypeFamilyDependencies: `= r | r -> a` names the result and the parameters it determines.
        let mut injective = Vec::new();
        if self.at_res(ROp::Eq) {
            self.bump();
            self.atype()?;
            if self.at_res(ROp::Bar) {
                injective = self.injectivity();
            }
        }
        let equations = if top && self.at_kw(Kw::Where) {
            self.bump();
            Some(self.block(|p| p.type_equation().map(|(_, e)| e))?)
        } else {
            None
        };
        Ok(Decl::TypeFamily {
            name,
            params,
            equations,
            injective,
            loc,
        })
    }

    /// `| r -> a b`, at the bar: the parameters named after the arrow.
    fn injectivity(&mut self) -> Vec<String> {
        let mut names = Vec::new();
        let mut after_arrow = false;
        while self.real() && !self.at_kw(Kw::Where) {
            if self.at_res(ROp::RArrow) {
                after_arrow = true;
            } else if after_arrow {
                if let Tok::VarId(v) = self.tok().clone() {
                    names.push(v.to_string());
                }
            }
            self.bump();
        }
        names
    }

    /// `type instance F t1 .. tn = rhs`, after `type instance`.
    fn type_instance(&mut self, loc: Loc) -> PResult<Decl> {
        let (name, equation) = self.type_equation()?;
        Ok(Decl::TypeInstance {
            name,
            equation,
            loc,
        })
    }

    /// `F t1 .. tn = rhs`: the family name and the equation.
    fn type_equation(&mut self) -> PResult<(QName, TypeEquation)> {
        let loc = self.loc();
        let lhs = self.btype()?;
        self.equation_from(lhs, loc)
    }

    /// The rest of an equation whose left-hand side has been parsed.
    fn equation_from(&mut self, lhs: Type, loc: Loc) -> PResult<(QName, TypeEquation)> {
        self.expect_res(ROp::Eq)?;
        let rhs = self.sigtype()?;
        // An infix head `a == b` (one operator) is the family applied to both operands.
        let (name, args) = match lhs {
            Type::OpChain(items, _) if items.len() == 3 => {
                let mut it = items.into_iter();
                match (it.next(), it.next(), it.next()) {
                    (Some(TypeOpItem::Operand(l)), Some(TypeOpItem::Op(q, _)), Some(TypeOpItem::Operand(r))) => {
                        (q, vec![l, r])
                    }
                    _ => return self.err("malformed type family equation"),
                }
            }
            lhs => {
                let mut args = Vec::new();
                let mut t = lhs;
                while let Type::App(f, a) = t {
                    args.push(*a);
                    t = *f;
                }
                args.reverse();
                match t {
                    Type::Con(q, _) => (q, args),
                    _ => return self.err("malformed type family equation"),
                }
            }
        };
        Ok((name, TypeEquation { args, rhs, loc }))
    }

    /// Splits a declaration head `T a (b :: k)` or `a :+: b` into its name and binders.
    pub(super) fn decl_head(&self, head: Type) -> PResult<(String, Vec<TyVarBind>)> {
        let mut args = Vec::new();
        let mut t = head;
        while let Type::App(f, a) = t {
            args.push(*a);
            t = *f;
        }
        args.reverse();
        let (name, mut tyvars) = match t {
            Type::Con(q, _) if q.qual.is_none() => (q.name, Vec::new()),
            // Infix head `a :+: b`, possibly parenthesised and applied to more binders.
            Type::OpChain(items, _) => match items.as_slice() {
                [TypeOpItem::Operand(l), TypeOpItem::Op(op, _), TypeOpItem::Operand(r)] => (
                    op.name.clone(),
                    vec![self.type_to_binder(l.clone())?, self.type_to_binder(r.clone())?],
                ),
                _ => return self.err("malformed declaration head"),
            },
            _ => return self.err("malformed declaration head"),
        };
        for a in args {
            tyvars.push(self.type_to_binder(a)?);
        }
        Ok((name, tyvars))
    }

    fn type_to_binder(&self, t: Type) -> PResult<TyVarBind> {
        match t {
            Type::Var(name, _) => Ok(TyVarBind { name, kind: None }),
            Type::Sig(inner, kind) => match *inner {
                Type::Var(name, _) => Ok(TyVarBind {
                    name,
                    kind: Some(*kind),
                }),
                _ => self.err("malformed type variable binder"),
            },
            _ => self.err("malformed type variable binder"),
        }
    }

    /// Parses one constructor of a `data` or `newtype` declaration.
    fn constr(&mut self) -> PResult<ConDecl> {
        let loc = self.loc();
        let mut exist = Vec::new();
        if self.at_varid("forall") {
            self.bump();
            while !self.at_varsym(".") {
                exist.push(self.tyvar_binder()?);
            }
            self.bump();
        }
        let context = self.try_context();

        // Prefix operator constructor: `(:+:) a b` or `(:+:) { ... }`.
        if self.at_special('(') && matches!(self.peek_raw(1), Tok::ConSym(_)) && *self.peek_raw(2) == Tok::Special(')') {
            let name = self.paren_op()?.name;
            if self.at_special('{') {
                return self.record_con_body(name, exist, context, loc);
            }
            let fields = self.field_seq()?;
            return Ok(ConDecl {
                name,
                exist,
                context,
                fields: ConFields::Positional(fields),
                gadt_result: None,
                infix: false,
                loc,
            });
        }
        // Record syntax: `C { f :: T, ... }`.
        if let Tok::ConId(name) = self.tok().clone() {
            if self.real() && *self.peek_raw(1) == Tok::Special('{') {
                self.bump();
                return self.record_con_body(name, exist, context, loc);
            }
        }
        // Positional or infix constructor.
        let first = self.field_seq()?;
        if let Some(op) = self.con_operator() {
            let left = self.fields_to_field(first)?;
            let rest = self.field_seq()?;
            let right = self.fields_to_field(rest)?;
            return Ok(ConDecl {
                name: op,
                exist,
                context,
                fields: ConFields::Positional(vec![left, right]),
                gadt_result: None,
                infix: true,
                loc,
            });
        }
        let mut it = first.into_iter();
        let name = match it.next() {
            Some(Field {
                ty: Type::Con(q, _),
                ..
            }) if q.qual.is_none() => q.name,
            _ => return self.err("malformed constructor declaration"),
        };
        let fields: Vec<Field> = it.collect();
        Ok(ConDecl {
            name,
            exist,
            context,
            fields: ConFields::Positional(fields),
            gadt_result: None,
            infix: false,
            loc,
        })
    }

    /// The `{ f1, f2 :: T, ... }` part of a record constructor. Braces open an
    /// explicit layout context, as in GHC, so layout inside them is not applied.
    fn record_con_body(
        &mut self,
        name: String,
        exist: Vec<TyVarBind>,
        context: Vec<Type>,
        loc: Loc,
    ) -> PResult<ConDecl> {
        self.expect_special('{')?;
        self.ctx.push(Ctx::Explicit);
        let mut fields = Vec::new();
        loop {
            if self.at_special('}') {
                break;
            }
            let mut names = Vec::new();
            loop {
                names.push(self.binder_name()?);
                if self.at_special(',') {
                    self.bump();
                } else {
                    break;
                }
            }
            self.expect_res(ROp::DColon)?;
            let field = self.field_type()?;
            fields.push((names, field));
            if self.at_special(',') {
                self.bump();
            } else {
                break;
            }
        }
        self.expect_special('}')?;
        self.ctx.pop();
        Ok(ConDecl {
            name,
            exist,
            context,
            fields: ConFields::Record(fields),
            gadt_result: None,
            infix: false,
            loc,
        })
    }

    /// A ConSym or backticked constructor used as an infix constructor.
    fn con_operator(&mut self) -> Option<String> {
        if !self.real() {
            return None;
        }
        match self.tok().clone() {
            Tok::ConSym(s) => {
                self.bump();
                Some(s)
            }
            Tok::Special('`') => {
                if let Tok::ConId(s) = self.peek_raw(1).clone() {
                    if *self.peek_raw(2) == Tok::Special('`') {
                        self.bump();
                        self.bump();
                        self.bump();
                        return Some(s);
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// Sequence of atomic field types, as in a positional constructor declaration.
    fn field_seq(&mut self) -> PResult<Vec<Field>> {
        let mut out = Vec::new();
        while self.starts_field() {
            out.push(self.field_atom()?);
        }
        Ok(out)
    }

    fn fields_to_field(&self, fields: Vec<Field>) -> PResult<Field> {
        let strict = fields.first().map(|f| f.strict).unwrap_or(false);
        let mut it = fields.into_iter();
        let Some(first) = it.next() else {
            return self.err("missing operand of infix constructor");
        };
        let mut ty = first.ty;
        for f in it {
            ty = Type::App(Box::new(ty), Box::new(f.ty));
        }
        Ok(Field { ty, strict })
    }

    fn starts_field(&self) -> bool {
        if !self.real() {
            return false;
        }
        match self.tok() {
            Tok::VarId(s) => s.as_str() != "forall",
            Tok::ConId(_) | Tok::QConId(..) | Tok::Special('(' | '[') => true,
            Tok::VarSym(s) => s == "!",
            Tok::Res(ROp::Tilde) => true,
            Tok::Int(_) | Tok::Str(_) | Tok::Char(_) => true,
            _ => false,
        }
    }

    fn field_atom(&mut self) -> PResult<Field> {
        if self.at_varsym("!") {
            self.bump();
            return Ok(Field {
                ty: self.atype()?,
                strict: true,
            });
        }
        if self.at_res(ROp::Tilde) {
            self.bump();
            return Ok(Field {
                ty: self.atype()?,
                strict: false,
            });
        }
        Ok(Field {
            ty: self.atype()?,
            strict: false,
        })
    }

    /// A record field's type, which may carry a strictness mark.
    fn field_type(&mut self) -> PResult<Field> {
        if self.at_varsym("!") {
            self.bump();
            return Ok(Field {
                ty: self.sigtype()?,
                strict: true,
            });
        }
        Ok(Field {
            ty: self.sigtype()?,
            strict: false,
        })
    }

    /// `C1, C2 :: forall a. Ctx => t1 -> t2 -> T a` in a GADT body.
    fn gadt_con_group(&mut self) -> PResult<Vec<ConDecl>> {
        let loc = self.loc();
        let mut names = Vec::new();
        loop {
            match self.tok().clone() {
                Tok::ConId(s) if self.real() => {
                    self.bump();
                    names.push(s);
                }
                Tok::Special('(') => {
                    let op = self.paren_op()?;
                    names.push(op.name);
                }
                _ => return self.unexpected(),
            }
            if self.at_special(',') {
                self.bump();
            } else {
                break;
            }
        }
        self.expect_res(ROp::DColon)?;
        // GADT record syntax: `C :: [forall vs.] [ctx =>] { f :: t, .. } -> result`.
        let mut record_exist = Vec::new();
        let mut record_context = Vec::new();
        if self.at_varid("forall") || !self.at_special('{') {
            let saved = self.pos_state();
            let mut vars = Vec::new();
            let mut ok = true;
            if self.at_varid("forall") {
                self.bump();
                while !self.at_varsym(".") && self.real() {
                    match self.tyvar_binder() {
                        Ok(v) => vars.push(v),
                        Err(_) => {
                            ok = false;
                            break;
                        }
                    }
                }
                if ok && self.at_varsym(".") {
                    self.bump();
                } else {
                    ok = false;
                }
            }
            let ctx = if ok && !self.at_special('{') { self.try_context() } else { Vec::new() };
            if ok && self.at_special('{') {
                record_exist = vars;
                record_context = ctx;
            } else {
                self.restore_pos_state(saved);
            }
        }
        if self.at_special('{') {
            self.bump();
            let mut groups: Vec<(Vec<String>, Field)> = Vec::new();
            while !self.at_special('}') {
                let mut fnames = Vec::new();
                loop {
                    match self.tok().clone() {
                        Tok::VarId(f) if self.real() => {
                            self.bump();
                            fnames.push(f);
                        }
                        _ => return self.unexpected(),
                    }
                    if self.at_special(',') {
                        self.bump();
                    } else {
                        break;
                    }
                }
                self.expect_res(ROp::DColon)?;
                let strict = self.at_varsym("!");
                if strict {
                    self.bump();
                }
                let fty = self.sigtype()?;
                groups.push((fnames, Field { ty: fty, strict }));
                if self.at_special(',') {
                    self.bump();
                }
            }
            self.bump();
            self.expect_res(ROp::RArrow)?;
            let result = self.sigtype()?;
            return Ok(names
                .into_iter()
                .map(|name| ConDecl {
                    name,
                    exist: record_exist.clone(),
                    context: record_context.clone(),
                    fields: ConFields::Record(groups.clone()),
                    gadt_result: Some(result.clone()),
                    infix: false,
                    loc,
                })
                .collect());
        }
        let mut ty = self.sigtype()?;
        let mut exist = Vec::new();
        if let Type::Forall(vars, body) = ty {
            exist = vars;
            ty = *body;
        }
        let mut context = Vec::new();
        if let Type::Qual(ctx, body) = ty {
            context = ctx;
            ty = *body;
        }
        let mut fields = Vec::new();
        while let Type::Fun(arg, res) = ty {
            fields.push(Field {
                ty: *arg,
                strict: false,
            });
            ty = *res;
        }
        Ok(names
            .into_iter()
            .map(|name| ConDecl {
                name,
                exist: exist.clone(),
                context: context.clone(),
                fields: ConFields::Positional(fields.clone()),
                gadt_result: Some(ty.clone()),
                infix: false,
                loc,
            })
            .collect())
    }

    fn deriving_clauses(&mut self) -> PResult<Vec<Deriving>> {
        let mut out = Vec::new();
        while self.at_kw(Kw::Deriving) {
            self.bump();
            let strategy = self.deriving_strategy();
            let classes = if self.at_special('(') {
                self.bump();
                let mut cls = Vec::new();
                while !self.at_special(')') {
                    cls.push(self.sigtype()?);
                    if self.at_special(',') {
                        self.bump();
                    } else {
                        break;
                    }
                }
                self.expect_special(')')?;
                cls
            } else {
                vec![self.atype()?]
            };
            let via = if self.at_varid("via") {
                self.bump();
                Some(self.sigtype()?)
            } else {
                None
            };
            out.push(Deriving {
                strategy,
                classes,
                via,
            });
        }
        Ok(out)
    }

    fn deriving_strategy(&mut self) -> Option<Strategy> {
        if self.at_varid("stock") {
            self.bump();
            return Some(Strategy::Stock);
        }
        if self.at_varid("anyclass") {
            self.bump();
            return Some(Strategy::Anyclass);
        }
        if self.at_kw(Kw::Newtype) {
            self.bump();
            return Some(Strategy::Newtype);
        }
        None
    }

    // ----- classes and instances ---------------------------------------------

    fn class_decl(&mut self, loc: Loc) -> PResult<Decl> {
        self.expect_kw(Kw::Class)?;
        let context = self.try_context();
        let head = self.btype()?;
        let (name, tyvars) = self.decl_head(head)?;
        let mut fundeps = Vec::new();
        if self.at_res(ROp::Bar) {
            self.bump();
            loop {
                let from = self.tyvar_names();
                self.expect_res(ROp::RArrow)?;
                let to = self.tyvar_names();
                fundeps.push((from, to));
                if self.at_special(',') {
                    self.bump();
                } else {
                    break;
                }
            }
        }
        let decls = if self.at_kw(Kw::Where) {
            self.bump();
            group_binds(self.block(|p| p.class_body_decl())?)
        } else {
            Vec::new()
        };
        Ok(Decl::Class(ClassDecl {
            context,
            name,
            tyvars,
            fundeps,
            decls,
            loc,
        }))
    }

    fn tyvar_names(&mut self) -> Vec<String> {
        let mut names = Vec::new();
        while self.real() {
            match self.tok().clone() {
                Tok::VarId(s) => {
                    self.bump();
                    names.push(s);
                }
                _ => break,
            }
        }
        names
    }

    fn class_body_decl(&mut self) -> PResult<Decl> {
        if self.at_kw(Kw::Type) {
            // An associated type: `type F a [:: k]` declares it; `type F T = t` is an equation
            // (an instance's definition, or a class default, which is accepted and ignored).
            let loc = self.loc();
            self.bump();
            if self.at_kw(Kw::Instance) {
                self.bump();
                return self.type_instance(loc);
            }
            if self.at_varid("family") {
                self.bump();
            }
            let head = self.btype()?;
            if self.at_res(ROp::Eq) {
                // `type F a = (r :: k) | r -> a` (TypeFamilyDependencies) declares an injective
                // family with a named result; injectivity is not checked, so the clause is skipped.
                let save = (self.pos, self.ctx.clone(), self.semi_done);
                self.bump();
                if self.atype().is_ok() && self.at_res(ROp::Bar) {
                    let injective = self.injectivity();
                    let (name, params) = self.decl_head(head)?;
                    return Ok(Decl::TypeFamily { name, params, equations: None, injective, loc });
                }
                (self.pos, self.ctx, self.semi_done) = save;
                let (name, equation) = self.equation_from(head, loc)?;
                return Ok(Decl::TypeInstance {
                    name,
                    equation,
                    loc,
                });
            }
            let (name, params) = self.decl_head(head)?;
            if self.at_res(ROp::DColon) {
                self.bump();
                self.sigtype()?;
            }
            return Ok(Decl::TypeFamily {
                name,
                params,
                equations: None,
                injective: Vec::new(),
                loc,
            });
        }
        if self.at_kw(Kw::Default) {
            self.bump();
            return match self.value_decl()? {
                Decl::Sig { names, ty, loc } => Ok(Decl::DefaultSig { names, ty, loc }),
                _ => self.err("a default signature must be a type signature"),
            };
        }
        if self.at_kw(Kw::Data) {
            return self.err("associated data families are not supported yet");
        }
        self.value_decl()
    }

    /// `instance forall a b. C a => ..`: the explicit binders of an instance head (its variables
    /// are implicitly bound anyway).
    fn skip_instance_forall(&mut self) {
        if self.at_varid("forall") {
            self.bump();
            while self.real() && !self.at_varsym(".") && !self.at_eof() {
                self.bump();
            }
            if self.at_varsym(".") {
                self.bump();
            }
        }
    }

    fn instance_decl(&mut self, loc: Loc) -> PResult<Decl> {
        self.expect_kw(Kw::Instance)?;
        self.skip_instance_forall();
        let context = self.try_context();
        let head = self.btype()?;
        let decls = if self.at_kw(Kw::Where) {
            self.bump();
            group_binds(self.block(|p| p.class_body_decl())?)
        } else {
            Vec::new()
        };
        Ok(Decl::Instance(InstDecl {
            overlap: None,
            context,
            head,
            decls,
            loc,
        }))
    }

    fn standalone_deriving(&mut self, loc: Loc) -> PResult<Decl> {
        self.expect_kw(Kw::Deriving)?;
        // DerivingVia: `deriving via (V) instance C T`.
        let via = if self.at_varid("via") {
            self.bump();
            Some(self.atype()?)
        } else {
            None
        };
        let strategy = if via.is_some() { None } else { self.deriving_strategy() };
        self.expect_kw(Kw::Instance)?;
        self.skip_instance_forall();
        let context = self.try_context();
        let head = self.btype()?;
        Ok(Decl::Deriving(StandaloneDeriving {
            strategy,
            via,
            context,
            head,
            loc,
        }))
    }

    fn foreign_decl(&mut self, loc: Loc) -> PResult<Decl> {
        self.expect_kw(Kw::Foreign)?;
        let import = if self.at_kw(Kw::Import) {
            self.bump();
            true
        } else if self.at_varid("export") {
            self.bump();
            false
        } else {
            return self.unexpected();
        };
        // The calling convention is kept (`prim` is the runtime's own primitives); safety is not.
        let prim = self.at_varid("prim");
        let capi = self.at_varid("capi");
        if ["ccall", "capi", "stdcall", "prim", "javascript"]
            .iter()
            .any(|cc| self.at_varid(cc))
        {
            self.bump();
        }
        // An unsafe call keeps the runtime to itself; a safe one (the default) lets other threads
        // run while it is in C: `%unsafe` marks the former in the entity.
        let mut unsafe_call = false;
        let mut interruptible = false;
        if self.at_varid("safe") || self.at_varid("unsafe") || self.at_varid("interruptible") {
            unsafe_call = self.at_varid("unsafe");
            interruptible = self.at_varid("interruptible");
            self.bump();
        }
        let entity = match self.tok().clone() {
            Tok::Str(s) if self.real() => {
                self.bump();
                Some(s)
            }
            _ => None,
        };
        let name = self.binder_name()?;
        // A capi import is called through a C wrapper (it may name a macro): `%capi` marks it.
        let entity = if capi { Some(format!("%capi {}", entity.unwrap_or_else(|| name.clone()))) } else { entity };
        let entity = if unsafe_call && !prim { Some(format!("%unsafe {}", entity.unwrap_or_else(|| name.clone()))) } else { entity };
        let entity = if interruptible && !prim { Some(format!("%interruptible {}", entity.unwrap_or_else(|| name.clone()))) } else { entity };
        self.expect_res(ROp::DColon)?;
        let ty = self.sigtype()?;
        Ok(Decl::Foreign {
            import,
            prim,
            entity,
            name,
            ty,
            loc,
        })
    }
}

/// True for names that denote constructors (start with `:`).
pub(super) fn is_con_op(name: &str) -> bool {
    name.starts_with(':')
}

impl Parser {
    /// After `pattern`: a signature `P, Q :: t`, or a synonym definition.
    fn pattern_decl(&mut self, loc: Loc) -> PResult<Decl> {
        let op_sig = *self.tok() == Tok::Special('(')
            && matches!(self.peek_raw(1), Tok::ConSym(_))
            && *self.peek_raw(2) == Tok::Special(')')
            && matches!(self.peek_raw(3), Tok::Res(ROp::DColon) | Tok::Special(','));
        if op_sig || (matches!(self.tok(), Tok::ConId(_)) && matches!(self.peek_raw(1), Tok::Res(ROp::DColon) | Tok::Special(','))) {
            let mut names = Vec::new();
            loop {
                match self.tok().clone() {
                    Tok::ConId(s) => {
                        self.bump();
                        names.push(s);
                    }
                    Tok::Special('(') => names.push(self.paren_op()?.name),
                    _ => return self.unexpected(),
                }
                if self.at_special(',') {
                    self.bump();
                } else {
                    break;
                }
            }
            self.expect_res(ROp::DColon)?;
            let ty = self.sigtype()?;
            return Ok(Decl::PatSynSig { names, ty, loc });
        }
        // An infix synonym: `pattern x :<| xs <- ...`.
        let infix = match (self.tok().clone(), self.peek_raw(1).clone(), self.peek_raw(2).clone()) {
            (Tok::VarId(l), Tok::ConSym(op), Tok::VarId(r)) => {
                self.bump();
                self.bump();
                self.bump();
                Some((op, vec![l, r]))
            }
            _ => None,
        };
        let is_infix = infix.is_some();
        let (name, mut args) = match infix {
            Some(x) => x,
            None => {
                let name = match self.tok().clone() {
                    Tok::ConId(s) if self.real() => {
                        self.bump();
                        s
                    }
                    Tok::Special('(') if matches!(self.peek_raw(1), Tok::ConSym(_)) => self.paren_op()?.name,
                    _ => return self.unexpected(),
                };
                (name, Vec::new())
            }
        };
        while self.real() && !is_infix {
            match self.tok().clone() {
                Tok::VarId(v) => {
                    self.bump();
                    args.push(v);
                }
                _ => break,
            }
        }
        // A record pattern synonym `P{f1, .., fn}`: its arguments are the fields, and each field
        // is also a selector function.
        let record = self.at_special('{');
        if record {
            self.bump();
            args.clear();
            while !self.at_special('}') {
                match self.tok().clone() {
                    Tok::VarId(v) if self.real() => {
                        self.bump();
                        args.push(v);
                    }
                    _ => return self.unexpected(),
                }
                if self.at_special(',') {
                    self.bump();
                }
            }
            self.bump();
        }
        let (pat, dir) = if self.at_res(ROp::Eq) {
            self.bump();
            let e = self.expr()?;
            (self.expr_to_pat(e)?, PatSynDir::Implicit)
        } else if self.at_res(ROp::LArrow) {
            self.bump();
            let e = self.expr()?;
            let pat = self.expr_to_pat(e)?;
            if self.at_kw(Kw::Where) {
                self.bump();
                let decls = group_binds(self.block(|p| p.local_decl())?);
                (pat, PatSynDir::Explicit(decls))
            } else {
                (pat, PatSynDir::Unidirectional)
            }
        } else {
            return self.unexpected();
        };
        let synonym = Decl::PatSyn { name: name.clone(), args: args.clone(), pat, dir, loc };
        if !record {
            return Ok(synonym);
        }
        let mut decls = vec![synonym];
        for (i, field) in args.iter().enumerate() {
            let pats: Vec<Pat> = (0..args.len())
                .map(|j| if j == i { Pat::Var("sel%x".into(), loc) } else { Pat::Wild(loc) })
                .collect();
            let body = Expr::Var(QName::unqual("sel%x"), loc);
            decls.push(Decl::Fun {
                name: field.clone(),
                clauses: vec![Clause {
                    pats: vec![Pat::Con(QName::unqual(name.clone()), pats, loc)],
                    rhs: Rhs { body: Body::Plain(body), wheres: Vec::new() },
                    loc,
                    end: loc,
                }],
                loc,
            });
        }
        Ok(Decl::Many(decls))
    }
}

impl Parser {
    /// `(a, b, ..)` or `()` as a whole constructor body (the built-in tuple types).
    fn is_builtin_tuple_body(&self) -> bool {
        if !self.at_special('(') {
            return false;
        }
        if *self.peek_raw(1) == Tok::Special(')') {
            return true;
        }
        let mut k = 1;
        let mut vars = 0;
        loop {
            match self.peek_raw(k) {
                Tok::VarId(_) => vars += 1,
                _ => return false,
            }
            k += 1;
            match self.peek_raw(k) {
                Tok::Special(',') => k += 1,
                Tok::Special(')') => return vars >= 2,
                _ => return false,
            }
        }
    }
}

/// The type variables of a type, in order of first occurrence.
fn type_vars_of(t: &Type, out: &mut Vec<String>) {
    match t {
        Type::Var(v, _) => {
            if !out.contains(v) {
                out.push(v.clone());
            }
        }
        // A kind annotation's variables are kinds, not parameters of the type.
        Type::Sig(t, _) => type_vars_of(t, out),
        Type::App(f, a) | Type::Fun(f, a) => {
            type_vars_of(f, out);
            type_vars_of(a, out);
        }
        Type::List(e, _) => type_vars_of(e, out),
        Type::Tuple(ts, _) => ts.iter().for_each(|x| type_vars_of(x, out)),
        Type::Qual(cs, b) => {
            cs.iter().for_each(|x| type_vars_of(x, out));
            type_vars_of(b, out);
        }
        Type::OpChain(items, _) => {
            for it in items {
                if let TypeOpItem::Operand(x) = it {
                    type_vars_of(x, out);
                }
            }
        }
        _ => {}
    }
}
