{-# LANGUAGE ViewPatterns #-}
-- View patterns as tuple elements in a let binding (Cabal's Distribution.Simple.Test).
module Main where

main :: IO ()
main = do
  let ( concat -> a
        , concat -> b
        ) = unzip [([1], [2]), ([3], [4 :: Int])]
  print (a, b)
  print (case (1 :: Int, "x") of (succ -> n, length -> l) -> (n, l))
