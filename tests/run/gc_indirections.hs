-- Large live data with sharing, and long chains of updated thunks (indirections) that the
-- collector shortens while it copies: results are as before.
import Data.List (foldl')

chain :: Int -> Int
chain 0 = 1
chain n = let x = chain (n - 1) in x
{-# NOINLINE chain #-}

main :: IO ()
main = do
  let xs = map (* 2) [1 .. 200000 :: Int]
      ys = zip xs (tail xs)
  print (length xs, sum (map fst ys), foldl' (+) 0 (map snd ys))
  -- Thunks whose values are other thunks, all updated, then collected while reachable.
  let ts = [let a = b; b = c; c = i * 3 in a | i <- [1 .. 50000 :: Int]]
  print (sum ts, sum ts)
  print (chain 100000)
  let shared = replicate 3 (concat (replicate 2000 "abc"))
  print (map length shared, length (concat shared))
