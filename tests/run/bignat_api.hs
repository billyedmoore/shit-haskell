{-# LANGUAGE MagicHash, UnboxedTuples #-}
-- ghc-bignum's BigNat API (bitvec builds F2 polynomials on it).
module Main where

import GHC.Exts
import GHC.Num.BigNat
import GHC.Num.Integer (integerFromBigNat#)

big :: BigNat# -> Integer
big = integerFromBigNat#

main :: IO ()
main = do
  print (big (unBigNat bigNatZero), big (unBigNat bigNatOne), big (bigNatOne# (# #)))
  print (big (bigNatFromWord# 12345##), big (bigNatFromWord 18446744073709551615))
  let a = bigNatFromWord# 0xff00ff00ff00ff00##
      b = bigNatMul a a
  print (big b, big (bigNatXor b a), big (bigNatAnd b a), big (bigNatOr a (bigNatFromWord# 255##)))
  print (bigNatIsZero (unBigNat bigNatZero), bigNatIsZero a, bigNatEq a a, bigNatCompare a b)
  print (W# (bigNatToWord# b), I# (bigNatSize# b), big (bigNatAdd b b))
