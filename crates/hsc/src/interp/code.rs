//! Resolution of Core into executable code. Variables become frame slots, captured
//! free variables, or global indices, so that evaluation never compares names.
//!
//! Every lambda is a flat closure: it copies the cells of the variables it uses from
//! the enclosing frame. Every function invocation and every top-level binding has a
//! frame of slots for its own binders.

use std::collections::HashMap;

use super::bigint::BigInt;
use crate::core::{Alt, Core, Lit, TopBind};

/// Where a variable's cell comes from at run time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Acc {
    /// A slot of the current frame.
    Local(u32),
    /// A captured variable of the current closure.
    Free(u32),
    /// A top-level binding.
    Global(u32),
}

#[derive(Debug)]
pub enum Code {
    Var(Acc),
    /// Integer literal, pre-parsed.
    Integer(std::rc::Rc<BigInt>),
    Int(i64),
    Double(f64),
    Frac(String),
    /// An `Addr#` literal: its bytes, NUL-terminated.
    Addr(std::rc::Rc<Vec<u8>>),
    Char(char),
    Str(String),
    App(Box<Code>, Box<Code>),
    Lam(&'static LamInfo),
    /// Recursive bindings into frame slots, then the body.
    Let(Vec<(u32, Code)>, Box<Code>),
    /// Forces the scrutinee into a slot, then selects an alternative.
    Case(Box<Code>, u32, Vec<CAlt>),
    Con(&'static str, Vec<Code>),
    Prim(String, Vec<Code>),
    Fail(String),
}

#[derive(Debug)]
pub enum CAlt {
    Con(&'static str, Vec<u32>, Code),
    Lit(Lit, Code),
    Default(Code),
}

#[derive(Debug)]
pub struct LamInfo {
    /// Accesses, in the creating frame, of the captured variables.
    pub captures: Vec<Acc>,
    /// Number of frame slots, including the parameter in slot 0.
    pub nslots: u32,
    pub body: Code,
}

/// The compiled program: global names and their code, with frame sizes.
pub struct Program {
    pub names: Vec<String>,
    pub index: HashMap<String, u32>,
    pub bodies: Vec<(Code, u32)>,
}

/// One function being compiled: the names bound in it, in lexical order, and the
/// variables it captures from enclosing functions.
struct Scope {
    /// Visible local binders: (name, slot). Searched from the end, so inner
    /// bindings shadow outer ones.
    locals: Vec<(String, u32)>,
    nslots: u32,
    /// Captured names with their index, in capture order.
    captures: Vec<(String, u32)>,
    /// The access of each capture in the enclosing scope.
    capture_accs: Vec<Acc>,
}

struct Compiler<'g> {
    globals: &'g HashMap<String, u32>,
    scopes: Vec<Scope>,
    /// Every name the top-level binding being compiled mentions (locals are unique, so this
    /// tells whether a case binder is used without searching its alternatives each time).
    mentioned: std::collections::HashSet<String>,
}

impl Compiler<'_> {
    fn new_scope(&mut self) {
        self.scopes.push(Scope {
            locals: Vec::new(),
            nslots: 0,
            captures: Vec::new(),
            capture_accs: Vec::new(),
        });
    }

    fn bind(&mut self, name: &str) -> u32 {
        let scope = self.scopes.last_mut().expect("scope");
        let slot = scope.nslots;
        scope.nslots += 1;
        scope.locals.push((name.to_string(), slot));
        slot
    }

    /// Resolves a variable in scope `depth` (an index into `scopes`).
    fn resolve_at(&mut self, depth: usize, name: &str) -> Option<Acc> {
        {
            let scope = &self.scopes[depth];
            if let Some((_, slot)) = scope.locals.iter().rev().find(|(n, _)| n == name) {
                return Some(Acc::Local(*slot));
            }
            if let Some(i) = scope.captures.iter().position(|(n, _)| n == name) {
                return Some(Acc::Free(i as u32));
            }
        }
        if depth == 0 {
            return self.globals.get(name).map(|g| Acc::Global(*g));
        }
        let outer = self.resolve_at(depth - 1, name)?;
        let scope = &mut self.scopes[depth];
        let idx = scope.captures.len() as u32;
        scope.captures.push((name.to_string(), idx));
        scope.capture_accs.push(outer);
        Some(Acc::Free(idx))
    }

    fn var(&mut self, name: &str) -> Acc {
        let depth = self.scopes.len() - 1;
        match self.resolve_at(depth, name) {
            Some(a) => a,
            None => panic!("internal: unbound variable {name}"),
        }
    }

    fn lit(&self, l: &Lit) -> Code {
        match l {
            Lit::Integer(s) => Code::Integer(std::rc::Rc::new(
                BigInt::parse(s).expect("integer literal"),
            )),
            Lit::Int(i) => Code::Int(*i),
            Lit::Double(d) => Code::Double(*d),
            Lit::Frac(s) => Code::Frac(s.clone()),
            Lit::Char(c) => Code::Char(*c),
            Lit::Str(s) => Code::Str(s.clone()),
            Lit::Addr(s) => {
                let mut bytes: Vec<u8> = s.chars().map(|c| c as u32 as u8).collect();
                bytes.push(0);
                Code::Addr(std::rc::Rc::new(bytes))
            }
        }
    }

    fn expr(&mut self, c: &Core) -> Code {
        match c {
            Core::Var(v) => Code::Var(self.var(v)),
            Core::Lit(l) => self.lit(l),
            Core::App(f, a) => Code::App(Box::new(self.expr(f)), Box::new(self.expr(a))),
            Core::Lam(x, body) => {
                self.new_scope();
                self.bind(x);
                let body_code = self.expr(body);
                let scope = self.scopes.pop().expect("scope");
                let info = LamInfo {
                    captures: scope.capture_accs,
                    nslots: scope.nslots,
                    body: body_code,
                };
                Code::Lam(Box::leak(Box::new(info)))
            }
            Core::Let(binds, body) => {
                let mark = self.scopes.last().expect("scope").locals.len();
                let slots: Vec<u32> = binds.iter().map(|(n, _)| self.bind(n)).collect();
                let codes: Vec<(u32, Code)> = binds
                    .iter()
                    .zip(slots)
                    .map(|((_, b), slot)| (slot, self.expr(b)))
                    .collect();
                let body_code = self.expr(body);
                self.scopes.last_mut().expect("scope").locals.truncate(mark);
                Code::Let(codes, Box::new(body_code))
            }
            Core::Case(scrut, b, alts) => {
                let scrut_code = self.expr(scrut);
                let mark = self.scopes.last().expect("scope").locals.len();
                // An unused binder needs no slot; its cell is then never read.
                let used = self.mentioned.contains(b.as_str());
                let bslot = if used { self.bind(b) } else { u32::MAX };
                let mut out = Vec::with_capacity(alts.len());
                for alt in alts {
                    match alt {
                        Alt::Con(name, vars, body) => {
                            let m2 = self.scopes.last().expect("scope").locals.len();
                            let slots: Vec<u32> = vars.iter().map(|v| self.bind(v)).collect();
                            let body_code = self.expr(body);
                            self.scopes.last_mut().expect("scope").locals.truncate(m2);
                            out.push(CAlt::Con(leak(name), slots, body_code));
                        }
                        Alt::Lit(l, body) => out.push(CAlt::Lit(l.clone(), self.expr(body))),
                        Alt::Default(body) => out.push(CAlt::Default(self.expr(body))),
                    }
                }
                self.scopes.last_mut().expect("scope").locals.truncate(mark);
                Code::Case(Box::new(scrut_code), bslot, out)
            }
            Core::Con(name, args) => Code::Con(leak(name), args.iter().map(|a| self.expr(a)).collect()),
            Core::Prim(name, args) => Code::Prim(name.clone(), args.iter().map(|a| self.expr(a)).collect()),
            Core::Fail(msg) => Code::Fail(msg.clone()),
        }
    }
}

/// The variable names an expression mentions.
fn names_in(c: &Core, out: &mut std::collections::HashSet<String>) {
    match c {
        Core::Var(v) => {
            if !out.contains(v.as_str()) {
                out.insert(v.clone());
            }
        }
        Core::Lit(_) | Core::Fail(_) => {}
        Core::App(f, a) => {
            names_in(f, out);
            names_in(a, out);
        }
        Core::Lam(_, b) => names_in(b, out),
        Core::Let(binds, body) => {
            binds.iter().for_each(|(_, b)| names_in(b, out));
            names_in(body, out);
        }
        Core::Case(s, _, alts) => {
            names_in(s, out);
            for a in alts {
                match a {
                    Alt::Con(_, _, b) | Alt::Lit(_, b) | Alt::Default(b) => names_in(b, out),
                }
            }
        }
        Core::Con(_, args) | Core::Prim(_, args) => args.iter().for_each(|a| names_in(a, out)),
    }
}

/// Whether a name occurs free-or-bound in a Core expression (a conservative test).
#[allow(dead_code)]
fn mentions(c: &Core, name: &str) -> bool {
    match c {
        Core::Var(v) => v == name,
        Core::Lit(_) | Core::Fail(_) => false,
        Core::App(f, a) => mentions(f, name) || mentions(a, name),
        Core::Lam(_, b) => mentions(b, name),
        Core::Let(binds, body) => binds.iter().any(|(_, b)| mentions(b, name)) || mentions(body, name),
        Core::Case(s, _, alts) => {
            mentions(s, name)
                || alts.iter().any(|a| match a {
                    Alt::Con(_, _, b) | Alt::Lit(_, b) | Alt::Default(b) => mentions(b, name),
                })
        }
        Core::Con(_, args) | Core::Prim(_, args) => args.iter().any(|a| mentions(a, name)),
    }
}

/// Interns a string for the program's lifetime.
pub fn leak(s: &str) -> &'static str {
    Box::leak(s.to_string().into_boxed_str())
}

