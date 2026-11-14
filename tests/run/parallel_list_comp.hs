{-# LANGUAGE ParallelListComp #-}
-- Parallel list comprehensions: branches zipped, each binding several variables.
module Main where

main :: IO ()
main = do
  print [(x, y) | x <- [1 :: Int ..] | y <- "abc"]
  print [x + y * z | x <- [1, 2, 3 :: Int] | y <- [10, 20], let w = y | z <- [w | w <- [7, 8, 9]]]
  print [(a, b, c) | (a, b) <- zip [1 :: Int ..] "xyz", odd a | c <- [True, False, True]]
  print [n | n <- [1 :: Int ..] | _ <- "four"]
  print [s ++ t | s <- ["p", "q"], let t = s ++ s | k <- [1 :: Int, 2], even k || odd k]
