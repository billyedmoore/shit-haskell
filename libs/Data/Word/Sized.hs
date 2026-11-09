-- Generated: sized integral types as wrapped Integer values.
module Data.Word.Sized where

import Base hiding (Word)
import Data.Bits

newtype Word8 = W8 Integer

normWord8 :: Integer -> Word8
normWord8 x = W8 (x `mod` 256)

instance Eq Word8 where
  W8 a == W8 b = a == b

instance Ord Word8 where
  compare (W8 a) (W8 b) = compare a b

instance Show Word8 where
  showsPrec d (W8 a) = showsPrec d a

instance Num Word8 where
  W8 a + W8 b = normWord8 (a + b)
  W8 a - W8 b = normWord8 (a - b)
  W8 a * W8 b = normWord8 (a * b)
  negate (W8 a) = normWord8 (negate a)
  abs (W8 a) = normWord8 (abs a)
  signum (W8 a) = normWord8 (signum a)
  fromInteger x = normWord8 x

instance Real Word8 where
  toRational (W8 a) = toRational a

instance Enum Word8 where
  toEnum n = normWord8 (toInteger n)
  fromEnum (W8 a) = fromInteger a
  enumFrom x = enumFromTo x maxBound
  enumFromTo (W8 a) (W8 b) = map W8 (enumFromTo a b)
  enumFromThen x y = enumFromThenTo x y (if y >= x then maxBound else minBound)
  enumFromThenTo (W8 a) (W8 b) (W8 c) = map W8 (enumFromThenTo a b c)

instance Integral Word8 where
  quot (W8 a) (W8 b) = normWord8 (quot a b)
  rem (W8 a) (W8 b) = normWord8 (rem a b)
  div (W8 a) (W8 b) = normWord8 (div a b)
  mod (W8 a) (W8 b) = normWord8 (mod a b)
  toInteger (W8 a) = a

instance Bounded Word8 where
  minBound = W8 (0)
  maxBound = W8 255

instance Bits Word8 where
  W8 a .&. W8 b = normWord8 (a .&. b)
  W8 a .|. W8 b = normWord8 (a .|. b)
  xor (W8 a) (W8 b) = normWord8 (xor a b)
  complement (W8 a) = normWord8 (complement a)
  shiftL (W8 a) n = normWord8 (shiftL a n)
  shiftR (W8 a) n = normWord8 (shiftR a n)
  testBit (W8 a) n = testBit a n
  bit n = normWord8 (bit n)
  setBit (W8 a) n = normWord8 (setBit a n)
  clearBit (W8 a) n = normWord8 (clearBit a n)
  popCount (W8 a) = popCount (a `mod` 256)
  zeroBits = W8 0


newtype Word16 = W16 Integer

normWord16 :: Integer -> Word16
normWord16 x = W16 (x `mod` 65536)

instance Eq Word16 where
  W16 a == W16 b = a == b

instance Ord Word16 where
  compare (W16 a) (W16 b) = compare a b

instance Show Word16 where
  showsPrec d (W16 a) = showsPrec d a

instance Num Word16 where
  W16 a + W16 b = normWord16 (a + b)
  W16 a - W16 b = normWord16 (a - b)
  W16 a * W16 b = normWord16 (a * b)
  negate (W16 a) = normWord16 (negate a)
  abs (W16 a) = normWord16 (abs a)
  signum (W16 a) = normWord16 (signum a)
  fromInteger x = normWord16 x

instance Real Word16 where
  toRational (W16 a) = toRational a

instance Enum Word16 where
  toEnum n = normWord16 (toInteger n)
  fromEnum (W16 a) = fromInteger a
  enumFrom x = enumFromTo x maxBound
  enumFromTo (W16 a) (W16 b) = map W16 (enumFromTo a b)
  enumFromThen x y = enumFromThenTo x y (if y >= x then maxBound else minBound)
  enumFromThenTo (W16 a) (W16 b) (W16 c) = map W16 (enumFromThenTo a b c)

instance Integral Word16 where
  quot (W16 a) (W16 b) = normWord16 (quot a b)
  rem (W16 a) (W16 b) = normWord16 (rem a b)
  div (W16 a) (W16 b) = normWord16 (div a b)
  mod (W16 a) (W16 b) = normWord16 (mod a b)
  toInteger (W16 a) = a

instance Bounded Word16 where
  minBound = W16 (0)
  maxBound = W16 65535

