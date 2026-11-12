//! The C preprocessor as GHC uses it for Haskell sources (traditional mode): conditionals,
//! object-like and function-like macros, `##` pasting, and `#include`. Directive lines and
//! inactive lines become empty lines so that source positions are preserved.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
enum Macro {
    Object(String),
    Function(Vec<String>, String),
}

/// Predefined macros, as GHC 9.12 on x86-64 Linux defines them.
const PREDEFINED: &[(&str, &str)] = &[
    ("__GLASGOW_HASKELL__", "912"),
    ("__GLASGOW_HASKELL_FULL_VERSION__", "\"9.12.2\""),
    ("__GLASGOW_HASKELL_PATCHLEVEL1__", "2"),
    ("linux_HOST_OS", "1"),
    ("x86_64_HOST_ARCH", "1"),
    ("linux_BUILD_OS", "1"),
    ("x86_64_BUILD_ARCH", "1"),
    ("WORD_SIZE_IN_BITS", "64"),
    ("SIZEOF_HSWORD", "8"),
    ("SIZEOF_HSINT", "8"),
    ("ALIGNMENT_HSWORD", "8"),
    ("TABLES_NEXT_TO_CODE", "1"),
    ("__LINE__", "0"),
];

/// Versions of the boot packages that `MIN_VERSION_<pkg>(a,b,c)` compares against.
const PACKAGE_VERSIONS: &[(&str, [u32; 3])] = &[
    ("base", [4, 21, 0]),
    ("ghc_prim", [0, 13, 0]),
    ("ghc", [9, 12, 2]),
    ("containers", [0, 7, 0]),
    ("bytestring", [0, 12, 2]),
    ("text", [2, 1, 2]),
    ("mtl", [2, 3, 1]),
    ("transformers", [0, 6, 1]),
    ("deepseq", [1, 5, 1]),
    ("array", [0, 5, 8]),
    ("template_haskell", [2, 23, 0]),
    ("stm", [2, 5, 3]),
    ("pretty", [1, 1, 3]),
    ("filepath", [1, 5, 4]),
    ("os_string", [2, 0, 7]),
    ("exceptions", [0, 10, 9]),
    ("binary", [0, 8, 9]),
    ("parsec", [3, 1, 18]),
    ("directory", [1, 3, 9]),
    ("unix", [2, 8, 6]),
    ("time", [1, 14, 0]),
    ("process", [1, 6, 25]),
    ("ghc_boot_th", [9, 12, 2]),
    ("file_io", [0, 1, 5]),
    ("ghc_internal", [9, 1202, 0]),
    ("ghc_bignum", [1, 3, 0]),
];

const DIRECTIVES: &[&str] = &[
    "if", "ifdef", "ifndef", "elif", "else", "endif", "define", "undef", "include", "error", "warning",
    "line", "pragma",
];

pub struct Preprocessor {
    defs: HashMap<String, Macro>,
    include_dirs: Vec<PathBuf>,
    /// The file being read, for `__FILE__`.
    file: String,
    /// The packages in scope and their versions, for `MIN_VERSION_<pkg>` (GHC defines it for the
    /// visible packages); None for the boot packages' table.
    versions: Option<HashMap<String, [u32; 3]>>,
}

/// Whether a source needs the preprocessor: the `CPP` extension is on by flag or pragma.
pub fn wants_cpp(src: &str, flags: &[String]) -> bool {
    if flags.iter().any(|f| f == "-cpp" || f == "-XCPP") {
        return true;
    }
    let mut rest = src;
    while let Some(i) = rest.find("{-#") {
        let after = &rest[i + 3..];
        let Some(end) = after.find("#-}") else { break };
        let body = after[..end].trim();
        let mut words = body.split(|c: char| c.is_whitespace() || c == ',');
        if words.next().is_some_and(|w| w.eq_ignore_ascii_case("LANGUAGE"))
            && words.any(|w| w == "CPP")
        {
            return true;
        }
        rest = &after[end..];
    }
    false
}

