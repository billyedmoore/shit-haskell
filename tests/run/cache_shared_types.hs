{-# LANGUAGE FlexibleInstances, TypeSynonymInstances #-}
module Main where
import CacheSharedTypes

main :: IO ()
main = do
  let input = Box (Box (Box (7, True)))
      (left, right) = poly id input
  print (left, right, left == right)
  putStrLn (label (second input))