instance Bits Word16 where
  W16 a .&. W16 b = normWord16 (a .&. b)
  W16 a .|. W16 b = normWord16 (a .|. b)
  xor (W16 a) (W16 b) = normWord16 (xor a b)
  complement (W16 a) = normWord16 (complement a)
  shiftL (W16 a) n = normWord16 (shiftL a n)
  shiftR (W16 a) n = normWord16 (shiftR a n)
  testBit (W16 a) n = testBit a n
  bit n = normWord16 (bit n)
  setBit (W16 a) n = normWord16 (setBit a n)
  clearBit (W16 a) n = normWord16 (clearBit a n)
  popCount (W16 a) = popCount (a `mod` 65536)
  zeroBits = W16 0


newtype Word32 = W32 Integer

normWord32 :: Integer -> Word32
normWord32 x = W32 (x `mod` 4294967296)

instance Eq Word32 where
  W32 a == W32 b = a == b

instance Ord Word32 where
  compare (W32 a) (W32 b) = compare a b

instance Show Word32 where
  showsPrec d (W32 a) = showsPrec d a

instance Num Word32 where
  W32 a + W32 b = normWord32 (a + b)
  W32 a - W32 b = normWord32 (a - b)
  W32 a * W32 b = normWord32 (a * b)
  negate (W32 a) = normWord32 (negate a)
  abs (W32 a) = normWord32 (abs a)
  signum (W32 a) = normWord32 (signum a)
  fromInteger x = normWord32 x

instance Real Word32 where
  toRational (W32 a) = toRational a

instance Enum Word32 where
  toEnum n = normWord32 (toInteger n)
  fromEnum (W32 a) = fromInteger a
  enumFrom x = enumFromTo x maxBound
  enumFromTo (W32 a) (W32 b) = map W32 (enumFromTo a b)
  enumFromThen x y = enumFromThenTo x y (if y >= x then maxBound else minBound)
  enumFromThenTo (W32 a) (W32 b) (W32 c) = map W32 (enumFromThenTo a b c)

instance Integral Word32 where
  quot (W32 a) (W32 b) = normWord32 (quot a b)
  rem (W32 a) (W32 b) = normWord32 (rem a b)
  div (W32 a) (W32 b) = normWord32 (div a b)
  mod (W32 a) (W32 b) = normWord32 (mod a b)
  toInteger (W32 a) = a

instance Bounded Word32 where
  minBound = W32 (0)
  maxBound = W32 4294967295

instance Bits Word32 where
  W32 a .&. W32 b = normWord32 (a .&. b)
  W32 a .|. W32 b = normWord32 (a .|. b)
  xor (W32 a) (W32 b) = normWord32 (xor a b)
  complement (W32 a) = normWord32 (complement a)
  shiftL (W32 a) n = normWord32 (shiftL a n)
  shiftR (W32 a) n = normWord32 (shiftR a n)
  testBit (W32 a) n = testBit a n
  bit n = normWord32 (bit n)
  setBit (W32 a) n = normWord32 (setBit a n)
  clearBit (W32 a) n = normWord32 (clearBit a n)
  popCount (W32 a) = popCount (a `mod` 4294967296)
  zeroBits = W32 0


newtype Word64 = W64 Integer

normWord64 :: Integer -> Word64
normWord64 x = W64 (x `mod` 18446744073709551616)

instance Eq Word64 where
  W64 a == W64 b = a == b

instance Ord Word64 where
  compare (W64 a) (W64 b) = compare a b

instance Show Word64 where
  showsPrec d (W64 a) = showsPrec d a

instance Num Word64 where
  W64 a + W64 b = normWord64 (a + b)
  W64 a - W64 b = normWord64 (a - b)
  W64 a * W64 b = normWord64 (a * b)
  negate (W64 a) = normWord64 (negate a)
  abs (W64 a) = normWord64 (abs a)
  signum (W64 a) = normWord64 (signum a)
  fromInteger x = normWord64 x

instance Real Word64 where
  toRational (W64 a) = toRational a

instance Enum Word64 where
  toEnum n = normWord64 (toInteger n)
  fromEnum (W64 a) = fromInteger a
  enumFrom x = enumFromTo x maxBound
  enumFromTo (W64 a) (W64 b) = map W64 (enumFromTo a b)
  enumFromThen x y = enumFromThenTo x y (if y >= x then maxBound else minBound)
  enumFromThenTo (W64 a) (W64 b) (W64 c) = map W64 (enumFromThenTo a b c)