impl Preprocessor {
    pub fn new(include_dirs: Vec<PathBuf>) -> Preprocessor {
        let defs = PREDEFINED
            .iter()
            .map(|(k, v)| (k.to_string(), Macro::Object(v.to_string())))
            .collect();
        Preprocessor { defs, include_dirs, file: String::new(), versions: None }
    }

    /// Defines an object-like macro (`-D`).
    pub fn define_macro(&mut self, name: &str, value: &str) {
        self.defs.insert(name.to_string(), Macro::Object(value.to_string()));
    }

    /// Whether a macro is defined: by `#define` or `-D`, or a `MIN_VERSION_<pkg>` of a known package
    /// (which GHC defines for the packages in scope).
    fn is_defined(&self, name: &str) -> bool {
        self.defs.contains_key(name) || name.strip_prefix("MIN_VERSION_").is_some_and(|p| self.package_version(p).is_some())
    }

    /// Records a package in scope (`-hsc-pkgver=name:1.2.3`): then only those packages have
    /// `MIN_VERSION_` macros.
    pub fn add_package_version(&mut self, name: &str, version: &str) {
        let mut v = [0u32; 3];
        for (k, part) in version.split('.').take(3).enumerate() {
            v[k] = part.parse().unwrap_or(0);
        }
        self.versions.get_or_insert_with(HashMap::new).insert(name.replace('-', "_"), v);
    }

    fn package_version(&self, pkg: &str) -> Option<[u32; 3]> {
        match &self.versions {
            Some(m) => m.get(pkg).copied(),
            None => PACKAGE_VERSIONS.iter().find(|(p, _)| *p == pkg).map(|(_, v)| *v),
        }
    }

    /// Names the file whose text `run` reads next (the expansion of `__FILE__`).
    pub fn set_file(&mut self, file: &str) {
        self.file = file.to_string();
    }

    /// Removes a macro (`-U`).
    pub fn undefine(&mut self, name: &str) {
        self.defs.remove(name);
    }

    /// Runs the preprocessor over a source file's text.
    pub fn run(&mut self, src: &str, dir: &Path) -> Result<String, String> {
        let mut out = String::with_capacity(src.len());
        self.process(src, dir, &mut out, 0)?;
        Ok(out)
    }

