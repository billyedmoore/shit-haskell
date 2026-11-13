{-# LANGUAGE MagicHash, UnboxedTuples, NoImplicitPrelude #-}
-- A boxed view of the native bignum behind Integer and Natural, and its word access.
module GHC.Num.BigNat
  ( BigNat(..), BigNat#, unBigNat, bigNatIndex#, bigNatSize#, bigNatSize, gcdInt, gcdWord
  , bigNatZero, bigNatOne, bigNatZero#, bigNatOne#, bigNatIsZero, bigNatIsZero#
  , bigNatFromWord#, bigNatFromWord, bigNatToWord#, bigNatToWord
  , bigNatAnd, bigNatOr, bigNatXor, bigNatAdd, bigNatMul, bigNatEq, bigNatEq#, bigNatCompare
  ) where

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

-- ----- GHC's BigNat API, through the integer primitives (a BigNat# is a magnitude's limbs) -----

bigNatZero :: BigNat
bigNatZero = BN# (integerBigNat# (IS 0#))

bigNatOne :: BigNat
bigNatOne = BN# (integerBigNat# (IS 1#))

bigNatZero# :: (# #) -> BigNat#
bigNatZero# _ = integerBigNat# (IS 0#)

bigNatOne# :: (# #) -> BigNat#
bigNatOne# _ = integerBigNat# (IS 1#)

bigNatIsZero :: BigNat# -> Bool
bigNatIsZero b = isTrue# (bigNatSize# b ==# 0#)

bigNatIsZero# :: BigNat# -> Int#
bigNatIsZero# b = bigNatSize# b ==# 0#

bigNatFromWord# :: Word# -> BigNat#
bigNatFromWord# w = integerBigNat# (integerFromWord# w)

bigNatFromWord :: Word -> BigNat#
bigNatFromWord (W# w) = bigNatFromWord# w

-- | The least significant word.
bigNatToWord# :: BigNat# -> Word#
bigNatToWord# b = bigNatIndex# b 0#

bigNatToWord :: BigNat# -> Word
bigNatToWord b = W# (bigNatToWord# b)

bigNatLift2 :: (Integer -> Integer -> Integer) -> BigNat# -> BigNat# -> BigNat#
bigNatLift2 f a b = integerBigNat# (f (integerFromBigNat# a) (integerFromBigNat# b))

bigNatAnd, bigNatOr, bigNatXor, bigNatAdd, bigNatMul :: BigNat# -> BigNat# -> BigNat#
bigNatAnd = bigNatLift2 integerAnd
bigNatOr = bigNatLift2 integerOr
bigNatXor = bigNatLift2 integerXor
bigNatAdd = bigNatLift2 integerAdd
bigNatMul = bigNatLift2 integerMul

bigNatEq :: BigNat# -> BigNat# -> Bool
bigNatEq a b = integerEq (integerFromBigNat# a) (integerFromBigNat# b)

bigNatEq# :: BigNat# -> BigNat# -> Int#
bigNatEq# a b = if bigNatEq a b then 1# else 0#

bigNatCompare :: BigNat# -> BigNat# -> Ordering
bigNatCompare a b = integerCompare (integerFromBigNat# a) (integerFromBigNat# b)
