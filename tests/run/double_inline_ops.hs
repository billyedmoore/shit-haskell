-- Double# arithmetic and comparisons computed inline (codegen double_fast_path): NaN, infinities
-- and signed zeros, operands that are thunks, results that are compared again.
main :: IO ()
main = do
  let nan = 0/0 :: Double
      xs = [1.5, -2.25, 0, nan, 1/0, -0.0] :: [Double]
  print [(a < b, a <= b, a > b, a >= b, a == b, a /= b) | a <- xs, b <- xs]
  print [(a + b, a - b, a * b, a / b) | a <- xs, b <- take 3 xs]
  print (map sqrt xs, map negate xs, map abs xs)
  let lazy = [fromIntegral i / 7 | i <- [1 .. 10 :: Int]] :: [Double]
  print (sum (zipWith (*) lazy (reverse lazy)), maximum lazy, minimum (map negate lazy))
