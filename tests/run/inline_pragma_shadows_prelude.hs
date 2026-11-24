module Main where

import qualified Prelude as P
import Prelude hiding ()

lookup :: P.Int -> [(P.Int, P.String)] -> P.Maybe P.String
lookup = P.lookup
{-# INLINE lookup #-}

main :: P.IO ()
main = P.print (Main.lookup 2 [(1, "one"), (2, "two")])
