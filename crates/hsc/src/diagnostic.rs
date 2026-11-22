use std::fmt::Write;
use std::path::Path;

use crate::syntax::ast::Loc;

#[derive(Clone, Copy, Debug)]
pub(crate) enum Warning {
    Tabs,
    MissingSignatures,
}

impl Warning {
    const ALL: [Self; 2] = [Self::Tabs, Self::MissingSignatures];

    fn name(self) -> &'static str {
        match self {
            Self::Tabs => "tabs",
            Self::MissingSignatures => "missing-signatures",
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct WarningOptions {
    enabled: [bool; Warning::ALL.len()],
    fatal: [bool; Warning::ALL.len()],
}

impl Default for WarningOptions {
    fn default() -> Self {
        let mut enabled = [false; Warning::ALL.len()];
        enabled[Warning::Tabs as usize] = true;
        Self { enabled, fatal: [false; Warning::ALL.len()] }
    }
}

impl WarningOptions {
    pub(crate) fn apply(&mut self, flag: &str) {
        match flag {
            "-Wall" | "-Weverything" => self.enabled.fill(true),
            "-w" | "-Wno-everything" => self.enabled.fill(false),
            "-Wno-all" => self.enabled[Warning::MissingSignatures as usize] = false,
            "-Werror" => self.fatal.fill(true),
            "-Wwarn" | "-Wno-error" => self.fatal.fill(false),
            _ => {
                let Some(flag) = flag.strip_prefix("-W") else { return };
                for warning in Warning::ALL {
                    let name = warning.name();
                    let index = warning as usize;
                    if flag == name {
                        self.enabled[index] = true;
                    } else if flag.strip_prefix("no-") == Some(name) {
                        self.enabled[index] = false;
                    } else if flag.strip_prefix("error=") == Some(name) {
                        self.enabled[index] = true;
                        self.fatal[index] = true;
                    } else if flag.strip_prefix("no-error=").or_else(|| flag.strip_prefix("warn=")) == Some(name) {
                        self.fatal[index] = false;
                    }
                }
            }
        }
    }

    /// Returns whether the selected warning is fatal, or None when it is disabled.
    pub(crate) fn level(&self, warning: Warning) -> Option<bool> {
        self.enabled[warning as usize].then_some(self.fatal[warning as usize])
    }
}

pub(crate) fn warning(
    path: &Path, source: Option<&str>, loc: Loc, message: &str, flag: Warning, fatal: bool,
) -> String {
    let severity = if fatal { "error" } else { "warning" };
    let name = flag.name();
    format_at(path, source, loc, &format!("{severity}: [-W{name}]"), message)
}

/// Formats a compiler error with its source line when the location is available.
pub fn error(path: &Path, source: Option<&str>, loc: Loc, message: &str) -> String {
    format_at(path, source, loc, "error:", message)
}

fn format_at(path: &Path, source: Option<&str>, loc: Loc, label: &str, message: &str) -> String {
    let Loc { line, col } = loc;
    let mut out = if line == 0 || col == 0 {
        format!("{}: {label}", path.display())
    } else {
        format!("{}:{line}:{col}: {label}", path.display())
    };
    for part in message.lines() {
        let _ = write!(out, "\n    {part}");
    }

    let Some(index) = line.checked_sub(1) else { return out };
    let Some(text) = source.and_then(|src| src.split('\n').nth(index as usize)) else {
        return out;
    };
    let mut expanded = String::new();
    let mut columns = 0;
    let offset = col.saturating_sub(1) as usize;
    let mut tab_width = None;
    for ch in text.trim_end_matches('\r').chars() {
        if ch == '\t' {
            let spaces = 8 - columns % 8;
            if columns == offset {
                tab_width = Some(spaces);
            }
            expanded.extend(std::iter::repeat_n(' ', spaces));
            columns += spaces;
        } else {
            expanded.push(ch);
            columns += 1;
        }
    }
    if col == 0 || col as usize > columns + 1 {
        return out;
    }
    let gutter = line.to_string().len();
    let width = tab_width.or_else(|| crate::syntax::lexer::first_token_width(expanded.chars().skip(offset)))
        .unwrap_or(1);
    let marker = "^".repeat(width);
    let _ = write!(out, "\n{space:gutter$} |\n{line} | {expanded}\n{space:gutter$} | {space:offset$}{marker}", space = "");
    out
}
