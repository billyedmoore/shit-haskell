{-# LANGUAGE ConstraintKinds, TypeFamilies, DataKinds, KindSignatures, RankNTypes, FlexibleContexts, UndecidableSuperClasses #-}
-- A superclass equality `b ~ (Body b) Ps` with `Body` an associated family of arity one: a
-- wanted `b ~ body0 (Pass p0)` is solved through it, `body0` being `Body b` (ghc-lib-parser's
-- `class (b ~ (Body b) GhcPs, ..) => DisambECP b`).
module Main where

import Data.Kind (Type)

data Pass = Parsed | Renamed

data GhcPass (p :: Pass) = GhcPass

type GhcPs = GhcPass 'Parsed

newtype Expr p = Expr String deriving Show
newtype Cmd p = Cmd Int deriving Show

-- Superclass equalities naming a family at the body, as GHC's `AnnoBody b`.
type AnnoBody b = (Anno2 (Maybe (Body b GhcPs)) ~ Bool, Anno2 [Body (Body b GhcPs) GhcPs] ~ Char)

type family Anno2 a
type instance Anno2 (Maybe (Expr GhcPs)) = Bool
type instance Anno2 (Maybe (Cmd GhcPs)) = Bool
type instance Anno2 [Expr GhcPs] = Char
type instance Anno2 [Cmd GhcPs] = Char

class (b ~ (Body b) GhcPs, AnnoBody b) => Disamb b where
  type Body b :: Type -> Type
  fromString :: String -> b

instance Disamb (Expr GhcPs) where
  type Body (Expr GhcPs) = Expr
  fromString = Expr

instance Disamb (Cmd GhcPs) where
  type Body (Cmd GhcPs) = Cmd
  fromString = Cmd . length

type family Anno a
type instance Anno [body (GhcPass p)] = Int

class HasLoc a where loc :: a -> String
instance HasLoc Int where loc _ = "int"

located :: HasLoc (Anno [b]) => [b] -> Anno [b] -> String
located _ a = loc a

-- Generic over the body constructor and the pass, as GHC's `unguardedGRHSs`.
wrap :: body (GhcPass p) -> [body (GhcPass p)]
wrap x = [x, x]

build :: Disamb b => String -> [b]
build s = wrap (fromString s)

-- A family equation over `body (GhcPass p)` matches the rigid `b` through its superclass.
whereIs :: Disamb b => [b] -> String
whereIs bs = located bs 0

-- Wanted at the rigid `b`: the superclass equalities give them.
annotations :: Disamb b => [b] -> (Anno2 (Maybe b), Anno2 [b])
annotations _ = (True, 'c')

main :: IO ()
main = do
  print (build "ab" :: [Expr GhcPs])
  print (build "abc" :: [Cmd GhcPs])
  putStrLn (whereIs (build "x" :: [Expr GhcPs]))
  print (annotations (build "y" :: [Cmd GhcPs]))
