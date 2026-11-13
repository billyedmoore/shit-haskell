{-# LANGUAGE GeneralizedNewtypeDeriving, UnboxedTuples #-}
-- Newtype deriving of classes whose parameter takes two arguments (Bifunctor, Bifoldable), as
-- bifunctors' Biap; an unboxed tuple pattern `(#q, r#)` in a module without MagicHash.
module Main where

import Data.Bifoldable (Bifoldable (..), biList)
import Data.Bifunctor (Bifunctor (..))

newtype Biap bi a b = Biap { getBiap :: bi a b }
  deriving (Eq, Show, Functor, Foldable, Bifunctor, Bifoldable)

divMod' :: Int -> Int -> (# Int, Int #)
divMod' n d = (# n `div` d, n `mod` d #)

main :: IO ()
main = do
  let p = Biap (1 :: Int, "x")
  print (bimap (+ 1) length p)
  print (biList (Biap (Left 3 :: Either Int Int)), bifoldMap show id p)
  case divMod' 17 5 of
    (#q, r#) -> print (q, r)
