//! Core-to-Core simplifications applied to whole programs.

use std::collections::HashMap;

use super::{Alt, Core, Lit, TopBind};

/// Replaces overloaded literals whose `Num`/`Fractional` dictionary is statically one of
/// the built-in instances with direct literals. The dictionary's evidence variable is
/// bound to the instance's global dictionary name, so the binding is found by name.
pub fn specialise_literals(binds: &mut [TopBind]) {
    let mut known: HashMap<String, String> = HashMap::new();
    for b in binds.iter() {
        collect_dict_aliases(&b.body, &mut known);
        if let Core::Var(g) = &b.body {
            if g.contains(".$f") {
                known.insert(b.name.clone(), g.clone());
            }
        }
    }
    for b in binds.iter_mut() {
        rewrite(&mut b.body, &known);
    }
}

fn dict_class(global: &str) -> Option<&str> {
    let rest = global.split_once(".$f")?.1;
    if rest.starts_with("NumInt_") {
        Some("Int")
    } else if rest.starts_with("NumInteger_") {
        Some("Integer")
    } else if rest.starts_with("NumDouble_") {
        Some("Double")
    } else if rest.starts_with("FractionalDouble_") {
        Some("FractionalDouble")
    } else {
        None
    }
}

fn collect_dict_aliases(c: &Core, known: &mut HashMap<String, String>) {
    match c {
        Core::Let(binds, body) => {
            for (n, b) in binds {
                if let Core::Var(g) = b {
                    if g.contains(".$f") {
                        known.insert(n.clone(), g.clone());
                    }
                }
                collect_dict_aliases(b, known);
            }
            collect_dict_aliases(body, known);
        }
        Core::App(f, a) => {
            collect_dict_aliases(f, known);
            collect_dict_aliases(a, known);
        }
        Core::Lam(_, b) => collect_dict_aliases(b, known),
        Core::Case(s, _, alts) => {
            collect_dict_aliases(s, known);
            for a in alts {
                match a {
                    Alt::Con(_, _, b) | Alt::Lit(_, b) | Alt::Default(b) => collect_dict_aliases(b, known),
                }
            }
        }
        Core::Con(_, args) | Core::Prim(_, args) => args.iter().for_each(|a| collect_dict_aliases(a, known)),
        _ => {}
    }
}

fn rewrite(c: &mut Core, known: &HashMap<String, String>) {
    // fromInteger d lit, with d a known dictionary
    if let Core::App(f, arg) = c {
        if let Core::App(g, dict) = f.as_ref() {
            if let (Core::Var(fname), Core::Var(ev)) = (g.as_ref(), dict.as_ref()) {
                if fname == "Base.fromInteger" {
                    if let (Some(cls), Core::Lit(Lit::Integer(s))) = (known.get(ev).and_then(|d| dict_class(d)), arg.as_ref()) {
                        match cls {
                            "Int" => {
                                if let Ok(v) = s.parse::<i64>() {
                                    *c = Core::Lit(Lit::Int(v));
                                    return;
                                }
                            }
                            "Integer" => {
                                *c = Core::Lit(Lit::Integer(s.clone()));
                                return;
                            }
                            "Double" => {
                                *c = Core::Lit(Lit::Double(s.parse::<f64>().unwrap_or(0.0)));
                                return;
                            }
                            _ => {}
                        }
                    }
                }
                if fname == "Base.fromRational" {
                    if let (Some("FractionalDouble"), Core::Prim(p, pargs)) =
                        (known.get(ev).and_then(|d| dict_class(d)), arg.as_ref())
                    {
                        if let ("ratioLit", [Core::Lit(Lit::Frac(s))]) = (p.as_str(), pargs.as_slice()) {
                            *c = Core::Lit(Lit::Double(s.parse::<f64>().unwrap_or(0.0)));
                            return;
                        }
                    }
                }
            }
        }
    }
    match c {
        Core::App(f, a) => {
            rewrite(f, known);
            rewrite(a, known);
        }
        Core::Lam(_, b) => rewrite(b, known),
        Core::Let(binds, body) => {
            for (_, b) in binds.iter_mut() {
                rewrite(b, known);
            }
            rewrite(body, known);
        }
        Core::Case(s, _, alts) => {
            rewrite(s, known);
            for a in alts.iter_mut() {
                match a {
                    Alt::Con(_, _, b) | Alt::Lit(_, b) | Alt::Default(b) => rewrite(b, known),
                }
            }
        }
        Core::Con(_, args) | Core::Prim(_, args) => args.iter_mut().for_each(|a| rewrite(a, known)),
        _ => {}
    }
}
