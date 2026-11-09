//! Operator precedence resolution, following Haskell 2010 report section 10.6.
//!
//! An infix expression arrives as a flat sequence of operands, operators and prefix
//! minus signs. `resolve` rebuilds the tree according to the fixities.

use crate::syntax::ast::{Assoc, Loc};
use std::collections::VecDeque;

/// One element of an unresolved infix expression.
pub enum Tk<T, O> {
    Operand(T),
    Op {
        op: O,
        name: String,
        assoc: Assoc,
        prec: u8,
        loc: Loc,
    },
    Neg(Loc),
}

#[derive(Debug)]
pub struct FixityError {
    pub loc: Loc,
    pub msg: String,
}

struct Ctx {
    assoc: Assoc,
    prec: u8,
    name: String,
}

fn show_fixity(name: &str, assoc: Assoc, prec: u8) -> String {
    let kw = match assoc {
        Assoc::Left => "infixl",
        Assoc::Right => "infixr",
        Assoc::None => "infix",
    };
    format!("‘{name}’ [{kw} {prec}]")
}

/// Resolves an infix chain. `bin` builds a binary node from an operator; `neg` builds a
/// prefix-minus node.
pub fn resolve<T, O>(
    items: Vec<Tk<T, O>>,
    bin: &impl Fn(O, Loc, T, T) -> T,
    neg: &impl Fn(Loc, T) -> T,
) -> Result<T, FixityError> {
    let mut toks: VecDeque<Tk<T, O>> = items.into();
    let e = parse_neg(None, &mut toks, bin, neg)?;
    match toks.front() {
        None => Ok(e),
        Some(Tk::Op { loc, .. }) | Some(Tk::Neg(loc)) => Err(FixityError {
            loc: *loc,
            msg: "parse error in infix expression".into(),
        }),
        Some(Tk::Operand(_)) => Err(FixityError {
            loc: Loc::default(),
            msg: "parse error in infix expression".into(),
        }),
    }
}

fn parse_neg<T, O>(
    op1: Option<&Ctx>,
    toks: &mut VecDeque<Tk<T, O>>,
    bin: &impl Fn(O, Loc, T, T) -> T,
    neg: &impl Fn(Loc, T) -> T,
) -> Result<T, FixityError> {
    match toks.pop_front() {
        Some(Tk::Operand(e)) => parse1(op1, e, toks, bin, neg),
        Some(Tk::Neg(loc)) => {
            if let Some(c) = op1 {
                if c.prec >= 6 {
                    return Err(FixityError {
                        loc,
                        msg: format!(
                            "Precedence parsing error\n    cannot mix prefix ‘-’ [infixl 6] and {} in the same infix expression",
                            show_fixity(&c.name, c.assoc, c.prec)
                        ),
                    });
                }
            }
            let minus = Ctx {
                assoc: Assoc::Left,
                prec: 6,
                name: "-".into(),
            };
            let r = parse_neg(Some(&minus), toks, bin, neg)?;
            parse1(op1, neg(loc, r), toks, bin, neg)
        }
        Some(Tk::Op { loc, .. }) => Err(FixityError {
            loc,
            msg: "parse error: operator without a left operand".into(),
        }),
        None => Err(FixityError {
            loc: Loc::default(),
            msg: "parse error: missing operand".into(),
        }),
    }
}

fn parse1<T, O>(
    op1: Option<&Ctx>,
    mut e1: T,
    toks: &mut VecDeque<Tk<T, O>>,
    bin: &impl Fn(O, Loc, T, T) -> T,
    neg: &impl Fn(Loc, T) -> T,
) -> Result<T, FixityError> {
    loop {
        let (name2, assoc2, prec2, loc2) = match toks.front() {
            Some(Tk::Op {
                name,
                assoc,
                prec,
                loc,
                ..
            }) => (name.clone(), *assoc, *prec, *loc),
            _ => return Ok(e1),
        };
        if let Some(c) = op1 {
            if c.prec == prec2 && (c.assoc != assoc2 || assoc2 == Assoc::None) {
                return Err(FixityError {
                    loc: loc2,
                    msg: format!(
                        "Precedence parsing error\n    cannot mix {} and {} in the same infix expression",
                        show_fixity(&c.name, c.assoc, c.prec),
                        show_fixity(&name2, assoc2, prec2)
                    ),
                });
            }
            if c.prec > prec2 || (c.prec == prec2 && c.assoc == Assoc::Left) {
                return Ok(e1);
            }
        }
        let Some(Tk::Op { op, loc, .. }) = toks.pop_front() else {
            unreachable!("front was an operator");
        };
        let ctx = Ctx {
            assoc: assoc2,
            prec: prec2,
            name: name2,
        };
        let r = parse_neg(Some(&ctx), toks, bin, neg)?;
        e1 = bin(op, loc, e1, r);
    }
}