    fn process(&mut self, src: &str, dir: &Path, out: &mut String, depth: usize) -> Result<(), String> {
        if depth > 32 {
            return Err("#include nested too deeply".into());
        }
        // (active, branch_taken, enclosing_active) for each open conditional.
        let mut conds: Vec<(bool, bool, bool)> = Vec::new();
        let mut in_c_comment = false;
        let lines: Vec<&str> = src.lines().collect();
        let mut i = 0;
        while i < lines.len() {
            // Lines ending in a backslash continue the directive or line.
            let mut line = lines[i].to_string();
            let mut extra = 0;
            let is_directive = line.trim_start().starts_with('#');
            // A macro call may continue on the following lines, until its parentheses close.
            // Only a backslash that ends a physical line splices (once): the backslash before it
            // in `\\` stays, followed by the next line.
            let mut spliced = lines[i].ends_with('\\');
            while i + extra + 1 < lines.len() && (spliced || (!is_directive && self.open_macro_call(&line))) {
                if spliced {
                    line.pop();
                } else {
                    line.push(' ');
                }
                extra += 1;
                spliced = lines[i + extra].ends_with('\\');
                // A line comment would swallow the rest of the joined line.
                line.push_str(strip_line_comment(lines[i + extra]));
            }
            let active = conds.last().map_or(true, |c| c.0);
            let trimmed = line.trim_start();
            // Only `#` followed by a known directive name is a directive; anything else
            // (such as the `#-}` that closes a pragma) is ordinary text.
            let directive_word = trimmed
                .strip_prefix('#')
                .map(|d| split_word(d.trim_start()).0)
                .filter(|w| DIRECTIVES.contains(w));
            if let Some(word) = directive_word {
                {
                    let directive = trimmed[1..].trim_start();
                    let (word, rest) = split_word(directive);
                    match word {
                        "if" => {
                            let on = active && self.eval_cond(rest)?;
                            conds.push((on, on, active));
                        }
                        "ifdef" => {
                            let on = active && self.is_defined(rest.trim());
                            conds.push((on, on, active));
                        }
                        "ifndef" => {
                            let on = active && !self.is_defined(rest.trim());
                            conds.push((on, on, active));
                        }
                        "elif" => {
                            let (_, taken, enc) = conds.pop().ok_or("#elif without #if")?;
                            let on = enc && !taken && self.eval_cond(rest)?;
                            conds.push((on, taken || on, enc));
                        }
                        "else" => {
                            let (_, taken, enc) = conds.pop().ok_or("#else without #if")?;
                            let on = enc && !taken;
                            conds.push((on, true, enc));
                        }
                        "endif" => {
                            conds.pop().ok_or("#endif without #if")?;
                        }
                        "define" if active => self.define(rest)?,
                        "undef" if active => {
                            self.defs.remove(rest.trim());
                        }
                        "include" if active => {
                            let name = rest.trim().trim_matches(|c| c == '"' || c == '<' || c == '>');
                            let text = self.find_include(name, dir)?;
                            let inner_dir = text.0.parent().unwrap_or(dir).to_path_buf();
                            // A header that only defines macros adds no lines, so the lines of the
                            // source keep their numbers (as GHC's line markers keep them).
                            let mut inner = String::new();
                            let outer_file = std::mem::replace(&mut self.file, text.0.display().to_string());
                            let r = self.process(&text.1, &inner_dir, &mut inner, depth + 1);
                            self.file = outer_file;
                            r?;
                            if inner.lines().any(|l| !l.trim().is_empty()) {
                                out.push_str(&inner);
                            }
                        }
                        "error" if active => return Err(format!("#error {}", rest.trim())),
                        _ => {}
                    }
                    for _ in 0..=extra {
                        out.push('\n');
                    }
                    i += extra + 1;
                    continue;
                }
            }
            if active {
                let stripped = strip_c_comments(&line, &mut in_c_comment);
                // `__LINE__` and `__FILE__` may come from a macro's body, so they are always current.
                self.defs.insert("__LINE__".into(), Macro::Object((i + 1).to_string()));
                if !matches!(self.defs.get("__FILE__"), Some(Macro::Object(f)) if f.len() == self.file.len() + 2) {
                    self.defs.insert("__FILE__".into(), Macro::Object(format!("{:?}", self.file)));
                }
                let expanded = self.expand_line(&stripped, 0)?;
                out.push_str(&expanded);
            }
            out.push('\n');
            for _ in 0..extra {
                out.push('\n');
            }
            i += extra + 1;
        }
        if !conds.is_empty() {
            return Err("unterminated #if".into());
        }
        Ok(())
    }

    fn find_include(&self, name: &str, dir: &Path) -> Result<(PathBuf, String), String> {
        let mut candidates = vec![dir.join(name)];
        candidates.extend(self.include_dirs.iter().map(|d| d.join(name)));
        for c in candidates {
            if let Ok(text) = std::fs::read_to_string(&c) {
                return Ok((c, text));
            }
        }
        Err(format!("{name}: No such file or directory"))
    }

    fn define(&mut self, rest: &str) -> Result<(), String> {
        let rest = rest.trim_start();
        let name_end = rest
            .find(|c: char| !(c.is_alphanumeric() || c == '_'))
            .unwrap_or(rest.len());
        let name = &rest[..name_end];
        if name.is_empty() {
            return Err("macro name missing".into());
        }
        let after = &rest[name_end..];
        if let Some(params_src) = after.strip_prefix('(') {
            let close = params_src.find(')').ok_or("missing ')' in macro parameters")?;
            let params: Vec<String> = params_src[..close]
                .split(',')
                .map(|p| p.trim().to_string())
                .filter(|p| !p.is_empty())
                .collect();
            let body = params_src[close + 1..].trim().to_string();
            self.defs.insert(name.to_string(), Macro::Function(params, body));
        } else {
            self.defs
                .insert(name.to_string(), Macro::Object(after.trim().to_string()));
        }
        Ok(())
    }

