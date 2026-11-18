-- Integer arithmetic on large values: products with operands of every size order, decimal
-- output with zero chunks and signs, division, and small Integers in a loop (each step a
-- runtime call), and popCount of negative values (negated, as GHC counts them).
import Data.Bits (popCount)

main :: IO ()
main = do
  let f = product [1 .. 500 :: Integer]
      g = 2 ^ (1000 :: Int) - 1 :: Integer
  print (f `mod` 1000000007, length (show f), sum (map (\c -> fromEnum c - 48) (show f)))
  print (g * 3 == 3 * g, (g * g) `div` g == g, (f * g) `quot` f == g)
  print (10 ^ (45 :: Int) :: Integer, 10 ^ (45 :: Int) + 1 :: Integer, -(10 ^ (18 :: Int)) - 7 :: Integer)
  print (1000000000 * 1000000000 * 1000000000 :: Integer, (-123456789123456789) * 987654321987654321 :: Integer)
  print (show (2 ^ (64 :: Int) :: Integer), show (negate (2 ^ (63 :: Int)) :: Integer))
  print (foldl (+) 0 [1 .. 100000 :: Integer], foldr (\x acc -> x - acc) 0 [1 .. 1000 :: Integer])
  print (gcd f g, lcm (2 ^ (70 :: Int)) (3 ^ (40 :: Int) :: Integer), abs (negate f) == f, signum (negate g))
  print (map popCount [0, 255, -255, 2 ^ (36 :: Int), -(2 ^ (36 :: Int)), -(2 ^ (36 :: Int) - 1) :: Integer])
