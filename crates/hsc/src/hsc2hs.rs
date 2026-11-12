//! hsc2hs: turns a `.hsc` file into Haskell source, as GHC's hsc2hs does. The file is translated into
//! a C program that prints the Haskell text, with the values the C compiler knows put in place:
//! constants (`#const`), sizes and alignments (`#size`, `#alignment`), field offsets (`#peek`,
//! `#poke`, `#ptr`, `#offset`), integral types (`#type`) and enumerations (`#enum`). C preprocessor
//! directives (`#include`, `#define`, `#if` ...) are kept in the program, so conditional parts of the
//! file follow the C headers.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// Bumped when the translation changes, so that cached translations are made again.
const TRANSLATOR_VERSION: &str = "hsc2hs-4";

const KEYWORDS: &[&str] = &[
    "const", "const_str", "type", "peek", "poke", "ptr", "offset", "size", "alignment", "enum", "let", "def", "comment",
    "documentation", "include", "define", "undef", "if", "ifdef", "ifndef", "elif", "else", "endif", "error", "warning",
];

/// Directives that are C preprocessor lines in the generated program.
const CPP: &[&str] = &["include", "define", "undef", "if", "ifdef", "ifndef", "elif", "else", "endif", "error", "warning"];

/// The parts of a `.hsc` file: Haskell text and constructs.
enum Part {
    Text(String),
    /// A construct: its keyword and argument text.
    Construct(String, String),
}

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '\''
}

/// Splits a `.hsc` source into text and constructs. A construct is `#keyword args`: it extends to the
/// end of the line (`\` continues it), or to the `}` of `#{keyword args}`, or, inside parentheses,
/// to the closing parenthesis.
fn split(src: &str) -> Vec<Part> {
    let chars: Vec<char> = src.chars().collect();
    let mut parts = Vec::new();
    let mut text = String::new();
    let mut i = 0;
    let symbol = |c: char| "!#$%&*+./<=>?@\\^|-~:".contains(c);
    while i < chars.len() {
        let c = chars[i];
        // Haskell comments are copied as they are: a directive in one is not processed.
        if c == '-' && chars.get(i + 1) == Some(&'-') && (i == 0 || !symbol(chars[i - 1])) {
            let mut k = i;
            while k < chars.len() && chars[k] == '-' {
                k += 1;
            }
            if k >= chars.len() || !symbol(chars[k]) {
                while i < chars.len() && chars[i] != '\n' {
                    text.push(chars[i]);
                    i += 1;
                }
                continue;
            }
        }
        if c == '{' && chars.get(i + 1) == Some(&'-') && chars.get(i + 2) != Some(&'#') {
            let mut depth = 0;
            while i < chars.len() {
                if chars[i] == '{' && chars.get(i + 1) == Some(&'-') {
                    depth += 1;
                    text.push_str("{-");
                    i += 2;
                } else if chars[i] == '-' && chars.get(i + 1) == Some(&'}') {
                    depth -= 1;
                    text.push_str("-}");
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    text.push(chars[i]);
                    i += 1;
                }
            }
            continue;
        }
        if c != '#' {
            text.push(c);
            i += 1;
            continue;
        }
        // `##` before a keyword stands for a single `#`.
        let braced = chars.get(i + 1) == Some(&'{');
        let start = if braced { i + 2 } else { i + 1 };
        let mut j = start;
        // `#{ kw ..}`, and `# if` at the start of a line (as C allows), may have spaces after `#`.
        let at_line_start = chars[..i].iter().rev().take_while(|&&ch| ch != '\n').all(|ch| ch.is_whitespace());
        let _ = at_line_start;
        while j < chars.len() && (chars[j] == ' ' || chars[j] == '\t') {
            j += 1;
        }
        let kw_start = j;
        while j < chars.len() && (chars[j].is_alphanumeric() || chars[j] == '_') {
            j += 1;
        }
        let kw: String = chars[kw_start..j].iter().collect();
        let prev_ident = i > 0 && is_ident(chars[i - 1]);
        if chars.get(i + 1) == Some(&'#') && !prev_ident {
            // `##keyword`: a literal `#`.
            let mut k = i + 2;
            while k < chars.len() && chars[k].is_alphanumeric() {
                k += 1;
            }
            let w: String = chars[i + 2..k].iter().collect();
            if KEYWORDS.contains(&w.as_str()) {
                text.push('#');
                i += 2;
                continue;
            }
        }
        // C preprocessor directives count only at the start of a line.
        let line_start = chars[..i].iter().rev().take_while(|&&ch| ch != '\n').all(|ch| ch.is_whitespace());
        let cpp_mid_line = CPP.contains(&kw.as_str()) && !braced && !line_start;
        if kw.is_empty() || prev_ident || cpp_mid_line || !KEYWORDS.contains(&kw.as_str()) && !braced {
            text.push(c);
            i += 1;
            continue;
        }
        // The argument.
        let mut arg = String::new();
        let mut depth = 0i32;
        let mut k = j;
        if braced {
            while k < chars.len() {
                match chars[k] {
                    '{' => depth += 1,
                    '}' if depth == 0 => break,
                    '}' => depth -= 1,
                    _ => {}
                }
                arg.push(chars[k]);
                k += 1;
            }
            k += 1;
        } else {
            while k < chars.len() {
                match chars[k] {
                    '\\' if chars.get(k + 1) == Some(&'\n') => {
                        arg.push(' ');
                        k += 2;
                        continue;
                    }
                    '\n' => break,
                    '(' | '[' | '{' => depth += 1,
                    ')' | ']' | '}' if depth == 0 => break,
                    ')' | ']' | '}' => depth -= 1,
                    _ => {}
                }
                arg.push(chars[k]);
                k += 1;
            }
        }
        if !text.is_empty() {
            parts.push(Part::Text(std::mem::take(&mut text)));
        }
        parts.push(Part::Construct(kw, arg.trim().to_string()));
        i = k;
    }
    if !text.is_empty() {
        parts.push(Part::Text(text));
    }
    parts
}