instance Integral Word64 where
  quot (W64 a) (W64 b) = normWord64 (quot a b)
  rem (W64 a) (W64 b) = normWord64 (rem a b)
  div (W64 a) (W64 b) = normWord64 (div a b)
  mod (W64 a) (W64 b) = normWord64 (mod a b)
  toInteger (W64 a) = a

instance Bounded Word64 where
  minBound = W64 (0)
  maxBound = W64 18446744073709551615

instance Bits Word64 where
  W64 a .&. W64 b = normWord64 (a .&. b)
  W64 a .|. W64 b = normWord64 (a .|. b)
  xor (W64 a) (W64 b) = normWord64 (xor a b)
  complement (W64 a) = normWord64 (complement a)
  shiftL (W64 a) n = normWord64 (shiftL a n)
  shiftR (W64 a) n = normWord64 (shiftR a n)
  testBit (W64 a) n = testBit a n
  bit n = normWord64 (bit n)
  setBit (W64 a) n = normWord64 (setBit a n)
  clearBit (W64 a) n = normWord64 (clearBit a n)
  popCount (W64 a) = popCount (a `mod` 18446744073709551616)
  zeroBits = W64 0


newtype Word = Wd Integer

normWord :: Integer -> Word
normWord x = Wd (x `mod` 18446744073709551616)

instance Eq Word where
  Wd a == Wd b = a == b

instance Ord Word where
  compare (Wd a) (Wd b) = compare a b

instance Show Word where
  showsPrec d (Wd a) = showsPrec d a

instance Num Word where
  Wd a + Wd b = normWord (a + b)
  Wd a - Wd b = normWord (a - b)
  Wd a * Wd b = normWord (a * b)
  negate (Wd a) = normWord (negate a)
  abs (Wd a) = normWord (abs a)
  signum (Wd a) = normWord (signum a)
  fromInteger x = normWord x

instance Real Word where
  toRational (Wd a) = toRational a

instance Enum Word where
  toEnum n = normWord (toInteger n)
  fromEnum (Wd a) = fromInteger a
  enumFrom x = enumFromTo x maxBound
  enumFromTo (Wd a) (Wd b) = map Wd (enumFromTo a b)
  enumFromThen x y = enumFromThenTo x y (if y >= x then maxBound else minBound)
  enumFromThenTo (Wd a) (Wd b) (Wd c) = map Wd (enumFromThenTo a b c)

instance Integral Word where
  quot (Wd a) (Wd b) = normWord (quot a b)
  rem (Wd a) (Wd b) = normWord (rem a b)
  div (Wd a) (Wd b) = normWord (div a b)
  mod (Wd a) (Wd b) = normWord (mod a b)
  toInteger (Wd a) = a

instance Bounded Word where
  minBound = Wd (0)
  maxBound = Wd 18446744073709551615

instance Bits Word where
  Wd a .&. Wd b = normWord (a .&. b)
  Wd a .|. Wd b = normWord (a .|. b)
  xor (Wd a) (Wd b) = normWord (xor a b)
  complement (Wd a) = normWord (complement a)
  shiftL (Wd a) n = normWord (shiftL a n)
  shiftR (Wd a) n = normWord (shiftR a n)
  testBit (Wd a) n = testBit a n
  bit n = normWord (bit n)
  setBit (Wd a) n = normWord (setBit a n)
  clearBit (Wd a) n = normWord (clearBit a n)
  popCount (Wd a) = popCount (a `mod` 18446744073709551616)
  zeroBits = Wd 0


newtype Int8 = I8 Integer

normInt8 :: Integer -> Int8
normInt8 x = I8 (((x + 128) `mod` 256) - 128)

instance Eq Int8 where
  I8 a == I8 b = a == b

instance Ord Int8 where
  compare (I8 a) (I8 b) = compare a b

instance Show Int8 where
  showsPrec d (I8 a) = showsPrec d a

instance Num Int8 where
  I8 a + I8 b = normInt8 (a + b)
  I8 a - I8 b = normInt8 (a - b)
  I8 a * I8 b = normInt8 (a * b)
  negate (I8 a) = normInt8 (negate a)
  abs (I8 a) = normInt8 (abs a)
  signum (I8 a) = normInt8 (signum a)
  fromInteger x = normInt8 x

instance Real Int8 where
  toRational (I8 a) = toRational a

