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

/// Remove local bindings that only forward to another variable. The target computation remains
/// shared, and recursive alias cycles retain their original thunks.
pub fn eliminate_alias_thunks(binds: &mut [TopBind]) {
    use std::collections::HashSet;
    let globals: HashSet<String> = binds.iter().map(|b| b.name.clone()).collect();
    for bind in binds {
        let mut names = HashSet::new();
        let mut aliases = HashMap::new();
        // Some matcher alternatives clone binders. Skip those bodies rather than risk capture
        // when a name is reused in another scope (including a shadowed global).
        if !local_aliases(&bind.body, &globals, &mut names, &mut aliases) || aliases.is_empty() {
            continue;
        }
        let mut resolved = HashMap::new();
        for name in aliases.keys() {
            let mut target = name.as_str();
            for _ in 0..=16 {
                match aliases.get(target) {
                    Some(next) => target = next,
                    None => {
                        resolved.insert(name.clone(), target.to_string());
                        break;
                    }
                }
            }
        }
        drop_aliases(&mut bind.body, &resolved);
    }
}

fn local_aliases(
    c: &Core,
    globals: &std::collections::HashSet<String>,
    names: &mut std::collections::HashSet<String>,
    aliases: &mut HashMap<String, String>,
) -> bool {
    let bind = |name: &String, names: &mut std::collections::HashSet<String>| {
        !globals.contains(name) && names.insert(name.clone())
    };
    match c {
        Core::Var(_) | Core::Lit(_) | Core::Fail(_) => true,
        Core::App(f, a) => local_aliases(f, globals, names, aliases) && local_aliases(a, globals, names, aliases),
        Core::Lam(name, body) => bind(name, names) && local_aliases(body, globals, names, aliases),
        Core::Let(bindings, body) => {
            for (name, rhs) in bindings {
                if !bind(name, names) {
                    return false;
                }
                if let Core::Var(target) = rhs {
                    aliases.insert(name.clone(), target.clone());
                }
            }
            bindings.iter().all(|(_, rhs)| local_aliases(rhs, globals, names, aliases))
                && local_aliases(body, globals, names, aliases)
        }
        Core::Case(scrut, binder, alts) => {
            local_aliases(scrut, globals, names, aliases) && bind(binder, names)
                && alts.iter().all(|alt| match alt {
                    Alt::Con(_, vars, body) => vars.iter().all(|name| bind(name, names))
                        && local_aliases(body, globals, names, aliases),
                    Alt::Lit(_, body) | Alt::Default(body) => local_aliases(body, globals, names, aliases),
                })
        }
        Core::Con(_, args) | Core::Prim(_, args) => args.iter().all(|a| local_aliases(a, globals, names, aliases)),
    }
}

