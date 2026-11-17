{-# LANGUAGE ParallelListComp #-}
-- List comprehensions built with foldr/build, so they fuse with what consumes them: their
-- results are the same, lazy where they were, and failed patterns still skip elements. Also
-- coercions and folds through newtype monoids, which the simplifier now sees through.
import Data.Coerce (coerce)
import Data.Monoid
import Debug.Trace (trace)

newtype Age = Age Int deriving Show

safe :: Int -> [Int] -> Bool
safe q qs = and [q /= c && abs (q - c) /= d | (d, c) <- zip [1 ..] qs]

queens :: Int -> [[Int]]
queens n = go n
  where
    go 0 = [[]]
    go k = [q : qs | qs <- go (k - 1), q <- [1 .. n], safe q qs]

main :: IO ()
main = do
  print (length (queens 6), head (queens 8))
  -- Infinite generators, and laziness in the elements.
  print (take 5 [x * x | x <- [1 :: Int ..], odd x])
  print (head [x | x <- [1 :: Int ..], x > 10])
  print (length [undefined | _ <- [1 .. 7 :: Int]])
  print (take 3 [trace ("elem " ++ show x) x | x <- [1 .. 10 :: Int]])
  -- Failed patterns skip; lets and nested generators.
  print [x | Just x <- [Just 1, Nothing, Just (3 :: Int)]]
  print [(x, y, s) | x <- [1 .. 3 :: Int], let s = x * 10, y <- "ab", odd x || y == 'b']
  print [[(i, j) | j <- [i .. 3]] | i <- [1 .. 3 :: Int]]
  print (sum [x | x <- [1 .. 100 :: Int], x `mod` 3 == 0], or [x > 99 | x <- [1 .. 100 :: Int]])
  print [(a, b) | a <- "xyz" | b <- [1 :: Int ..]]
  -- Folds through newtype monoids and coercions.
  print (getAll (foldMap (All . even) [2, 4, 6 :: Int]), getAny (foldMap (Any . odd) [2, 4 :: Int]))
  print (getSum (foldMap Sum [1 .. 10 :: Int]), getProduct (foldMap Product [1 .. 5 :: Int]))
  print (coerce (Age 3) + (1 :: Int), map (coerce :: Int -> Age) [1, 2])
  print (all even [x * 2 | x <- [1 .. 50 :: Int]], any (> 3) [] :: Bool)
  print (and [], or [True, undefined])
