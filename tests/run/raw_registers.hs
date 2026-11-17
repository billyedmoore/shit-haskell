{-# LANGUAGE BangPatterns #-}
-- Unboxed arithmetic kept in registers: division by constants (multiplication), many values
-- live at once, comparisons that branch directly, arguments that change places in a tail call,
-- and a loop that other threads interrupt while its numbers are in registers.
import Control.Concurrent
import Control.Exception
import Data.Bits
import Data.Char (chr, ord)
import Data.Word
import System.Timeout (timeout)

divs :: Int -> [Int]
divs n =
  [ n `quot` 7, n `rem` 7, n `div` 7, n `mod` 7
  , n `quot` (-7), n `rem` (-7), n `div` (-7), n `mod` (-7)
  , n `quot` 3, n `rem` 3, n `quot` 10, n `rem` 10, n `div` 16, n `mod` 16
  , n `quot` 1000000007, n `rem` 1000000007, n `quot` 6700417, n `div` (-2), n `mod` 2
  , n `quot` 4611686018427387903, n `rem` (-4611686018427387903)
  ]

samples :: [Int]
samples = [0, 1, -1, 6, 7, 8, -6, -7, -8, 13, -13, 100, -100, 123456789, -987654321, maxBound, minBound, maxBound - 1, minBound + 1, 2 ^ 62, -(2 ^ 62)]

-- More values live at once than there are registers for them.
spread :: Int -> Int -> Int
spread !acc 0 = acc
spread !acc n =
  let a = n * 3
      b = n + 7
      c = a - b
      d = a * b
      e = c `xor` d
      f = e + a
      g = f - b
      h = g * c
      i = h `quot` 9
      j = (a + b + c + d + e + f + g + h + i) `rem` 1000003
   in spread ((acc + j + a + b + c + d + e) `mod` 998244353) (n - 1)

-- The arguments swap places in each call.
swapLoop :: Int -> Int -> Int -> Int
swapLoop a b 0 = a * 1000 + b
swapLoop a b n = swapLoop (b + 1) a (n - 1)

-- Boxed arguments swapping places too.
swapList :: [Int] -> [Int] -> Int -> ([Int], [Int])
swapList xs ys 0 = (xs, ys)
swapList xs ys n = swapList ys (n : xs) (n - 1)

-- Comparisons on characters and words, used as branches and as values.
classify :: Char -> Int
classify c
  | c >= 'a' && c <= 'z' = 1
  | c >= 'A' && c <= 'Z' = 2
  | c < '0' = 3
  | otherwise = 4

wordMix :: Word -> Word -> Word
wordMix x y = if x > y then (x - y) `shiftR` 1 else (y * 3) `xor` (x `shiftL` 2)

countBelow :: Int -> Int -> Int -> Int
countBelow !acc lim n
  | n == 0 = acc
  | otherwise = countBelow (acc + fromEnum (n `rem` 13 < lim)) lim (n - 1)

spin :: Int -> Int -> Int -> Int
spin !a !b n = if n == -1 then a + b else spin ((a * 31 + n) `rem` 1000003) ((b + a) `quot` 3) (n + 1)
{-# NOINLINE spin #-}

main :: IO ()
main = do
  mapM_ (print . divs) samples
  print (spread 0 100000)
  print (swapLoop 1 2 10, swapLoop 1 2 11)
  print (swapList [] [0] 5)
  print (map classify "aZ !9z")
  print (wordMix 10 3, wordMix 3 10, wordMix maxBound 1)
  print (countBelow 0 5 100000, chr (ord 'a' + 3))
  r <- timeout 100000 (evaluate (spin 1 2 0))
  print r
