{-# LANGUAGE MagicHash #-}
-- Derived instances of types with unboxed fields, through a synonym (happy's HappyAction).
module Main where

import GHC.Exts

type HInt = Int#

data Action = Fail | Reduce HInt | Shift Int# Word# | Lit Char# Double# Float#
  deriving Show

data P = P Int# Double# deriving (Eq, Ord, Show)

main :: IO ()
main = do
  print [Fail, Reduce 3#, Reduce -2#, Shift 4# 5##, Lit 'x'# 1.5## 2.5#]
  print (P 1# 2.0## == P 1# 2.0##, compare (P 1# 2.0##) (P 1# 3.0##), P 0# -1.0## < P -1# 0.0##)
  print (Just (P 7# 0.5##))
