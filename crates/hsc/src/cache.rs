//! The module cache: what checking a module adds to the compiler's state (its interface, its
//! Core, and its part of the type environment), saved on disk so that a later compilation with
//! the same compiler loads it instead of parsing, renaming and checking the module again.
//!
//! An entry is found by the module's source and flags; it records the keys of the modules it
//! was checked against, and is used only when those modules have the same keys now (see
//! `Driver::load`). Entries live in `~/.cache/hsc/modules/<compiler>/`; the directories of other
//! compiler builds are removed, so the cache holds the entries of the latest builds only.

use std::collections::{BTreeMap, HashMap};

use crate::core::{Alt, Core, Lit as CLit, TopBind};
use crate::rename::iface::{EntKind, Entity, GName, Iface, PatSynDef};
use crate::syntax::ast::*;
use crate::syntax::token::PrimLit;
use crate::tc::env::{ClassInfo, DataCon, Env, FamEqn, FamilyInfo, InstInfo, Synonym, TyConInfo};
use crate::tc::kinds::Kind;
use crate::tc::types::{Pred, Scheme, Ty};

// ----- binary encoding -------------------------------------------------------------------

/// The bytes being written; `ok` turns false when a value cannot be encoded (the entry is then
/// not saved).
pub struct W {
    pub out: Vec<u8>,
    pub ok: bool,
}

pub struct R<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl W {
    fn u8(&mut self, b: u8) {
        self.out.push(b);
    }
}

impl R<'_> {
    fn u8(&mut self) -> Option<u8> {
        let b = *self.bytes.get(self.pos)?;
        self.pos += 1;
        Some(b)
    }

    fn take(&mut self, n: usize) -> Option<&[u8]> {
        let s = self.bytes.get(self.pos..self.pos + n)?;
        self.pos += n;
        Some(s)
    }
}

pub trait Bin: Sized {
    fn put(&self, w: &mut W);
    fn get(r: &mut R) -> Option<Self>;
}

impl Bin for u8 {
    fn put(&self, w: &mut W) {
        w.u8(*self);
    }
    fn get(r: &mut R) -> Option<Self> {
        r.u8()
    }
}

impl Bin for u64 {
    fn put(&self, w: &mut W) {
        w.out.extend_from_slice(&self.to_le_bytes());
    }
    fn get(r: &mut R) -> Option<Self> {
        Some(u64::from_le_bytes(r.take(8)?.try_into().ok()?))
    }
}

impl Bin for u32 {
    fn put(&self, w: &mut W) {
        w.out.extend_from_slice(&self.to_le_bytes());
    }
    fn get(r: &mut R) -> Option<Self> {
        Some(u32::from_le_bytes(r.take(4)?.try_into().ok()?))
    }
}

impl Bin for i64 {
    fn put(&self, w: &mut W) {
        (*self as u64).put(w);
    }
    fn get(r: &mut R) -> Option<Self> {
        Some(u64::get(r)? as i64)
    }
}

impl Bin for usize {
    fn put(&self, w: &mut W) {
        (*self as u64).put(w);
    }
    fn get(r: &mut R) -> Option<Self> {
        Some(u64::get(r)? as usize)
    }
}

impl Bin for f64 {
    fn put(&self, w: &mut W) {
        self.to_bits().put(w);
    }
    fn get(r: &mut R) -> Option<Self> {
        Some(f64::from_bits(u64::get(r)?))
    }
}

impl Bin for bool {
    fn put(&self, w: &mut W) {
        w.u8(*self as u8);
    }
    fn get(r: &mut R) -> Option<Self> {
        Some(r.u8()? != 0)
    }
}

impl Bin for char {
    fn put(&self, w: &mut W) {
        (*self as u32).put(w);
    }
    fn get(r: &mut R) -> Option<Self> {
        char::from_u32(u32::get(r)?)
    }
}

impl Bin for String {
    fn put(&self, w: &mut W) {
        (self.len() as u32).put(w);
        w.out.extend_from_slice(self.as_bytes());
    }
    fn get(r: &mut R) -> Option<Self> {
        let n = u32::get(r)? as usize;
        String::from_utf8(r.take(n)?.to_vec()).ok()
    }
}

impl<T: Bin> Bin for Vec<T> {
    fn put(&self, w: &mut W) {
        (self.len() as u32).put(w);
        for x in self {
            x.put(w);
        }
    }
    fn get(r: &mut R) -> Option<Self> {
        let n = u32::get(r)? as usize;
        let mut v = Vec::with_capacity(n.min(1 << 16));
        for _ in 0..n {
            v.push(T::get(r)?);
        }
        Some(v)
    }
}

