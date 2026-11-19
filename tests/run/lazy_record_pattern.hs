-- Lazy and strict record patterns (cabal-install's `(f, ~KnownPackage{pinfoId, ..}) <- m`).
{-# LANGUAGE NamedFieldPuns, BangPatterns #-}
data K = K { a :: Int, b :: String }

lazyA :: K -> Int
lazyA ~K{a} = a

strictB :: K -> String
strictB !K{b} = b

main :: IO ()
main = do
  (x, ~K{a, b}) <- return (1 :: Int, K 2 "s")
  print (x, a, b)
  print (lazyA (K 5 ""), strictB (K 0 "q"))
  (y, ~(K c _)) <- return (3 :: Int, K 4 "t")
  print (y, c)
  let (z, ~K{a = w}) = (7 :: Int, undefined :: K)
  print z
