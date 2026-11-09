//! Core lint (`-dcore-lint`): checks that every variable occurrence is bound, either by an
//! enclosing binder or as a top-level binding of the program.

use std::collections::{HashMap, HashSet};

use super::{Alt, Core, TopBind};

/// Checks a whole program. Returns the first scope error found, naming the top-level binding
/// it occurs in.
pub fn lint_program(binds: &[TopBind]) -> Result<(), String> {
    let mut globals: HashSet<&str> = HashSet::with_capacity(binds.len());
    for b in binds {
        if !globals.insert(b.name.as_str()) {
            return Err(format!("duplicate top-level binding {}", b.name));
        }
    }
    let mut scope: HashMap<String, usize> = HashMap::new();
    for b in binds {
        check(&b.body, &mut scope, &globals).map_err(|e| format!("in {}: {e}", b.name))?;
    }
    Ok(())
}

fn bind(scope: &mut HashMap<String, usize>, name: &str) {
    *scope.entry(name.to_string()).or_insert(0) += 1;
}

fn unbind(scope: &mut HashMap<String, usize>, name: &str) {
    if let Some(n) = scope.get_mut(name) {
        *n -= 1;
        if *n == 0 {
            scope.remove(name);
        }
    }
}

fn check(c: &Core, scope: &mut HashMap<String, usize>, globals: &HashSet<&str>) -> Result<(), String> {
    match c {
        Core::Var(v) => {
            if scope.contains_key(v) || globals.contains(v.as_str()) {
                Ok(())
            } else {
                Err(format!("variable {v} is not in scope"))
            }
        }
        Core::Lit(_) | Core::Fail(_) => Ok(()),
        Core::App(f, a) => {
            check(f, scope, globals)?;
            check(a, scope, globals)
        }
        Core::Lam(x, body) => {
            bind(scope, x);
            let r = check(body, scope, globals);
            unbind(scope, x);
            r
        }
        Core::Let(binds, body) => {
            for (n, _) in binds {
                bind(scope, n);
            }
            let mut r = Ok(());
            for (_, rhs) in binds {
                r = check(rhs, scope, globals);
                if r.is_err() {
                    break;
                }
            }
            if r.is_ok() {
                r = check(body, scope, globals);
            }
            for (n, _) in binds {
                unbind(scope, n);
            }
            r
        }
        Core::Case(s, b, alts) => {
            check(s, scope, globals)?;
            bind(scope, b);
            let mut r = Ok(());
            for alt in alts {
                r = match alt {
                    Alt::Con(_, vars, body) => {
                        for v in vars {
                            bind(scope, v);
                        }
                        let res = check(body, scope, globals);
                        for v in vars {
                            unbind(scope, v);
                        }
                        res
                    }
                    Alt::Lit(_, body) | Alt::Default(body) => check(body, scope, globals),
                };
                if r.is_err() {
                    break;
                }
            }
            unbind(scope, b);
            r
        }
        Core::Con(_, args) | Core::Prim(_, args) => {
            for a in args {
                check(a, scope, globals)?;
            }
            Ok(())
        }
    }
}
