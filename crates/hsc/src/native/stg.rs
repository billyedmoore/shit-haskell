//! Lowering of Core to STG: every lambda and every delayed expression becomes a closure with its
//! own frame slots and the variables it captures; variables are slots of the current frame, captured
//! free variables, or globals. Arguments and constructor fields are atoms (variables or literals);
//! anything else is allocated as a thunk.

use std::collections::{HashMap, HashSet};

use crate::core::{Alt, Core, Lit, TopBind};

/// A value that can be passed without evaluating it.
#[derive(Clone, Debug, PartialEq)]
pub enum Atom {
    /// A slot of the current frame (slot 0 is the closure itself).
    Slot(u32),
    /// A variable captured by the current closure.
    Free(u32),
    /// A top-level binding.
    Global(u32),
    /// A literal, materialised as a static object (or a string for `Str`).
    Lit(SLit),
}

#[derive(Clone, Debug, PartialEq)]
pub enum SLit {
    Int(i64),
    Double(f64),
    Char(char),
    Integer(String),
    Str(String),
    Addr(String),
    Frac(String),
}

/// An expression of the body of a closure.
#[derive(Clone, Debug)]
pub enum SExpr {
    /// Evaluate an atom to weak head normal form and return it.
    Atom(Atom),
    /// Apply a function to atoms (evaluated when the function is).
    App(Atom, Vec<Atom>),
    /// Allocate a constructor and return it.
    Con(String, Vec<Atom>),
    /// A primitive operation on (lazy) atoms.
    Prim(String, Vec<Atom>),
    /// Allocate closures into slots, then continue.
    Let(Vec<(u32, SClosure)>, Box<SExpr>),
    /// Evaluate the scrutinee, bind it to a slot, select an alternative.
    Case(Box<SExpr>, u32, Vec<SAlt>),
    /// Allocate a closure and return it (a lambda in a strict position).
    Closure(Box<SClosure>),
    Fail(String),
    /// A join point `id` with its parameter slots and code, in scope in the body: it runs in the
    /// frame of the enclosing closure, and is reached only by jumps in tail position.
    Join(u32, Vec<u32>, Box<SExpr>, Box<SExpr>),
    /// Stores the atoms in the join point's parameter slots and continues at its code.
    Jump(u32, Vec<Atom>),
}

#[derive(Clone, Debug)]
pub enum SAlt {
    /// A constructor with its field slots.
    Con(String, Vec<u32>, SExpr),
    Lit(Lit, SExpr),
    Default(SExpr),
}

/// A closure: a function of `arity` arguments, or a thunk (arity 0), with its captured atoms.
#[derive(Clone, Debug)]
pub struct SClosure {
    pub arity: u32,
    /// The atoms, in the enclosing frame, that are captured, in capture order.
    pub captures: Vec<Atom>,
    /// The number of frame slots: slot 0 is the closure, then the arguments, then the locals.
    pub nslots: u32,
    pub body: SExpr,
}

/// A top-level binding: a closure with no captures (a function, or a CAF thunk).
pub struct SGlobal {
    pub name: String,
    pub closure: SClosure,
}

pub struct SProgram {
    pub globals: Vec<SGlobal>,
    pub index: HashMap<String, u32>,
}

/// The lowering state of one closure body being built.
struct Body {
    /// Local names bound in this body, to their slots.
    locals: HashMap<String, u32>,
    nslots: u32,
    /// Captured names, to their index, with the outer variable each is copied from.
    captures: Vec<(String, Atom)>,
    /// Captured names by index into `captures`.
    capture_index: HashMap<String, u32>,
    /// The join points in scope, to their id and arity.
    joins: HashMap<String, (u32, usize)>,
}

pub struct Lowering {
    globals: HashSet<String>,
    index: HashMap<String, u32>,
    /// The stack of bodies being lowered: the innermost is last.
    bodies: Vec<Body>,
    /// Thunks for the non-atomic arguments of the operation being lowered, to be allocated before it.
    pending: Vec<(u32, SClosure)>,
    /// The next join point id (unique in the program).
    next_join: u32,
}

