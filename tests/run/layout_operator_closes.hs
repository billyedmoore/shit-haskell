-- An operator at the column of a do block's statements ends the block (parse-error(t)), as
-- Cabal-syntax's version range parser writes `(do ..\n <|> return t)`.
module Main where

import Control.Applicative ((<|>))

pick :: Maybe Int -> Maybe Int
pick m =
  ( do
      x <- m
      if x > 0 then return x else Nothing
      <|> return 0
  )

total :: [Int] -> Int
total xs = let s = sum xs
               in s
           + length xs

main :: IO ()
main = do
  print (map pick [Just 3, Just (-1), Nothing])
  print (total [1, 2, 3])
  let neg = 5
  print (- neg, [ y | y <- [1 .. 3 :: Int] ])
