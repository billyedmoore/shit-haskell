{-# LANGUAGE MonadComprehensions #-}
-- Monad comprehensions: a comprehension in any monad, guards needing Alternative.
module Main where

import Data.List.NonEmpty (NonEmpty (..))

pairs :: Maybe (Int, Int)
pairs = [(x, y) | x <- Just 1, y <- Just 2]

guarded :: Int -> Maybe Int
guarded n = [n * 10 | n > 0, let m = n + 1, odd m || even m]

mapAccum :: Monad m => (acc -> x -> m (acc, y)) -> acc -> NonEmpty x -> m (acc, NonEmpty y)
mapAccum f s (x :| xs) = [(s2, y :| ys) | (s1, y) <- f s x, (s2, ys) <- go s1 xs]
  where
    go s' [] = return (s', [])
    go s' (z : zs) = do
      (s1, z') <- f s' z
      (s2, zs') <- go s1 zs
      return (s2, z' : zs')

main :: IO ()
main = do
  print pairs
  print (guarded 3, guarded (-1))
  print [x * 2 | x <- [1 .. 5 :: Int], odd x]
  r <- mapAccum (\a x -> [(a + x, show x) | True]) 0 (1 :| [2, 3 :: Int])
  print r
  line <- [length s | s <- return "hello" :: IO String]
  print line
