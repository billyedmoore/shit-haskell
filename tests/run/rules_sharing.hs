-- Moving values and rewriting with rules must not move work under a lambda that runs more than
-- once: each trace prints as often as in GHC.
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

main :: IO ()
main = do
  print (addAll [1, 2, 3] [10, 20, 30, 40])
  print (scaleAll 5 [1, 2, 3])
  print (keep 7 [1 .. 20])
