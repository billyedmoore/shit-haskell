{-# LANGUAGE MagicHash, UnboxedTuples, NoImplicitPrelude #-}
-- A boxed view of the native bignum behind Integer and Natural, and its word access.
module GHC.Num.BigNat
  ( BigNat(..), BigNat#, bigNatIndex#, bigNatSize#, bigNatSize, gcdInt, gcdWord
  , bigNatZero, bigNatOne, bigNatZero#, bigNatOne#, bigNatIsZero, bigNatIsZero#
  , bigNatFromWord#, bigNatFromWord, bigNatToWord#, bigNatToWord
  , bigNatAnd, bigNatOr, bigNatXor, bigNatAdd, bigNatMul, bigNatEq, bigNatEq#, bigNatCompare
  -- GHC's API beyond these (cborg, integer-gmp), through the integer primitives
  , WordArray#, bigNatAddWord#, bigNatBit#, bigNatCheck#, bigNatClearBit#, bigNatCompareWord#
  , bigNatComplementBit#, bigNatEqWord#, bigNatGtWord#, bigNatFromAddr#, bigNatFromByteArray#
  , bigNatFromWord2#, bigNatFromWordArray, bigNatFromWordArray#, bigNatGcd, bigNatGcdWord#
  , bigNatMulWord#, bigNatPopCount#, bigNatQuot, bigNatRem, bigNatQuotRem#, bigNatQuotRemWord#
  , bigNatQuotWord#, bigNatRemWord#, bigNatSetBit#, bigNatTestBit#, bigNatShiftL#, bigNatShiftR#
  , bigNatSizeInBase#, bigNatSizeInBase, bigNatSqr, bigNatSub, bigNatSubWord#, bigNatToAddr#
  , bigNatToInt#, bigNatToMutableByteArray#
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


-- ----- GHC's API beyond the above (cborg, integer-gmp): each through the integer primitives -----

-- | A bignum's words (GHC.Num.WordArray's name).
type WordArray# = ByteArray#

toI :: BigNat# -> Integer
toI = integerFromBigNat#

fromI :: Integer -> BigNat#
fromI = integerToBigNatClamp

bigNatAddWord# :: BigNat# -> Word# -> BigNat#
bigNatAddWord# a w = fromI (integerAdd (toI a) (integerFromWord# w))

bigNatMulWord# :: BigNat# -> Word# -> BigNat#
bigNatMulWord# a w = fromI (integerMul (toI a) (integerFromWord# w))

bigNatBit# :: Word# -> BigNat#
bigNatBit# i = fromI (integerBit# i)

bigNatCheck# :: BigNat# -> Int#
bigNatCheck# _ = 1#

bigNatTestBit# :: BigNat# -> Word# -> Int#
bigNatTestBit# a n = integerTestBit# (toI a) n

bigNatSetBit# :: BigNat# -> Word# -> BigNat#
bigNatSetBit# a n = fromI (integerOr (toI a) (integerBit# n))

bigNatClearBit# :: BigNat# -> Word# -> BigNat#
bigNatClearBit# a n = case bigNatTestBit# a n of
  0# -> a
  _ -> fromI (integerXor (toI a) (integerBit# n))

bigNatComplementBit# :: BigNat# -> Word# -> BigNat#
bigNatComplementBit# a n = fromI (integerXor (toI a) (integerBit# n))

bigNatCompareWord# :: BigNat# -> Word# -> Ordering
bigNatCompareWord# a w = integerCompare (toI a) (integerFromWord# w)

bigNatEqWord# :: BigNat# -> Word# -> Int#
bigNatEqWord# a w = integerEq# (toI a) (integerFromWord# w)

bigNatGtWord# :: BigNat# -> Word# -> Int#
bigNatGtWord# a w = integerGt# (toI a) (integerFromWord# w)

bigNatFromAddr# :: Word# -> Addr# -> Int# -> State# s -> (# State# s, BigNat# #)
bigNatFromAddr# sz addr e s = case integerFromAddr# sz addr e s of (# s', i #) -> (# s', fromI i #)

bigNatFromByteArray# :: Word# -> ByteArray# -> Word# -> Int# -> State# s -> (# State# s, BigNat# #)
bigNatFromByteArray# sz ba off e s = (# s, fromI (integerFromByteArray sz ba off e) #)

bigNatFromWord2# :: Word# -> Word# -> BigNat#
bigNatFromWord2# h l = fromI (integerAdd (integerShiftL# (integerFromWord# h) 64##) (integerFromWord# l))

-- | The bignum of the first `n` words of an array (least significant first).
bigNatFromWordArray# :: WordArray# -> Word# -> BigNat#
bigNatFromWordArray# wa n = fromI (go (word2Int# n -# 1#) integerZero)
  where
    go i acc = case i <# 0# of
      1# -> acc
      _ -> go (i -# 1#) (integerAdd (integerShiftL# acc 64##) (integerFromWord# (indexWordArray# wa i)))

bigNatFromWordArray :: WordArray# -> Word# -> BigNat
bigNatFromWordArray wa n = BN# (bigNatFromWordArray# wa n)

bigNatGcd :: BigNat# -> BigNat# -> BigNat#
bigNatGcd a b = fromI (integerGcd (toI a) (toI b))

bigNatGcdWord# :: BigNat# -> Word# -> Word#
bigNatGcdWord# a w = integerToWord# (integerGcd (toI a) (integerFromWord# w))

bigNatPopCount# :: BigNat# -> Word#
bigNatPopCount# a = int2Word# (integerPopCount# (toI a))

bigNatQuot :: BigNat# -> BigNat# -> BigNat#
bigNatQuot a b = fromI (integerQuot (toI a) (toI b))

bigNatRem :: BigNat# -> BigNat# -> BigNat#
bigNatRem a b = fromI (integerRem (toI a) (toI b))

bigNatQuotRem# :: BigNat# -> BigNat# -> (# BigNat#, BigNat# #)
bigNatQuotRem# a b = case integerQuotRem# (toI a) (toI b) of (# q, r #) -> (# fromI q, fromI r #)

bigNatQuotWord# :: BigNat# -> Word# -> BigNat#
bigNatQuotWord# a w = fromI (integerQuot (toI a) (integerFromWord# w))

bigNatRemWord# :: BigNat# -> Word# -> Word#
bigNatRemWord# a w = integerToWord# (integerRem (toI a) (integerFromWord# w))

bigNatQuotRemWord# :: BigNat# -> Word# -> (# BigNat#, Word# #)
bigNatQuotRemWord# a w = case integerQuotRem# (toI a) (integerFromWord# w) of (# q, r #) -> (# fromI q, integerToWord# r #)

bigNatShiftL# :: BigNat# -> Word# -> BigNat#
bigNatShiftL# a n = fromI (integerShiftL# (toI a) n)

bigNatShiftR# :: BigNat# -> Word# -> BigNat#
bigNatShiftR# a n = fromI (integerShiftR# (toI a) n)

bigNatSizeInBase# :: Word# -> BigNat# -> Word#
bigNatSizeInBase# b a = case bigNatIsZero# a of
  1# -> 0##
  _ -> integerSizeInBase# b (toI a)

bigNatSizeInBase :: Word -> BigNat# -> Word
bigNatSizeInBase (W# b) a = W# (bigNatSizeInBase# b a)

bigNatSqr :: BigNat# -> BigNat#
bigNatSqr a = fromI (integerSqr (toI a))

-- | `a - b`, or nothing when it would be negative.
bigNatSub :: BigNat# -> BigNat# -> (# (# #) | BigNat# #)
bigNatSub a b = if integerLt (toI a) (toI b) then (# (# #) | #) else (# | fromI (integerSub (toI a) (toI b)) #)

bigNatSubWord# :: BigNat# -> Word# -> (# (# #) | BigNat# #)
bigNatSubWord# a w = bigNatSub a (fromI (integerFromWord# w))

bigNatToAddr# :: BigNat# -> Addr# -> Int# -> State# s -> (# State# s, Word# #)
bigNatToAddr# a = integerToAddr# (toI a)

bigNatToInt# :: BigNat# -> Int#
bigNatToInt# a = integerToInt# (toI a)

bigNatToMutableByteArray# :: BigNat# -> MutableByteArray# s -> Word# -> Int# -> State# s -> (# State# s, Word# #)
bigNatToMutableByteArray# a = integerToMutableByteArray# (toI a)