    /// Whether a line calls a function-like macro whose argument list is still open.
    fn open_macro_call(&self, line: &str) -> bool {
        let chars: Vec<char> = line.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            if chars[i].is_alphabetic() || chars[i] == '_' {
                let start = i;
                while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                let word: String = chars[start..i].iter().collect();
                if matches!(self.defs.get(&word), Some(Macro::Function(..))) && i < chars.len() && chars[i] == '(' {
                    let rest: String = chars[i..].iter().collect();
                    if paren_depth(&rest) > 0 {
                        return true;
                    }
                }
                continue;
            }
            i += 1;
        }
        false
    }

    /// Expands macros in one source line, leaving string and character literals alone.
    fn expand_line(&self, line: &str, depth: usize) -> Result<String, String> {
        if depth > 16 {
            return Err("macro expansion too deep".into());
        }
        let chars: Vec<char> = line.chars().collect();
        let mut out = String::with_capacity(line.len());
        let mut i = 0;
        let mut changed = false;
        while i < chars.len() {
            let c = chars[i];
            if c == '"' {
                // Copy a string literal verbatim.
                out.push(c);
                i += 1;
                while i < chars.len() {
                    out.push(chars[i]);
                    if chars[i] == '\\' && i + 1 < chars.len() {
                        out.push(chars[i + 1]);
                        i += 2;
                        continue;
                    }
                    i += 1;
                    if chars[i - 1] == '"' {
                        break;
                    }
                }
                continue;
            }
            if c.is_alphabetic() || c == '_' {
                let start = i;
                while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '\'') {
                    i += 1;
                }
                let word: String = chars[start..i].iter().collect();
                // A prime that closes a name cannot start a literal, so the whole word is the name.
                if let Some(m) = self.defs.get(&word) {
                    match m {
                        Macro::Object(body) => {
                            out.push_str(body);
                            changed = true;
                            continue;
                        }
                        Macro::Function(params, body) => {
                            let mut j = i;
                            while j < chars.len() && chars[j] == ' ' {
                                j += 1;
                            }
                            if j < chars.len() && chars[j] == '(' {
                                let (args, end) = collect_args(&chars, j)?;
                                let replaced = substitute(params, body, &args, &word)?;
                                out.push_str(&replaced);
                                i = end;
                                changed = true;
                                continue;
                            }
                        }
                    }
                }
                if let Some(v) = min_version(&word, &chars, &mut i, &|p| self.package_version(p)) {
                    out.push_str(if v { "1" } else { "0" });
                    changed = true;
                    continue;
                }
                out.push_str(&word);
                continue;
            }
            out.push(c);
            i += 1;
        }
        if changed && (out.contains("MIN_VERSION_") || self.defs.keys().any(|k| out.contains(k.as_str()))) {
            return self.expand_line(&out, depth + 1);
        }
        Ok(out)
    }

    fn eval_cond(&self, expr: &str) -> Result<bool, String> {
        // `defined X` and `defined(X)` are resolved before macro expansion.
        let mut with_defined = String::new();
        let chars: Vec<char> = expr.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            if chars[i].is_alphabetic() || chars[i] == '_' {
                let start = i;
                while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                let word: String = chars[start..i].iter().collect();
                if word == "defined" {
                    let mut j = i;
                    while j < chars.len() && chars[j] == ' ' {
                        j += 1;
                    }
                    let paren = j < chars.len() && chars[j] == '(';
                    if paren {
                        j += 1;
                        while j < chars.len() && chars[j] == ' ' {
                            j += 1;
                        }
                    }
                    let s = j;
                    while j < chars.len() && (chars[j].is_alphanumeric() || chars[j] == '_') {
                        j += 1;
                    }
                    let name: String = chars[s..j].iter().collect();
                    if paren {
                        while j < chars.len() && chars[j] == ' ' {
                            j += 1;
                        }
                        if j < chars.len() && chars[j] == ')' {
                            j += 1;
                        }
                    }
                    with_defined.push_str(if self.is_defined(&name) { " 1 " } else { " 0 " });
                    i = j;
                    continue;
                }
                with_defined.push_str(&word);
                continue;
            }
            with_defined.push(chars[i]);
            i += 1;
        }
        let expanded = self.expand_line(&with_defined, 0)?;
        let mut p = ExprParser { toks: tokenize(&expanded)?, pos: 0 };
        let v = p.ternary()?;
        if p.pos != p.toks.len() {
            return Err(format!("invalid #if expression: {expr}"));
        }
        Ok(v != 0)
    }
}

