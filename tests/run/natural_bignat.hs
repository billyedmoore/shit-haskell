-- Large Naturals (built through BigNat#, as binary's Natural instance does with them).
module Main where

import Data.Bits (shiftL, shiftR, (.&.), (.|.), testBit, popCount)
import Numeric.Natural (Natural)

roll :: [Word] -> Natural
roll = foldr (\w acc -> acc `shiftL` 8 .|. fromIntegral w) 0

unroll :: Natural -> [Word]
unroll 0 = []
unroll n = fromIntegral (n .&. 255) : unroll (n `shiftR` 8)

main :: IO ()
main = do
  let big = 2 ^ (130 :: Int) + 12345 :: Natural
      huge = fromIntegral (3 ^ (60 :: Int) :: Integer) :: Natural
  print (big, huge, big * huge, huge `div` 7, huge `mod` 1000003)
  print (unroll big, roll (unroll huge) == huge)
  print (toInteger big - 1, fromInteger (toInteger huge) :: Natural, testBit big 130, popCount huge)
  print (big - 12345 == 2 ^ (130 :: Int), compare big huge, fromIntegral big :: Double)
