//! Language extension sets. GHC 9.12 defaults to GHC2021 when no `{-# LANGUAGE #-}`
//! or cabal setting selects another edition.

use std::collections::{BTreeMap, BTreeSet};

/// Extensions enabled by the GHC2021 language edition.
const GHC2021: &[&str] = &[
    "BangPatterns",
    "BinaryLiterals",
    "ConstrainedClassMethods",
    "ConstraintKinds",
    "DeriveDataTypeable",
    "DeriveFoldable",
    "DeriveFunctor",
    "DeriveGeneric",
    "DeriveLift",
    "DeriveTraversable",
    "DoAndIfThenElse",
    "EmptyCase",
    "EmptyDataDecls",
    "EmptyDataDeriving",
    "ExistentialQuantification",
    "ExplicitForAll",
    "FieldSelectors",
    "FlexibleContexts",
    "FlexibleInstances",
    "ForeignFunctionInterface",
    "GADTSyntax",
    "GeneralizedNewtypeDeriving",
    "HexFloatLiterals",
    "ImplicitPrelude",
    "ImportQualifiedPost",
    "InstanceSigs",
    "KindSignatures",
    "MonomorphismRestriction",
    "MultiParamTypeClasses",
    "NamedFieldPuns",
    "NamedWildCards",
    "NumericUnderscores",
    "PatternGuards",
    "PolyKinds",
    "PostfixOperators",
    "RankNTypes",
    "RelaxedPolyRec",
    "ScopedTypeVariables",
    "StandaloneDeriving",
    "StandaloneKindSignatures",
    "StarIsType",
    "TraditionalRecordSyntax",
    "TupleSections",
    "TypeApplications",
    "TypeOperators",
    "TypeSynonymInstances",
];

/// Extensions added by GHC2024 to the GHC2021 edition.
const GHC2024_ADDITIONS: &[&str] = &[
    "DataKinds", "DerivingStrategies", "DisambiguateRecordFields", "ExplicitNamespaces",
    "GADTs", "MonoLocalBinds", "LambdaCase", "RoleAnnotations",
];

/// Extensions enabled by the Haskell2010 language edition.
const HASKELL2010: &[&str] = &[
    "CUSKs",
    "DatatypeContexts",
    "DoAndIfThenElse",
    "EmptyDataDecls",
    "FieldSelectors",
    "ForeignFunctionInterface",
    "ImplicitPrelude",
    "MonomorphismRestriction",
    "PatternGuards",
    "RelaxedPolyRec",
    "StarIsType",
    "TraditionalRecordSyntax",
];

/// Every extension name the parser recognises. Unknown names in pragmas are errors.
pub const KNOWN: &[&str] = &[
    "AllowAmbiguousTypes", "ApplicativeDo", "Arrows", "BangPatterns", "BinaryLiterals",
    "BlockArguments", "CApiFFI", "CPP", "CUSKs", "ConstrainedClassMethods", "ConstraintKinds",
    "DataKinds", "DatatypeContexts", "DeepSubsumption", "DefaultSignatures", "DeriveAnyClass",
    "DeriveDataTypeable", "DeriveFoldable", "DeriveFunctor", "DeriveGeneric", "DeriveLift",
    "DeriveTraversable", "DerivingStrategies", "DerivingVia", "DisambiguateRecordFields",
    "DoAndIfThenElse", "DuplicateRecordFields", "EmptyCase", "EmptyDataDecls",
    "EmptyDataDeriving", "ExistentialQuantification", "ExplicitForAll", "ExplicitNamespaces",
    "ExtendedDefaultRules", "FieldSelectors", "FlexibleContexts", "FlexibleInstances",
    "ForeignFunctionInterface", "FunctionalDependencies", "GADTSyntax", "GADTs",
    "GHCForeignImportPrim", "GeneralisedNewtypeDeriving", "GeneralizedNewtypeDeriving",
    "HexFloatLiterals", "ImplicitParams", "ImplicitPrelude", "ImportQualifiedPost",
    "ImpredicativeTypes", "IncoherentInstances", "InstanceSigs", "InterruptibleFFI",
    "KindSignatures", "LambdaCase", "LexicalNegation", "LiberalTypeSynonyms",
    "LinearTypes", "MagicHash", "MonadComprehensions", "MonoLocalBinds",
    "MonomorphismRestriction", "MultiParamTypeClasses", "MultiWayIf", "NamedFieldPuns",
    "NamedWildCards", "NegativeLiterals", "NondecreasingIndentation", "NPlusKPatterns",
    "NullaryTypeClasses", "NumDecimals", "NumericUnderscores", "OverlappingInstances",
    "OverloadedLabels", "OverloadedLists", "OverloadedRecordDot", "OverloadedStrings",
    "PackageImports", "ParallelListComp", "PartialTypeSignatures", "PatternGuards",
    "PatternSynonyms", "PolyKinds", "PostfixOperators", "QualifiedDo", "QuantifiedConstraints",
    "QuasiQuotes", "RankNTypes", "RebindableSyntax", "RecordWildCards", "RecursiveDo",
    "RelaxedLayout", "RelaxedPolyRec", "RoleAnnotations", "ScopedTypeVariables",
    "StandaloneDeriving", "StandaloneKindSignatures", "StarIsType", "StaticPointers",
    "Strict", "StrictData", "TemplateHaskell", "TemplateHaskellQuotes", "TraditionalRecordSyntax",
    "TransformListComp", "TupleSections", "TypeAbstractions", "TypeApplications",
    "TypeData", "TypeFamilies", "TypeFamilyDependencies", "TypeOperators",
    "TypeSynonymInstances", "UnboxedSums", "UnboxedTuples", "UndecidableInstances",
    "UndecidableSuperClasses", "UnicodeSyntax", "UnliftedDatatypes", "UnliftedFFITypes",
    "UnliftedNewtypes", "ViewPatterns", "Haskell98", "Haskell2010", "GHC2021", "Safe",
    "Trustworthy", "Unsafe", "ExtendedLiterals",
];

