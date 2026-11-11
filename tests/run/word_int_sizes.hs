import Data.Bits
import Data.Int
import Data.Word

main :: IO ()
main = do
  print (maxBound :: Int8, minBound :: Int8, maxBound :: Int16, minBound :: Int16)
  print (maxBound :: Int32, minBound :: Int32, maxBound :: Int64, minBound :: Int64)
  print (maxBound :: Word8, maxBound :: Word16, maxBound :: Word32, maxBound :: Word64, maxBound :: Word)
  print ((127 :: Int8) + 1, (200 :: Word8) + 100, (0 :: Word8) - 1, (32767 :: Int16) + 1)
  print ((maxBound :: Int32) + 1, (maxBound :: Int64) + 1, (0 :: Word32) - 1, (0 :: Word64) - 1)
  print (fromIntegral (300 :: Int) :: Word8, fromIntegral (-1 :: Int) :: Word16, fromIntegral (200 :: Word8) :: Int8)
  print (fromIntegral (maxBound :: Word64) :: Int, fromIntegral (-1 :: Int8) :: Word64, fromIntegral (2 ^ (40 :: Int) + 7 :: Int) :: Int32)
  print ((100 :: Int8) * 3, (65535 :: Word16) * 65535, (-128 :: Int8) `div` (-1 + 2), (7 :: Word8) `div` 2, (-7 :: Int8) `mod` 3, (-7 :: Int8) `rem` 3)
  print ((0xf0 :: Word8) .&. 0x3c, (0xf0 :: Word8) .|. 0x0f, xor (0xff :: Word8) 0x0f, complement (0 :: Word16), complement (0 :: Int32))
  print (shiftL (1 :: Word8) 7, shiftL (1 :: Word8) 8, shiftR (0x80 :: Word8) 3, shiftR (-128 :: Int8) 3, rotateL (0x81 :: Word8) 1, rotateR (1 :: Word32) 1)
  print (popCount (0xffff :: Word32), countLeadingZeros (1 :: Word16), countTrailingZeros (8 :: Int64), finiteBitSize (0 :: Int8), testBit (5 :: Word8) 2)
  print (toInteger (maxBound :: Word64) + 1, fromInteger (2 ^ (70 :: Int) + 3) :: Word64, fromInteger (-1) :: Word32)
  print (sum (map fromIntegral [1 .. 100 :: Int]) :: Word8, product [1 .. 10] :: Int32, product [1 .. 20] :: Int64, product [1 .. 21] :: Int64)
  print ([minBound .. maxBound :: Int8] !! 200, length [minBound .. maxBound :: Word8], succ (254 :: Word8), pred (-127 :: Int8))
  print (compare (200 :: Word8) 100, (200 :: Word8) > 100, (-56 :: Int8) < 100, minimum [3, 250, 7 :: Word8])
  print (read "255" :: Word8, read "-128" :: Int8, show (maxBound :: Word64), (fromIntegral (ord' 'A') :: Word8))
  print (quotRem (minBound :: Int64) 10, divMod (minBound :: Int64) 10, divMod (maxBound :: Word64) 7)
  print (realToFrac (3 :: Word8) / 2 :: Double, truncate (300.7 :: Double) :: Word8, round (-1.5 :: Double) :: Int8)
  print (byteSwap32 0x11223344, byteSwap16 0x1122, bit 5 :: Word16, zeroBits :: Int32)
  where
    ord' = fromEnum
