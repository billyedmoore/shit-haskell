{-# LANGUAGE GADTs #-}
{-# LANGUAGE TypeOperators #-}
module Main where

import Data.Maybe (isJust)
import Data.Type.Equality

data Tag a where
  TInt  :: Tag Int
  TBool :: Tag Bool

data Pair f g a = Pair (f a) (g a)

eqTag :: Tag a -> Tag b -> Maybe (a :~: b)
eqTag TInt  TInt  = Just Refl
eqTag TBool TBool = Just Refl
eqTag _     _     = Nothing

eqPair :: Pair Tag Tag a -> Pair Tag Tag b -> Maybe (a :~: b)
eqPair (Pair x y) (Pair x' y') = do
  Refl <- eqTag x x'
  Refl <- eqTag y y'
  return Refl

main :: IO ()
main = do
  print (isJust (eqPair (Pair TInt TInt) (Pair TInt TInt)))
  print (isJust (eqPair (Pair TInt TInt) (Pair TBool TBool)))
