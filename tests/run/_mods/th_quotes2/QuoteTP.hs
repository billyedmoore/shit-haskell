{-# LANGUAGE TemplateHaskell #-}
module QuoteTP (annotated, typedNothing, swapper, pairSum) where

import Language.Haskell.TH

-- | A literal with a quoted type annotation.
annotated :: Integer -> Q Exp
annotated n = sigE (litE (integerL n)) [t| Int |]

-- | `Nothing` at a quoted type with lists, tuples and applications.
typedNothing :: Q Exp
typedNothing = sigE [| Nothing |] [t| Maybe (Either Bool [(Int, Char)]) |]

-- | `\(a, b) -> (b, a)`, its pattern quoted.
swapper :: Q Exp
swapper = lamE [ [p| (a, b) |] ] (tupE [varE (mkName "b"), varE (mkName "a")])

-- | A case on a quoted pattern with a literal, a constructor and a wildcard.
pairSum :: Q Exp
pairSum = lamE [varP (mkName "v")] $
  caseE (varE (mkName "v"))
    [ match [p| (0, _) |] (normalB [| "zero first" |]) []
    , match [p| (x, Just y) |] (normalB [| show ($(varE (mkName "x")) + $(varE (mkName "y"))) |]) []
    , match [p| _ |] (normalB [| "other" |]) []
    ]
