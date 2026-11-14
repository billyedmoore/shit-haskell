{-# LANGUAGE FlexibleInstances, KindSignatures #-}
module CacheBootA where
import Data.Kind (Type)

data T = T Int
class Score a where
  score :: a -> Int
  score _ = 99
instance Score (T :: Type)
make :: Int -> T
