//! Operands of the inline `Double#` primitives that may be thunks (or updated thunks) are
//! evaluated first, natively: `prim .. v ..` becomes `case v as v' of _ -> prim .. v' ..`. The
//! inline path (`codegen::Gen::double_fast_path`) needs DOUBLE cells; a thunk operand would take
//! the runtime's primitive, which evaluates it by a nested call into the program.
//!
//! (Swapping case binders in general, `case x as b` making later uses of `x` read `b`, would do
//! the same for operands evaluated before, but hides a strict worker's raw parameters from the
//! numeric analysis: `loop !acc i` loses its unboxed loop.)

use std::collections::{HashMap, HashSet};

use crate::core::{Alt, Core, TopBind};

/// The primitives whose operands are evaluated here (those `codegen` computes inline).
fn strict_operands(prim: &str) -> bool {
    matches!(
        prim,
        "+##" | "-##" | "*##" | "/##" | "==##" | "/=##" | "<##" | "<=##" | ">##" | ">=##" | "sqrtDouble#"
            | "negateDouble#" | "fabsDouble#"
    )
}

pub fn evaluate_double_operands(binds: &mut [TopBind], strict: &HashMap<String, Vec<bool>>) {
    for b in binds {
        let mut cx = Cx { fresh: 0, evaluated: HashSet::new(), strict };
        cx.expr(&mut b.body);
    }
}

struct Cx<'a> {
    fresh: u64,
    /// Constructors whose wrapper evaluates some fields (`simplify::extract_strict_cons`): their
    /// pattern variables for those fields are values too.
    strict: &'a HashMap<String, Vec<bool>>,
    /// The case binders in scope: values. (A binder shadowed by a lambda or let of the same name
    /// is then taken for a value too, which only loses the native evaluation: the primitive's
    /// runtime path evaluates what it is given.)
    evaluated: HashSet<String>,
}

impl Cx<'_> {
    fn expr(&mut self, c: &mut Core) {
        match c {
            Core::Var(_) | Core::Lit(_) | Core::Fail(_) => {}
            Core::App(f, a) => {
                self.expr(f);
                self.expr(a);
            }
            Core::Lam(_, body) => self.expr(body),
            Core::Let(bs, body) => {
                for (_, rhs) in bs.iter_mut() {
                    self.expr(rhs);
                }
                self.expr(body);
            }
            Core::Case(s, b, alts) => {
                self.expr(s);
                let added = self.evaluated.insert(b.clone());
                for alt in alts.iter_mut() {
                    match alt {
                        Alt::Con(con, vars, body) => {
                            let mut fields = Vec::new();
                            if let Some(flags) = self.strict.get(con.as_str()) {
                                for (v, &f) in vars.iter().zip(flags) {
                                    if f && self.evaluated.insert(v.clone()) {
                                        fields.push(v.clone());
                                    }
                                }
                            }
                            self.expr(body);
                            for v in fields {
                                self.evaluated.remove(v.as_str());
                            }
                        }
                        Alt::Lit(_, body) | Alt::Default(body) => self.expr(body),
                    }
                }
                if added {
                    self.evaluated.remove(b.as_str());
                }
            }
            Core::Con(_, args) => args.iter_mut().for_each(|a| self.expr(a)),
            Core::Prim(name, args) => {
                args.iter_mut().for_each(|a| self.expr(a));
                if strict_operands(name) {
                    self.evaluate_operands(c);
                }
            }
        }
    }

    /// Wraps the primitive in a case of each operand variable that may be a thunk (once per
    /// variable), and makes it use the case binders.
    fn evaluate_operands(&mut self, c: &mut Core) {
        let Core::Prim(_, args) = c else { return };
        let mut wraps: Vec<(String, String)> = Vec::new();
        for a in args.iter_mut() {
            let Core::Var(v) = a else { continue };
            if let Some((_, nb)) = wraps.iter().find(|(w, _)| w == v) {
                *v = nb.clone();
                continue;
            }
            if self.evaluated.contains(v.as_str()) {
                continue;
            }
            self.fresh += 1;
            let nb = format!("{v}~ev{}", self.fresh);
            wraps.push((v.clone(), nb.clone()));
            *v = nb;
        }
        let mut out = std::mem::replace(c, Core::Fail(String::new()));
        for (v, nb) in wraps.into_iter().rev() {
            out = Core::Case(Box::new(Core::Var(v)), nb, vec![Alt::Default(out)]);
        }
        *c = out;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn var(x: &str) -> Core {
        Core::Var(x.into())
    }

    fn run(mut e: Core) -> Core {
        Cx { fresh: 0, evaluated: HashSet::new(), strict: &HashMap::new() }.expr(&mut e);
        e
    }

    #[test]
    fn double_operands_are_evaluated_once_each() {
        assert_eq!(
            run(Core::Prim("*##".into(), vec![var("d%1"), var("d%1")])),
            Core::Case(
                Box::new(var("d%1")),
                "d%1~ev1".into(),
                vec![Alt::Default(Core::Prim("*##".into(), vec![var("d%1~ev1"), var("d%1~ev1")]))],
            )
        );
    }

    #[test]
    fn case_binders_and_literals_are_used_as_they_are() {
        let e = Core::Case(
            Box::new(var("d%1")),
            "b%2".into(),
            vec![Alt::Default(Core::Prim("+##".into(), vec![var("b%2"), Core::Lit(crate::core::Lit::Double(1.0))]))],
        );
        assert_eq!(run(e.clone()), e);
    }

    #[test]
    fn strict_constructor_fields_are_values() {
        let body = Core::Prim("-##".into(), vec![var("x%1"), var("y%2")]);
        let e = Core::Case(
            Box::new(var("p%0")),
            "b%3".into(),
            vec![Alt::Con("T.P".into(), vec!["x%1".into(), "y%2".into()], body.clone())],
        );
        let strict: HashMap<String, Vec<bool>> = [("T.P".to_string(), vec![true, false])].into_iter().collect();
        let mut out = e.clone();
        Cx { fresh: 0, evaluated: HashSet::new(), strict: &strict }.expr(&mut out);
        // Only the lazy field is evaluated first.
        let expected = Core::Case(
            Box::new(var("p%0")),
            "b%3".into(),
            vec![Alt::Con(
                "T.P".into(),
                vec!["x%1".into(), "y%2".into()],
                Core::Case(Box::new(var("y%2")), "y%2~ev1".into(), vec![Alt::Default(Core::Prim("-##".into(), vec![var("x%1"), var("y%2~ev1")]))]),
            )],
        );
        assert_eq!(out, expected);
    }

    #[test]
    fn other_primitives_are_left_alone() {
        let e = Core::Prim("+#".into(), vec![var("i%1"), var("j%2")]);
        assert_eq!(run(e.clone()), e);
    }
}
