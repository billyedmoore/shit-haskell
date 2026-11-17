-- Moving values and rewriting with rules must not move work under a lambda that runs more than
-- once: each trace prints as often as in GHC.
import Data.List (foldl')
import Debug.Trace (trace)

expensive :: Int -> Int
expensive n = trace ("expensive " ++ show n) (n * 2)
{-# NOINLINE expensive #-}

-- A saturated call let-bound outside a lambda stays shared.
addAll :: [Int] -> [Int] -> [Int]
addAll xs ys = let n = trace "length" (length xs) in map (\y -> y + n) ys

-- A partial application of a call's result: the call is not moved under the lambda.
scaleAll :: Int -> [Int] -> [Int]
scaleAll k ys = let t = expensive k; f = (+) t in map (\y -> f y) ys

-- A rule ("filter", "filterFB") whose right side has a lambda per element.
keep :: Int -> [Int] -> [Int]
keep k xs = filter (\x -> x < expensive k) (filter even xs)

-- foldl'/foldl through foldr with oneShot: each step's work happens once per element.
stepCount :: [Int] -> Int
stepCount xs = let base = expensive 3 in foldl' (\acc x -> trace "step" (acc + x + base)) 0 xs

main :: IO ()
main = do
  print (stepCount [1, 2, 3], foldl (\acc x -> acc * 10 + x) 0 [1 .. 6 :: Int])
  print (addAll [1, 2, 3] [10, 20, 30, 40])
  print (scaleAll 5 [1, 2, 3])
  print (keep 7 [1 .. 20])
