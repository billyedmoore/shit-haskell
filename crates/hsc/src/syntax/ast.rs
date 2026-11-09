//! Surface abstract syntax. Operator applications stay as flat `OpChain`s until
//! fixity resolution; patterns are converted from expressions by the parser.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Loc {
    pub line: u32,
    pub col: u32,
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
    pub imports: Vec<Import>,
    pub decls: Vec<Decl>,
    pub loc: Loc,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Import {
    pub module: String,
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
    /// A pattern synonym `pattern P x1 .. xn = pat`, `pattern P xs <- pat [where ..]`.
    PatSyn {
        name: String,
        args: Vec<String>,
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
    /// A type family: open (`equations` is `None`) or closed.
    TypeFamily {
        name: String,
        params: Vec<TyVarBind>,
        equations: Option<Vec<TypeEquation>>,
        loc: Loc,
    },
    /// An equation of an open family: `type instance F t1 .. tn = rhs`.
    TypeInstance {
        name: QName,
        equation: TypeEquation,
        loc: Loc,
    },
    Foreign {
        import: bool,
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
}

impl Expr {
    pub fn loc(&self) -> Loc {
        match self {
            Expr::Var(_, l)
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
            | Expr::BangPat(_, l) => *l,
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
    Forall(Vec<TyVarBind>, Box<Type>),
    /// `context => body`.
    Qual(Vec<Type>, Box<Type>),
    /// `(t :: k)`.
    Sig(Box<Type>, Box<Type>),
    /// Infix type operators, resolved by fixity.
    OpChain(Vec<TypeOpItem>, Loc),
    Lit(Lit, Loc),
    Wild(Loc),
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
    pub loc: Loc,
}

#[derive(Clone, Debug, PartialEq)]
pub struct InstDecl {
    pub overlap: Option<String>,
    pub context: Vec<Type>,
    /// The class and its argument types, as a type application chain.
    pub head: Type,
    pub decls: Vec<Decl>,
    pub loc: Loc,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StandaloneDeriving {
    pub strategy: Option<Strategy>,
    pub via: Option<Type>,
    pub context: Vec<Type>,
    pub head: Type,
    pub loc: Loc,
}
