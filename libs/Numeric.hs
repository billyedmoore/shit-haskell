module Numeric (showHex, showOct, showFFloat, showEFloat, showGFloat, showInt, showSigned, readHex, readOct, readDec, showFloat) where

import Base

showInt :: Integral a => a -> ShowS
showInt n r = show (toInteger n) ++ r

showSigned :: Real a => (a -> ShowS) -> Int -> a -> ShowS
showSigned showPos p x
  | x < 0 = showParen (p > 6) (showChar '-' . showPos (negate x))
  | otherwise = showPos x

showIntAtBase :: Integral a => a -> (Int -> Char) -> a -> ShowS
showIntAtBase base toChr n0 r0 = go (quotRem n0 base) r0
  where
    go (n, d) r = let r' = toChr (fromIntegral d) : r
                  in if n == 0 then r' else go (quotRem n base) r'

showHex :: Integral a => a -> ShowS
showHex = showIntAtBase 16 intToDigitN

showOct :: Integral a => a -> ShowS
showOct = showIntAtBase 8 intToDigitN

intToDigitN :: Int -> Char
intToDigitN i = if i < 10 then chr (ord '0' + i) else chr (ord 'a' + i - 10)

showFFloat :: RealFloat' a => Maybe Int -> a -> ShowS
showFFloat md x r = fixedFormat (maybe 6 id md) (toDouble x) ++ r

showEFloat :: RealFloat' a => Maybe Int -> a -> ShowS
showEFloat md x r = expFormat (maybe 6 id md) (toDouble x) ++ r

showGFloat :: RealFloat' a => Maybe Int -> a -> ShowS
showGFloat md x r = showFFloat md x r

showFloat :: RealFloat' a => a -> ShowS
showFloat x r = show (toDouble x) ++ r

class RealFloat' a where
  toDouble :: a -> Double

instance RealFloat' Double where
  toDouble x = x

foreign import prim "doubleFixed" primDoubleFixedN :: Int -> Double -> String
foreign import prim "doubleExpFmt" primDoubleExpFmtN :: Int -> Double -> String

fixedFormat :: Int -> Double -> String
fixedFormat d x = primDoubleFixedN d x

expFormat :: Int -> Double -> String
expFormat d x = primDoubleExpFmtN d x

readHex :: (Eq a, Num a) => ReadS a
readHex s = case span isHexC s of
  ([], _) -> []
  (ds, rest) -> [(foldl (\acc c -> acc * 16 + fromIntegral (digitVal c)) 0 ds, rest)]
  where
    isHexC c = isDigitC c || (c >= 'a' && c <= 'f') || (c >= 'A' && c <= 'F')
    digitVal c = if isDigitC c then ord c - 48 else if c >= 'a' then ord c - 87 else ord c - 55

readOct :: (Eq a, Num a) => ReadS a
readOct s = case span (\c -> c >= '0' && c <= '7') s of
  ([], _) -> []
  (ds, rest) -> [(foldl (\acc c -> acc * 8 + fromIntegral (ord c - 48)) 0 ds, rest)]

readDec :: (Eq a, Num a) => ReadS a
readDec s = case span isDigitC s of
  ([], _) -> []
  (ds, rest) -> [(foldl (\acc c -> acc * 10 + fromIntegral (ord c - 48)) 0 ds, rest)]