/// Compiles all top-level bindings.
pub fn compile_program(binds: Vec<TopBind>) -> Program {
    let mut index = HashMap::new();
    let mut names = Vec::with_capacity(binds.len());
    for (i, b) in binds.iter().enumerate() {
        index.insert(b.name.clone(), i as u32);
        names.push(b.name.clone());
    }
    let mut bodies = Vec::with_capacity(binds.len());
    let timing = std::env::var_os("HSC_TIME_CODE").is_some();
    let mut slow: Vec<(std::time::Duration, String)> = Vec::new();
    for b in &binds {
        let t0 = std::time::Instant::now();
        let mut mentioned = std::collections::HashSet::new();
        names_in(&b.body, &mut mentioned);
        let mut comp = Compiler {
            globals: &index,
            scopes: Vec::new(),
            mentioned,
        };
        comp.new_scope();
        let code = comp.expr(&b.body);
        let scope = comp.scopes.pop().expect("scope");
        debug_assert!(scope.captures.is_empty(), "top-level code captures nothing");
        bodies.push((code, scope.nslots));
        if timing {
            slow.push((t0.elapsed(), b.name.clone()));
        }
    }
    if timing {
        slow.sort();
        for (t, n) in slow.iter().rev().take(8) {
            eprintln!("  {t:?} {n}");
        }
    }
    Program { names, index, bodies }
}
