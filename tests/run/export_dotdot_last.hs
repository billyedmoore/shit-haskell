-- An export list naming some children and then `..` (Cabal's LocalBuildInfo exports).
module Main where

import Rec

main :: IO ()
main = do
  let r = R { name = "p", x = 1, y = 2 }
  print (r, name r, x r + y r)
  print Origin
