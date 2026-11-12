{-# LANGUAGE TemplateHaskell #-}
-- Declaration splices: top-level `$(..)` and bare expressions generate declarations, which later
-- code (and later splices, through name quotes) can use.
module Main where

import MkDecls

$(mkConst "answer" 42)

mkEnum "Colour" ["Red", "Green", "Blue"]

base :: [Int]
base = [1, 2, 3]

mkAlias 'base "shifted"

$(mkShowName ''Colour "colourName")

main :: IO ()
main = do
  print answer
  print [minBound .. maxBound :: Colour]
  print (Red == Red, succ Red)
  print shifted
  putStrLn colourName