/// A C string literal for a piece of text.
fn c_string(s: &str) -> String {
    let mut out = String::from("\"");
    for b in s.bytes() {
        match b {
            b'"' => out.push_str("\\\""),
            b'\\' => out.push_str("\\\\"),
            b'\n' => out.push_str("\\n"),
            b'%' => out.push('%'),
            32..=126 => out.push(b as char),
            _ => {
                let _ = write!(out, "\\{b:03o}");
            }
        }
    }
    out.push('"');
    out
}

/// Splits `T, field` at its last top-level comma.
fn type_and_field(arg: &str) -> (String, String) {
    match arg.rfind(',') {
        Some(i) => (arg[..i].trim().to_string(), arg[i + 1..].trim().to_string()),
        None => (arg.to_string(), String::new()),
    }
}

/// Splits arguments at top-level commas.
fn split_commas(arg: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0;
    let mut cur = String::new();
    for c in arg.chars() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => {
                out.push(cur.trim().to_string());
                cur.clear();
                continue;
            }
            _ => {}
        }
        cur.push(c);
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

/// The Haskell name hsc2hs gives a C constant in `#enum`: `FOO_BAR` becomes `fooBar`.
fn enum_name(c: &str) -> String {
    let mut out = String::new();
    let mut upper = false;
    for (i, ch) in c.chars().enumerate() {
        if ch == '_' {
            upper = true;
            continue;
        }
        if i == 0 {
            out.extend(ch.to_lowercase());
        } else if upper {
            out.extend(ch.to_uppercase());
            upper = false;
        } else {
            out.extend(ch.to_lowercase());
        }
    }
    out
}