impl<T: Bin> Bin for Option<T> {
    fn put(&self, w: &mut W) {
        match self {
            None => w.u8(0),
            Some(x) => {
                w.u8(1);
                x.put(w);
            }
        }
    }
    fn get(r: &mut R) -> Option<Self> {
        match r.u8()? {
            0 => Some(None),
            1 => Some(Some(T::get(r)?)),
            _ => None,
        }
    }
}

impl<T: Bin> Bin for Box<T> {
    fn put(&self, w: &mut W) {
        (**self).put(w);
    }
    fn get(r: &mut R) -> Option<Self> {
        Some(Box::new(T::get(r)?))
    }
}

impl<A: Bin, B: Bin> Bin for (A, B) {
    fn put(&self, w: &mut W) {
        self.0.put(w);
        self.1.put(w);
    }
    fn get(r: &mut R) -> Option<Self> {
        Some((A::get(r)?, B::get(r)?))
    }
}

impl<A: Bin, B: Bin, C: Bin> Bin for (A, B, C) {
    fn put(&self, w: &mut W) {
        self.0.put(w);
        self.1.put(w);
        self.2.put(w);
    }
    fn get(r: &mut R) -> Option<Self> {
        Some((A::get(r)?, B::get(r)?, C::get(r)?))
    }
}

impl<K: Bin + std::hash::Hash + Eq, V: Bin> Bin for HashMap<K, V> {
    fn put(&self, w: &mut W) {
        (self.len() as u32).put(w);
        for (k, v) in self {
            k.put(w);
            v.put(w);
        }
    }
    fn get(r: &mut R) -> Option<Self> {
        let n = u32::get(r)? as usize;
        let mut m = HashMap::with_capacity(n.min(1 << 16));
        for _ in 0..n {
            let k = K::get(r)?;
            m.insert(k, V::get(r)?);
        }
        Some(m)
    }
}

impl<K: Bin + Ord, V: Bin> Bin for BTreeMap<K, V> {
    fn put(&self, w: &mut W) {
        (self.len() as u32).put(w);
        for (k, v) in self {
            k.put(w);
            v.put(w);
        }
    }
    fn get(r: &mut R) -> Option<Self> {
        let n = u32::get(r)? as usize;
        let mut m = BTreeMap::new();
        for _ in 0..n {
            let k = K::get(r)?;
            m.insert(k, V::get(r)?);
        }
        Some(m)
    }
}

/// A struct, field by field.
macro_rules! bin_struct {
    ($t:ty { $($f:ident),* $(,)? }) => {
        impl Bin for $t {
            fn put(&self, w: &mut W) {
                $( self.$f.put(w); )*
            }
            fn get(r: &mut R) -> Option<Self> {
                Some(Self { $( $f: Bin::get(r)?, )* })
            }
        }
    };
}

/// An enum, by a tag per variant (unit, tuple or struct variants).
macro_rules! bin_enum {
    ($t:ident { $( $tag:literal => $v:ident $( ( $($a:ident),* ) )? $( { $($fa:ident),* } )? ),* $(,)? }) => {
        impl Bin for $t {
            #[allow(unused_variables)]
            fn put(&self, w: &mut W) {
                match self {
                    $( $t::$v $( ( $($a),* ) )? $( { $($fa),* } )? => {
                        w.u8($tag);
                        $( $( $a.put(w); )* )?
                        $( $( $fa.put(w); )* )?
                    } )*
                }
            }
            fn get(r: &mut R) -> Option<Self> {
                match r.u8()? {
                    $( $tag => Some($t::$v $( ( $( { let $a = Bin::get(r)?; $a } ),* ) )? $( { $( $fa: Bin::get(r)? ),* } )? ), )*
                    _ => None,
                }
            }
        }
    };
}

// ----- the types the cache holds ---------------------------------------------------------

