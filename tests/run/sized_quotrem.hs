{-# LANGUAGE MagicHash, UnboxedTuples #-}
-- quotRem on the sized unboxed types (bytestring's decimal and hex builders use them).
module Main where

import GHC.Exts
import GHC.Word
import GHC.Int

main :: IO ()
main = do
  let qrW8 (W8# a) (W8# b) = case quotRemWord8# a b of (# q, r #) -> (W8# q, W8# r)
      qrW16 (W16# a) (W16# b) = case quotRemWord16# a b of (# q, r #) -> (W16# q, W16# r)
      qrW32 (W32# a) (W32# b) = case quotRemWord32# a b of (# q, r #) -> (W32# q, W32# r)
      qrI8 (I8# a) (I8# b) = case quotRemInt8# a b of (# q, r #) -> (I8# q, I8# r)
      qrI16 (I16# a) (I16# b) = case quotRemInt16# a b of (# q, r #) -> (I16# q, I16# r)
      qrI32 (I32# a) (I32# b) = case quotRemInt32# a b of (# q, r #) -> (I32# q, I32# r)
  print (qrW8 255 10, qrW16 65535 7, qrW32 4000000000 13)
  print (qrI8 (-128) 7, qrI16 (-30000) 11, qrI32 (-2000000000) 3, qrI8 100 (-9))
  print (map (`quotRem` 10) [minBound, -1, 0, maxBound :: Int8], map (`quotRem` 16) [0, 200, 255 :: Word8])
