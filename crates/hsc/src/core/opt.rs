//! Core-to-Core simplifications applied to whole programs.

use std::collections::HashMap;

use super::{Alt, Core, Lit, TopBind};

/// Replaces overloaded literals whose `Num`/`Fractional` dictionary is statically one of
/// the built-in instances with direct literals. The dictionary's evidence variable is
/// bound to the instance's global dictionary name, so the binding is found by name.
pub fn specialise_literals(binds: &mut [TopBind]) {
    let mut known: HashMap<String, String> = HashMap::new();
    for b in binds.iter() {
        collect_aliases(&b.body, &mut known);
        if let Core::Var(g) = &b.body {
            known.insert(b.name.clone(), g.clone());
        }
    }
    for b in binds.iter_mut() {
        rewrite(&mut b.body, &known);
    }
}

fn dict_class(name: &str, aliases: &HashMap<String, String>) -> Option<&'static str> {
    let mut name = name;
    for _ in 0..16 {
        match aliases.get(name) {
            Some(next) => name = next,
            None => return builtin_dict_class(name),
        }
    }
    None
}

fn builtin_dict_class(global: &str) -> Option<&'static str> {
    let (module, rest) = global.split_once(".$f")?;
    let classes: &[(&str, &str)] = match module {
        "GHC.Internal.Num" => &[("NumInt_", "Int"), ("NumInteger_", "Integer")],
        "GHC.Internal.Float" => &[("NumDouble_", "Double"), ("FractionalDouble_", "FractionalDouble")],
        _ => return None,
    };
    for (prefix, class) in classes {
        if let Some(index) = rest.strip_prefix(prefix) {
            if !index.is_empty() && index.bytes().all(|b| b.is_ascii_digit()) {
                return Some(class);
            }
        }
    }
    None
}

