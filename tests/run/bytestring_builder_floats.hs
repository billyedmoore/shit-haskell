-- bytestring's builders of floating-point numbers and sized words: Ryu's shortest digits,
-- byte orders, and the decimal and hexadecimal encodings (bytestring's own tests).
module Main where

import qualified Data.ByteString.Builder as B
import qualified Data.ByteString.Builder.RealFloat as R
import qualified Data.ByteString.Lazy as L
import Data.Int (Int8, Int16, Int32)
import Data.Word (Word8, Word16, Word32)

bytes :: B.Builder -> [Word]
bytes = map fromIntegral . L.unpack . B.toLazyByteString

main :: IO ()
main = do
  print (bytes (B.floatLE 0.1), bytes (B.floatBE 0.1), bytes (B.doubleLE 0.1))
  print (B.toLazyByteString (B.floatDec 0.1 <> B.char7 ' ' <> B.doubleDec 1.2345678e10 <> B.char7 ' ' <> B.doubleDec 5.0e-324))
  print (B.toLazyByteString (R.formatDouble (R.standard 3) 3.14159 <> B.char7 ' ' <> R.formatFloat R.scientific 1.0e-45))
  print (B.toLazyByteString (mconcat [B.int8Dec (-128), B.char7 ' ', B.int16Dec 31234, B.char7 ' ', B.int32Dec (-7)]))
  print (B.toLazyByteString (mconcat [B.word8Dec 255, B.char7 ' ', B.word16Hex 4660, B.char7 ' ', B.word32HexFixed 3735928559, B.char7 ' ', B.word8HexFixed 10]))
  print (B.toLazyByteString (mconcat [B.int8HexFixed (-1), B.char7 ' ', B.int16HexFixed 255, B.char7 ' ', B.int32HexFixed (-2)]))
  print (B.toLazyByteString (B.floatHexFixed 1.5 <> B.char7 ' ' <> B.doubleHexFixed (-2.25)))
  print (map (\x -> B.toLazyByteString (B.doubleDec x)) [0, 1, 1.0e23, 2 ^ 60, 9.007199254740993e15, 1 / 0])
  print (map (\x -> B.toLazyByteString (B.floatDec x)) [3.4028235e38, 1.17549435e-38, 123456.78 :: Float])
  print (fromIntegral (maxBound :: Word16) :: Int32, fromIntegral (minBound :: Int8) :: Word8, fromIntegral (200 :: Word8) :: Int16)
  print (fromIntegral (3000000000 :: Word32) :: Int32)
