{-# LANGUAGE DeriveTraversable #-}
-- Derived Functor, Foldable and Traversable through fields whose types are synonyms of
-- applications that mention the parameter (GHC's BooleanFormula over LocatedL).
module Main where

data Located l e = L l e deriving (Show, Functor, Foldable, Traversable)

type LocatedS = Located String

type LFormula a = LocatedS (Formula a)

data Formula a = Var a | And [LFormula a] | Or [LFormula a] | Parens (LFormula a)
  deriving (Show, Functor, Foldable, Traversable)

type Pair a = (Int, a)

data Tagged a = Tagged (Pair a) [Pair a] deriving (Show, Functor, Foldable, Traversable)

main :: IO ()
main = do
  let f = And [L "x" (Var 1), L "y" (Or [L "z" (Var 2), L "w" (Parens (L "v" (Var (3 :: Int))))])]
  print (fmap (* 10) f)
  print (sum f, length f, foldr (:) [] f)
  print (traverse (\x -> if x > 0 then Just (show x) else Nothing) f)
  let t = Tagged (1, 'a') [(2, 'b'), (3, 'c')]
  print (fmap succ t, foldr (:) [] t)
  mapM_ print (sequenceA (Tagged (0, [1, 2]) [(0, [3 :: Int])]))