fn rewrite(c: &mut Core, known: &HashMap<String, String>) {
    // fromInteger d lit, with d a known dictionary
    if let Core::App(f, arg) = c {
        if let Core::App(g, dict) = f.as_ref() {
            if let (Core::Var(fname), Core::Var(ev)) = (g.as_ref(), dict.as_ref()) {
                if fname == "GHC.Internal.Num.fromInteger" {
                    if let (Some(cls), Core::Lit(Lit::Integer(s))) = (dict_class(ev, known), arg.as_ref()) {
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
                                if let Ok(value) = s.parse::<f64>() {
                                    *c = Core::Lit(Lit::Double(value));
                                    return;
                                }
                            }
                            _ => {}
                        }
                    }
                }
                if fname == "GHC.Internal.Real.fromRational" {
                    if let (Some("FractionalDouble"), Core::Prim(p, pargs)) =
                        (dict_class(ev, known), arg.as_ref())
                    {
                        if let ("ratioLit", [Core::Lit(Lit::Frac(s))]) = (p.as_str(), pargs.as_slice()) {
                            if let Ok(value) = s.parse::<f64>() {
                                *c = Core::Lit(Lit::Double(value));
                                return;
                            }
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

/// Resolves method selections from known dictionaries. A selector applied to a constant
/// instance dictionary becomes a direct reference to that instance's method.
pub fn resolve_known_methods(binds: &mut [TopBind]) {
    // Method selectors: `\d -> case d of C .. x .. -> x` gives the field index.
    let mut selectors: HashMap<String, (String, usize)> = HashMap::new();
    // A constant dictionary can mix global methods with lazy default applications.
    // Record each global field independently; selecting it does not force the other fields.
    let mut dicts: HashMap<String, (String, Vec<Option<String>>)> = HashMap::new();
    // Aliases of globals (evidence bound to a dictionary, or a dictionary bound to another).
    let mut aliases: HashMap<String, String> = HashMap::new();
    for b in binds.iter() {
        collect_aliases(&b.body, &mut aliases);
        if let Core::Var(g) = &b.body {
            aliases.insert(b.name.clone(), g.clone());
        }
        if let Some(index) = selector_index(&b.body) {
            selectors.insert(b.name.clone(), index);
        }
        if let Core::Con(con, fields) = &b.body {
            if b.name.contains(".$f") {
                let globals = fields.iter().map(|field| match field {
                    Core::Var(name) => Some(name.clone()),
                    _ => None,
                }).collect();
                dicts.insert(b.name.clone(), (con.clone(), globals));
            }
        }
    }
    let resolve = |name: &str| -> Option<String> {
        let mut cur = name.to_string();
        for _ in 0..16 {
            match aliases.get(&cur) {
                Some(next) => cur = next.clone(),
                None => return Some(cur),
            }
        }
        None
    };
    for b in binds.iter_mut() {
        rewrite_selection(&mut b.body, &selectors, &dicts, &resolve);
    }
}

fn selector_index(c: &Core) -> Option<(String, usize)> {
    if let Core::Lam(d, body) = c {
        if let Core::Case(scrut, _, alts) = body.as_ref() {
            if matches!(scrut.as_ref(), Core::Var(v) if v == d) {
                if let [Alt::Con(con, vars, Core::Var(field))] = alts.as_slice() {
                    return vars.iter().position(|v| v == field).map(|index| (con.clone(), index));
                }
            }
        }
    }
    None
}

fn collect_aliases(c: &Core, aliases: &mut HashMap<String, String>) {
    match c {
        Core::Let(binds, body) => {
            for (n, b) in binds {
                if let Core::Var(g) = b {
                    aliases.insert(n.clone(), g.clone());
                }
                collect_aliases(b, aliases);
            }
            collect_aliases(body, aliases);
        }
        Core::App(f, a) => {
            collect_aliases(f, aliases);
            collect_aliases(a, aliases);
        }
        Core::Lam(_, b) => collect_aliases(b, aliases),
        Core::Case(s, _, alts) => {
            collect_aliases(s, aliases);
            for a in alts {
                match a {
                    Alt::Con(_, _, b) | Alt::Lit(_, b) | Alt::Default(b) => collect_aliases(b, aliases),
                }
            }
        }
        Core::Con(_, args) | Core::Prim(_, args) => args.iter().for_each(|a| collect_aliases(a, aliases)),
        _ => {}
    }
}

fn rewrite_selection(
    c: &mut Core,
    selectors: &HashMap<String, (String, usize)>,
    dicts: &HashMap<String, (String, Vec<Option<String>>)>,
    resolve: &dyn Fn(&str) -> Option<String>,
) {
    match c {
        Core::App(f, a) => {
            rewrite_selection(f, selectors, dicts, resolve);
            rewrite_selection(a, selectors, dicts, resolve);
            if let (Core::Var(sel), Core::Var(d)) = (f.as_ref(), a.as_ref()) {
                if let (Some((expected_con, index)), Some(dict)) = (selectors.get(sel), resolve(d)) {
                    if let Some((actual_con, fields)) = dicts.get(&dict) {
                        if expected_con == actual_con {
                            if let Some(Some(method)) = fields.get(*index) {
                                *c = Core::Var(method.clone());
                            }
                        }
                    }
                }
            }
        }
        Core::Lam(_, b) => rewrite_selection(b, selectors, dicts, resolve),
        Core::Let(binds, body) => {
            for (_, b) in binds.iter_mut() {
                rewrite_selection(b, selectors, dicts, resolve);
            }
            rewrite_selection(body, selectors, dicts, resolve);
        }
        Core::Case(s, _, alts) => {
            rewrite_selection(s, selectors, dicts, resolve);
            for a in alts.iter_mut() {
                match a {
                    Alt::Con(_, _, b) | Alt::Lit(_, b) | Alt::Default(b) => rewrite_selection(b, selectors, dicts, resolve),
                }
            }
        }
        Core::Con(_, args) | Core::Prim(_, args) => {
            args.iter_mut().for_each(|a| rewrite_selection(a, selectors, dicts, resolve))
        }
        _ => {}
    }
}

/// A foreign primitive is a global `\p1 .. pn -> Prim(name, [p1 .. pn])`. A saturated call
/// becomes the primitive applied directly to its argument expressions, so that `seq a b`
/// keeps `b` in tail position instead of returning it through a parameter variable.
pub fn inline_prim_wrappers(binds: &mut [TopBind]) {
    let mut wrappers: HashMap<String, (usize, String)> = HashMap::new();
    for b in binds.iter() {
        let mut names = Vec::new();
        let mut body = &b.body;
        while let Core::Lam(x, inner) = body {
            names.push(x.clone());
            body = inner;
        }
        if let Core::Prim(p, args) = body {
            let direct = !names.is_empty()
                && args.len() == names.len()
                && args.iter().zip(&names).all(|(a, n)| matches!(a, Core::Var(v) if v == n));
            if direct {
                wrappers.insert(b.name.clone(), (names.len(), p.clone()));
            }
        }
    }
    if wrappers.is_empty() {
        return;
    }
    for b in binds.iter_mut() {
        inline_calls(&mut b.body, &wrappers);
    }
}

fn inline_calls(c: &mut Core, wrappers: &HashMap<String, (usize, String)>) {
    match c {
        Core::App(f, a) => {
            inline_calls(f, wrappers);
            inline_calls(a, wrappers);
        }
        Core::Lam(_, b) => inline_calls(b, wrappers),
        Core::Let(binds, body) => {
            for (_, b) in binds.iter_mut() {
                inline_calls(b, wrappers);
            }
            inline_calls(body, wrappers);
        }
        Core::Case(s, _, alts) => {
            inline_calls(s, wrappers);
            for a in alts.iter_mut() {
                match a {
                    Alt::Con(_, _, b) | Alt::Lit(_, b) | Alt::Default(b) => inline_calls(b, wrappers),
                }
            }
        }
        Core::Con(_, args) | Core::Prim(_, args) => args.iter_mut().for_each(|a| inline_calls(a, wrappers)),
        _ => {}
    }
    // A saturated call of a wrapper: `w a1 .. an` with the wrapper's arity n.
    let mut n = 0;
    let mut head = &*c;
    while let Core::App(f, _) = head {
        n += 1;
        head = f;
    }
    let Core::Var(name) = head else { return };
    let Some((arity, prim)) = wrappers.get(name) else { return };
    if *arity != n {
        return;
    }
    let mut args = Vec::with_capacity(n);
    let mut cur = std::mem::replace(c, Core::Fail(String::new()));
    while let Core::App(f, a) = cur {
        args.push(*a);
        cur = *f;
    }
    args.reverse();
    *c = Core::Prim(prim.clone(), args);
}

#[cfg(test)]
mod literal_tests {
    use super::*;

    fn integer(function: &str, dictionary: &str, text: &str) -> Core {
        Core::App(Box::new(Core::App(Box::new(Core::Var(function.into())), Box::new(Core::Var(dictionary.into())))),
                  Box::new(Core::Lit(Lit::Integer(text.into()))))
    }

    fn fractional(dictionary: &str, text: &str) -> Core {
        Core::App(Box::new(Core::App(Box::new(Core::Var("GHC.Internal.Real.fromRational".into())), Box::new(Core::Var(dictionary.into())))),
                  Box::new(Core::Prim("ratioLit".into(), vec![Core::Lit(Lit::Frac(text.into()))])))
    }

    fn optimise(body: Core, aliases: &[(&str, &str)]) -> Core {
        let mut binds: Vec<_> = aliases.iter().map(|(name, target)| TopBind { name: (*name).into(), body: Core::Var((*target).into()) }).collect();
        binds.push(TopBind { name: "Main.value".into(), body });
        specialise_literals(&mut binds);
        binds.pop().unwrap().body
    }

    #[test]
    fn standard_literals_specialise_through_dictionary_alias_chains() {
        let body = Core::Let(vec![("d0".into(), Core::Var("GHC.Internal.Num.$fNumInt_1".into())),
                                  ("d1".into(), Core::Var("d0".into())), ("d2".into(), Core::Var("d1".into()))],
                             Box::new(integer("GHC.Internal.Num.fromInteger", "d2", "7")));
        let Core::Let(_, body) = optimise(body, &[]) else { panic!() };
        assert_eq!(*body, Core::Lit(Lit::Int(7)));
        assert_eq!(optimise(integer("GHC.Internal.Num.fromInteger", "GHC.Internal.Num.$fNumInt_1", "-9"), &[]), Core::Lit(Lit::Int(-9)));
        assert_eq!(optimise(integer("GHC.Internal.Num.fromInteger", "GHC.Internal.Num.$fNumInteger_2", "123456789012345678901234567890"), &[]),
                   Core::Lit(Lit::Integer("123456789012345678901234567890".into())));
        assert_eq!(optimise(integer("GHC.Internal.Num.fromInteger", "GHC.Internal.Float.$fNumDouble_8", "9007199254740993"), &[]),
                   Core::Lit(Lit::Double(9007199254740992.0)));
        assert_eq!(optimise(integer("GHC.Internal.Num.fromInteger", "GHC.Internal.Float.$fNumDouble_8", "36893488147419107329"), &[]),
                   Core::Lit(Lit::Double(36893488147419111424.0)));
        assert_eq!(optimise(fractional("d", "0.125"), &[("d", "GHC.Internal.Float.$fFractionalDouble_9")]), Core::Lit(Lit::Double(0.125)));
    }

    #[test]
    fn custom_dictionaries_functions_and_cycles_keep_their_calls() {
        for dictionary in ["Custom.$fNumInt_1", "Base.$fNumInt_1", "GHC.Internal.Real.$fNumInt_1",
                           "GHC.Internal.Num.$fNumInt_", "GHC.Internal.Num.$fNumInt_1%mi%2", "GHC.Internal.Num.$fNumInt_١"] {
            let call = integer("GHC.Internal.Num.fromInteger", dictionary, "7");
            assert_eq!(optimise(call.clone(), &[]), call, "{dictionary}");
        }
        let custom = integer("Custom.fromInteger", "GHC.Internal.Num.$fNumInt_1", "7");
        assert_eq!(optimise(custom.clone(), &[]), custom);
        let base = integer("Base.fromInteger", "GHC.Internal.Num.$fNumInt_1", "7");
        assert_eq!(optimise(base.clone(), &[]), base);
        let base_fraction = Core::App(Box::new(Core::App(Box::new(Core::Var("Base.fromRational".into())),
                                                        Box::new(Core::Var("GHC.Internal.Float.$fFractionalDouble_9".into())))),
                                      Box::new(Core::Prim("ratioLit".into(), vec![Core::Lit(Lit::Frac("0.5".into()))])));
        assert_eq!(optimise(base_fraction.clone(), &[]), base_fraction);
        let cycle = integer("GHC.Internal.Num.fromInteger", "GHC.Internal.Num.$fNumInt_1", "7");
        assert_eq!(optimise(cycle.clone(), &[("GHC.Internal.Num.$fNumInt_1", "d"), ("d", "GHC.Internal.Num.$fNumInt_1")]), cycle);
        assert_eq!(optimise(cycle.clone(), &[("GHC.Internal.Num.$fNumInt_1", "GHC.Internal.Err.undefined")]), cycle);
    }

    #[test]
    fn integer_overflow_failed_parses_and_double_edge_values_are_preserved() {
        for text in ["9223372036854775808", "-9223372036854775809", "not-an-integer"] {
            let call = integer("GHC.Internal.Num.fromInteger", "GHC.Internal.Num.$fNumInt_1", text);
            assert_eq!(optimise(call.clone(), &[]), call);
        }
        for call in [integer("GHC.Internal.Num.fromInteger", "GHC.Internal.Float.$fNumDouble_8", "bad"),
                     fractional("GHC.Internal.Float.$fFractionalDouble_9", "bad")] {
            assert_eq!(optimise(call.clone(), &[]), call);
        }
        let Core::Lit(Lit::Double(zero)) = optimise(fractional("GHC.Internal.Float.$fFractionalDouble_9", "-0.0"), &[]) else { panic!() };
        assert_eq!(zero.to_bits(), (-0.0f64).to_bits());
        let Core::Lit(Lit::Double(tiny)) = optimise(fractional("GHC.Internal.Float.$fFractionalDouble_9", "5e-324"), &[]) else { panic!() };
        assert_eq!(tiny.to_bits(), 1);
    }
}

#[cfg(test)]
mod method_selection_tests {
    use super::*;

    fn selector(con: &str, index: usize) -> Core {
        let fields = vec!["first%1".to_string(), "second%2".to_string()];
        Core::Lam("dict%3".into(), Box::new(Core::Case(
            Box::new(Core::Var("dict%3".into())), "case%4".into(),
            vec![Alt::Con(con.into(), fields.clone(), Core::Var(fields[index].clone()))],
        )))
    }

    fn call(method: &str, dict: &str) -> Core {
        Core::App(Box::new(Core::Var(method.into())), Box::new(Core::Var(dict.into())))
    }

    #[test]
    fn global_field_resolves_beside_lazy_default_and_bottom_fields() {
        for other in [call("Test.default", "Test.$fMixed_1"), Core::Fail("unused".into())] {
            let original_default = call("Test.second", "Test.$fMixed_1");
            let mut binds = vec![
                TopBind { name: "Test.first".into(), body: selector("Test.D:Mixed", 0) },
                TopBind { name: "Test.second".into(), body: selector("Test.D:Mixed", 1) },
                TopBind { name: "Test.$fMixed_1".into(), body: Core::Con("Test.D:Mixed".into(), vec![Core::Var("Test.fast".into()), other]) },
                TopBind { name: "Test.alias".into(), body: Core::Var("Test.$fMixed_1".into()) },
                TopBind { name: "Test.use".into(), body: call("Test.first", "Test.alias") },
                TopBind { name: "Test.useDefault".into(), body: original_default.clone() },
            ];
            resolve_known_methods(&mut binds);
            assert!(matches!(&binds[4].body, Core::Var(name) if name == "Test.fast"));
            assert_eq!(binds[5].body, original_default);
        }
    }

    #[test]
    fn nonzero_field_after_a_superclass_resolves_without_forcing_other_fields() {
        let mut binds = vec![
            TopBind { name: "Test.second".into(), body: selector("Test.D:Mixed", 1) },
            TopBind { name: "Test.$fMixed_1".into(), body: Core::Con("Test.D:Mixed".into(), vec![Core::Fail("superclass is unused".into()), Core::Var("Test.fast".into())]) },
            TopBind { name: "Test.use".into(), body: call("Test.second", "Test.$fMixed_1") },
        ];
        resolve_known_methods(&mut binds);
        assert_eq!(binds[2].body, Core::Var("Test.fast".into()));
    }

    #[test]
    fn constructor_mismatches_cycles_and_noninstance_values_stay_unresolved() {
        for (dict_name, body) in [
            ("Test.$fWrong", Core::Con("Other.D:Mixed".into(), vec![Core::Var("wrong".into())])),
            ("Test.$fCycle", Core::Var("Test.$fCycle".into())),
            ("Test.value", Core::Con("Test.D:Mixed".into(), vec![Core::Var("ordinary".into())])),
        ] {
            let original = call("Test.first", dict_name);
            let mut binds = vec![
                TopBind { name: "Test.first".into(), body: selector("Test.D:Mixed", 0) },
                TopBind { name: dict_name.into(), body },
                TopBind { name: "Test.use".into(), body: original.clone() },
            ];
            resolve_known_methods(&mut binds);
            assert_eq!(binds[2].body, original);
        }
    }
}