impl Body {
    fn new() -> Body {
        // Slot 0 is the closure itself.
        Body { locals: HashMap::new(), nslots: 1, captures: Vec::new(), capture_index: HashMap::new(), joins: HashMap::new() }
    }

    fn fresh_slot(&mut self) -> u32 {
        let s = self.nslots;
        self.nslots += 1;
        s
    }
}

impl Lowering {
    /// Lowers a whole program. Global names are the top-level bindings.
    pub fn program(binds: &[TopBind]) -> SProgram {
        let index: HashMap<String, u32> = binds.iter().enumerate().map(|(i, b)| (b.name.clone(), i as u32)).collect();
        let mut lw = Lowering {
            globals: index.keys().cloned().collect(),
            index: index.clone(),
            bodies: Vec::new(),
            pending: Vec::new(),
            next_join: 0,
        };
        let globals = binds
            .iter()
            .map(|b| {
                let closure = lw.closure_of(&b.body);
                SGlobal { name: b.name.clone(), closure }
            })
            .collect();
        SProgram { globals, index }
    }

    /// The closure of a top-level or delayed expression: a lambda's parameters, or a thunk.
    fn closure_of(&mut self, e: &Core) -> SClosure {
        self.bodies.push(Body::new());
        let (arity, body_expr) = match e {
            Core::Lam(..) => {
                let mut params = Vec::new();
                let mut cur = e;
                while let Core::Lam(p, b) = cur {
                    params.push(p.clone());
                    cur = b;
                }
                let top = self.bodies.last_mut().expect("body");
                for p in &params {
                    let s = top.fresh_slot();
                    top.locals.insert(p.clone(), s);
                }
                (params.len() as u32, cur)
            }
            other => (0, other),
        };
        let body = self.strict(body_expr);
        let b = self.bodies.pop().expect("body");
        let captures = b.captures.iter().map(|(_, a)| a.clone()).collect();
        SClosure { arity, captures, nslots: b.nslots, body }
    }

    /// Resolves a variable in the current body: a slot, a captured variable (captured now from
    /// the enclosing bodies), or a global.
    fn resolve(&mut self, name: &str) -> Atom {
        let depth = self.bodies.len() - 1;
        if let Some(a) = self.resolve_at(depth, name) {
            return a;
        }
        if let Some(&g) = self.index.get(name) {
            return Atom::Global(g);
        }
        panic!("internal: unbound variable {name} in lowering");
    }

    fn resolve_at(&mut self, depth: usize, name: &str) -> Option<Atom> {
        if let Some(&s) = self.bodies[depth].locals.get(name) {
            return Some(Atom::Slot(s));
        }
        if let Some(&i) = self.bodies[depth].capture_index.get(name) {
            return Some(Atom::Free(i));
        }
        if depth == 0 {
            return None;
        }
        // Capture from the enclosing body.
        let outer = self.resolve_at(depth - 1, name)?;
        let b = &mut self.bodies[depth];
        let i = b.captures.len() as u32;
        b.captures.push((name.to_string(), outer));
        b.capture_index.insert(name.to_string(), i);
        Some(Atom::Free(i))
    }

