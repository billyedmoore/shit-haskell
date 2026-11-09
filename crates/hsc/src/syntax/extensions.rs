//! Language extension sets. GHC 9.12 defaults to GHC2021 when no `{-# LANGUAGE #-}`
//! or cabal setting selects another edition.

use std::collections::BTreeSet;

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
    "GeneralisedNewtypeDeriving",
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
        }
    }

    pub fn is_on(&self, name: &str) -> bool {
        self.on.contains(name)
    }

    /// Applies one `LANGUAGE` item or `-X` flag. Returns an error for unknown names.
    pub fn apply(&mut self, item: &str) -> Result<(), String> {
        let item = item.trim();
        if item.is_empty() {
            return Ok(());
        }
        match item {
            "GHC2021" => {
                *self = Extensions::ghc2021();
                return Ok(());
            }
            "Haskell2010" | "Haskell98" => {
                self.on = HASKELL2010.iter().map(|s| s.to_string()).collect();
                return Ok(());
            }
            _ => {}
        }
        let (enable, name) = match item.strip_prefix("No") {
            Some(rest) if KNOWN.contains(&rest) => (false, rest),
            _ => (true, item),
        };
        if !KNOWN.contains(&name) {
            return Err(format!("Unsupported extension: {item}"));
        }
        if enable {
            self.on.insert(name.to_string());
            for implied in implied_by(name) {
                self.on.insert(implied.to_string());
            }
        } else {
            self.on.remove(name);
        }
        Ok(())
    }
}

/// Extensions switched on by enabling `name`.
fn implied_by(name: &str) -> &'static [&'static str] {
    match name {
        "GADTs" => &["GADTSyntax", "MonoLocalBinds"],
        "TypeFamilies" => &["MonoLocalBinds", "KindSignatures", "ExplicitNamespaces"],
        "RecordWildCards" => &["DisambiguateRecordFields"],
        "RankNTypes" => &["ExplicitForAll"],
        "ScopedTypeVariables" => &["ExplicitForAll"],
        "TemplateHaskell" => &["TemplateHaskellQuotes"],
        "DerivingVia" => &["DerivingStrategies"],
        "Strict" => &["StrictData"],
        _ => &[],
    }
}
