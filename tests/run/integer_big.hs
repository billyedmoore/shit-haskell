import Data.Bits
import Data.List (foldl')

fact :: Integer -> Integer
fact n = product [1 .. n]

fibs :: [Integer]
fibs = 0 : 1 : zipWith (+) fibs (tail fibs)

isqrt :: Integer -> Integer
isqrt 0 = 0
isqrt n = go n
  where
    go x = let y = (x + n `div` x) `div` 2 in if y >= x then x else go y

main :: IO ()
main = do
  print (fact 25)
  print (fact 60)
  print (fibs !! 300)
  let a = 2 ^ (200 :: Int) + 12345678901234567890 :: Integer
      b = 3 ^ (120 :: Int) - 987654321 :: Integer
  print (a + b)
  print (a - b)
  print (b - a)
  print (a * b)
  print (a `divMod` b, b `divMod` a)
  print ((a * b + 17) `quotRem` b)
  print ((-a) `divMod` 1000000007, (-a) `quotRem` 1000000007)
  print (a `divMod` (-1000000007), a `quotRem` (-1000000007))
  print (gcd a b, lcm 123456789012345678901234567890 987654321098765432109876543210)
  print (compare a b, a == b, a < b, max a b == a)
  print (negate a, abs (negate b), signum (negate a), signum (0 :: Integer))
  print (isqrt (10 ^ (80 :: Int)))
  print (fromIntegral (a `mod` 1000000) :: Int)
  print (fromInteger (2 ^ (64 :: Int) + 5) :: Int, fromInteger (2 ^ (63 :: Int)) :: Int)
  print (toInteger (maxBound :: Int) + 1, toInteger (minBound :: Int) - 1)
  print (a `shiftL` 70, a `shiftR` 150, b .&. a, b .|. 255, xor a b, popCount a, testBit a 200, testBit a 199)
  print (complement a, (-5 :: Integer) `shiftR` 1, (-a) .&. 0xffff)
  print (read "123456789012345678901234567890" + 1 :: Integer)
  print (show (-(10 ^ (30 :: Int)) :: Integer))
  print (foldl' (\acc d -> acc * 10 + d) 0 (concat (replicate 10 [1 .. 9])) :: Integer)
  print (fromIntegral a :: Double, fromIntegral (-b) :: Double)
  print (truncate (1.5e30 :: Double) :: Integer, round (-2.5e20 :: Double) :: Integer, floor (-0.5 :: Double) :: Integer, ceiling (1e18 + 0.5 :: Double) :: Integer)
  print (2 ^ (64 :: Int) `div` 3 :: Integer, (2 ^ (64 :: Int) - 1) * (2 ^ (64 :: Int) - 1) :: Integer)
  print (toRational (0.75 :: Double), fromRational (7 / 8) :: Double)
  print (minimum [a, b, a - b], sum (map fact [1 .. 20]))