#[derive(Clone, Debug)]
pub struct Extensions {
    on: BTreeSet<String>,
    overrides: BTreeMap<String, bool>,
    pub(crate) warnings: crate::diagnostic::WarningOptions,
    pub(crate) tabs: Option<(crate::syntax::ast::Loc, usize)>,
    pub(crate) unknown_pragmas: Vec<(crate::syntax::ast::Loc, String)>,
    pub(crate) deprecated_flags: Vec<(crate::syntax::ast::Loc, String)>,
}

impl Default for Extensions {
    fn default() -> Self {
        Self::ghc2021()
    }
}

impl Extensions {
    pub fn ghc2021() -> Self {
        Extensions {
            on: GHC2021.iter().map(|s| s.to_string()).collect(),
            overrides: BTreeMap::new(),
            warnings: Default::default(),
            tabs: None,
            unknown_pragmas: Vec::new(),
            deprecated_flags: Vec::new(),
        }
    }

    pub fn is_on(&self, name: &str) -> bool {
        self.on.contains(name)
    }

    /// The extensions on, by name.
    pub fn names(&self) -> Vec<String> {
        self.on.iter().cloned().collect()
    }

    /// Applies one `LANGUAGE` item or `-X` flag. Returns an error for unknown names.
    pub fn apply(&mut self, item: &str) -> Result<(), String> {
        let item = item.trim();
        if item.is_empty() {
            return Ok(());
        }
        let defaults: Option<BTreeSet<String>> = match item {
            "GHC2021" => Some(GHC2021.iter().map(|s| s.to_string()).collect()),
            "GHC2024" => Some(GHC2021.iter().chain(GHC2024_ADDITIONS).map(|s| s.to_string()).collect()),
            "Haskell2010" | "Haskell98" => Some(HASKELL2010.iter().map(|s| s.to_string()).collect()),
            _ => None,
        };
        if let Some(mut defaults) = defaults {
            if item == "Haskell98" {
                defaults.remove("ForeignFunctionInterface");
                defaults.insert("NondecreasingIndentation".to_string());
                defaults.insert("NPlusKPatterns".to_string());
            }
            // The edition sets defaults. Explicit switches apply even when they precede it.
            for (name, enabled) in &self.overrides {
                if *enabled {
                    defaults.insert(name.clone());
                } else {
                    defaults.remove(name);
                }
            }
            self.on = defaults;
            return Ok(());
        }
        // Old names of extensions.
        let canonical = |n: &str| -> String {
            match n {
                "PolymorphicComponents" | "Rank2Types" => "RankNTypes".into(),
                "GeneralisedNewtypeDeriving" => "GeneralizedNewtypeDeriving".into(),
                "RecordPuns" => "NamedFieldPuns".into(),
                "PatternSignatures" => "ScopedTypeVariables".into(),
                "NullaryTypeClasses" => "MultiParamTypeClasses".into(),
                "DoRec" => "RecursiveDo".into(),
                other => other.to_string(),
            }
        };
        let known = |n: &str| KNOWN.contains(&n) || supported_languages().iter().any(|l| l == n);
        let (enable, name) = match item.strip_prefix("No") {
            Some(rest) if known(&canonical(rest)) => (false, canonical(rest)),
            _ => (true, canonical(item)),
        };
        // An extension GHC has is accepted, whether or not this compiler does anything with it.
        if !known(&name) {
            return Err(format!("Unsupported extension: {item}"));
        }
        let name = name.as_str();
        // RebindableSyntax implies NoImplicitPrelude, as in GHC.
        if enable && name == "RebindableSyntax" {
            self.set_extension("ImplicitPrelude", false);
        }
        self.set_extension(name, enable || name == "RelaxedPolyRec");
        if enable {
            for implied in implied_by(name) {
                self.set_extension(implied, true);
            }
        }
        Ok(())
    }

