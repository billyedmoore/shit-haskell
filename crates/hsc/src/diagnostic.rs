use std::fmt::Write;
use std::path::Path;

use crate::syntax::ast::Loc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Warning {
    Tabs,
    MissingSignatures,
    MissingFields,
    UnrecognisedPragmas,
    MissingDerivingStrategies,
    MissingMethods,
    MissingExportLists,
    MissingImportLists,
    DuplicateExports,
    DeprecatedFlags,
    UntickedPromotedConstructors,
    IncompleteMinimal,
}

impl Warning {
    const ALL: [Self; 12] = [
        Self::Tabs, Self::MissingSignatures, Self::MissingFields, Self::UnrecognisedPragmas,
        Self::MissingDerivingStrategies, Self::MissingMethods, Self::MissingExportLists,
        Self::MissingImportLists, Self::DuplicateExports, Self::DeprecatedFlags,
        Self::UntickedPromotedConstructors, Self::IncompleteMinimal,
    ];

    fn name(self) -> Option<&'static str> {
        Some(match self {
            Self::Tabs => "tabs",
            Self::MissingSignatures => "missing-signatures",
            Self::MissingFields => "missing-fields",
            Self::UnrecognisedPragmas => "unrecognised-pragmas",
            Self::MissingDerivingStrategies => "missing-deriving-strategies",
            Self::MissingMethods => "missing-methods",
            Self::MissingExportLists => "missing-export-lists",
            Self::MissingImportLists => "missing-import-lists",
            Self::DuplicateExports => "duplicate-exports",
            Self::DeprecatedFlags => "deprecated-flags",
            Self::UntickedPromotedConstructors => "unticked-promoted-constructors",
            Self::IncompleteMinimal => return None,
        })
    }

    fn in_wall(self) -> bool {
        !matches!(
            self,
            Self::MissingDerivingStrategies | Self::MissingExportLists | Self::MissingImportLists | Self::UntickedPromotedConstructors
        )
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
        enabled[Warning::MissingFields as usize] = true;
        enabled[Warning::UnrecognisedPragmas as usize] = true;
        enabled[Warning::MissingMethods as usize] = true;
        enabled[Warning::DuplicateExports as usize] = true;
        enabled[Warning::DeprecatedFlags as usize] = true;
        enabled[Warning::IncompleteMinimal as usize] = true;
        Self { enabled, fatal: [false; Warning::ALL.len()] }
    }
}

impl WarningOptions {
    pub(crate) fn apply(&mut self, flag: &str) {
        match flag {
            "-Wall" | "-Wno-all" | "-Weverything" | "-w" | "-Wno-everything" => {
                for warning in Warning::ALL.into_iter().filter(|w| w.name().is_some()) {
                    if !matches!(flag, "-Wall" | "-Wno-all") || warning.in_wall() {
                        self.enabled[warning as usize] = matches!(flag, "-Wall" | "-Weverything");
                    }
                }
            }
            "-Werror" => self.fatal.fill(true),
            "-Wwarn" | "-Wno-error" => self.fatal.fill(false),
            _ => {
                let Some(flag) = flag.strip_prefix("-W") else { return };
                for warning in Warning::ALL {
                    let Some(name) = warning.name() else { continue };
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
    let label = match flag.name() {
        Some(name) => format!("{severity}: [-W{name}]"),
        None if fatal => "error: [-Werror]".into(),
        None => "warning:".into(),
    };
    format_at(path, source, loc, &label, message)
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

/// Match one insertion, deletion, substitution or adjacent transposition.
pub(crate) fn one_edit_apart(a: &str, b: &str) -> bool {
    let mut a = a.chars().peekable();
    let mut b = b.chars().peekable();
    while a.peek().is_some() && a.peek() == b.peek() {
        a.next();
        b.next();
    }
    match (a.next(), b.next()) {
        (None, Some(_)) => b.next().is_none(),
        (Some(_), None) => a.next().is_none(),
        (Some(x), Some(y)) => {
            if a.peek() == Some(&y) && b.peek() == Some(&x) && a.clone().skip(1).eq(b.clone().skip(1)) {
                return true;
            }
            a.clone().eq(b.clone())
                || a.clone().eq(std::iter::once(y).chain(b.clone()))
                || std::iter::once(x).chain(a).eq(b)
        }
        (None, None) => false,
    }
}