bin_struct!(Loc { line, col });
bin_struct!(QName { qual, name });
bin_enum!(PrimLit { 0 => Int(a), 1 => Word(a), 2 => Float(a), 3 => Double(a), 4 => Char(a), 5 => Str(a) });
bin_enum!(Lit { 0 => Prim(a), 1 => Int(a), 2 => Frac(a), 3 => Char(a), 4 => Str(a) });
bin_struct!(TyVarBind { name, kind });
bin_enum!(Type {
    0 => Var(a, b), 1 => Con(a, b), 2 => App(a, b), 3 => Fun(a, b), 4 => List(a, b), 5 => Tuple(a, b),
    6 => Forall(a, b), 7 => Qual(a, b), 8 => Sig(a, b), 9 => OpChain(a, b), 10 => Lit(a, b), 11 => Wild(a),
});
bin_enum!(TypeOpItem { 0 => Operand(a), 1 => Op(a, b) });
bin_enum!(Pat {
    0 => Var(a, b), 1 => Wild(a), 2 => Lit(a, b), 3 => NegLit(a, b), 4 => Con(a, b, c),
    5 => Rec { con, fields, dotdot, loc }, 6 => Tuple(a, b), 7 => List(a, b), 8 => As(a, b, c),
    9 => Lazy(a, b), 10 => Bang(a, b), 11 => Sig(a, b, c), 12 => View(a, b, c), 13 => OpChain(a, b),
});
bin_enum!(PatOpItem { 0 => Operand(a), 1 => Op(a, b) });

/// Expressions are not kept (only a view pattern of a pattern synonym has one): a module whose
/// interface needs one is not cached.
impl Bin for Expr {
    fn put(&self, w: &mut W) {
        w.ok = false;
    }
    fn get(_: &mut R) -> Option<Self> {
        None
    }
}

bin_struct!(Field { ty, strict });
bin_enum!(ConFields { 0 => Positional(a), 1 => Record(a) });
bin_struct!(ConDecl { name, exist, context, fields, gadt_result, infix, loc });
bin_enum!(Strategy { 0 => Stock, 1 => Newtype, 2 => Anyclass });
bin_struct!(Deriving { strategy, classes, via });
bin_struct!(DataDecl { newtype, context, name, tyvars, cons, deriving, loc });
bin_enum!(Assoc { 0 => Left, 1 => Right, 2 => None });

bin_struct!(GName { module, name });
bin_enum!(EntKind { 0 => PatSyn, 1 => Var, 2 => Con, 3 => Field, 4 => Method, 5 => TyCon, 6 => Class });
bin_struct!(Entity { kind, global, subs, fields });
bin_struct!(PatSynDef { params, names, pat, builder, arg_types });
bin_struct!(Iface { module, exports, fixities, patsyns });

bin_struct!(Pred { class, args });
bin_enum!(Ty {
    0 => Con(a), 1 => App(a, b), 2 => Meta(a), 3 => Skol(a, b), 4 => Gen(a), 5 => Qual(a, b), 6 => Bound(a), 7 => Forall(a, b, c),
});
bin_struct!(Scheme { nvars, names, preds, ty });
bin_enum!(Kind { 0 => Star, 1 => Arr(a, b), 2 => Meta(a), 3 => Gen(a) });
bin_struct!(TyConInfo { arity, cons });
bin_struct!(DataCon { name, tycon, tag, arity, scheme, fields, newtype, strict });
bin_struct!(Synonym { nparams, rhs });
bin_struct!(ClassInfo { name, nparams, supers, methods, defaults, default_schemes, dict_con, fundeps });
bin_struct!(InstInfo { class, nvars, ctx, head, dict });
bin_struct!(FamEqn { nvars, args, rhs, kinds });
bin_struct!(FamilyInfo { arity, closed, equations, injective });

bin_enum!(CLit { 0 => Integer(a), 1 => Int(a), 2 => Double(a), 3 => Frac(a), 4 => Char(a), 5 => Str(a), 6 => Addr(a) });
bin_enum!(Core {
    0 => Var(a), 1 => Lit(a), 2 => App(a, b), 3 => Lam(a, b), 4 => Let(a, b), 5 => Case(a, b, c), 6 => Con(a, b), 7 => Prim(a, b), 8 => Fail(a),
});
bin_enum!(Alt { 0 => Con(a, b, c), 1 => Lit(a, b), 2 => Default(a) });
bin_struct!(TopBind { name, body });

// ----- what a module adds ----------------------------------------------------------------

/// What the environment had before a module was checked: enough to tell its additions.
pub struct Snapshot {
    sizes: [usize; 9],
    family_eqns: HashMap<String, usize>,
}

/// The additions a module made to the environment.
#[derive(Default)]
pub struct EnvDelta {
    pub data_decls: Vec<(String, (String, DataDecl))>,
    pub families: Vec<(String, FamilyInfo)>,
    /// Equations added to families of other modules (`type instance`).
    pub family_eqns: Vec<(String, Vec<FamEqn>)>,
    pub tycons: Vec<(String, TyConInfo)>,
    pub synonyms: Vec<(String, Synonym)>,
    pub datacons: Vec<(String, DataCon)>,
    pub classes: Vec<(String, ClassInfo)>,
    pub instances: Vec<InstInfo>,
    pub values: Vec<(String, Scheme)>,
    pub fields: Vec<(String, Vec<(String, usize)>)>,
    pub kinds: Vec<(String, Kind)>,
}

