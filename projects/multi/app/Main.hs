module Main (main) where

import Multi.Report
import Multi.Shapes

main :: IO ()
main = do
  putStrLn versionText
  mapM_ putStrLn (report [Circle 1, Circle 2, Rect 3 4, Rect 1 1])
