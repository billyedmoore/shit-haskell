{-# LANGUAGE MagicHash, UnboxedTuples, UnboxedSums, NoImplicitPrelude #-}
-- The Integer and Natural types of ghc-bignum, with their representation constructors. The
-- arithmetic is the runtime's native bignum (the integer primitives), not GHC's Haskell-level
-- BigNat code: a BigNat# is that native number, and IS holds the values that fit an Int#.
-- Natural lives here too, since its conversions need Integer.
module GHC.Num.Integer (
    Integer(..), BigNat#, Natural(..),
    integerCheck, integerZero, integerOne, integerIsZero, integerIsNegative, integerIsNegative#,
    integerIsOne, integerFromInt#, integerFromInt, integerToInt#, integerToInt,
    integerFromInt64#, integerToInt64#, integerToInt64,
    integerFromWord#, integerFromWord, integerToWord#, integerToWord, integerFromWord64#,
    integerToWord64#, integerFromWordList, integerFromWordNeg, integerFromWordSign,
    integerAdd, integerSub, integerMul, integerNegate, integerAbs, integerSignum,
    integerQuot, integerRem, integerDiv, integerMod, integerQuotRem, integerQuotRem#,
    integerDivMod, integerDivMod#, integerGcd, integerLcm, integerGcde, integerPowMod,
    integerRecipMod, integerSqr,
    integerEq, integerNe, integerLt, integerLe, integerGt, integerGe, integerCompare,
    integerEq#, integerNe#, integerLt#, integerLe#, integerGt#, integerGe#,
    integerAnd, integerOr, integerXor, integerComplement, integerShiftL, integerShiftL#,
    integerShiftR, integerShiftR#, integerTestBit, integerTestBit#, integerBit#,
    integerPopCount#, integerLog2#, integerLogBase, integerLogBase#, integerLogBaseWord,
    integerIsPowerOf2#, integerSizeInBase#, integerTake,
    integerEncodeDouble#, integerEncodeFloat#, integerDecodeDouble#,
    integerFromNatural, integerToNatural, integerToNaturalClamp, integerToNaturalThrow,
    integerToBigNatClamp, integerToBigNatSign, integerBigNat#, integerFromBigNat#,
    integerFromBigNatNeg, integerFromBigNatSign, integerToWordMaybe#,
    naturalAdd, naturalAnd, naturalAndNot, naturalBit, naturalBit#, naturalCheck, naturalCheck#, naturalClearBit, naturalClearBit#, naturalCompare, naturalComplementBit, naturalComplementBit#, naturalEncodeDouble#, naturalEncodeFloat#, naturalEq, naturalEq#, naturalFromBigNat#, naturalFromWord, naturalFromWord#, naturalFromWord2#, naturalFromWordList, naturalGcd, naturalGe, naturalGe#, naturalGt, naturalGt#, naturalIsOne, naturalIsPowerOf2#, naturalIsZero, naturalLcm, naturalLe, naturalLe#, naturalLog2, naturalLog2#, naturalLogBase, naturalLogBase#, naturalLogBaseWord, naturalLogBaseWord#, naturalLt, naturalLt#, naturalMul, naturalNe, naturalNe#, naturalNegate, naturalOne, naturalOr, naturalPopCount, naturalPopCount#, naturalPowMod, naturalQuot, naturalQuotRem, naturalQuotRem#, naturalRem, naturalSetBit, naturalSetBit#, naturalShiftL, naturalShiftL#, naturalShiftR, naturalShiftR#, naturalSignum, naturalSizeInBase#, naturalSqr, naturalSub, naturalSubThrow, naturalSubUnsafe, naturalTestBit, naturalTestBit#, naturalToBigNat#, naturalToWord, naturalToWord#, naturalToWordClamp, naturalToWordClamp#, naturalToWordMaybe#, naturalXor, naturalZero
) where

import GHC.Prim
import GHC.Types
import GHC.Classes

-- | A magnitude, as GHC's: a byte array of 64-bit limbs, least significant first (the
-- runtime's integer primitives read and write it).
type BigNat# = ByteArray#

-- | An integer: small values in IS, otherwise a magnitude with a sign (IP positive, IN negative).
data Integer = IS Int# | IP BigNat# | IN BigNat#

-- | A natural number: NS holds one that fits a Word#, NB the others.
data Natural = NS Word# | NB BigNat#

-- ----- primitives over the native bignum -------------------------------------------------

foreign import prim "integerAdd" integerAdd :: Integer -> Integer -> Integer
foreign import prim "integerSub" integerSub :: Integer -> Integer -> Integer
foreign import prim "integerMul" integerMul :: Integer -> Integer -> Integer
foreign import prim "integerNegate" integerNegate :: Integer -> Integer
foreign import prim "integerAbs" integerAbs :: Integer -> Integer
foreign import prim "integerSignum" integerSignum :: Integer -> Integer
foreign import prim "integerQuot" integerQuot :: Integer -> Integer -> Integer
foreign import prim "integerRem" integerRem :: Integer -> Integer -> Integer
foreign import prim "integerDiv" integerDiv :: Integer -> Integer -> Integer
foreign import prim "integerMod" integerMod :: Integer -> Integer -> Integer
foreign import prim "integerEq" integerEqB :: Integer -> Integer -> Bool
foreign import prim "integerLt" integerLt :: Integer -> Integer -> Bool
foreign import prim "integerLe" integerLe :: Integer -> Integer -> Bool
foreign import prim "integerToInt" integerToInt# :: Integer -> Int#
foreign import prim "intToInteger" integerFromInt# :: Int# -> Integer
foreign import prim "integerToDouble" integerToDoubleP# :: Integer -> Double#
foreign import prim "integerAnd" integerAnd :: Integer -> Integer -> Integer
foreign import prim "integerOr" integerOr :: Integer -> Integer -> Integer
foreign import prim "integerXor" integerXor :: Integer -> Integer -> Integer
foreign import prim "integerShiftL" integerShiftL# :: Integer -> Word# -> Integer
foreign import prim "integerShiftR" integerShiftR# :: Integer -> Word# -> Integer
foreign import prim "integerGcd" integerGcd :: Integer -> Integer -> Integer
foreign import prim "integerPopCount" integerPopCount# :: Integer -> Int#
foreign import prim "integerLog2" integerLog2# :: Integer -> Word#
foreign import prim "integerTestBit" integerTestBitB# :: Integer -> Word# -> Bool
foreign import prim "integerBigNat" integerBigNat# :: Integer -> BigNat#
foreign import prim "integerFromBigNat" integerFromBigNat# :: BigNat# -> Integer
foreign import prim "integerDecodeDouble" integerDecodeDouble# :: Double# -> (# Integer, Int# #)
foreign import prim "integerEncodeDouble" integerEncodeDouble# :: Integer -> Int# -> Double#

-- ----- helpers ----------------------------------------------------------------------------

boolInt :: Bool -> Int#
boolInt True = 1#
boolInt False = 0#

integerEq :: Integer -> Integer -> Bool
integerEq = integerEqB

integerNe :: Integer -> Integer -> Bool
integerNe a b = not (integerEqB a b)

integerGt :: Integer -> Integer -> Bool
integerGt a b = integerLt b a

integerGe :: Integer -> Integer -> Bool
integerGe a b = integerLe b a

integerCompare :: Integer -> Integer -> Ordering
integerCompare a b = if integerEqB a b then EQ else if integerLt a b then LT else GT

integerEq#, integerNe#, integerLt#, integerLe#, integerGt#, integerGe# :: Integer -> Integer -> Int#
integerEq# a b = boolInt (integerEqB a b)
integerNe# a b = boolInt (integerNe a b)
integerLt# a b = boolInt (integerLt a b)
integerLe# a b = boolInt (integerLe a b)
integerGt# a b = boolInt (integerLt b a)
integerGe# a b = boolInt (integerLe b a)

integerToInt :: Integer -> Int
integerToInt n = I# (integerToInt# n)

integerFromInt :: Int -> Integer
integerFromInt (I# i) = integerFromInt# i

integerZero, integerOne :: Integer
integerZero = IS 0#
integerOne = IS 1#

integerIsZero :: Integer -> Bool
integerIsZero n = integerEqB n integerZero

integerIsOne :: Integer -> Bool
integerIsOne n = integerEqB n integerOne

integerIsNegative :: Integer -> Bool
integerIsNegative n = integerLt n integerZero

integerIsNegative# :: Integer -> Int#
integerIsNegative# n = boolInt (integerIsNegative n)

integerCheck :: Integer -> Bool
integerCheck _ = True

integerFromInt64# :: Int64# -> Integer
integerFromInt64# i = integerFromInt# (int64ToInt# i)

integerToInt64# :: Integer -> Int64#
integerToInt64# n = intToInt64# (integerToInt# n)

integerToInt64 :: Integer -> Int
integerToInt64 n = I# (integerToInt# n)

-- | The low 64 bits as a Word#.
integerToWord# :: Integer -> Word#
integerToWord# n = int2Word# (integerToInt# n)

integerToWord :: Integer -> Word
integerToWord n = W# (integerToWord# n)

integerFromWord# :: Word# -> Integer
integerFromWord# w =
  let i = word2Int# w
  in if isTrue# (i >=# 0#) then integerFromInt# i else integerAdd (integerFromInt# i) (integerShiftL# integerOne 64##)

integerFromWord :: Word -> Integer
integerFromWord (W# w) = integerFromWord# w

integerFromWord64# :: Word64# -> Integer
integerFromWord64# w = integerFromWord# (word64ToWord# w)

integerToWord64# :: Integer -> Word64#
integerToWord64# n = wordToWord64# (integerToWord# n)

integerFromWordList :: Bool -> [Word] -> Integer
integerFromWordList neg ws = go ws integerZero
  where
    go [] acc = if neg then integerNegate acc else acc
    go (W# w : rest) acc = go rest (integerAdd (integerShiftL# acc 64##) (integerFromWord# w))

integerFromWordNeg :: Word# -> Integer
integerFromWordNeg w = integerNegate (integerFromWord# w)

integerFromWordSign :: Bool -> Word# -> Integer
integerFromWordSign neg w = if neg then integerFromWordNeg w else integerFromWord# w

-- | The value as a Word#, if it is in range.
integerToWordMaybe# :: Integer -> (# (# #) | Word# #)
integerToWordMaybe# n =
  if integerLt n integerZero then (# (# #) | #)
  else if integerLe n integerMaxWord then (# | integerToWord# n #)
  else (# (# #) | #)

integerMaxWord :: Integer
integerMaxWord = integerSub (integerShiftL# integerOne 64##) integerOne

integerShiftL, integerShiftR :: Integer -> Word -> Integer
integerShiftL n (W# k) = integerShiftL# n k
integerShiftR n (W# k) = integerShiftR# n k

integerLcm :: Integer -> Integer -> Integer
integerLcm a b =
  if integerIsZero a || integerIsZero b then integerZero
  else integerAbs (integerMul (integerQuot a (integerGcd a b)) b)

integerComplement :: Integer -> Integer
integerComplement n = integerSub (integerNegate n) integerOne

integerSqr :: Integer -> Integer
integerSqr n = integerMul n n

integerBit# :: Word# -> Integer
integerBit# n = integerShiftL# integerOne n

integerTestBit :: Integer -> Word -> Bool
integerTestBit n (W# k) = integerTestBitB# n k

integerTestBit# :: Integer -> Word# -> Int#
integerTestBit# n k = boolInt (integerTestBitB# n k)

integerLogBaseWord :: Word -> Integer -> Word
integerLogBaseWord (W# b) n = W# (integerLogBase# (integerFromWord# b) n)

integerLogBase# :: Integer -> Integer -> Word#
integerLogBase# b n = go 0## n
  where
    go acc m = if integerLt m b then acc else go (plusWord# acc 1##) (integerQuot m b)

integerLogBase :: Integer -> Integer -> Word
integerLogBase b n = W# (integerLogBase# b n)

integerIsPowerOf2# :: Integer -> (# (# #) | Word# #)
integerIsPowerOf2# n =
  if integerLe n integerZero then (# (# #) | #)
  else if integerEqB (integerAnd n (integerSub n integerOne)) integerZero then (# | integerLog2# n #)
  else (# (# #) | #)

integerSizeInBase# :: Word# -> Integer -> Word#
integerSizeInBase# b n = plusWord# (integerLogBase# (integerFromWord# b) (integerAbs n)) 1##

integerTake :: Word -> Integer -> Integer
integerTake (W# n) i = integerAnd i (integerSub (integerShiftL# integerOne n) integerOne)

integerEncodeFloat# :: Integer -> Int# -> Float#
integerEncodeFloat# m e = double2Float# (integerEncodeDouble# m e)

integerQuotRem :: Integer -> Integer -> (Integer, Integer)
integerQuotRem a b = (integerQuot a b, integerRem a b)

integerQuotRem# :: Integer -> Integer -> (# Integer, Integer #)
integerQuotRem# a b = (# integerQuot a b, integerRem a b #)

integerDivMod :: Integer -> Integer -> (Integer, Integer)
integerDivMod a b = (integerDiv a b, integerMod a b)

integerDivMod# :: Integer -> Integer -> (# Integer, Integer #)
integerDivMod# a b = (# integerDiv a b, integerMod a b #)

-- | Extended gcd: (g, x, y) with a*x + b*y = g.
integerGcde :: Integer -> Integer -> (Integer, Integer, Integer)
integerGcde a b = go a b integerOne integerZero integerZero integerOne
  where
    go r0 r1 s0 s1 t0 t1 =
      if integerIsZero r1
        then (if integerLt r0 integerZero then (integerNegate r0, integerNegate s0, integerNegate t0) else (r0, s0, t0))
        else
          let q = integerQuot r0 r1
          in go r1 (integerSub r0 (integerMul q r1)) s1 (integerSub s0 (integerMul q s1)) t1 (integerSub t0 (integerMul q t1))

integerPowMod :: Integer -> Integer -> Integer -> Integer
integerPowMod b e m = integerMod (go b e integerOne) m
  where
    go base ex acc =
      if integerIsZero ex then acc
      else
        let acc' = if integerTestBit ex (W# 0##) then integerMod (integerMul acc base) m else acc
        in go (integerMod (integerMul base base) m) (integerShiftR# ex 1##) acc'

integerRecipMod :: Integer -> Integer -> Integer
integerRecipMod a m = case integerGcde a m of
  (_, x, _) -> integerMod x m

integerFromBigNatNeg :: BigNat# -> Integer
integerFromBigNatNeg b = integerNegate (integerFromBigNat# b)

integerFromBigNatSign :: Int# -> BigNat# -> Integer
integerFromBigNatSign s b = if isTrue# (s ==# 0#) then integerFromBigNat# b else integerFromBigNatNeg b

integerToBigNatClamp :: Integer -> BigNat#
integerToBigNatClamp n = integerBigNat# (if integerLt n integerZero then integerZero else n)

integerToBigNatSign :: Integer -> (# Int#, BigNat# #)
integerToBigNatSign n = (# boolInt (integerLt n integerZero), integerBigNat# n #)

integerDecodeDouble :: Double# -> (# Integer, Int# #)
integerDecodeDouble d = integerDecodeDouble# d

-- ----- Natural ---------------------------------------------------------------------------

integerFromNatural :: Natural -> Integer
integerFromNatural n = naturalToInteger n

integerToNatural :: Integer -> Natural
integerToNatural n = naturalFromInteger (integerAbs n)

integerToNaturalClamp :: Integer -> Natural
integerToNaturalClamp n = if integerLt n integerZero then naturalZero else naturalFromInteger n

integerToNaturalThrow :: Integer -> Natural
integerToNaturalThrow n = naturalFromInteger n

naturalCheck :: Natural -> Bool
naturalCheck (NS _) = True
naturalCheck (NB b) = integerLt integerMaxWord (integerFromBigNat# b)

naturalToInteger :: Natural -> Integer
naturalToInteger (NS w) = integerFromWord# w
naturalToInteger (NB b) = integerFromBigNat# b

-- | The argument must not be negative.
naturalFromInteger :: Integer -> Natural
naturalFromInteger n = case integerToWordMaybe# n of
  (# | w #) -> NS w
  (# (# #) | #) -> NB (integerBigNat# n)

naturalFromWord# :: Word# -> Natural
naturalFromWord# w = NS w

naturalFromWord :: Word -> Natural
naturalFromWord (W# w) = NS w

naturalToWord# :: Natural -> Word#
naturalToWord# (NS w) = w
naturalToWord# (NB b) = integerToWord# (integerFromBigNat# b)

naturalToWord :: Natural -> Word
naturalToWord n = W# (naturalToWord# n)

naturalToWordMaybe# :: Natural -> (# (# #) | Word# #)
naturalToWordMaybe# (NS w) = (# | w #)
naturalToWordMaybe# (NB _) = (# (# #) | #)

naturalToWordClamp# :: Natural -> Word#
naturalToWordClamp# (NS w) = w
naturalToWordClamp# (NB _) = 18446744073709551615##

naturalZero, naturalOne :: Natural
naturalZero = NS 0##
naturalOne = NS 1##

naturalIsZero :: Natural -> Bool
naturalIsZero n = integerIsZero (naturalToInteger n)

naturalEq, naturalNe, naturalLt, naturalLe, naturalGt, naturalGe :: Natural -> Natural -> Bool
naturalEq a b = integerEqB (naturalToInteger a) (naturalToInteger b)
naturalNe a b = not (naturalEq a b)
naturalLt a b = integerLt (naturalToInteger a) (naturalToInteger b)
naturalLe a b = integerLe (naturalToInteger a) (naturalToInteger b)
naturalGt a b = naturalLt b a
naturalGe a b = naturalLe b a

naturalCompare :: Natural -> Natural -> Ordering
naturalCompare a b = integerCompare (naturalToInteger a) (naturalToInteger b)

naturalLift2 :: (Integer -> Integer -> Integer) -> Natural -> Natural -> Natural
naturalLift2 f a b = naturalFromInteger (f (naturalToInteger a) (naturalToInteger b))

naturalAdd, naturalMul, naturalQuot, naturalRem, naturalAnd, naturalOr, naturalXor :: Natural -> Natural -> Natural
naturalAdd = naturalLift2 integerAdd
naturalMul = naturalLift2 integerMul
naturalQuot = naturalLift2 integerQuot
naturalRem = naturalLift2 integerRem
naturalAnd = naturalLift2 integerAnd
naturalOr = naturalLift2 integerOr
naturalXor = naturalLift2 integerXor

naturalGcd, naturalLcm :: Natural -> Natural -> Natural
naturalGcd = naturalLift2 integerGcd
naturalLcm = naturalLift2 integerLcm

naturalQuotRem :: Natural -> Natural -> (Natural, Natural)
naturalQuotRem a b = (naturalQuot a b, naturalRem a b)

-- | Subtraction; the sum tells whether the result is negative (the unit alternative).
naturalSub :: Natural -> Natural -> (# (# #) | Natural #)
naturalSub a b =
  let d = integerSub (naturalToInteger a) (naturalToInteger b)
  in if integerLt d integerZero then (# (# #) | #) else (# | naturalFromInteger d #)

naturalSubThrow :: Natural -> Natural -> Natural
naturalSubThrow a b = naturalFromInteger (integerSub (naturalToInteger a) (naturalToInteger b))

naturalSubUnsafe :: Natural -> Natural -> Natural
naturalSubUnsafe = naturalSubThrow

naturalSignum :: Natural -> Natural
naturalSignum n = if naturalIsZero n then naturalZero else naturalOne

naturalNegate :: Natural -> Natural
naturalNegate n = if naturalIsZero n then n else naturalFromInteger (integerNegate (naturalToInteger n))

naturalShiftL, naturalShiftR :: Natural -> Word -> Natural
naturalShiftL n (W# k) = naturalFromInteger (integerShiftL# (naturalToInteger n) k)
naturalShiftR n (W# k) = naturalFromInteger (integerShiftR# (naturalToInteger n) k)

naturalTestBit :: Natural -> Word -> Bool
naturalTestBit n (W# k) = integerTestBitB# (naturalToInteger n) k

naturalPopCount :: Natural -> Word
naturalPopCount n = W# (int2Word# (integerPopCount# (naturalToInteger n)))

naturalPopCount# :: Natural -> Word#
naturalPopCount# n = int2Word# (integerPopCount# (naturalToInteger n))

naturalToDouble# :: Natural -> Double#
naturalToDouble# n = integerToDoubleP# (naturalToInteger n)

naturalToFloat# :: Natural -> Float#
naturalToFloat# n = double2Float# (naturalToDouble# n)

naturalLog2 :: Natural -> Word
naturalLog2 n = W# (integerLog2# (naturalToInteger n))

naturalFromWordList :: [Word] -> Natural
naturalFromWordList ws = naturalFromInteger (integerFromWordList False ws)

naturalBit# :: Word# -> Natural
naturalBit# k = naturalFromInteger (integerBit# k)

naturalSetBit, naturalClearBit, naturalComplementBit :: Natural -> Word -> Natural
naturalSetBit n (W# i) = naturalOr n (naturalBit# i)
naturalClearBit n (W# i) = if naturalTestBit n (W# i) then naturalXor n (naturalBit# i) else n
naturalComplementBit n (W# i) = naturalXor n (naturalBit# i)

naturalPowMod :: Natural -> Natural -> Natural -> Natural
naturalPowMod b e m = naturalFromInteger (integerPowMod (naturalToInteger b) (naturalToInteger e) (naturalToInteger m))

-- ----- instances --------------------------------------------------------------------------

instance Eq Integer where
  a == b = integerEqB a b
  a /= b = integerNe a b

instance Ord Integer where
  compare = integerCompare
  a < b = integerLt a b
  a <= b = integerLe a b
  a > b = integerLt b a
  a >= b = integerLe b a

instance Eq Natural where
  a == b = naturalEq a b
  a /= b = naturalNe a b

instance Ord Natural where
  compare = naturalCompare
  a < b = naturalLt a b
  a <= b = naturalLe a b
  a > b = naturalGt a b
  a >= b = naturalGe a b

naturalIsOne :: Natural -> Bool
naturalIsOne n = integerIsOne (naturalToInteger n)

naturalFromWord2 :: Word# -> Word# -> Natural
naturalFromWord2 hi lo = naturalFromInteger (integerAdd (integerShiftL# (integerFromWord# hi) 64##) (integerFromWord# lo))

naturalFromBigNat :: BigNat# -> Natural
naturalFromBigNat b = naturalFromInteger (integerFromBigNat# b)

naturalToBigNat :: Natural -> BigNat#
naturalToBigNat n = integerBigNat# (naturalToInteger n)

naturalBit :: Word -> Natural
naturalBit (W# i) = naturalBit# i

naturalAndNot :: Natural -> Natural -> Natural
naturalAndNot a b = naturalFromInteger (integerAnd (naturalToInteger a) (integerComplement (naturalToInteger b)))

naturalSqr :: Natural -> Natural
naturalSqr a = naturalMul a a

naturalLogBase :: Natural -> Natural -> Word
naturalLogBase b a = W# (integerLogBase# (naturalToInteger b) (naturalToInteger a))

naturalLogBaseWord :: Word -> Natural -> Word
naturalLogBaseWord (W# b) a = W# (integerLogBase# (integerFromWord# b) (naturalToInteger a))

naturalSizeInBase :: Word -> Natural -> Word
naturalSizeInBase (W# b) n = W# (integerSizeInBase# b (naturalToInteger n))

naturalIsPowerOf2# :: Natural -> (# (# #) | Word# #)
naturalIsPowerOf2# n = integerIsPowerOf2# (naturalToInteger n)

naturalToWordClamp :: Natural -> Word
naturalToWordClamp n = W# (naturalToWordClamp# n)

naturalCheck# :: Natural -> Int#
naturalCheck# n = boolInt (naturalCheck n)

naturalClearBit#, naturalComplementBit#, naturalSetBit# :: Natural -> Word# -> Natural
naturalSetBit# n i = naturalOr n (naturalBit# i)
naturalClearBit# n i = if isTrue# (naturalTestBit# n i) then naturalXor n (naturalBit# i) else n
naturalComplementBit# n i = naturalXor n (naturalBit# i)

naturalEq#, naturalNe#, naturalLt#, naturalLe#, naturalGt#, naturalGe# :: Natural -> Natural -> Int#
naturalEq# a b = boolInt (naturalEq a b)
naturalNe# a b = boolInt (naturalNe a b)
naturalLt# a b = boolInt (naturalLt a b)
naturalLe# a b = boolInt (naturalLe a b)
naturalGt# a b = boolInt (naturalGt a b)
naturalGe# a b = boolInt (naturalGe a b)

naturalFromWord2# :: Word# -> Word# -> Natural
naturalFromWord2# hi lo = naturalFromWord2 hi lo

naturalFromBigNat# :: BigNat# -> Natural
naturalFromBigNat# b = naturalFromInteger (integerFromBigNat# b)

naturalToBigNat# :: Natural -> BigNat#
naturalToBigNat# n = integerBigNat# (naturalToInteger n)

naturalLog2# :: Natural -> Word#
naturalLog2# n = integerLog2# (naturalToInteger n)

naturalLogBase# :: Natural -> Natural -> Word#
naturalLogBase# b a = integerLogBase# (naturalToInteger b) (naturalToInteger a)

naturalLogBaseWord# :: Word# -> Natural -> Word#
naturalLogBaseWord# b a = integerLogBase# (integerFromWord# b) (naturalToInteger a)

naturalQuotRem# :: Natural -> Natural -> (# Natural, Natural #)
naturalQuotRem# a b = (# naturalQuot a b, naturalRem a b #)

naturalShiftL#, naturalShiftR# :: Natural -> Word# -> Natural
naturalShiftL# n k = naturalFromInteger (integerShiftL# (naturalToInteger n) k)
naturalShiftR# n k = naturalFromInteger (integerShiftR# (naturalToInteger n) k)

naturalTestBit# :: Natural -> Word# -> Int#
naturalTestBit# n k = integerTestBit# (naturalToInteger n) k

naturalEncodeDouble# :: Natural -> Int# -> Double#
naturalEncodeDouble# n e = integerEncodeDouble# (naturalToInteger n) e

naturalEncodeFloat# :: Natural -> Int# -> Float#
naturalEncodeFloat# n e = double2Float# (naturalEncodeDouble# n e)

naturalSizeInBase# :: Word# -> Natural -> Word#
naturalSizeInBase# b n = integerSizeInBase# b (naturalToInteger n)