    pub(super) fn apply_source_flag(&mut self, item: &str, loc: crate::syntax::ast::Loc) -> Result<(), String> {
        self.apply(item)?;
        if let Some(message) = deprecated_extension(item) {
            self.deprecated_flags.push((loc, message));
        }
        Ok(())
    }

    fn set_extension(&mut self, name: &str, enabled: bool) {
        self.overrides.insert(name.to_string(), enabled);
        if enabled {
            self.on.insert(name.to_string());
        } else {
            self.on.remove(name);
        }
    }
}

pub(crate) fn deprecated_extension(item: &str) -> Option<String> {
    let (prefix, name) = item.strip_prefix("No").map_or(("", item), |name| ("No", name));
    let replacement = match name {
        "PatternSignatures" => Some("ScopedTypeVariables"),
        "RecordPuns" => Some("NamedFieldPuns"),
        "NullaryTypeClasses" => Some("MultiParamTypeClasses"),
        "DoRec" => Some("RecursiveDo"),
        _ => None,
    };
    let advice = if let Some(replacement) = replacement {
        format!("Use ‘{prefix}{replacement}’.")
    } else {
        match item {
            "TypeInType" => "Use ‘DataKinds’ and ‘PolyKinds’.".into(),
            "NoTypeInType" => "Remove this flag. Use ‘NoDataKinds’ or ‘NoPolyKinds’ to disable those extensions explicitly.".into(),
            "OverlappingInstances" | "NoOverlappingInstances" =>
                "Use an OVERLAPPING, OVERLAPPABLE, or OVERLAPS pragma on each relevant instance.".into(),
            "AutoDeriveTypeable" => "Typeable instances are generated automatically.".into(),
            "DatatypeContexts" => "Move the constraints to the functions that need them.".into(),
            "NoRelaxedPolyRec" => "RelaxedPolyRec is always enabled. Remove this flag.".into(),
            _ => return None,
        }
    };
    Some(format!("Language extension ‘{item}’ is deprecated\n{advice}"))
}

/// Extensions switched on by enabling `name`.
fn implied_by(name: &str) -> &'static [&'static str] {
    match name {
        "GADTs" => &["GADTSyntax", "MonoLocalBinds"],
        "FlexibleInstances" => &["TypeSynonymInstances"],
        "TypeFamilies" => &["MonoLocalBinds", "KindSignatures", "ExplicitNamespaces"],
        "TypeFamilyDependencies" => &["TypeFamilies", "MonoLocalBinds", "KindSignatures", "ExplicitNamespaces"],
        "TypeOperators" => &["ExplicitNamespaces"],
        "PolyKinds" => &["KindSignatures"],
        "TypeInType" => &["DataKinds", "PolyKinds", "KindSignatures"],
        "RecordWildCards" => &["DisambiguateRecordFields"],
        "RankNTypes" | "ScopedTypeVariables" | "ExistentialQuantification"
        | "LiberalTypeSynonyms" | "QuantifiedConstraints" => &["ExplicitForAll"],
        "TemplateHaskell" => &["TemplateHaskellQuotes"],
        "DerivingVia" => &["DerivingStrategies"],
        "UnliftedDatatypes" => &["DataKinds", "StandaloneKindSignatures"],
        "Strict" => &["StrictData"],
        _ => &[],
    }
}

/// The language names `--supported-languages` lists: GHC 9.12's, so that build tools accept the
/// same package descriptions.
pub fn supported_languages() -> Vec<String> {
    include_str!("languages.txt").lines().map(String::from).collect()
}