fn split_word(s: &str) -> (&str, &str) {
    let end = s
        .find(|c: char| !c.is_alphanumeric() && c != '_')
        .unwrap_or(s.len());
    (&s[..end], &s[end..])
}

/// Collects the comma-separated arguments of a macro call whose `(` is at `open`.
fn collect_args(chars: &[char], open: usize) -> Result<(Vec<String>, usize), String> {
    let mut depth = 0;
    let mut args = Vec::new();
    let mut cur = String::new();
    let mut i = open;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '(' => {
                depth += 1;
                if depth > 1 {
                    cur.push(c);
                }
            }
            ')' => {
                depth -= 1;
                if depth == 0 {
                    args.push(cur.trim().to_string());
                    return Ok((args, i + 1));
                }
                cur.push(c);
            }
            ',' if depth == 1 => {
                args.push(cur.trim().to_string());
                cur.clear();
            }
            // A string literal's commas and parentheses are its own.
            '"' => {
                cur.push(c);
                i += 1;
                while i < chars.len() && chars[i] != '"' && chars[i] != '\n' {
                    if chars[i] == '\\' && i + 1 < chars.len() {
                        cur.push(chars[i]);
                        i += 1;
                    }
                    cur.push(chars[i]);
                    i += 1;
                }
                if i < chars.len() {
                    cur.push(chars[i]);
                }
            }
            _ => cur.push(c),
        }
        i += 1;
    }
    Err("unterminated macro call".into())
}

/// Replaces parameters in a function-like macro body; `##` pastes adjacent tokens.
fn substitute(params: &[String], body: &str, args: &[String], name: &str) -> Result<String, String> {
    if params.len() != args.len() && !(params.is_empty() && args.len() == 1 && args[0].is_empty()) {
        return Err(format!("macro {name} passed {} arguments, but takes {}", args.len(), params.len()));
    }
    let chars: Vec<char> = body.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '#' && i + 1 < chars.len() && chars[i + 1] == '#' {
            // Token pasting: drop the whitespace around `##`.
            while out.ends_with(' ') {
                out.pop();
            }
            i += 2;
            while i < chars.len() && chars[i] == ' ' {
                i += 1;
            }
            continue;
        }
        if chars[i].is_alphabetic() || chars[i] == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            match params.iter().position(|p| *p == word) {
                Some(k) => out.push_str(&args[k]),
                None => out.push_str(&word),
            }
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    Ok(out)
}

/// A line without its `--` comment (a `--` followed by a space or the end of the line).
fn strip_line_comment(line: &str) -> &str {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'-' && bytes[i + 1] == b'-' && (i + 2 == bytes.len() || bytes[i + 2] == b' ') {
            return &line[..i];
        }
        i += 1;
    }
    line
}

/// The number of parentheses left open by a line, ignoring string and character literals.
fn paren_depth(line: &str) -> i64 {
    let chars: Vec<char> = line.chars().collect();
    let mut depth = 0i64;
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '"' => {
                i += 1;
                while i < chars.len() && chars[i] != '"' {
                    if chars[i] == '\\' {
                        i += 1;
                    }
                    i += 1;
                }
            }
            '\'' if i + 2 < chars.len() && chars[i + 2] == '\'' => i += 2,
            '(' => depth += 1,
            ')' => depth -= 1,
            _ => {}
        }
        i += 1;
    }
    depth
}

