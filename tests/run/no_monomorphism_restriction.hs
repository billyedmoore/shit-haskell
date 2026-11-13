{-# LANGUAGE NoMonomorphismRestriction #-}
-- Without the monomorphism restriction a binding with no arguments is generalised over its
-- constraints (ShellCheck's parser helpers, used at several monads).
module Main where

import Data.Functor.Identity

plusOne = fmap (+ 1)

twice = \f -> f . f

base = 10

main :: IO ()
main = do
  print (plusOne [1, 2 :: Int], plusOne (Just (2.5 :: Double)))
  print (runIdentity (plusOne (Identity (3 :: Integer))))
  r <- plusOne (return (4 :: Int))
  print r
  print (twice (* 2) (base :: Int), twice (/ 2) (base :: Double))
