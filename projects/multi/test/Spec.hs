module Main (main) where

import Control.Monad (unless)
import Multi.Shapes
import System.Exit (exitFailure)

main :: IO ()
main = do
  let ok = area (Rect 2 3) == 6 && classify (Circle 3) == "big"
  putStrLn (if ok then "spec: all passed" else "spec: FAILED")
  unless ok exitFailure
