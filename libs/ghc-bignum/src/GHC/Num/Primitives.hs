{-# LANGUAGE MagicHash, NoImplicitPrelude #-}
-- | The word primitives of ghc-bignum that other packages use.
module GHC.Num.Primitives (wordLog2#, wordLogBase#, intLog2#) where

import GHC.Prim
import GHC.Types

-- | The floor of the base-2 logarithm of a word (the bit length less one); -1 as a word for 0.
wordLog2# :: Word# -> Word#
wordLog2# w = minusWord# 63## (clz# w)

-- | The floor of the base-2 logarithm of an Int.
intLog2# :: Int# -> Int#
intLog2# x = word2Int# (wordLog2# (int2Word# x))

-- | The floor of the logarithm of a word in a base (above 1).
wordLogBase# :: Word# -> Word# -> Word#
wordLogBase# base a = go 0## base
  where
    go n p = case gtWord# p a of
      1# -> n
      _ -> case timesWord2# p base of
        (# 0##, p' #) -> go (plusWord# n 1##) p'
        _ -> plusWord# n 1##