bin_struct!(EnvDelta { data_decls, families, family_eqns, tycons, synonyms, datacons, classes, instances, values, fields, kinds });

/// The keys of a map before a module, to find the new ones after it.
pub struct Keys(Vec<std::collections::HashSet<String>>);

pub fn snapshot(env: &Env, kinds: &HashMap<String, Kind>) -> (Snapshot, Keys) {
    let keys = |m: Vec<&String>| m.into_iter().cloned().collect::<std::collections::HashSet<String>>();
    let k = Keys(vec![
        keys(env.data_decls.keys().collect()),
        keys(env.families.keys().collect()),
        keys(env.tycons.keys().collect()),
        keys(env.synonyms.keys().collect()),
        keys(env.datacons.keys().collect()),
        keys(env.classes.keys().collect()),
        keys(env.values.keys().collect()),
        keys(env.fields.keys().collect()),
        keys(kinds.keys().collect()),
    ]);
    let s = Snapshot {
        sizes: [env.instances.len(), 0, 0, 0, 0, 0, 0, 0, 0],
        family_eqns: env.families.iter().map(|(n, f)| (n.clone(), f.equations.len())).collect(),
    };
    (s, k)
}

/// The additions since the snapshot. Kinds are taken resolved (`resolve` prunes their metas).
pub fn delta(env: &Env, kinds: &HashMap<String, Kind>, snap: &Snapshot, keys: &Keys, resolve: impl Fn(&Kind) -> Kind) -> EnvDelta {
    fn new<V: Clone>(m: &HashMap<String, V>, old: &std::collections::HashSet<String>) -> Vec<(String, V)> {
        let mut v: Vec<(String, V)> = m.iter().filter(|(k, _)| !old.contains(*k)).map(|(k, v)| (k.clone(), v.clone())).collect();
        v.sort_by(|a, b| a.0.cmp(&b.0));
        v
    }
    let k = &keys.0;
    let mut family_eqns = Vec::new();
    for (name, f) in &env.families {
        if let Some(&before) = snap.family_eqns.get(name) {
            if f.equations.len() > before {
                family_eqns.push((name.clone(), f.equations[before..].to_vec()));
            }
        }
    }
    family_eqns.sort_by(|a, b| a.0.cmp(&b.0));
    EnvDelta {
        data_decls: new(&env.data_decls, &k[0]),
        families: new(&env.families, &k[1]),
        family_eqns,
        tycons: new(&env.tycons, &k[2]),
        synonyms: new(&env.synonyms, &k[3]),
        datacons: new(&env.datacons, &k[4]),
        classes: new(&env.classes, &k[5]),
        instances: env.instances[snap.sizes[0].min(env.instances.len())..].to_vec(),
        values: new(&env.values, &k[6]),
        fields: new(&env.fields, &k[7]),
        kinds: new(kinds, &k[8]).into_iter().map(|(n, kd)| (n, resolve(&kd))).collect(),
    }
}

/// Puts a module's additions into the environment.
pub fn apply(env: &mut Env, kinds: &mut HashMap<String, Kind>, d: EnvDelta) {
    env.data_decls.extend(d.data_decls);
    env.families.extend(d.families);
    for (name, eqns) in d.family_eqns {
        if let Some(f) = env.families.get_mut(&name) {
            f.equations.extend(eqns);
        }
    }
    env.tycons.extend(d.tycons);
    env.synonyms.extend(d.synonyms);
    env.datacons.extend(d.datacons);
    env.classes.extend(d.classes);
    env.instances.extend(d.instances);
    env.values.extend(d.values);
    env.fields.extend(d.fields);
    kinds.extend(d.kinds);
}

// ----- entries ---------------------------------------------------------------------------

const MAGIC: &str = "hsc-module-cache 1";

/// A cached module: the keys of the modules it imports (as they were), its interface, Core and
/// environment additions, and the extra state the driver keeps.
pub struct Entry {
    pub deps: Vec<(String, String)>,
    pub iface: Iface,
    pub binds: Vec<TopBind>,
    pub env: EnvDelta,
    pub explicit_sigs: Vec<String>,
}

bin_struct!(Entry { deps, iface, binds, env, explicit_sigs });

