-- One type imported through two modules, one exporting it abstractly and one with its
-- constructor (ghc-lib-parser's GHC.Core.Type and GHC.Core.TyCo.Rep, re-exported by
-- GHC.Core.Multiplicity): the constructor is in scope, and exported by `T(..)`.
module Main where

import Ty as T
import Mult

main :: IO ()
main = do
  let s = Scaled 2 "x"
  print (T.scaledThing s)
  case s of Scaled n _ -> print n
