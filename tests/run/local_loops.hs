{-# LANGUAGE BangPatterns #-}
-- Local loops (from fusion or written as `where` functions): calls by value only for the
-- parameters every returning path evaluates, case-of-case copies that run each body once, and
-- case binders of partial applications.
import Control.Exception (ErrorCall (..), evaluate, try)
import Data.List (foldl')
import Debug.Trace (trace)

-- Fused: a loop whose accumulator is strict.
sumSquares :: Int -> Int
sumSquares n = sum (map (\x -> x * x) [1 .. n])

-- A local loop lazy in one parameter: an undefined argument it never uses is not evaluated.
lazyIn :: Int -> Int
lazyIn n = go n undefined 0
  where
    go :: Int -> Int -> Int -> Int
    go 0 _ !acc = acc
    go k unused !acc = if k > 100 then unused else go (k - 1) unused (acc + k)

-- Strict only on some paths: the argument is evaluated by the callee, not before the call.
someStrict :: Int -> Int
someStrict n = go n (error "forced")
  where
    go :: Int -> Int -> Int
    go 0 _ = 0
    go k e = if k == 1000 then e else go (k - 1) e

-- A case whose alternatives choose a function, applied afterwards (with a traced body: each
-- alternative's copy runs once).
choose :: Int -> Int -> Int
choose k = let f = if even k then trace "even" (+ 1) else trace "odd" (* 2) in f `seq` \x -> f x

-- foldl' with a function-valued accumulator step and shared work per element.
pipeline :: [Int] -> Int
pipeline = foldl' (\acc x -> let y = trace ("step " ++ show x) (x * 3) in acc + y + y) 0

main :: IO ()
main = do
  print (sumSquares 1000)
  print (lazyIn 50)
  print (someStrict 10)
  r <- try (evaluate (someStrict 2000))
  case r of
    Left (ErrorCall m) -> putStrLn ("caught: " ++ m)
    Right v -> print v
  print (map (choose 3) [1, 2], map (choose 4) [1, 2])
  print (pipeline [1, 2, 3])
  print (foldr (\x k acc -> k (acc + x)) id [1 .. 10 :: Int] 0)