/// The directory of this compiler build's entries (created; other builds' directories removed
/// the first time).
pub fn dir() -> Option<std::path::PathBuf> {
    if std::env::var_os("HSC_NO_CACHE").is_some() {
        return None;
    }
    let base = crate::driver::cache_dir("modules");
    let id = compiler_id()?;
    let d = base.join(&id);
    if !d.is_dir() {
        std::fs::create_dir_all(&d).ok()?;
        // The cache keeps the entries of the two latest compiler builds (the release and the
        // development build are used in turn), so that it does not grow without bound.
        if let Ok(rd) = std::fs::read_dir(&base) {
            let mut others: Vec<(std::time::SystemTime, std::path::PathBuf)> = rd
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.is_dir() && *p != d)
                .filter_map(|p| Some((std::fs::metadata(&p).ok()?.modified().ok()?, p)))
                .collect();
            others.sort();
            others.reverse();
            for (_, p) in others.into_iter().skip(1) {
                let _ = std::fs::remove_dir_all(p);
            }
        }
    }
    Some(d)
}

/// The identity of the running compiler: its executable's size and modification time.
fn compiler_id() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    let md = std::fs::metadata(&exe).ok()?;
    let mtime = md.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_nanos();
    Some(crate::driver::md5_hex(format!("{}:{}:{}", exe.display(), md.len(), mtime).as_bytes()))
}

pub fn load(path: &std::path::Path) -> Option<Entry> {
    let bytes = std::fs::read(path).ok()?;
    let rest = bytes.strip_prefix(MAGIC.as_bytes())?;
    let mut r = R { bytes: rest, pos: 0 };
    let e = Entry::get(&mut r)?;
    (r.pos == rest.len()).then_some(e)
}

pub fn save(path: &std::path::Path, e: &Entry) {
    let mut w = W { out: MAGIC.as_bytes().to_vec(), ok: true };
    e.put(&mut w);
    if !w.ok {
        return;
    }
    let tmp = path.with_extension(format!("tmp{}", std::process::id()));
    if std::fs::write(&tmp, &w.out).is_ok() {
        let _ = std::fs::rename(&tmp, path);
    }
}

/// Substitutes the solved unification variables in every type of a module's additions.
/// Answers false when an unsolved one remains (the additions then mean nothing outside this
/// compilation, and are not saved).
pub fn zonk_delta(d: &mut EnvDelta, z: &dyn Fn(&Ty) -> Ty) -> bool {
    let mut ok = true;
    let mut ty = |t: &mut Ty| {
        *t = z(t);
        if has_meta(t) {
            ok = false;
        }
    };
    fn pred(p: &mut Pred, ty: &mut dyn FnMut(&mut Ty)) {
        p.args.iter_mut().for_each(|t| ty(t));
    }
    fn scheme(s: &mut Scheme, ty: &mut dyn FnMut(&mut Ty)) {
        s.preds.iter_mut().for_each(|p| pred(p, ty));
        ty(&mut s.ty);
    }
    for (_, f) in d.families.iter_mut() {
        for e in &mut f.equations {
            e.args.iter_mut().for_each(|t| ty(t));
            ty(&mut e.rhs);
        }
    }
    for (_, eqs) in d.family_eqns.iter_mut() {
        for e in eqs {
            e.args.iter_mut().for_each(|t| ty(t));
            ty(&mut e.rhs);
        }
    }
    for (_, s) in d.synonyms.iter_mut() {
        ty(&mut s.rhs);
    }
    for (_, c) in d.datacons.iter_mut() {
        scheme(&mut c.scheme, &mut ty);
    }
    for (_, c) in d.classes.iter_mut() {
        c.supers.iter_mut().for_each(|p| pred(p, &mut ty));
        c.methods.iter_mut().for_each(|(_, s)| scheme(s, &mut ty));
        c.default_schemes.values_mut().for_each(|s| scheme(s, &mut ty));
    }
    for i in d.instances.iter_mut() {
        i.ctx.iter_mut().for_each(|p| pred(p, &mut ty));
        i.head.iter_mut().for_each(|t| ty(t));
    }
    for (_, s) in d.values.iter_mut() {
        scheme(s, &mut ty);
    }
    ok
}

fn has_meta(t: &Ty) -> bool {
    match t {
        Ty::Meta(_) => true,
        Ty::App(f, a) => has_meta(f) || has_meta(a),
        Ty::Qual(ps, b) | Ty::Forall(_, ps, b) => ps.iter().any(|p| p.args.iter().any(has_meta)) || has_meta(b),
        _ => false,
    }
}
