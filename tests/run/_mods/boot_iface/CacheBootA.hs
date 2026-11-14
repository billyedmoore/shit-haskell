{-# LANGUAGE FlexibleInstances #-}
module CacheBootA where

data T = T Int deriving Show

class Score a where
  score :: a -> Int
  score _ = 99

instance Score Int where score n = n * 2
instance Score Bool
instance Score T where score (T n) = n + 10

make :: Int -> T
make = T
