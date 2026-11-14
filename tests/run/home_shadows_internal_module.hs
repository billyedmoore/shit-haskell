-- A home module named like a ghc-internal module that libraries import (ghc-lib-parser's
-- GHC.Internal.TH.Syntax): the libraries keep theirs, including its self-export.
module Main where

import GHC.Internal.TH.Syntax (homeVersion)
import Language.Haskell.TH (mkName, nameBase, nameModule)

main :: IO ()
main = do
  putStrLn homeVersion
  let n = mkName "Data.List.foldl'"
  print (nameBase n, nameModule n)
