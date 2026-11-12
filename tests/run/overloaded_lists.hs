{-# LANGUAGE OverloadedLists, TypeFamilies #-}
-- OverloadedLists: literals, sequences and patterns through IsList (alex's UTF8 module).
module Main where

import qualified Data.List.NonEmpty as NE
import GHC.Exts (IsList (..))

encode :: Int -> NE.NonEmpty Int
encode = fmap (* 2) . go
  where
    go n
      | n < 10 = [n]
      | n < 100 = [n `div` 10, n `mod` 10]
      | otherwise = [1, 2, 3]

newtype Bag = Bag [Int] deriving Show

instance IsList Bag where
  type Item Bag = Int
  fromList = Bag . reverse
  toList (Bag xs) = reverse xs

firstTwo :: Bag -> Maybe (Int, Int)
firstTwo [a, b] = Just (a, b)
firstTwo _ = Nothing

main :: IO ()
main = do
  print (encode 5, encode 42, encode 500)
  print ([3, 1, 2, 1] :: NE.NonEmpty Int)
  print ([1 .. 5] :: Bag)
  print (firstTwo [7, 8], firstTwo [1, 2, 3])
  print ([] :: [Int], ['a', 'b'] :: String)
  case [1, 2] :: [Int] of
    [x, y] -> print (x + y)
    _ -> putStrLn "no"
