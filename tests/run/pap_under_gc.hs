-- Partial applications of an unknown function, made while the program allocates: a collection
-- may happen as the partial application object is allocated, which then must hold the
-- function's new address (the native runtime's apply kept the old one).
module Main where

{-# NOINLINE partially #-}
partially :: (Int -> Int -> Int -> Int) -> Int -> Int -> (Int -> Int)
partially f x y = f x y

combine :: Int -> Int -> Int -> Int
combine a b c = a * 31 + b * 7 + c + length (show (a + b))

main :: IO ()
main = do
  let fs = [partially combine i (i `mod` 13) | i <- [1 .. 200000]]
      total = sum (zipWith ($) fs [1 ..])
  print total
  print (length (concatMap (\g -> show (g 3)) (take 50000 fs)))