instance Enum Int8 where
  toEnum n = normInt8 (toInteger n)
  fromEnum (I8 a) = fromInteger a
  enumFrom x = enumFromTo x maxBound
  enumFromTo (I8 a) (I8 b) = map I8 (enumFromTo a b)
  enumFromThen x y = enumFromThenTo x y (if y >= x then maxBound else minBound)
  enumFromThenTo (I8 a) (I8 b) (I8 c) = map I8 (enumFromThenTo a b c)

instance Integral Int8 where
  quot (I8 a) (I8 b) = normInt8 (quot a b)
  rem (I8 a) (I8 b) = normInt8 (rem a b)
  div (I8 a) (I8 b) = normInt8 (div a b)
  mod (I8 a) (I8 b) = normInt8 (mod a b)
  toInteger (I8 a) = a

instance Bounded Int8 where
  minBound = I8 (-128)
  maxBound = I8 127

instance Bits Int8 where
  I8 a .&. I8 b = normInt8 (a .&. b)
  I8 a .|. I8 b = normInt8 (a .|. b)
  xor (I8 a) (I8 b) = normInt8 (xor a b)
  complement (I8 a) = normInt8 (complement a)
  shiftL (I8 a) n = normInt8 (shiftL a n)
  shiftR (I8 a) n = normInt8 (shiftR a n)
  testBit (I8 a) n = testBit a n
  bit n = normInt8 (bit n)
  setBit (I8 a) n = normInt8 (setBit a n)
  clearBit (I8 a) n = normInt8 (clearBit a n)
  popCount (I8 a) = popCount (a `mod` 256)
  zeroBits = I8 0


newtype Int16 = I16 Integer

normInt16 :: Integer -> Int16
normInt16 x = I16 (((x + 32768) `mod` 65536) - 32768)

instance Eq Int16 where
  I16 a == I16 b = a == b

instance Ord Int16 where
  compare (I16 a) (I16 b) = compare a b

instance Show Int16 where
  showsPrec d (I16 a) = showsPrec d a

instance Num Int16 where
  I16 a + I16 b = normInt16 (a + b)
  I16 a - I16 b = normInt16 (a - b)
  I16 a * I16 b = normInt16 (a * b)
  negate (I16 a) = normInt16 (negate a)
  abs (I16 a) = normInt16 (abs a)
  signum (I16 a) = normInt16 (signum a)
  fromInteger x = normInt16 x

instance Real Int16 where
  toRational (I16 a) = toRational a

instance Enum Int16 where
  toEnum n = normInt16 (toInteger n)
  fromEnum (I16 a) = fromInteger a
  enumFrom x = enumFromTo x maxBound
  enumFromTo (I16 a) (I16 b) = map I16 (enumFromTo a b)
  enumFromThen x y = enumFromThenTo x y (if y >= x then maxBound else minBound)
  enumFromThenTo (I16 a) (I16 b) (I16 c) = map I16 (enumFromThenTo a b c)

instance Integral Int16 where
  quot (I16 a) (I16 b) = normInt16 (quot a b)
  rem (I16 a) (I16 b) = normInt16 (rem a b)
  div (I16 a) (I16 b) = normInt16 (div a b)
  mod (I16 a) (I16 b) = normInt16 (mod a b)
  toInteger (I16 a) = a

instance Bounded Int16 where
  minBound = I16 (-32768)
  maxBound = I16 32767

instance Bits Int16 where
  I16 a .&. I16 b = normInt16 (a .&. b)
  I16 a .|. I16 b = normInt16 (a .|. b)
  xor (I16 a) (I16 b) = normInt16 (xor a b)
  complement (I16 a) = normInt16 (complement a)
  shiftL (I16 a) n = normInt16 (shiftL a n)
  shiftR (I16 a) n = normInt16 (shiftR a n)
  testBit (I16 a) n = testBit a n
  bit n = normInt16 (bit n)
  setBit (I16 a) n = normInt16 (setBit a n)
  clearBit (I16 a) n = normInt16 (clearBit a n)
  popCount (I16 a) = popCount (a `mod` 65536)
  zeroBits = I16 0


newtype Int32 = I32 Integer

normInt32 :: Integer -> Int32
normInt32 x = I32 (((x + 2147483648) `mod` 4294967296) - 2147483648)

instance Eq Int32 where
  I32 a == I32 b = a == b

instance Ord Int32 where
  compare (I32 a) (I32 b) = compare a b

instance Show Int32 where
  showsPrec d (I32 a) = showsPrec d a

