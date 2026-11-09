module Data.Bits (Bits(..), FiniteBits(..)) where

import Base

infixl 8 `shiftL`, `shiftR`, `shift`
infixl 7 .&.
infixl 6 `xor`
infixl 5 .|.

foreign import prim "intAnd" primIntAnd :: Int -> Int -> Int
foreign import prim "intOr" primIntOr :: Int -> Int -> Int
foreign import prim "intXor" primIntXor :: Int -> Int -> Int
foreign import prim "intShiftL" primIntShiftL :: Int -> Int -> Int
foreign import prim "intShiftR" primIntShiftR :: Int -> Int -> Int
foreign import prim "intPopCount" primIntPopCount :: Int -> Int

class Eq a => Bits a where
  (.&.) :: a -> a -> a
  (.|.) :: a -> a -> a
  xor :: a -> a -> a
  complement :: a -> a
  shift :: a -> Int -> a
  shiftL :: a -> Int -> a
  shiftR :: a -> Int -> a
  testBit :: a -> Int -> Bool
  bit :: Int -> a
  setBit :: a -> Int -> a
  clearBit :: a -> Int -> a
  popCount :: a -> Int
  zeroBits :: a
  shift x n = if n >= 0 then shiftL x n else shiftR x (negate n)

class Bits b => FiniteBits b where
  finiteBitSize :: b -> Int
  countLeadingZeros :: b -> Int
  countTrailingZeros :: b -> Int

instance Bits Int where
  (.&.) = primIntAnd
  (.|.) = primIntOr
  xor = primIntXor
  complement x = primIntXor x (negate 1)
  shiftL x n = primIntShiftL x n
  shiftR x n = primIntShiftR x n
  testBit x n = primIntAnd (primIntShiftR x n) 1 == 1
  bit n = primIntShiftL 1 n
  setBit x n = primIntOr x (primIntShiftL 1 n)
  clearBit x n = primIntAnd x (complement (primIntShiftL 1 n))
  popCount x = primIntPopCount x
  zeroBits = 0

instance FiniteBits Int where
  finiteBitSize _ = 64
  countLeadingZeros x = length (takeWhile (\i -> not (testBit x i)) (reverse (enumFromTo 0 63)))
  countTrailingZeros x = length (takeWhile (\i -> not (testBit x i)) (enumFromTo 0 63))

instance Bits Integer where
  x .&. y = fromIntegral (primIntAnd (fromIntegral x) (fromIntegral y))
  x .|. y = fromIntegral (primIntOr (fromIntegral x) (fromIntegral y))
  xor x y = fromIntegral (primIntXor (fromIntegral x) (fromIntegral y))
  complement x = negate x - 1
  shiftL x n = x * 2 ^ n
  shiftR x n = x `div` (2 ^ n)
  testBit x n = odd (x `div` (2 ^ n))
  bit n = 2 ^ n
  setBit x n = x .|. bit n
  clearBit x n = if testBit x n then x - bit n else x
  popCount x = popCount (fromIntegral x :: Int)
  zeroBits = 0
