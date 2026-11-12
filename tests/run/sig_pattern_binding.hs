{-# LANGUAGE ScopedTypeVariables #-}
-- `x :: t = e` binds a variable through a signature pattern (exceptions' test suite).
module Main where

total :: Int = sum [1 .. 10]

main :: IO ()
main = do
  print total
  print (tfst (3, ()), pair)
  where
    tfst :: (Int, ()) -> Int = fst
    pair :: (Bool, String) = (True, "x")
