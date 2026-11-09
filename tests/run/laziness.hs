primes :: [Int]
primes = sieve [2 ..]
  where sieve (p : xs) = p : sieve [x | x <- xs, x `mod` p /= 0]

fibs :: [Integer]
fibs = 0 : 1 : zipWith (+) fibs (tail fibs)

main :: IO ()
main = do
  print (take 10 primes)
  print (fibs !! 100)
  let xs = [1 ..] :: [Int]
  print (takeWhile (< 20) (filter odd xs))
  print (fst (1 :: Int, undefined :: Int))
  print (length [undefined, undefined :: Int])
  print (take 7 (cycle [1, 2, 3 :: Int]))
  print (const 'k' (error "never" :: Int))
