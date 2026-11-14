-- A normal import before the SOURCE importer: cached code must refer to the same
-- instance dictionary as it does when the boot interface is encountered first.
module Main where
import CacheBootA
import CacheBootB

main :: IO ()
main = do
  print (makeFromB 7)
  print (scoreFromB (T 7), score (4 :: Int), score True)