/// The C program that prints the Haskell source.
fn program(parts: &[Part]) -> Result<String, String> {
    // The file's own includes come first (a package header may set _GNU_SOURCE for the system
    // headers); the program's are added after them.
    let mut head = crate::syntax::cpp::c_macro_definitions();
    let mut body = String::new();
    let mut lets: Vec<String> = Vec::new();
    for p in parts {
        match p {
            Part::Text(t) => {
                let _ = writeln!(body, "fputs({}, stdout);", c_string(t));
            }
            Part::Construct(kw, arg) if CPP.contains(&kw.as_str()) => {
                let line = format!("#{kw} {arg}\n");
                head.push_str(&line);
                if !matches!(kw.as_str(), "include" | "define" | "undef") {
                    body.push_str(&line);
                }
            }
            Part::Construct(kw, arg) => match kw.as_str() {
                "const" => {
                    let _ = writeln!(
                        body,
                        "if ((long long)({arg}) < 0) printf(\"%lld\", (long long)({arg})); else printf(\"%llu\", (unsigned long long)({arg}));"
                    );
                }
                "const_str" => {
                    let _ = writeln!(body, "{{ const char *s = ({arg}); putchar('\"'); for (; *s; s++) {{ if (*s == '\"' || *s == '\\\\') putchar('\\\\'); putchar(*s); }} putchar('\"'); }}");
                }
                "size" => {
                    let _ = writeln!(body, "printf(\"%lu\", (unsigned long)sizeof({arg}));");
                }
                "alignment" => {
                    let _ = writeln!(body, "printf(\"%lu\", (unsigned long)offsetof(struct {{ char c__; {arg} t__; }}, t__));");
                }
                "offset" => {
                    let (t, f) = type_and_field(arg);
                    let _ = writeln!(body, "printf(\"%lu\", (unsigned long)offsetof({t}, {f}));");
                }
                "peek" | "poke" | "ptr" => {
                    let (t, f) = type_and_field(arg);
                    let shape = match kw.as_str() {
                        "peek" => "(\\\\hsc_ptr -> peekByteOff hsc_ptr %lu)",
                        "poke" => "(\\\\hsc_ptr -> pokeByteOff hsc_ptr %lu)",
                        _ => "(\\\\hsc_ptr -> hsc_ptr `plusPtr` %lu)",
                    };
                    let _ = writeln!(body, "printf(\"{shape}\", (unsigned long)offsetof({t}, {f}));");
                }
                "type" => {
                    let _ = writeln!(
                        body,
                        "{{ if ((({arg})1.4) != 1) printf(\"%s\", sizeof({arg}) == sizeof(double) ? \"Double\" : sizeof({arg}) == sizeof(float) ? \"Float\" : \"LDouble\"); \
                         else printf(\"%s%lu\", (({arg})-1) < 0 ? \"Int\" : \"Word\", (unsigned long)(sizeof({arg}) * 8)); }}"
                    );
                }
                "enum" => {
                    let items = split_commas(arg);
                    if items.len() < 2 {
                        return Err(format!("malformed #enum {arg}"));
                    }
                    let (ty, con) = (items[0].clone(), items[1].clone());
                    for it in &items[2..] {
                        let (name, value) = match it.split_once('=') {
                            Some((n, v)) => (n.trim().to_string(), v.trim().to_string()),
                            None => (enum_name(it), it.clone()),
                        };
                        let _ = writeln!(
                            body,
                            "fputs({}, stdout); if ((long long)({value}) < 0) printf(\"%lld\", (long long)({value})); else printf(\"%llu\", (unsigned long long)({value})); fputs(\"\\n\", stdout);",
                            c_string(&format!("{name} :: {ty}\n{name} = {con} "))
                        );
                    }
                }
                "let" => {
                    // `#let name args = "format", expressions`: a construct of its own, used as `#name args`.
                    lets.push(arg.clone());
                }
                "def" => {
                    head.push_str(arg);
                    head.push('\n');
                }
                _ => {}
            },
        }
    }
    let _ = lets;
    Ok(format!("{head}\n#include <stddef.h>\n#include <stdio.h>\n#include <string.h>\n#include <stdint.h>\nint main(void) {{\n{body}return 0;\n}}\n"))
}

/// Translates a `.hsc` file into Haskell source, compiling and running the C program with the C
/// compiler's `flags` (`-I`, `-D`). The result is cached by the source and flags.
/// The prefix of a flag that goes to the C compiler as it is (`--cflag` of the hsc2hs command).
pub const CFLAG_PREFIX: &str = "-hsc2hs-cflag=";

pub fn translate(path: &Path, flags: &[String]) -> Result<PathBuf, String> {
    let src = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut key = src.clone();
    // The translator's own version: a change to it invalidates the cached translations.
    key.push_str(TRANSLATOR_VERSION);
    key.push_str(&flags.join(" "));
    key.push_str(&path.display().to_string());
    let mut ctx = [0u8; 88];
    crate::interp::md5::init(&mut ctx);
    crate::interp::md5::update(&mut ctx, key.as_bytes());
    let digest: String = crate::interp::md5::finish(&mut ctx).iter().map(|b| format!("{b:02x}")).collect();
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .unwrap_or_else(std::env::temp_dir)
        .join("hsc")
        .join("hsc2hs");
    // The generated module keeps its file name, so that messages and module lookups name it.
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let out = base.join(&digest).join(format!("{stem}.hs"));
    if out.is_file() {
        return Ok(out);
    }
    let dir = base.join(format!("build-{digest}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let c = dir.join("prog.c");
    let exe = dir.join("prog");
    std::fs::write(&c, program(&split(&src))?).map_err(|e| e.to_string())?;
    let mut cmd = std::process::Command::new("gcc");
    cmd.arg(&c).arg("-o").arg(&exe).arg("-w");
    if let Some(d) = path.parent() {
        cmd.arg(format!("-I{}", d.display()));
    }
    cmd.arg(format!("-I{}", crate::driver::default_lib_dir().join("include").display()));
    for f in flags {
        if f.starts_with("-I") || f.starts_with("-D") {
            cmd.arg(f);
        } else if let Some(c) = f.strip_prefix(CFLAG_PREFIX) {
            cmd.arg(c);
        }
    }
    let o = cmd.output().map_err(|e| format!("gcc: {e}"))?;
    if !o.status.success() {
        return Err(format!("{}: hsc2hs: the C program did not compile:\n{}", path.display(), String::from_utf8_lossy(&o.stderr)));
    }
    let run = std::process::Command::new(&exe).output().map_err(|e| e.to_string())?;
    if !run.status.success() {
        return Err(format!("{}: hsc2hs: the C program failed", path.display()));
    }
    let target = base.join(&digest);
    std::fs::create_dir_all(&target).map_err(|e| e.to_string())?;
    std::fs::write(&out, &run.stdout).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_dir_all(&dir);
    Ok(out)
}