/// Removes C-style `/* ... */` comments, which may span lines (`in_comment` carries over),
/// leaving string literals alone.
fn strip_c_comments(line: &str, in_comment: &mut bool) -> String {
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::with_capacity(line.len());
    let mut i = 0;
    let mut in_string = false;
    while i < chars.len() {
        if *in_comment {
            if chars[i] == '*' && chars.get(i + 1) == Some(&'/') {
                *in_comment = false;
                i += 2;
            } else {
                i += 1;
            }
            continue;
        }
        if in_string {
            out.push(chars[i]);
            if chars[i] == '\\' && i + 1 < chars.len() {
                out.push(chars[i + 1]);
                i += 2;
                continue;
            }
            if chars[i] == '"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        if chars[i] == '"' {
            in_string = true;
            out.push('"');
            i += 1;
        } else if chars[i] == '/' && chars.get(i + 1) == Some(&'*') {
            *in_comment = true;
            i += 2;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

/// `MIN_VERSION_pkg(a,b,c)`: whether the boot package version is at least a.b.c.
fn min_version(word: &str, chars: &[char], i: &mut usize, version_of: &dyn Fn(&str) -> Option<[u32; 3]>) -> Option<bool> {
    let pkg = word.strip_prefix("MIN_VERSION_")?;
    let version = version_of(pkg)?;
    let mut j = *i;
    // White space may come between the name and its arguments, as with any function-like macro.
    while j < chars.len() && (chars[j] == ' ' || chars[j] == '\t') {
        j += 1;
    }
    if j >= chars.len() || chars[j] != '(' {
        return None;
    }
    let (args, end) = collect_args(chars, j).ok()?;
    j = end;
    let want: Vec<u32> = args.iter().map(|a| a.parse().unwrap_or(0)).collect();
    *i = j;
    let mut ok = true;
    for k in 0..3 {
        let w = want.get(k).copied().unwrap_or(0);
        if version[k] != w {
            ok = version[k] > w;
            break;
        }
    }
    Some(ok)
}

// ----- #if expressions --------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
enum Tk {
    Num(i64),
    Op(String),
}

fn tokenize(s: &str) -> Result<Vec<Tk>, String> {
    let chars: Vec<char> = s.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
        } else if c.is_ascii_digit() {
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric()) {
                i += 1;
            }
            let text: String = chars[start..i].iter().collect();
            let text = text.trim_end_matches(|c: char| c == 'L' || c == 'U' || c == 'l' || c == 'u');
            let v = if let Some(h) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
                i64::from_str_radix(h, 16)
            } else {
                text.parse::<i64>()
            };
            out.push(Tk::Num(v.map_err(|_| format!("bad number {text} in #if"))?));
        } else if c.is_alphabetic() || c == '_' {
            // Identifiers that survived expansion are zero, as in cpp.
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            out.push(Tk::Num(0));
        } else {
            let two: String = chars[i..(i + 2).min(chars.len())].iter().collect();
            if ["&&", "||", "==", "!=", "<=", ">=", "<<", ">>"].contains(&two.as_str()) {
                out.push(Tk::Op(two));
                i += 2;
            } else {
                out.push(Tk::Op(c.to_string()));
                i += 1;
            }
        }
    }
    Ok(out)
}

struct ExprParser {
    toks: Vec<Tk>,
    pos: usize,
}

impl ExprParser {
    fn peek_op(&self) -> Option<&str> {
        match self.toks.get(self.pos) {
            Some(Tk::Op(o)) => Some(o.as_str()),
            _ => None,
        }
    }

    fn ternary(&mut self) -> Result<i64, String> {
        let c = self.binary(0)?;
        if self.peek_op() == Some("?") {
            self.pos += 1;
            let a = self.ternary()?;
            if self.peek_op() != Some(":") {
                return Err("expected ':' in #if".into());
            }
            self.pos += 1;
            let b = self.ternary()?;
            return Ok(if c != 0 { a } else { b });
        }
        Ok(c)
    }