    /// An expression evaluated in place (a case scrutinee, a body's result).
    fn strict(&mut self, e: &Core) -> SExpr {
        match e {
            Core::Var(v) => match self.join(v, 0) {
                Some(id) => SExpr::Jump(id, Vec::new()),
                None => SExpr::Atom(self.resolve(v)),
            },
            Core::Lit(l) => SExpr::Atom(Atom::Lit(slit(l))),
            Core::App(..) => {
                let mark = self.pending.len();
                let (f, args) = spine(e);
                if let Some(id) = match f { Core::Var(v) => self.join(v, args.len()), _ => None } {
                    let aa = args.iter().map(|a| self.atom(a)).collect();
                    return self.allocating(mark, SExpr::Jump(id, aa));
                }
                let fa = self.atom(f);
                let aa = args.iter().map(|a| self.atom(a)).collect();
                self.allocating(mark, SExpr::App(fa, aa))
            }
            Core::Lam(..) => SExpr::Closure(Box::new(self.closure_of(e))),
            Core::Let(binds, body) if is_join(binds, body) => {
                let (name, rhs) = &binds[0];
                let mut params = Vec::new();
                let mut code = rhs;
                while let Core::Lam(x, b) = code {
                    params.push(self.bind_local(x));
                    code = b;
                }
                let code = self.strict(code);
                let id = self.next_join;
                self.next_join += 1;
                let prev = self.bodies.last_mut().expect("body").joins.insert(name.clone(), (id, params.len()));
                let body = self.strict(body);
                let joins = &mut self.bodies.last_mut().expect("body").joins;
                match prev {
                    Some(p) => joins.insert(name.clone(), p),
                    None => joins.remove(name),
                };
                SExpr::Join(id, params, Box::new(code), Box::new(body))
            }
            Core::Let(binds, body) => {
                let (slots, rhs) = self.let_binds(binds);
                let body = self.strict(body);
                SExpr::Let(rhs.into_iter().zip(slots).map(|(c, s)| (s, c)).collect(), Box::new(body))
            }
            Core::Case(scrut, binder, alts) => {
                // A nested case in a scrutinee becomes a join point, and a let is floated out of
                // the scrutinee. (A scrutinee can still be a case made by `strict`, from `seq` and
                // its identities: codegen gives its return address an empty frame.)
                match scrut.as_ref() {
                    Core::Let(binds, body) => {
                        let inner = Core::Let(
                            binds.clone(),
                            Box::new(Core::Case(body.clone(), binder.clone(), alts.clone())),
                        );
                        return self.strict(&inner);
                    }
                    Core::Case(..) => {
                        let join = join_point(scrut, binder, alts);
                        return self.strict(&join);
                    }
                    _ => {}
                }
                let scrut = self.strict(scrut);
                let bslot = self.bind_local(binder);
                let alts = alts.iter().map(|a| self.alt(a)).collect();
                SExpr::Case(Box::new(scrut), bslot, alts)
            }
            Core::Con(name, args) => {
                let mark = self.pending.len();
                let atoms = args.iter().map(|a| self.atom(a)).collect();
                self.allocating(mark, SExpr::Con(name.clone(), atoms))
            }
            // `seq a b` evaluates a, then answers b; the identities answer their argument.
            Core::Prim(name, args) if name == "seq" && args.len() == 2 => {
                let slot = self.bind_local(&format!("seq%{}", self.bodies.len()));
                let scrut = self.strict(&args[0]);
                let body = self.strict(&args[1]);
                SExpr::Case(Box::new(scrut), slot, vec![SAlt::Default(body)])
            }
            Core::Prim(name, args) if IDENTITY_PRIMS.contains(&name.as_str()) && args.len() == 1 => {
                self.strict(&args[0])
            }
            Core::Prim(name, args) => {
                let mark = self.pending.len();
                let atoms = args.iter().map(|a| self.atom(a)).collect();
                self.allocating(mark, SExpr::Prim(name.clone(), atoms))
            }
            Core::Fail(msg) => SExpr::Fail(msg.clone()),
        }
    }

    /// Wraps an operation with the allocations of the thunks its arguments needed.
    fn allocating(&mut self, mark: usize, op: SExpr) -> SExpr {
        if self.pending.len() == mark {
            return op;
        }
        let bindings: Vec<(u32, SClosure)> = self.pending.drain(mark..).collect();
        SExpr::Let(bindings, Box::new(op))
    }

    /// The join point a variable names in the current body, when it is applied to `n` arguments.
    fn join(&self, name: &str, n: usize) -> Option<u32> {
        let b = self.bodies.last()?;
        b.joins.get(name).filter(|(_, arity)| *arity == n).map(|(id, _)| *id)
    }

    fn bind_local(&mut self, name: &str) -> u32 {
        let b = self.bodies.last_mut().expect("body");
        b.joins.remove(name);
        let s = b.fresh_slot();
        b.locals.insert(name.to_string(), s);
        s
    }

