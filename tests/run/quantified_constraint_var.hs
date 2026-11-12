{-# LANGUAGE ConstraintKinds, QuantifiedConstraints, RankNTypes, KindSignatures, DataKinds, FlexibleContexts, FunctionalDependencies, AllowAmbiguousTypes, TypeApplications, ScopedTypeVariables #-}
-- A quantified constraint headed by a constraint variable, over a synonym with promoted
-- arguments (Cabal-syntax's packageDescriptionFieldGrammar).
module Main where

import Data.Kind (Type, Constraint)

data Kind = File | Dir
data Root = Pkg | Build
newtype SymPath (r :: Root) (k :: Kind) = SymPath String
type RelPath = SymPath
newtype ListOf sep a b = ListOf [b]
data FSep = FSep

class Render a where render :: a -> String
instance Render (ListOf sep a (SymPath r k)) where
  render (ListOf ps) = unwords [s | SymPath s <- ps]

useAt :: forall (c :: Type -> Constraint) r. (forall from to. c (ListOf FSep () (RelPath from to)))
      => (forall x. c x => x -> r) -> [r]
useAt f = [f (ListOf [SymPath "a", SymPath "b"] :: ListOf FSep () (SymPath 'Pkg 'File))]

-- Passed on: the quantified constraint is wanted, and solved from the given one.
passOn :: forall (c :: Type -> Constraint) r. (forall from to. c (ListOf FSep () (RelPath from to)))
       => (forall x. c x => x -> r) -> [r]
passOn f = useAt @c f ++ useAt @c f

-- The constraint is decided by a functional dependency only after the quantified constraint is
-- wanted (Cabal-syntax's libraryFieldGrammar calling buildInfoFieldGrammar).
class Grammar (c :: Type -> Constraint) g | g -> c where
  each :: c x => g -> x -> String
data Plain = Plain
instance Grammar Render Plain where each _ = render

viaGrammar :: forall c g. (Grammar c g, forall from to. c (ListOf FSep () (RelPath from to))) => g -> [String]
viaGrammar g = useAt @c (each g)

inner :: forall c g. (Grammar c g, forall from to. c (ListOf FSep () (RelPath from to))) => g -> [String]
inner = viaGrammar

main :: IO ()
main = do
  mapM_ putStrLn (inner Plain)
  mapM_ putStrLn (useAt @Render render)
  mapM_ putStrLn (passOn @Render render)