    fn binary(&mut self, min_prec: u8) -> Result<i64, String> {
        let mut lhs = self.unary()?;
        loop {
            let Some(op) = self.peek_op().map(str::to_string) else { break };
            let prec = match op.as_str() {
                "||" => 1,
                "&&" => 2,
                "|" => 3,
                "^" => 4,
                "&" => 5,
                "==" | "!=" => 6,
                "<" | ">" | "<=" | ">=" => 7,
                "<<" | ">>" => 8,
                "+" | "-" => 9,
                "*" | "/" | "%" => 10,
                _ => break,
            };
            if prec < min_prec.max(1) {
                break;
            }
            self.pos += 1;
            let rhs = self.binary(prec + 1)?;
            lhs = match op.as_str() {
                "||" => ((lhs != 0) || (rhs != 0)) as i64,
                "&&" => ((lhs != 0) && (rhs != 0)) as i64,
                "|" => lhs | rhs,
                "^" => lhs ^ rhs,
                "&" => lhs & rhs,
                "==" => (lhs == rhs) as i64,
                "!=" => (lhs != rhs) as i64,
                "<" => (lhs < rhs) as i64,
                ">" => (lhs > rhs) as i64,
                "<=" => (lhs <= rhs) as i64,
                ">=" => (lhs >= rhs) as i64,
                "<<" => lhs.wrapping_shl(rhs as u32),
                ">>" => lhs.wrapping_shr(rhs as u32),
                "+" => lhs.wrapping_add(rhs),
                "-" => lhs.wrapping_sub(rhs),
                "*" => lhs.wrapping_mul(rhs),
                "/" | "%" if rhs == 0 => return Err("division by zero in #if".into()),
                "/" => lhs / rhs,
                _ => lhs % rhs,
            };
        }
        Ok(lhs)
    }

    fn unary(&mut self) -> Result<i64, String> {
        match self.toks.get(self.pos).cloned() {
            Some(Tk::Num(n)) => {
                self.pos += 1;
                Ok(n)
            }
            Some(Tk::Op(o)) if o == "!" => {
                self.pos += 1;
                Ok((self.unary()? == 0) as i64)
            }
            Some(Tk::Op(o)) if o == "-" => {
                self.pos += 1;
                Ok(-self.unary()?)
            }
            Some(Tk::Op(o)) if o == "+" => {
                self.pos += 1;
                self.unary()
            }
            Some(Tk::Op(o)) if o == "(" => {
                self.pos += 1;
                let v = self.ternary()?;
                if self.peek_op() != Some(")") {
                    return Err("missing ')' in #if".into());
                }
                self.pos += 1;
                Ok(v)
            }
            _ => Err("invalid #if expression".into()),
        }
    }
}

/// Used by `MIN_VERSION_*` tests: the table entry for a package, if known.
pub fn package_version(pkg: &str) -> Option<[u32; 3]> {
    PACKAGE_VERSIONS.iter().find(|(p, _)| *p == pkg).map(|(_, v)| *v)
}

/// The C definitions of the predefined macros and `MIN_VERSION_<pkg>` macros, for C code that
/// Haskell tools generate (hsc2hs): as cabal's `cabal_macros.h` and GHC define them.
pub fn c_macro_definitions() -> String {
    let mut out = String::new();
    for (k, v) in PREDEFINED {
        if *k != "__LINE__" {
            out.push_str(&format!("#ifndef {k}\n#define {k} {v}\n#endif\n"));
        }
    }
    for (p, [a, b, c]) in PACKAGE_VERSIONS {
        out.push_str(&format!(
            "#define MIN_VERSION_{p}(x,y,z) ((x)<{a} || ((x)=={a} && ((y)<{b} || ((y)=={b} && (z)<={c}))))\n"
        ));
    }
    out
}
