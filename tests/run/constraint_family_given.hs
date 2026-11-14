{-# LANGUAGE DataKinds, TypeFamilies, ConstraintKinds, UndecidableInstances, ScopedTypeVariables, TypeApplications, AllowAmbiguousTypes, KindSignatures, MagicHash, StandaloneKindSignatures, PolyKinds, TypeFamilyDependencies #-}
-- A constraint family given in an instance context provides what it reduces to (ghc-lib-parser's
-- `KnownConstructor con` giving `KnownNat (GhcDiagnosticCode con)`), and its evidence is passed
-- where it is wanted.
module Main where

import Data.Kind (Constraint, Type)
import GHC.Exts (proxy#)
import GHC.TypeLits

type Code :: Symbol -> Nat
type family Code c = n | n -> c where
  Code "Empty" = 10190
  Code "Overflow" = 97441

type KnownOrErr :: Constraint -> Nat -> Constraint
type KnownOrErr err n = (Assert err n, KnownNat n)

type Assert :: Constraint -> k -> Constraint
type family Assert err n where
  Assert _ Dummy = Dummy
  Assert _ n = ()

data family Dummy :: k

type KnownConstructor :: Symbol -> Constraint
type family KnownConstructor con where
  KnownConstructor con = KnownOrErr (TypeError ('Text "no code for " ':<>: 'Text con)) (Code con)

class HasCode (con :: Symbol) where
  code :: Integer

instance KnownConstructor con => HasCode con where
  code = natVal' @(Code con) proxy#

main :: IO ()
main = print (code @"Empty", code @"Overflow")