fn drop_aliases(c: &mut Core, resolved: &HashMap<String, String>) {
    match c {
        Core::Var(name) => {
            if let Some(target) = resolved.get(name) {
                *name = target.clone();
            }
        }
        Core::Lit(_) | Core::Fail(_) => {}
        Core::App(f, a) => {
            drop_aliases(f, resolved);
            drop_aliases(a, resolved);
        }
        Core::Lam(_, body) => drop_aliases(body, resolved),
        Core::Let(bindings, body) => {
            bindings.retain(|(name, _)| !resolved.contains_key(name));
            for (_, rhs) in bindings.iter_mut() {
                drop_aliases(rhs, resolved);
            }
            drop_aliases(body, resolved);
            if bindings.is_empty() {
                let Core::Let(_, body) = std::mem::replace(c, Core::Fail(String::new())) else { unreachable!() };
                *c = *body;
            }
        }
        Core::Case(scrut, _, alts) => {
            drop_aliases(scrut, resolved);
            for alt in alts {
                match alt {
                    Alt::Con(_, _, body) | Alt::Lit(_, body) | Alt::Default(body) => drop_aliases(body, resolved),
                }
            }
        }
        Core::Con(_, args) | Core::Prim(_, args) => args.iter_mut().for_each(|a| drop_aliases(a, resolved)),
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

#[cfg(test)]
mod alias_thunk_tests {
    use super::*;

    fn var(name: &str) -> Core { Core::Var(name.into()) }
    fn program(body: Core) -> Vec<TopBind> {
        vec![TopBind { name: "Test.value".into(), body: Core::Lit(crate::core::Lit::Int(42)) },
             TopBind { name: "Test.use".into(), body }]
    }

    #[test]
    fn aliases_redirect_uses_without_duplicating_the_shared_computation() {
        let computation = Core::Prim("+#".into(), vec![var("Test.value"), var("Test.value")]);
        let mut binds = program(Core::Let(vec![
            ("shared%1".into(), computation.clone()),
            ("alias%2".into(), var("shared%1")),
            ("alias%3".into(), var("alias%2")),
            ("result%4".into(), Core::Prim("*#".into(), vec![var("alias%3"), var("alias%2")])),
        ], Box::new(Core::Lam("arg%5".into(), Box::new(var("result%4"))))));
        eliminate_alias_thunks(&mut binds);
        crate::core::lint::lint_program(&binds).unwrap();
        let Core::Let(kept, _) = &binds[1].body else { panic!("shared computation must remain") };
        assert_eq!(kept.len(), 2);
        assert_eq!(kept[0].1, computation);
        assert_eq!(kept[1].1, Core::Prim("*#".into(), vec![var("shared%1"), var("shared%1")]));
    }

    #[test]
    fn alias_substitution_keeps_case_forcing_and_lambda_capture() {
        let mut binds = program(Core::Lam("outer%1".into(), Box::new(Core::Let(
            vec![("alias%2".into(), var("outer%1"))],
            Box::new(Core::Case(Box::new(var("alias%2")), "case%3".into(),
                vec![Alt::Default(Core::Lam("inner%4".into(), Box::new(var("alias%2"))))])),
        ))));
        eliminate_alias_thunks(&mut binds);
        crate::core::lint::lint_program(&binds).unwrap();
        let Core::Lam(_, body) = &binds[1].body else { panic!("lambda") };
        let Core::Case(scrut, _, alts) = body.as_ref() else { panic!("case must remain strict") };
        assert_eq!(scrut.as_ref(), &var("outer%1"));
        assert_eq!(alts.as_slice(), &[Alt::Default(Core::Lam("inner%4".into(), Box::new(var("outer%1"))))]);
    }

    #[test]
    fn recursive_cycles_and_shadowed_names_are_preserved() {
        for body in [
            Core::Let(vec![("x%1".into(), var("y%2")), ("y%2".into(), var("x%1"))], Box::new(var("x%1"))),
            Core::Lam("x%1".into(), Box::new(Core::Let(vec![("alias%2".into(), var("x%1"))], Box::new(Core::Lam("x%1".into(), Box::new(var("alias%2"))))))),
            Core::Let(vec![("alias%1".into(), var("Test.value"))], Box::new(Core::Lam("Test.value".into(), Box::new(var("alias%1"))))),
        ] {
            let mut binds = program(body.clone());
            eliminate_alias_thunks(&mut binds);
            assert_eq!(binds[1].body, body);
            crate::core::lint::lint_program(&binds).unwrap();
        }
    }

    #[test]
    fn bounded_resolution_never_leaves_a_dangling_alias_in_long_chains() {
        let mut aliases: Vec<(String, Core)> = (0..40).map(|i| (format!("alias%{i}"), var(&format!("alias%{}", i + 1)))).collect();
        aliases.push(("alias%40".into(), var("Test.value")));
        let mut binds = program(Core::Let(aliases, Box::new(var("alias%0"))));
        eliminate_alias_thunks(&mut binds);
        crate::core::lint::lint_program(&binds).unwrap();
    }
}

/// Simplifies each binding: a non-recursive local binding that is used exactly once, outside
/// any lambda within its scope, moves to its use (no work is duplicated, and a guard's
/// fall-through no longer becomes a thunk entered in tail position); a case of a global that is
/// a known nullary constructor (`otherwise`) selects its alternative. Bodies whose binders are
/// not unique (some matcher alternatives clone them) are left alone, to rule out capture.
pub fn simplify(binds: &mut [TopBind]) {
    use std::collections::HashSet;
    let globals: HashSet<String> = binds.iter().map(|b| b.name.clone()).collect();
    let mut known: HashMap<String, String> = HashMap::new();
    let direct: HashMap<&str, &Core> = binds.iter().map(|b| (b.name.as_str(), &b.body)).collect();
    for b in binds.iter() {
        let mut body = &b.body;
        for _ in 0..8 {
            match body {
                Core::Var(v) => match direct.get(v.as_str()) {
                    Some(next) => body = next,
                    None => break,
                },
                Core::Con(con, args) if args.is_empty() => {
                    known.insert(b.name.clone(), con.clone());
                    break;
                }
                _ => break,
            }
        }
    }
    for bind in binds.iter_mut() {
        let mut names = HashSet::new();
        if !unique_binders(&bind.body, &globals, &mut names) {
            continue;
        }
        for _ in 0..4 {
            let mut occ = HashMap::new();
            let mut depth = HashMap::new();
            occurrences(&bind.body, 0, &mut depth, &mut occ);
            let once: HashSet<String> = occ
                .into_iter()
                .filter(|(_, o): &(String, Occ)| o.count == 1 && !o.under_lambda && !o.recursive)
                .map(|(n, _)| n)
                .collect();
            let mut pending = HashMap::new();
            let mut changed = false;
            inline_once(&mut bind.body, &once, &mut pending, &known, &mut changed);
            if !changed {
                break;
            }
        }
    }
}

#[derive(Default)]
struct Occ {
    count: u32,
    under_lambda: bool,
    recursive: bool,
}

fn unique_binders(c: &Core, globals: &std::collections::HashSet<String>, names: &mut std::collections::HashSet<String>) -> bool {
    fn bind(name: &String, globals: &std::collections::HashSet<String>, names: &mut std::collections::HashSet<String>) -> bool {
        !globals.contains(name) && names.insert(name.clone())
    }
    match c {
        Core::Var(_) | Core::Lit(_) | Core::Fail(_) => true,
        Core::App(f, a) => unique_binders(f, globals, names) && unique_binders(a, globals, names),
        Core::Lam(name, body) => bind(name, globals, names) && unique_binders(body, globals, names),
        Core::Let(bindings, body) => {
            bindings.iter().all(|(name, _)| bind(name, globals, names))
                && bindings.iter().all(|(_, rhs)| unique_binders(rhs, globals, names))
                && unique_binders(body, globals, names)
        }
        Core::Case(scrut, binder, alts) => {
            unique_binders(scrut, globals, names)
                && bind(binder, globals, names)
                && alts.iter().all(|alt| match alt {
                    Alt::Con(_, vars, body) => {
                        vars.iter().all(|v| bind(v, globals, names))
                            && unique_binders(body, globals, names)
                    }
                    Alt::Lit(_, body) | Alt::Default(body) => unique_binders(body, globals, names),
                })
        }
        Core::Con(_, args) | Core::Prim(_, args) => args.iter().all(|a| unique_binders(a, globals, names)),
    }
}

/// Counts the uses of each let-bound local: `depth` maps a let-bound name to the lambda depth of
/// its binding while it is in scope, with a negative depth while its own right-hand side is walked.
fn occurrences(c: &Core, lam: i32, depth: &mut HashMap<String, (i32, bool)>, occ: &mut HashMap<String, Occ>) {
    match c {
        Core::Var(v) => {
            if let Some(&(d, in_rhs)) = depth.get(v) {
                let o = occ.entry(v.clone()).or_default();
                o.count += 1;
                o.under_lambda |= lam > d;
                o.recursive |= in_rhs;
            }
        }
        Core::Lit(_) | Core::Fail(_) => {}
        Core::App(f, a) => {
            occurrences(f, lam, depth, occ);
            occurrences(a, lam, depth, occ);
        }
        Core::Lam(_, b) => occurrences(b, lam + 1, depth, occ),
        Core::Let(bindings, body) => {
            for (name, _) in bindings {
                depth.insert(name.clone(), (lam, false));
                occ.entry(name.clone()).or_default();
            }
            for (name, rhs) in bindings {
                depth.insert(name.clone(), (lam, true));
                occurrences(rhs, lam, depth, occ);
                depth.insert(name.clone(), (lam, false));
            }
            occurrences(body, lam, depth, occ);
        }
        Core::Case(s, _, alts) => {
            occurrences(s, lam, depth, occ);
            for a in alts {
                match a {
                    Alt::Con(_, _, b) | Alt::Lit(_, b) | Alt::Default(b) => occurrences(b, lam, depth, occ),
                }
            }
        }
        Core::Con(_, args) | Core::Prim(_, args) => args.iter().for_each(|a| occurrences(a, lam, depth, occ)),
    }
}

/// Moves the bindings named in `once` to their single use, and selects the alternative of a
/// case on a known nullary constructor.
fn inline_once(
    c: &mut Core,
    once: &std::collections::HashSet<String>,
    pending: &mut HashMap<String, Core>,
    known: &HashMap<String, String>,
    changed: &mut bool,
) {
    match c {
        Core::Var(v) => {
            if let Some(e) = pending.remove(v.as_str()) {
                *c = e;
                *changed = true;
                inline_once(c, once, pending, known, changed);
            }
        }
        Core::Lit(_) | Core::Fail(_) => {}
        Core::App(f, a) => {
            inline_once(f, once, pending, known, changed);
            inline_once(a, once, pending, known, changed);
        }
        Core::Lam(_, b) => inline_once(b, once, pending, known, changed),
        Core::Let(bindings, body) => {
            let mut kept = Vec::with_capacity(bindings.len());
            for (name, rhs) in std::mem::take(bindings) {
                if once.contains(&name) {
                    pending.insert(name, rhs);
                } else {
                    kept.push((name, rhs));
                }
            }
            for (_, rhs) in kept.iter_mut() {
                inline_once(rhs, once, pending, known, changed);
            }
            inline_once(body, once, pending, known, changed);
            if kept.is_empty() {
                let Core::Let(_, body) = std::mem::replace(c, Core::Fail(String::new())) else { unreachable!() };
                *c = *body;
            } else {
                *bindings = kept;
            }
        }
        Core::Case(s, binder, alts) => {
            inline_once(s, once, pending, known, changed);
            if let Core::Var(v) = &**s {
                if let Some(con) = known.get(v.as_str()) {
                    let pick = alts.iter().position(|a| matches!(a, Alt::Con(k, vars, _) if k == con && vars.is_empty()))
                        .or_else(|| alts.iter().position(|a| matches!(a, Alt::Default(_))));
                    if let Some(i) = pick {
                        let global = v.clone();
                        let binder = binder.clone();
                        let (Alt::Con(_, _, body) | Alt::Lit(_, body) | Alt::Default(body)) = alts.swap_remove(i);
                        let mut body = body;
                        rename_var(&mut body, &binder, &global);
                        *c = body;
                        *changed = true;
                        inline_once(c, once, pending, known, changed);
                        return;
                    }
                }
            }
            for a in alts {
                match a {
                    Alt::Con(_, _, b) | Alt::Lit(_, b) | Alt::Default(b) => inline_once(b, once, pending, known, changed),
                }
            }
        }
        Core::Con(_, args) | Core::Prim(_, args) => {
            args.iter_mut().for_each(|a| inline_once(a, once, pending, known, changed))
        }
    }
}

/// Replaces the free occurrences of a (uniquely bound) local by another variable.
fn rename_var(c: &mut Core, from: &str, to: &str) {
    match c {
        Core::Var(v) => {
            if v == from {
                *v = to.to_string();
            }
        }
        Core::Lit(_) | Core::Fail(_) => {}
        Core::App(f, a) => {
            rename_var(f, from, to);
            rename_var(a, from, to);
        }
        Core::Lam(_, b) => rename_var(b, from, to),
        Core::Let(bindings, body) => {
            bindings.iter_mut().for_each(|(_, r)| rename_var(r, from, to));
            rename_var(body, from, to);
        }
        Core::Case(s, _, alts) => {
            rename_var(s, from, to);
            for a in alts {
                match a {
                    Alt::Con(_, _, b) | Alt::Lit(_, b) | Alt::Default(b) => rename_var(b, from, to),
                }
            }
        }
        Core::Con(_, args) | Core::Prim(_, args) => args.iter_mut().for_each(|a| rename_var(a, from, to)),
    }
}
