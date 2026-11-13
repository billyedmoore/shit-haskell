{-# LANGUAGE TemplateHaskell #-}
-- Splices too costly for the interpreter run as a native program; their results come back
-- through a serialised form (strings, characters, numbers, nested constructors).
module Main where

import Heavy

main :: IO ()
main = do
  print $(digitCount 60000)
  let xs = $(bigList)
  print (length xs, take 3 xs, last xs)
