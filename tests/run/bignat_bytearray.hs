{-# LANGUAGE MagicHash #-}
-- A BigNat# is a ByteArray# of 64-bit limbs (hashable hashes an Integer's bytes); BigNat has
-- Eq and Ord; GHC.Num.Primitives' wordLog2#.
module Main where

import GHC.Exts
import GHC.Num.BigNat (BigNat (..))
import GHC.Num.Integer (Integer (..))
import GHC.Num.Natural (Natural (..))
import GHC.Num.Primitives (wordLog2#)
import GHC.Word (Word8 (..))

limbBytes :: ByteArray# -> [Word8]
limbBytes ba = [W8# (indexWord8Array# ba i) | I# i <- [0 .. I# (sizeofByteArray# ba) - 1]]

describe :: Integer -> String
describe (IS i) = "small " ++ show (I# i)
describe (IP bn) = "positive, " ++ show (I# (sizeofByteArray# bn)) ++ " bytes " ++ show (limbBytes bn)
describe (IN bn) = "negative, " ++ show (I# (sizeofByteArray# bn)) ++ " bytes " ++ show (limbBytes bn)

big :: Integer -> BigNat
big (IP bn) = BN# bn
big (IN bn) = BN# bn
big _ = error "small"

main :: IO ()
main = do
  mapM_ (putStrLn . describe) [42, 2 ^ 64, 2 ^ 100 + 12345, negate (2 ^ 70 + 1), 2 ^ 63 - 1]
  print (big (2 ^ 80) == big (2 ^ 80), big (2 ^ 80) == big (2 ^ 81), compare (big (2 ^ 90)) (big (2 ^ 80)))
  case (2 ^ 65 :: Natural) of
    NB bn -> print (I# (sizeofByteArray# bn), limbBytes bn)
    NS _ -> putStrLn "small natural"
  print [W# (wordLog2# w) | W# w <- [1, 2, 3, 1024, maxBound]]
  -- Arithmetic on the byte-array representation.
  let x = 2 ^ 100 + 7 :: Integer
  print (x * x - 1, x `div` 3, gcd x (2 ^ 64), negate x `mod` 1000)
