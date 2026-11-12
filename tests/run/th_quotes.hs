{-# LANGUAGE TemplateHaskell #-}
-- Quotations build syntax: variables of the enclosing code are lifted, nested splices run,
-- lambdas, case, let, comprehensions and typed quotes (spliced with $$).
module Main where

import Gen
main :: IO ()
main = do
  print ($(power 3) (2 :: Int))
  print $(pair 3 "hey")
  print (map $(classify) [Just 20, Just (3 :: Int), Nothing])
  print ($(letQ) :: Int)
  print ($$(typedTwice) 21)
