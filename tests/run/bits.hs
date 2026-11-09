import Data.Bits
import Data.Word
import Data.Int

main :: IO ()
main = do
  print (5 .&. 3 :: Int, 5 .|. 3 :: Int, xor 5 3 :: Int)
  print (shiftL 1 10 :: Int, shiftR 1024 3 :: Int, complement 0 :: Int)
  print (popCount (255 :: Int), testBit (5 :: Int) 2)
  print (fromIntegral (300 :: Int) :: Word8, fromIntegral (200 :: Int) :: Int8)
  print (maxBound :: Word, maxBound :: Int32, minBound :: Int64)
  print ((2 :: Integer) ^ (100 :: Int) `mod` 1000007)
  print (toInteger (maxBound :: Word64) + 1)
  print (shiftL (1 :: Integer) 70)