    /// Letrec bindings: slots are bound first (so the right-hand sides can refer to each other),
    /// then the closures are lowered.
    fn let_binds(&mut self, binds: &[(String, Core)]) -> (Vec<u32>, Vec<SClosure>) {
        let slots: Vec<u32> = binds.iter().map(|(n, _)| self.bind_local(n)).collect();
        let closures = binds.iter().map(|(_, rhs)| self.closure_of(rhs)).collect();
        (slots, closures)
    }

    fn alt(&mut self, a: &Alt) -> SAlt {
        match a {
            Alt::Con(name, vars, body) => {
                let slots = vars.iter().map(|v| self.bind_local(v)).collect();
                SAlt::Con(name.clone(), slots, self.strict(body))
            }
            Alt::Lit(l, body) => SAlt::Lit(l.clone(), self.strict(body)),
            Alt::Default(body) => SAlt::Default(self.strict(body)),
        }
    }

    /// An argument: a variable or literal is passed as it is; anything else becomes a thunk.
    fn atom(&mut self, e: &Core) -> Atom {
        match e {
            Core::Var(v) => self.resolve(v),
            // Numbers and characters are static objects; the others are built when they are used.
            Core::Lit(l @ (Lit::Int(_) | Lit::Double(_) | Lit::Char(_))) => Atom::Lit(slit(l)),
            other => {
                // A delayed expression (or a lambda): a closure, allocated before the operation.
                let c = self.closure_of(other);
                let b = self.bodies.last_mut().expect("body");
                let s = b.fresh_slot();
                self.pending.push((s, c));
                Atom::Slot(s)
            }
        }
    }
}

/// `case (case e of p -> r) of alts` as `let j = \x -> case x of alts in case e of p -> j r`: the
/// alternatives of the inner case end in calls of the join point `j`, so no case is a scrutinee.
fn join_point(scrut: &Core, binder: &str, alts: &[Alt]) -> Core {
    let j = format!("{binder}%join");
    let x = format!("{binder}%x");
    let body = Core::Case(Box::new(Core::Var(x.clone())), binder.to_string(), alts.to_vec());
    let join = Core::Lam(x, Box::new(body));
    let jumped = redirect(scrut, &j);
    Core::Let(vec![(j, join)], Box::new(jumped))
}

/// Replaces each result of `e` (the tail expressions of its cases and lets) with an application of
/// the join point `j` to it.
fn redirect(e: &Core, j: &str) -> Core {
    match e {
        Core::Let(binds, body) => Core::Let(binds.clone(), Box::new(redirect(body, j))),
        Core::Case(scrut, b, alts) => Core::Case(
            scrut.clone(),
            b.clone(),
            alts.iter()
                .map(|a| match a {
                    Alt::Con(c, vs, body) => Alt::Con(c.clone(), vs.clone(), redirect(body, j)),
                    Alt::Lit(l, body) => Alt::Lit(l.clone(), redirect(body, j)),
                    Alt::Default(body) => Alt::Default(redirect(body, j)),
                })
                .collect(),
        ),
        Core::Fail(_) => e.clone(),
        other => Core::App(Box::new(Core::Var(j.to_string())), Box::new(other.clone())),
    }
}

/// Whether a let is lowered as a join point: one non-recursive binding all of whose uses in the
/// body are calls with as many arguments as it has parameters, in tail position.
fn is_join(binds: &[(String, Core)], body: &Core) -> bool {
    let [(name, rhs)] = binds else { return false };
    !mentions(rhs, name) && tail_calls(body, name, lambdas(rhs).0)
}

/// The leading parameters of an expression, and what is under them.
fn lambdas(e: &Core) -> (usize, &Core) {
    let mut n = 0;
    let mut cur = e;
    while let Core::Lam(_, b) = cur {
        n += 1;
        cur = b;
    }
    (n, cur)
}

