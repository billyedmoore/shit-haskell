{-# LANGUAGE TypeFamilies, TypeFamilyDependencies, DataKinds, KindSignatures, FlexibleContexts, UndecidableInstances #-}
-- A field whose type is a synonym over a family of the parameter (`[LBndr flag (NoTc pass)]`),
-- of a type used through a partly applied synonym (ghc-lib-parser's HsOuterFamEqnTyVarBndrs).
module Main where

import Data.Kind (Type)

data Located l e = L l e deriving Show

type family XRec p a = r | r -> a

data Pass = Parsed | Renamed | Typechecked

data GhcPass (c :: Pass) = GhcPass

type family Anno a :: Type

type instance XRec (GhcPass p) a = Located (Anno a) a

data Bndr flag pass = Bndr flag String deriving Show

type instance Anno (Bndr flag (GhcPass p)) = Int

type family NoTcPass (p :: Pass) :: Pass where
  NoTcPass 'Typechecked = 'Renamed
  NoTcPass other = other

type family NoTc p
type instance NoTc (GhcPass pass) = GhcPass (NoTcPass pass)

type LBndr flag pass = XRec pass (Bndr flag pass)

data Outer flag pass
  = Implicit
  | Explicit { bndrs :: [LBndr flag (NoTc pass)] }

type OuterFam = Outer ()

render :: Show (LBndr () (NoTc (GhcPass p))) => OuterFam (GhcPass p) -> String
render Implicit = "none"
render Explicit { bndrs = qs } = unwords (map show qs)

main :: IO ()
main = do
  putStrLn (render (Implicit :: OuterFam (GhcPass 'Parsed)))
  putStrLn (render (Explicit [L 1 (Bndr () "a")] :: OuterFam (GhcPass 'Typechecked)))
