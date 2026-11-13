{-# LANGUAGE MagicHash, NoImplicitPrelude #-}
-- A boxed view of the native bignum behind Integer and Natural, and its word access.
module GHC.Num.BigNat (BigNat(..), BigNat#, unBigNat, bigNatIndex#, bigNatSize#, bigNatSize, gcdInt, gcdWord) where

import GHC.Prim
import GHC.Types
import GHC.Classes
import GHC.Num.Integer

data BigNat = BN# { unBigNat :: BigNat# }

instance Eq BigNat where
  BN# a == BN# b = integerEq (integerFromBigNat# a) (integerFromBigNat# b)

instance Ord BigNat where
  compare (BN# a) (BN# b) = integerCompare (integerFromBigNat# a) (integerFromBigNat# b)

-- | The i-th 64-bit word of a bignum (little-endian), 0 beyond its end.
foreign import prim "bigNatIndex" bigNatIndex# :: BigNat# -> Int# -> Word#

-- | The number of 64-bit words of a bignum.
foreign import prim "bigNatSize" bigNatSize# :: BigNat# -> Int#

bigNatSize :: BigNat# -> Int
bigNatSize b = I# (bigNatSize# b)

gcdInt :: Int# -> Int# -> Int#
gcdInt a b = integerToInt# (integerGcd (integerFromInt# a) (integerFromInt# b))

gcdWord :: Word# -> Word# -> Word#
gcdWord a b = integerToWord# (integerGcd (integerFromWord# a) (integerFromWord# b))