const IDENTITY_PRIMS: [&str; 6] = ["unsafeCoerce", "unsafeCoerce#", "lazy", "oneShot", "inline", "noinline"];

/// Whether every use of `j` in `e` is a call `j a1 .. an` in tail position (its arguments not
/// mentioning `j`). Tail positions are those `strict` lowers in the frame and mode of `e`: case
/// alternatives, let bodies, the code of join points, and the result of `seq` and identities.
fn tail_calls(e: &Core, j: &str, n: usize) -> bool {
    match e {
        Core::Var(v) => v != j || n == 0,
        Core::Lit(_) | Core::Fail(_) => true,
        Core::App(..) => {
            let (f, args) = spine(e);
            match f {
                Core::Var(v) if v == j => args.len() == n && args.iter().all(|a| !mentions(a, j)),
                _ => !mentions(e, j),
            }
        }
        Core::Lam(x, b) => x == j || !mentions(b, j),
        Core::Let(binds, body) => {
            if binds.iter().any(|(x, _)| x == j) {
                return true;
            }
            if is_join(binds, body) {
                let (_, code) = lambdas(&binds[0].1);
                let shadowed = {
                    let mut cur = &binds[0].1;
                    let mut hit = false;
                    while let Core::Lam(x, b) = cur {
                        hit |= x == j;
                        cur = b;
                    }
                    hit
                };
                (shadowed || tail_calls(code, j, n)) && tail_calls(body, j, n)
            } else {
                binds.iter().all(|(_, rhs)| !mentions(rhs, j)) && tail_calls(body, j, n)
            }
        }
        Core::Case(scrut, binder, alts) => {
            !mentions(scrut, j)
                && (binder == j
                    || alts.iter().all(|a| match a {
                        Alt::Con(_, vars, b) => vars.iter().any(|v| v == j) || tail_calls(b, j, n),
                        Alt::Lit(_, b) | Alt::Default(b) => tail_calls(b, j, n),
                    }))
        }
        Core::Prim(p, args) if p == "seq" && args.len() == 2 => !mentions(&args[0], j) && tail_calls(&args[1], j, n),
        Core::Prim(p, args) if IDENTITY_PRIMS.contains(&p.as_str()) && args.len() == 1 => tail_calls(&args[0], j, n),
        Core::Con(_, args) | Core::Prim(_, args) => args.iter().all(|a| !mentions(a, j)),
    }
}

/// Whether a variable occurs in an expression (shadowing ignored: a conservative answer).
fn mentions(e: &Core, j: &str) -> bool {
    match e {
        Core::Var(v) => v == j,
        Core::Lit(_) | Core::Fail(_) => false,
        Core::App(f, a) => mentions(f, j) || mentions(a, j),
        Core::Lam(_, b) => mentions(b, j),
        Core::Let(binds, body) => binds.iter().any(|(_, r)| mentions(r, j)) || mentions(body, j),
        Core::Case(s, _, alts) => {
            mentions(s, j)
                || alts.iter().any(|a| match a {
                    Alt::Con(_, _, b) | Alt::Lit(_, b) | Alt::Default(b) => mentions(b, j),
                })
        }
        Core::Con(_, args) | Core::Prim(_, args) => args.iter().any(|a| mentions(a, j)),
    }
}

/// The head and arguments of an application spine.
fn spine(e: &Core) -> (&Core, Vec<&Core>) {
    let mut args = Vec::new();
    let mut cur = e;
    while let Core::App(f, a) = cur {
        args.push(a.as_ref());
        cur = f;
    }
    args.reverse();
    (cur, args)
}

fn slit(l: &Lit) -> SLit {
    match l {
        Lit::Integer(s) => SLit::Integer(s.clone()),
        Lit::Int(i) => SLit::Int(*i),
        Lit::Double(d) => SLit::Double(*d),
        Lit::Frac(s) => SLit::Frac(s.clone()),
        Lit::Char(c) => SLit::Char(*c),
        Lit::Str(s) => SLit::Str(s.clone()),
        Lit::Addr(s) => SLit::Addr(s.clone()),
    }
}
