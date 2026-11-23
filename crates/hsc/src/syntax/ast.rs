//! Surface abstract syntax. Operator applications stay as flat `OpChain`s until
//! fixity resolution; patterns are converted from expressions by the parser.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Loc {
    pub line: u32,
    pub col: u32,
}

impl Loc {
    pub(crate) fn after(self, text: &str) -> Self {
        let mut loc = self;
        for ch in text.chars() {
            match ch {
                '\n' => {
                    loc.line += 1;
                    loc.col = 1;
                }
                '\t' => loc.col = ((loc.col - 1) / 8 + 1) * 8 + 1,
                _ => loc.col += 1,
            }
        }
        loc
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct QName {
    pub qual: Option<String>,
    pub name: String,
}

impl QName {
    pub fn unqual(name: impl Into<String>) -> Self {
        QName {
            qual: None,
            name: name.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Lit {
    /// A MagicHash literal of a primitive type; not overloaded.
    Prim(crate::syntax::token::PrimLit),
    /// Decimal digits.
    Int(String),
    /// Literal text such as `1.5e-3`.
    Frac(String),
    Char(char),
    Str(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Module {
    pub name: String,
    pub exports: Option<Vec<IEItem>>,
    /// Source locations of the explicit export items, in list order.
    pub export_locs: Vec<Loc>,
    pub imports: Vec<Import>,
    pub decls: Vec<Decl>,
    pub loc: Loc,
    /// The module's rewrite rules (`{-# RULES #-}`): optimisations only, never needed for meaning.
    pub rules: Vec<Rule>,
    /// Its top-level `{-# INLINE #-}` and `{-# NOINLINE #-}` pragmas (optimisations too).
    pub inlines: Vec<InlinePragma>,
}

/// `{-# INLINE [activation] name #-}` (`inline`), or `NOINLINE` (not `inline`).
#[derive(Clone, Debug, PartialEq)]
pub struct InlinePragma {
    pub name: QName,
    pub inline: bool,
    pub activation: Activation,
    pub loc: Loc,
}

/// A rewrite rule: `"name" [activation] forall binders . lhs = rhs`.
#[derive(Clone, Debug, PartialEq)]
pub struct Rule {
    pub name: String,
    pub activation: Activation,
    /// The rule's variables, with their signatures when given (`(g :: forall b. ..)`).
    pub binders: Vec<(String, Option<Type>)>,
    pub lhs: Expr,
    pub rhs: Expr,
    pub loc: Loc,
}

/// When a rule (or an INLINE pragma) is active, in GHC's simplifier phases (2, then 1, then 0).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Activation {
    Always,
    /// `[n]`: from phase n on.
    From(u32),
    /// `[~n]`: before phase n only.
    Before(u32),
    /// `[~]`: never (as a rule written only to be disabled).
    Never,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Import {
    pub module: String,
    /// The package of a package-qualified import (`import "pkg" M`).
    pub package: Option<String>,
    pub qualified: bool,
    pub alias: Option<String>,
    pub hiding: bool,
    pub items: Option<Vec<IEItem>>,
    pub loc: Loc,
}

/// An entry in an import or export list.
#[derive(Clone, Debug, PartialEq)]
pub enum IEItem {
    Var(QName),
    Thing { name: QName, subs: Subs },
    Module(String),
    Pattern(QName),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Subs {
    None,
    All,
    Some(Vec<String>),
    /// `T(.., P, Q)`: every constructor and field, and these bundled names too.
    AllPlus(Vec<String>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Assoc {
    Left,
    Right,
    None,
}

/// How a pattern synonym is used in expressions.
#[derive(Clone, Debug, PartialEq)]
pub enum PatSynDir {
    /// `pattern P x = pat`: the pattern is also the expression that builds the value.
    Implicit,
    /// `pattern P x <- pat`: only matching is defined.
    Unidirectional,
    /// `pattern P x <- pat where P x = e`: the builder clauses.
    Explicit(Vec<Decl>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Decl {
    /// A Template Haskell declaration splice: `$(e)`, `$x` or a bare expression at top level,
    /// which generates declarations.
    Splice(Expr, Loc),
    /// A default signature in a class: `default m :: ctx => t` (DefaultSignatures).
    DefaultSig {
        names: Vec<String>,
        ty: Type,
        loc: Loc,
    },
    /// An implicit-parameter binding `?x = e` (in a `let`).
    IPBind {
        name: String,
        expr: Expr,
        loc: Loc,
    },
    /// A pattern synonym `pattern P x1 .. xn = pat`, `pattern P xs <- pat [where ..]`.
    PatSyn {
        name: String,
        args: Vec<String>,
        record: bool,
        pat: Pat,
        dir: PatSynDir,
        loc: Loc,
    },
    /// `pattern P, Q :: type`; the type is not checked against the definition.
    PatSynSig {
        names: Vec<String>,
        ty: Type,
        loc: Loc,
    },
    Sig {
        names: Vec<String>,
        ty: Type,
        loc: Loc,
    },
    Fixity {
        assoc: Assoc,
        prec: u8,
        ops: Vec<String>,
        loc: Loc,
    },
    /// A function or variable binding, with all its clauses grouped.
    Fun {
        name: String,
        clauses: Vec<Clause>,
        loc: Loc,
    },
    PatBind {
        pat: Pat,
        rhs: Rhs,
        loc: Loc,
    },
    Data(DataDecl),
    TypeSyn {
        name: String,
        tyvars: Vec<TyVarBind>,
        rhs: Type,
        loc: Loc,
    },
    Class(ClassDecl),
    Instance(InstDecl),
    Deriving(StandaloneDeriving),
    Default {
        types: Vec<Type>,
        loc: Loc,
    },
    /// Standalone kind signature `type T :: kind`.
    KindSig {
        name: String,
        kind: Type,
        loc: Loc,
    },
    RoleAnnotation {
        name: QName,
        roles: usize,
        loc: Loc,
    },
    /// An open or closed type family, or a data family (always open).
    TypeFamily {
        name: String,
        params: Vec<TyVarBind>,
        equations: Option<Vec<TypeEquation>>,
        /// Location of the `..` in an abstract closed family from an hs-boot file.
        abstract_closed: Option<Loc>,
        data: bool,
        /// The parameters an injectivity annotation (`= r | r -> a b`) names.
        injective: Vec<String>,
        loc: Loc,
    },
    /// An equation of an open family: `type instance F t1 .. tn = rhs`.
    TypeInstance {
        name: QName,
        equation: TypeEquation,
        loc: Loc,
    },
    /// Several declarations from one source declaration (a data instance is a data type and
    /// a family equation); flattened when the module is built.
    Many(Vec<Decl>),
    Foreign {
        import: bool,
        /// `foreign import prim`: the runtime's own primitive, which answers in Haskell's terms.
        prim: bool,
        entity: Option<String>,
        name: String,
        ty: Type,
        loc: Loc,
    },
}

/// One equation `F t1 .. tn = rhs` of a type family.
#[derive(Clone, Debug, PartialEq)]
pub struct TypeEquation {
    pub args: Vec<Type>,
    pub rhs: Type,
    pub loc: Loc,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Clause {
    pub pats: Vec<Pat>,
    pub rhs: Rhs,
    pub loc: Loc,
    /// The last character of the clause (for source spans in pattern-match failures).
    pub end: Loc,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Rhs {
    pub body: Body,
    pub wheres: Vec<Decl>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Body {
    Plain(Expr),
    /// Guarded alternatives; each guard is a list of statements.
    Guarded(Vec<(Vec<Stmt>, Expr)>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Stmt {
    Bind { pat: Pat, expr: Expr, loc: Loc },
    Let { decls: Vec<Decl>, loc: Loc },
    Expr { expr: Expr, loc: Loc },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Alt {
    pub pat: Pat,
    pub rhs: Rhs,
    pub loc: Loc,
}

#[derive(Clone, Debug, PartialEq)]
pub enum OpItem {
    Operand(Expr),
    Op(QName, Loc),
    /// Prefix minus, resolved during fixity resolution.
    Neg(Loc),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    /// An implicit parameter `?x`.
    IPVar(String, Loc),
    Var(QName, Loc),
    Con(QName, Loc),
    Lit(Lit, Loc),
    App(Box<Expr>, Box<Expr>),
    /// Visible type application `f @T`.
    TyApp(Box<Expr>, Type, Loc),
    OpChain(Vec<OpItem>, Loc),
    /// A view pattern `(f -> p)`; only meaningful where a pattern is expected.
    ViewPat(Box<Expr>, Box<Expr>, Loc),
    Lam(Vec<Pat>, Box<Expr>, Loc),
    Let(Vec<Decl>, Box<Expr>, Loc),
    If(Box<Expr>, Box<Expr>, Box<Expr>, Loc),
    MultiIf(Vec<(Vec<Stmt>, Expr)>, Loc),
    Case(Box<Expr>, Vec<Alt>, Loc),
    LamCase(Vec<Alt>, Loc),
    Do(Vec<Stmt>, Loc),
    /// Tuple; `None` entries are holes of a tuple section.
    Tuple(Vec<Option<Expr>>, Loc),
    List(Vec<Expr>, Loc),
    /// `[from ..]`, `[from, then ..]`, `[from .. to]`, `[from, then .. to]`.
    ArithSeq {
        from: Box<Expr>,
        then: Option<Box<Expr>>,
        to: Option<Box<Expr>>,
        loc: Loc,
    },
    /// List comprehension; more than one branch means a parallel comprehension.
    ListComp(Box<Expr>, Vec<Vec<Stmt>>, Loc),
    /// `(e op)`.
    LeftSection(Box<Expr>, QName, Loc),
    /// `(op e)`.
    RightSection(QName, Box<Expr>, Loc),
    RecCon {
        con: QName,
        fields: Vec<(QName, Option<Expr>)>,
        dotdot: bool,
        loc: Loc,
    },
    RecUpd(Box<Expr>, Vec<(QName, Option<Expr>)>, Loc),
    Sig(Box<Expr>, Type, Loc),
    /// `_`: a hole in expressions, a wildcard in patterns.
    Wild(Loc),
    /// Pattern-only forms. They appear in expressions only if the program is
    /// malformed, and the renamer rejects them there.
    AsPat(String, Box<Expr>, Loc),
    LazyPat(Box<Expr>, Loc),
    BangPat(Box<Expr>, Loc),
    /// A Template Haskell quotation: 'e' `[| e |]`, 'T' `[|| e ||]` (with their expressions),
    /// 'd' `[d| .. |]`, 't' `[t| .. |]`, 'p' `[p| .. |]`.
    Quote(char, Option<Box<Expr>>, Loc),
    /// `[t| type |]`.
    QuoteType(Type, Loc),
    /// `[p| pattern |]`.
    QuotePat(Box<Pat>, Loc),
    /// A Template Haskell name quotation: `'f`, `'Con` (false) or `''T` (true).
    NameQuote(bool, QName, Loc),
    /// A Template Haskell splice: `$(e)`, `$x` (false) or `$$(e)` (true, typed).
    Splice(bool, Box<Expr>, Loc),
}

impl Expr {
    pub fn loc(&self) -> Loc {
        match self {
            Expr::IPVar(_, l)
            | Expr::Var(_, l)
            | Expr::Con(_, l)
            | Expr::Lit(_, l)
            | Expr::TyApp(_, _, l)
            | Expr::OpChain(_, l)
            | Expr::ViewPat(_, _, l)
            | Expr::Lam(_, _, l)
            | Expr::Let(_, _, l)
            | Expr::If(_, _, _, l)
            | Expr::MultiIf(_, l)
            | Expr::Case(_, _, l)
            | Expr::LamCase(_, l)
            | Expr::Do(_, l)
            | Expr::Tuple(_, l)
            | Expr::List(_, l)
            | Expr::ListComp(_, _, l)
            | Expr::LeftSection(_, _, l)
            | Expr::RightSection(_, _, l)
            | Expr::RecUpd(_, _, l)
            | Expr::Sig(_, _, l)
            | Expr::Wild(l)
            | Expr::AsPat(_, _, l)
            | Expr::LazyPat(_, l)
            | Expr::BangPat(_, l)
            | Expr::Quote(_, _, l)
            | Expr::QuoteType(_, l)
            | Expr::QuotePat(_, l)
            | Expr::NameQuote(_, _, l)
            | Expr::Splice(_, _, l) => *l,
            Expr::App(f, _) => f.loc(),
            Expr::ArithSeq { loc, .. } | Expr::RecCon { loc, .. } => *loc,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum PatOpItem {
    Operand(Pat),
    Op(QName, Loc),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Pat {
    Var(String, Loc),
    Wild(Loc),
    Lit(Lit, Loc),
    /// Negative numeric literal pattern.
    NegLit(Lit, Loc),
    Con(QName, Vec<Pat>, Loc),
    Rec {
        con: QName,
        fields: Vec<(QName, Option<Pat>)>,
        dotdot: bool,
        loc: Loc,
    },
    Tuple(Vec<Pat>, Loc),
    List(Vec<Pat>, Loc),
    As(String, Box<Pat>, Loc),
    Lazy(Box<Pat>, Loc),
    Bang(Box<Pat>, Loc),
    Sig(Box<Pat>, Type, Loc),
    View(Box<Expr>, Box<Pat>, Loc),
    /// Infix constructor patterns such as `x : xs`, resolved by fixity.
    OpChain(Vec<PatOpItem>, Loc),
}

#[derive(Clone, Debug, PartialEq)]
pub struct TyVarBind {
    pub name: String,
    pub kind: Option<Type>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Type {
    Var(String, Loc),
    Con(QName, Loc),
    App(Box<Type>, Box<Type>),
    Fun(Box<Type>, Box<Type>),
    List(Box<Type>, Loc),
    /// Tuple type; `Con("()")` is the unit type.
    Tuple(Vec<Type>, Loc),
    Forall {
        binders: Vec<TyVarBind>,
        body: Box<Type>,
        /// Parentheses separate this quantifier from the enclosing signature's binders.
        parenthesized: bool,
    },
    /// `context => body`.
    Qual(Vec<Type>, Box<Type>),
    /// `(t :: k)`.
    Sig(Box<Type>, Box<Type>),
    /// Infix type operators, resolved by fixity.
    OpChain(Vec<TypeOpItem>, Loc),
    Lit(Lit, Loc),
    Wild(Loc),
    /// A Template Haskell type splice (`$t` or `$(expr)`).
    Splice(Box<Expr>, Loc),
}

impl Type {
    pub fn loc(&self) -> Loc {
        match self {
            Self::Var(_, loc) | Self::Con(_, loc) | Self::List(_, loc) |
            Self::Tuple(_, loc) | Self::OpChain(_, loc) | Self::Lit(_, loc) |
            Self::Wild(loc) | Self::Splice(_, loc) => *loc,
            Self::App(t, _) | Self::Fun(t, _) | Self::Sig(t, _) | Self::Forall { body: t, .. } => t.loc(),
            Self::Qual(ctx, body) => ctx.first().map_or_else(|| body.loc(), Type::loc),
        }
    }

    /// Named wildcards can be recognized when there are no enclosing type binders.
    pub(crate) fn first_wildcard(&self, named: bool) -> Option<(&str, Loc)> {
        self.find_signature_unbound(|ty, outer_binder_kind| match ty {
            Type::Wild(loc) => Some(("_", *loc)),
            Type::Var(name, loc) if named && !outer_binder_kind && name.starts_with('_') => Some((name.as_str(), *loc)),
            _ => None,
        })
    }

    /// The scope predicate also sees whether the variable occurs in an outer binder's kind.
    pub(crate) fn first_unbound_variable(&self, mut in_scope: impl FnMut(&str, bool) -> bool) -> Option<(&str, Loc)> {
        self.find_signature_unbound(|ty, outer_binder_kind| match ty {
            Type::Var(name, loc) if !in_scope(name, outer_binder_kind) => Some((name.as_str(), *loc)),
            _ => None,
        })
    }

    fn find_signature_unbound<'a, T>(&'a self, mut inspect: impl FnMut(&'a Type, bool) -> Option<T>) -> Option<T> {
        let mut bound = Vec::new();
        let body = if let Type::Forall { binders, body, parenthesized: false } = self {
            // The outer binder kinds are resolved before the signature's named wildcards.
            for binder in binders {
                if let Some(kind) = &binder.kind {
                    if let Some(found) = kind.find_unbound(&mut bound, &mut |ty| inspect(ty, true)) {
                        return Some(found);
                    }
                }
                bound.push(binder.name.as_str());
            }
            body.as_ref()
        } else {
            self
        };
        body.find_unbound(&mut bound, &mut |ty| inspect(ty, false))
    }

    fn find_unbound<'a, T>(
        &'a self, bound: &mut Vec<&'a str>, inspect: &mut impl FnMut(&'a Type) -> Option<T>,
    ) -> Option<T> {
        match self {
            Type::Var(name, _) if !bound.contains(&name.as_str()) => inspect(self),
            Type::Wild(_) => inspect(self),
            Type::App(a, b) | Type::Fun(a, b) | Type::Sig(a, b) =>
                a.find_unbound(bound, inspect).or_else(|| b.find_unbound(bound, inspect)),
            Type::List(t, _) => t.find_unbound(bound, inspect),
            Type::Forall { binders, body, .. } => {
                let depth = bound.len();
                let found = binders.iter().find_map(|binder| {
                    let found = binder.kind.as_ref().and_then(|kind| kind.find_unbound(bound, inspect));
                    bound.push(binder.name.as_str());
                    found
                }).or_else(|| body.find_unbound(bound, inspect));
                bound.truncate(depth);
                found
            }
            Type::Qual(ctx, body) =>
                ctx.iter().find_map(|ty| ty.find_unbound(bound, inspect)).or_else(|| body.find_unbound(bound, inspect)),
            Type::Tuple(types, _) => types.iter().find_map(|ty| ty.find_unbound(bound, inspect)),
            Type::OpChain(items, _) => items.iter().find_map(|item| match item {
                TypeOpItem::Operand(ty) => ty.find_unbound(bound, inspect),
                TypeOpItem::Op(..) => None,
            }),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum TypeOpItem {
    Operand(Type),
    Op(QName, Loc),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Field {
    pub ty: Type,
    pub strict: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ConFields {
    Positional(Vec<Field>),
    Record(Vec<(Vec<String>, Field)>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConDecl {
    pub name: String,
    pub exist: Vec<TyVarBind>,
    pub context: Vec<Type>,
    pub fields: ConFields,
    /// Result type of a GADT-style constructor.
    pub gadt_result: Option<Type>,
    /// Declared infix (`a :+: b`); used by derived `Show`.
    pub infix: bool,
    pub loc: Loc,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Strategy {
    Stock,
    Newtype,
    Anyclass,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Deriving {
    pub loc: Loc,
    pub strategy: Option<Strategy>,
    pub classes: Vec<Type>,
    pub via: Option<Type>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DataDecl {
    pub newtype: bool,
    pub context: Vec<Type>,
    pub name: String,
    pub tyvars: Vec<TyVarBind>,
    pub cons: Vec<ConDecl>,
    pub deriving: Vec<Deriving>,
    pub loc: Loc,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClassDecl {
    pub context: Vec<Type>,
    pub name: String,
    pub tyvars: Vec<TyVarBind>,
    pub fundeps: Vec<(Vec<String>, Vec<String>)>,
    pub decls: Vec<Decl>,
    pub minimal: Option<(Minimal, Loc)>,
    pub loc: Loc,
}

/// A class's minimal complete definition. An empty conjunction requires no methods.
#[derive(Clone, Debug, PartialEq)]
pub enum Minimal {
    Method(String, Loc),
    All(Vec<Minimal>),
    Any(Vec<Minimal>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct InstDecl {
    pub overlap: Option<String>,
    /// The explicitly bound type variables from the instance header.
    pub binders: Vec<TyVarBind>,
    pub context: Vec<Type>,
    /// The class and its argument types, as a type application chain.
    pub head: Type,
    /// The parser erases the distinction between boxed and unboxed tuple types.
    pub unboxed_head: bool,
    pub decls: Vec<Decl>,
    pub loc: Loc,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StandaloneDeriving {
    pub strategy: Option<Strategy>,
    pub via: Option<Type>,
    pub context: Vec<Type>,
    pub head: Type,
    pub unboxed_head: bool,
    pub loc: Loc,
}