instance Num Int32 where
  I32 a + I32 b = normInt32 (a + b)
  I32 a - I32 b = normInt32 (a - b)
  I32 a * I32 b = normInt32 (a * b)
  negate (I32 a) = normInt32 (negate a)
  abs (I32 a) = normInt32 (abs a)
  signum (I32 a) = normInt32 (signum a)
  fromInteger x = normInt32 x

instance Real Int32 where
  toRational (I32 a) = toRational a

instance Enum Int32 where
  toEnum n = normInt32 (toInteger n)
  fromEnum (I32 a) = fromInteger a
  enumFrom x = enumFromTo x maxBound
  enumFromTo (I32 a) (I32 b) = map I32 (enumFromTo a b)
  enumFromThen x y = enumFromThenTo x y (if y >= x then maxBound else minBound)
  enumFromThenTo (I32 a) (I32 b) (I32 c) = map I32 (enumFromThenTo a b c)

instance Integral Int32 where
  quot (I32 a) (I32 b) = normInt32 (quot a b)
  rem (I32 a) (I32 b) = normInt32 (rem a b)
  div (I32 a) (I32 b) = normInt32 (div a b)
  mod (I32 a) (I32 b) = normInt32 (mod a b)
  toInteger (I32 a) = a

instance Bounded Int32 where
  minBound = I32 (-2147483648)
  maxBound = I32 2147483647

instance Bits Int32 where
  I32 a .&. I32 b = normInt32 (a .&. b)
  I32 a .|. I32 b = normInt32 (a .|. b)
  xor (I32 a) (I32 b) = normInt32 (xor a b)
  complement (I32 a) = normInt32 (complement a)
  shiftL (I32 a) n = normInt32 (shiftL a n)
  shiftR (I32 a) n = normInt32 (shiftR a n)
  testBit (I32 a) n = testBit a n
  bit n = normInt32 (bit n)
  setBit (I32 a) n = normInt32 (setBit a n)
  clearBit (I32 a) n = normInt32 (clearBit a n)
  popCount (I32 a) = popCount (a `mod` 4294967296)
  zeroBits = I32 0


newtype Int64 = I64 Integer

normInt64 :: Integer -> Int64
normInt64 x = I64 (((x + 9223372036854775808) `mod` 18446744073709551616) - 9223372036854775808)

instance Eq Int64 where
  I64 a == I64 b = a == b

instance Ord Int64 where
  compare (I64 a) (I64 b) = compare a b

instance Show Int64 where
  showsPrec d (I64 a) = showsPrec d a

instance Num Int64 where
  I64 a + I64 b = normInt64 (a + b)
  I64 a - I64 b = normInt64 (a - b)
  I64 a * I64 b = normInt64 (a * b)
  negate (I64 a) = normInt64 (negate a)
  abs (I64 a) = normInt64 (abs a)
  signum (I64 a) = normInt64 (signum a)
  fromInteger x = normInt64 x

instance Real Int64 where
  toRational (I64 a) = toRational a

instance Enum Int64 where
  toEnum n = normInt64 (toInteger n)
  fromEnum (I64 a) = fromInteger a
  enumFrom x = enumFromTo x maxBound
  enumFromTo (I64 a) (I64 b) = map I64 (enumFromTo a b)
  enumFromThen x y = enumFromThenTo x y (if y >= x then maxBound else minBound)
  enumFromThenTo (I64 a) (I64 b) (I64 c) = map I64 (enumFromThenTo a b c)

instance Integral Int64 where
  quot (I64 a) (I64 b) = normInt64 (quot a b)
  rem (I64 a) (I64 b) = normInt64 (rem a b)
  div (I64 a) (I64 b) = normInt64 (div a b)
  mod (I64 a) (I64 b) = normInt64 (mod a b)
  toInteger (I64 a) = a

instance Bounded Int64 where
  minBound = I64 (-9223372036854775808)
  maxBound = I64 9223372036854775807

instance Bits Int64 where
  I64 a .&. I64 b = normInt64 (a .&. b)
  I64 a .|. I64 b = normInt64 (a .|. b)
  xor (I64 a) (I64 b) = normInt64 (xor a b)
  complement (I64 a) = normInt64 (complement a)
  shiftL (I64 a) n = normInt64 (shiftL a n)
  shiftR (I64 a) n = normInt64 (shiftR a n)
  testBit (I64 a) n = testBit a n
  bit n = normInt64 (bit n)
  setBit (I64 a) n = normInt64 (setBit a n)
  clearBit (I64 a) n = normInt64 (clearBit a n)
  popCount (I64 a) = popCount (a `mod` 18446744073709551616)
  zeroBits = I64 0

