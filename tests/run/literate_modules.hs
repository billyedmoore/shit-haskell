-- Literate Haskell modules (.lhs), Bird and LaTeX style (happy-lib's sources are literate).
module Main where

import Bird
import Latex

main :: IO ()
main = do
  print (map area [Square 3, Rect 2 5])
  print (Rect 1 2)
  print (twice 21)
