-- A home module named like a module of another package that a library imports (containers
-- imports base's Data.Semigroup): the library's modules use the package's, not the program's.
module Main where

import qualified Data.Map as M
import Data.Semigroup (shout)

main :: IO ()
main = do
  let m = M.fromList [(1 :: Int, "one"), (2, "two")] <> M.fromList [(3, "three")]
  print (M.toList m)
  putStrLn (shout (concat (M.elems m)))
